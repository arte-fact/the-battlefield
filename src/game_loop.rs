use crate::animation;
use crate::animation::TurnAnimator;
use crate::autotile;
use crate::game::Game;
use crate::grid::{self, TileKind, TILE_SIZE};
use crate::input::Input;
use crate::particle::{Particle, Projectile};
use crate::renderer::{draw_sprite, load_image, Canvas2d, TextureId, TextureManager};
use crate::sprite::SpriteSheet;
use crate::unit::{Facing, Faction, UnitAnim, UnitKind, DEATH_FADE_DURATION};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// Texture keys for loaded unit animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct UnitTextureKey {
    faction: Faction,
    kind: UnitKind,
    anim: UnitAnim,
}

struct LoadedTextures {
    unit_textures: HashMap<UnitTextureKey, TextureId>,
    particle_textures: HashMap<&'static str, TextureId>,
    arrow_texture: Option<TextureId>,
    tilemap_texture: Option<TextureId>,
    tilemap_texture2: Option<TextureId>,
    water_texture: Option<TextureId>,
    shadow_texture: Option<TextureId>,
    foam_texture: Option<TextureId>,
    /// Tree sprite sheets: (texture_id, frame_w, frame_h) for 4 variants
    tree_textures: Vec<(TextureId, u32, u32)>,
}

impl LoadedTextures {
    fn new() -> Self {
        Self {
            unit_textures: HashMap::new(),
            particle_textures: HashMap::new(),
            arrow_texture: None,
            tilemap_texture: None,
            tilemap_texture2: None,
            water_texture: None,
            shadow_texture: None,
            foam_texture: None,
            tree_textures: Vec::new(),
        }
    }
}

/// Cached HUD DOM elements.
struct HudElements {
    turn_display: web_sys::Element,
    hp_bar_fill: web_sys::HtmlElement,
}

impl HudElements {
    fn from_document(doc: &web_sys::Document) -> Option<Self> {
        let turn_display = doc.get_element_by_id("turn-display")?;
        let hp_bar_fill = doc
            .get_element_by_id("hp-bar-fill")?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        Some(Self {
            turn_display,
            hp_bar_fill,
        })
    }

    fn update(&self, game: &Game) {
        let turn_text = format!("Turn {}", game.turn_number());
        self.turn_display.set_text_content(Some(&turn_text));

        if let Some(player) = game.player_unit() {
            let ratio = player.hp as f32 / player.stats.max_hp as f32;
            let pct = format!("{}%", (ratio * 100.0) as u32);
            let _ = self.hp_bar_fill.style().set_property("width", &pct);

            let color = if ratio > 0.5 {
                "#4caf50"
            } else if ratio > 0.25 {
                "#ff9800"
            } else {
                "#f44336"
            };
            let _ = self.hp_bar_fill.style().set_property("background", color);
        } else {
            let _ = self.hp_bar_fill.style().set_property("width", "0%");
        }
    }
}

pub fn run(
    canvas2d: Canvas2d,
    game: Game,
    texture_manager: TextureManager,
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<(), JsValue> {
    let input = Rc::new(RefCell::new(Input::new()));
    let loaded_textures = Rc::new(RefCell::new(LoadedTextures::new()));
    let textures_loading = Rc::new(RefCell::new(false));

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let hud = HudElements::from_document(&document);
    let hud = Rc::new(hud);

    setup_input_listeners(canvas, &input)?;

    // Create offscreen fog canvas (1 pixel per tile, bilinear-interpolated when drawn)
    let fog_canvas = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    fog_canvas.set_width(grid::GRID_SIZE);
    fog_canvas.set_height(grid::GRID_SIZE);
    let fog_ctx = fog_canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    let state = Rc::new(RefCell::new(LoopState {
        canvas2d,
        game,
        texture_manager,
        last_time: None,
        elapsed: 0.0,
        fog_canvas,
        fog_ctx,
        animator: TurnAnimator::new(),
    }));

    let preview_path: Rc<RefCell<Vec<(u32, u32)>>> = Rc::new(RefCell::new(Vec::new()));

    // Start async texture loading
    {
        let state_clone = state.clone();
        let loaded_clone = loaded_textures.clone();
        let loading_flag = textures_loading.clone();
        *loading_flag.borrow_mut() = true;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = load_textures(&state_clone, &loaded_clone).await {
                log::error!("Failed to load textures: {:?}", e);
            }
            *loading_flag.borrow_mut() = false;
            log::info!("All textures loaded");
        });
    }

    // Game loop
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {
        {
            let mut state_guard = state.borrow_mut();
            let last_time = state_guard.last_time;
            state_guard.last_time = Some(timestamp);

            let dt = match last_time {
                Some(last) => (timestamp - last) / 1000.0,
                None => 0.0,
            };
            let dt = dt.min(0.1);
            state_guard.elapsed += dt;

            // Process input
            {
                let animating = state_guard.animator.is_playing();
                let game = &mut state_guard.game;
                let mut inp = input.borrow_mut();

                // Camera controls (always available, even during animations)
                let (pan_x, pan_y) = inp.camera_pan();
                if pan_x != 0.0 || pan_y != 0.0 {
                    game.camera.pan(pan_x, pan_y, dt as f32);
                }

                // Mouse wheel zoom
                let scroll = inp.take_scroll();
                if scroll != 0.0 {
                    game.camera.zoom_by(scroll);
                }

                // Touch: pinch-to-zoom
                let pinch = inp.take_pinch_zoom();
                if pinch.abs() > f32::EPSILON {
                    game.camera.zoom_by(pinch);
                }

                // Touch: two-finger pan
                let (pan_tx, pan_ty) = inp.take_touch_pan();
                if pan_tx.abs() > f32::EPSILON || pan_ty.abs() > f32::EPSILON {
                    game.camera.x -= pan_tx / game.camera.zoom;
                    game.camera.y -= pan_ty / game.camera.zoom;
                }

                // Game actions are gated behind animation completion
                if !animating {
                    // Any manual input cancels auto-move
                    let has_manual_input = inp.keys_down.iter().any(|k| k.starts_with("Arrow"))
                        || inp.swipe.is_some()
                        || inp.mouse_clicked;

                    if has_manual_input {
                        game.cancel_auto_path();
                    }

                    // Arrow keys -> movement (1 tile per press)
                    if let Some(dir) = inp.take_arrow_step() {
                        game.player_step(dir);
                    }

                    // Touch short swipe -> movement (1 tile per swipe)
                    if let Some(dir) = inp.take_swipe() {
                        game.player_step(dir);
                    }

                    // Touch long swipe -> apply delta from player position for pathfinding
                    if let Some((sdx, sdy)) = inp.take_long_swipe() {
                        if let Some(player) = game.player_unit() {
                            let (pwx, pwy) = grid::grid_to_world(player.grid_x, player.grid_y);
                            let wdx = sdx / game.camera.zoom;
                            let wdy = sdy / game.camera.zoom;
                            let (gx, gy) = grid::world_to_grid(pwx + wdx, pwy + wdy);
                            if game.grid.in_bounds(gx, gy) {
                                game.set_auto_path(gx as u32, gy as u32);
                            }
                        }
                    }

                    // Mouse click -> derive direction from player, step 1 tile
                    if let Some((sx, sy)) = inp.take_click() {
                        handle_click(game, sx, sy);
                    }
                }
            }

            // Compute live swipe preview path (delta applied from player position)
            {
                let game = &state_guard.game;
                let inp = input.borrow();
                let mut pp = preview_path.borrow_mut();
                pp.clear();
                if let Some(swipe) = inp.swipe_state() {
                    let sdx = swipe.current_x - swipe.start_x;
                    let sdy = swipe.current_y - swipe.start_y;
                    let dist = (sdx * sdx + sdy * sdy).sqrt();
                    if dist >= 60.0 {
                        if let Some(player) = game.player_unit() {
                            let (pwx, pwy) = grid::grid_to_world(player.grid_x, player.grid_y);
                            let wdx = sdx / game.camera.zoom;
                            let wdy = sdy / game.camera.zoom;
                            let (gx, gy) = grid::world_to_grid(pwx + wdx, pwy + wdy);
                            if game.grid.in_bounds(gx, gy) {
                                if let Some(path) = game.grid
                                    .find_path(player.grid_x, player.grid_y, gx as u32, gy as u32, 30, |_, _| false) {
                                    *pp = path;
                                }
                            }
                        }
                    }
                }
            }

            // Process turn events: spawn dust for moves, enqueue attack animations
            if !state_guard.game.turn_events.is_empty() {
                let events = state_guard.game.turn_events.drain(..).collect::<Vec<_>>();
                let alive_ids: Vec<_> = state_guard.game.units.iter()
                    .filter(|u| u.alive).map(|u| u.id).collect();
                state_guard.animator.init_visual_alive(alive_ids.into_iter());
                let anim_output = state_guard.animator.enqueue(events);
                // Immediately spawn dust particles from move events
                for (kind, x, y) in anim_output.particles {
                    state_guard.game.particles.push(Particle::new(kind, x, y));
                }
            }

            // Process auto-move path (time-based: 0.15s per step, gated by animations)
            if !state_guard.animator.is_playing() && state_guard.game.is_auto_moving() {
                state_guard.game.auto_move_timer += dt as f32;
                if state_guard.game.auto_move_timer >= 0.15 {
                    state_guard.game.auto_move_timer = 0.0;
                    state_guard.game.auto_move_step();
                }
            }

            // Advance attack animations and collect spawned effects
            {
                let LoopState { ref mut animator, ref mut game, .. } = *state_guard;
                if animator.is_playing() {
                    let anim_output = animator.update(dt as f32, &mut game.units);
                    for (kind, x, y) in anim_output.particles {
                        game.particles.push(Particle::new(kind, x, y));
                    }
                    for (sx, sy, tx, ty) in anim_output.projectiles {
                        game.projectiles.push(Projectile::new(sx, sy, tx, ty));
                    }
                }
            }

            // Smoothly lerp all unit visual positions toward their grid positions
            animation::lerp_visual_positions(&mut state_guard.game.units, dt as f32);

            // Update game state (animations, particles, camera follow)
            state_guard.game.update(dt);

            // Update HUD
            if let Some(ref hud) = *hud.as_ref() {
                hud.update(&state_guard.game);
            }
        }

        // Render
        {
            if !*textures_loading.borrow() {
                let loaded = loaded_textures.borrow();
                let pp = preview_path.borrow();
                let mut state_guard = state.borrow_mut();
                if let Err(e) = render_frame(&mut state_guard, &loaded, &pp) {
                    log::error!("Render error: {:?}", e);
                }
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn handle_click(game: &mut Game, screen_x: f32, screen_y: f32) {
    use crate::input::SwipeDir;

    let (wx, wy) = game.camera.screen_to_world(screen_x, screen_y);
    let (gx, gy) = grid::world_to_grid(wx, wy);

    if !game.grid.in_bounds(gx, gy) {
        return;
    }

    let gx = gx as u32;
    let gy = gy as u32;

    // Derive direction from player to clicked tile
    if let Some(player) = game.player_unit() {
        let dx = gx as i32 - player.grid_x as i32;
        let dy = gy as i32 - player.grid_y as i32;
        if let Some(dir) = SwipeDir::from_grid_delta(dx, dy) {
            game.player_step(dir);
        }
    }
}

fn setup_input_listeners(
    canvas: &web_sys::HtmlCanvasElement,
    input: &Rc<RefCell<Input>>,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;

    // Keyboard
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            input_clone.borrow_mut().key_down(e.key());
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            input_clone.borrow_mut().key_up(&e.key());
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Mouse click
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
            let mut inp = input_clone.borrow_mut();
            inp.mouse_x = e.offset_x() as f32;
            inp.mouse_y = e.offset_y() as f32;
            inp.mouse_clicked = true;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Mouse wheel (zoom)
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            let delta = -e.delta_y().signum() as f32;
            input_clone.borrow_mut().scroll_delta += delta;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Touch events
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            let count = touches.length();
            if count >= 1 {
                let t = e.changed_touches().get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                input_clone
                    .borrow_mut()
                    .on_touch_start(t.identifier(), cx, cy, count);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            if touches.length() == 1 {
                let t = touches.get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                input_clone.borrow_mut().on_touch_move_single(cx, cy);
            } else if touches.length() >= 2 {
                let t0 = touches.get(0).unwrap();
                let t1 = touches.get(1).unwrap();
                let (x0, y0) = canvas_touch_coords(&canvas_clone, &t0);
                let (x1, y1) = canvas_touch_coords(&canvas_clone, &t1);
                input_clone
                    .borrow_mut()
                    .on_touch_move_two_finger(x0, y0, x1, y1);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let ct = e.changed_touches();
            if ct.length() >= 1 {
                let t = ct.get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                let remaining = e.touches().length();
                input_clone
                    .borrow_mut()
                    .on_touch_end(t.identifier(), cx, cy, remaining);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Focus the canvas for keyboard events
    canvas.set_tab_index(0);
    canvas.focus()?;

    Ok(())
}

/// Convert a Touch's client coordinates to canvas-relative coordinates.
fn canvas_touch_coords(
    canvas: &web_sys::HtmlCanvasElement,
    touch: &web_sys::Touch,
) -> (f32, f32) {
    let rect = canvas.get_bounding_client_rect();
    let scale_x = canvas.width() as f64 / rect.width();
    let scale_y = canvas.height() as f64 / rect.height();
    let cx = (touch.client_x() as f64 - rect.left()) * scale_x;
    let cy = (touch.client_y() as f64 - rect.top()) * scale_y;
    (cx as f32, cy as f32)
}

/// Load a texture through `LoopState`, splitting borrows around the await.
async fn load_texture(
    state: &Rc<RefCell<LoopState>>,
    url: &str,
    frame_w: u32,
    frame_h: u32,
    frame_count: u32,
) -> Result<TextureId, JsValue> {
    {
        let guard = state.borrow();
        if let Some(id) = guard.texture_manager.get_cached(url) {
            return Ok(id);
        }
    }

    let element = load_image(url).await?;

    log::info!(
        "Loaded sprite sheet: {url} ({}x{}, {frame_count} frames of {frame_w}x{frame_h})",
        element.natural_width(),
        element.natural_height()
    );

    let id = {
        let mut guard = state.borrow_mut();
        guard
            .texture_manager
            .register(url, element, frame_w, frame_h, frame_count)
    };

    Ok(id)
}

async fn load_textures(
    state: &Rc<RefCell<LoopState>>,
    loaded: &Rc<RefCell<LoadedTextures>>,
) -> Result<(), JsValue> {
    let factions = [Faction::Blue, Faction::Red];
    let unit_kinds = [
        (UnitKind::Warrior, "Warrior"),
        (UnitKind::Archer, "Archer"),
        (UnitKind::Lancer, "Lancer"),
        (UnitKind::Pawn, "Pawn"),
        (UnitKind::Monk, "Monk"),
    ];
    let anims = [
        (UnitAnim::Idle, "Idle"),
        (UnitAnim::Run, "Run"),
        (UnitAnim::Attack, "Attack"),
    ];

    for &faction in &factions {
        for &(kind, kind_name) in &unit_kinds {
            for &(anim, _anim_name) in &anims {
                let (filename, frame_count) = match (kind, anim) {
                    (UnitKind::Warrior, UnitAnim::Idle) => ("Warrior_Idle.png", 8),
                    (UnitKind::Warrior, UnitAnim::Run) => ("Warrior_Run.png", 6),
                    (UnitKind::Warrior, UnitAnim::Attack) => ("Warrior_Attack1.png", 4),
                    (UnitKind::Archer, UnitAnim::Idle) => ("Archer_Idle.png", 6),
                    (UnitKind::Archer, UnitAnim::Run) => ("Archer_Run.png", 4),
                    (UnitKind::Archer, UnitAnim::Attack) => ("Archer_Shoot.png", 8),
                    (UnitKind::Lancer, UnitAnim::Idle) => ("Lancer_Idle.png", 12),
                    (UnitKind::Lancer, UnitAnim::Run) => ("Lancer_Run.png", 6),
                    (UnitKind::Lancer, UnitAnim::Attack) => ("Lancer_Right_Attack.png", 3),
                    (UnitKind::Pawn, UnitAnim::Idle) => ("Pawn_Idle.png", 8),
                    (UnitKind::Pawn, UnitAnim::Run) => ("Pawn_Run.png", 6),
                    (UnitKind::Pawn, UnitAnim::Attack) => ("Pawn_Interact Axe.png", 6),
                    (UnitKind::Monk, UnitAnim::Idle) => ("Idle.png", 6),
                    (UnitKind::Monk, UnitAnim::Run) => ("Run.png", 4),
                    (UnitKind::Monk, UnitAnim::Attack) => ("Heal.png", 11),
                    _ => continue,
                };

                let frame_size = kind.frame_size();
                let url = format!(
                    "{}/Units/{}/{}/{}",
                    ASSET_BASE,
                    faction.asset_folder(),
                    kind_name,
                    filename
                );

                let tex_id =
                    load_texture(state, &url, frame_size, frame_size, frame_count).await?;

                loaded.borrow_mut().unit_textures.insert(
                    UnitTextureKey {
                        faction,
                        kind,
                        anim,
                    },
                    tex_id,
                );
            }
        }
    }

    // Load particle effects
    let particles = [
        ("Dust_01.png", 64, 8),
        ("Explosion_01.png", 192, 8),
        ("Explosion_02.png", 192, 10),
    ];
    for &(filename, frame_size, frame_count) in &particles {
        let url = format!("{}/Particle FX/{}", ASSET_BASE, filename);
        let tex_id = load_texture(state, &url, frame_size, frame_size, frame_count).await?;
        loaded
            .borrow_mut()
            .particle_textures
            .insert(filename, tex_id);
    }

    // Load arrow projectile
    {
        let url = format!("{}/Units/Blue Units/Archer/Arrow.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().arrow_texture = Some(tex_id);
    }

    // Load tilemap textures
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color1.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 576, 384, 1).await?;
        loaded.borrow_mut().tilemap_texture = Some(tex_id);
    }
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color2.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 576, 384, 1).await?;
        loaded.borrow_mut().tilemap_texture2 = Some(tex_id);
    }

    // Load water background texture
    {
        let url = format!(
            "{}/Terrain/Tileset/Water Background color.png",
            ASSET_BASE
        );
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().water_texture = Some(tex_id);
    }

    // Load shadow texture
    {
        let url = format!("{}/Terrain/Tileset/Shadow.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 192, 192, 1).await?;
        loaded.borrow_mut().shadow_texture = Some(tex_id);
    }

    // Load water foam texture
    {
        let url = format!("{}/Terrain/Tileset/Water Foam.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 192, 192, 16).await?;
        loaded.borrow_mut().foam_texture = Some(tex_id);
    }

    // Load tree sprites (4 variants)
    // Tree1/2: 1536x256, 8 frames of 192x256
    // Tree3/4: 1536x192, 8 frames of 192x192
    let trees = [
        ("Tree1.png", 192u32, 256u32, 8u32),
        ("Tree2.png", 192, 256, 8),
        ("Tree3.png", 192, 192, 8),
        ("Tree4.png", 192, 192, 8),
    ];
    for &(filename, fw, fh, frame_count) in &trees {
        let url = format!("{}/Terrain/Resources/Wood/Trees/{}", ASSET_BASE, filename);
        let tex_id = load_texture(state, &url, fw, fh, frame_count).await?;
        loaded.borrow_mut().tree_textures.push((tex_id, fw, fh));
    }

    Ok(())
}

fn render_frame(state: &mut LoopState, loaded: &LoadedTextures, preview_path: &[(u32, u32)]) -> Result<(), JsValue> {
    // Update fog offscreen canvas if FOV changed
    if state.game.fog_dirty {
        update_fog_canvas(&state.fog_ctx, &state.game)?;
        state.game.fog_dirty = false;
    }

    let ctx = &state.canvas2d.ctx;
    let canvas_w = state.canvas2d.width;
    let canvas_h = state.canvas2d.height;
    let game = &state.game;
    let tm = &state.texture_manager;

    let ts = TILE_SIZE as f64;

    // 1. Clear canvas
    ctx.set_fill_style_str("#1a1a26");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    // 2. Apply camera transform
    let zoom = game.camera.zoom as f64;
    ctx.save();
    ctx.translate(canvas_w / 2.0, canvas_h / 2.0)?;
    ctx.scale(zoom, zoom)?;
    ctx.translate(-(game.camera.x as f64), -(game.camera.y as f64))?;

    // Visible tile range
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(game.grid.width as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(game.grid.height as i32) as u32;

    // 3. Draw terrain tiles
    draw_tiles(
        ctx,
        game,
        loaded,
        tm,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        state.elapsed,
    )?;

    // 4. Draw grid overlay
    draw_grid_lines(ctx, min_gx, min_gy, max_gx, max_gy)?;

    // 5. Draw overlays (player highlight, HP bars, path preview)
    draw_overlays(ctx, game, min_gx, min_gy, max_gx, max_gy, ts, preview_path, &state.animator)?;

    // 6. Draw foreground sprites (units, particles, projectiles)
    draw_foreground(ctx, game, loaded, tm, &state.animator)?;

    // 7. Draw trees on forest tiles (on top of units for depth)
    draw_trees(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy, state.elapsed)?;

    // 8. Draw fog of war from cached offscreen canvas (single drawImage call)
    let grid_world_size = (game.grid.width as f64) * ts;
    ctx.set_image_smoothing_enabled(true);
    ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
        &state.fog_canvas,
        0.0,
        0.0,
        grid_world_size,
        grid_world_size,
    )?;

    ctx.restore();

    Ok(())
}

fn draw_tiles(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;
    let ts_draw = ts + 1.0;

    // Layer 1: Water background
    if let Some(water_tex_id) = loaded.water_texture {
        if let Some((img, _, _, _)) = tm.get_image(water_tex_id) {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    let dx = (gx as f64) * ts;
                    let dy = (gy as f64) * ts;
                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, 64.0, 64.0, dx, dy, ts_draw, ts_draw,
                    )?;
                }
            }
        }
    }

    // Layer 2: Water foam (on land tiles adjacent to water)
    if let Some(foam_tex_id) = loaded.foam_texture {
        if let Some((img, _, _, _)) = tm.get_image(foam_tex_id) {
            let foam_fps = 8.0;
            let global_frame = (elapsed * foam_fps) as u32;

            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    let idx = (gy * game.grid.width + gx) as usize;
                    if !game.water_adjacency.get(idx).copied().unwrap_or(false) {
                        continue;
                    }
                    let tile_offset =
                        (gx.wrapping_mul(7).wrapping_add(gy.wrapping_mul(13))) % 16;
                    let frame = (global_frame + tile_offset) % 16;
                    let foam_size = 192.0_f64;
                    let sx = (frame as f64) * foam_size;
                    let dx = (gx as f64) * ts + ts / 2.0 - foam_size / 2.0;
                    let dy = (gy as f64) * ts + ts / 2.0 - foam_size / 2.0;
                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, sx, 0.0, foam_size, foam_size, dx, dy, foam_size, foam_size,
                    )?;
                }
            }
        }
    }

    // Layer 3: Flat ground (auto-tiled)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if !game.grid.get(gx, gy).is_land() {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts;
                    let dy = (gy as f64) * ts;
                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, sx, sy, sw, sh, dx, dy, ts_draw, ts_draw,
                    )?;
                }
            }
        }
    }

    // Layer 4: Elevation (shadow + elevated surface + cliff)
    for level in 2..=2u8 {
        // Shadow pass
        if let Some(shadow_tex_id) = loaded.shadow_texture {
            if let Some((img, _, _, _)) = tm.get_image(shadow_tex_id) {
                ctx.set_global_alpha(0.5);
                for gy in min_gy..max_gy {
                    for gx in min_gx..max_gx {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        if gy + 1 < game.grid.height
                            && game.grid.elevation(gx, gy + 1) < level
                        {
                            let shadow_size = 192.0_f64;
                            let dx = (gx as f64) * ts + ts / 2.0 - shadow_size / 2.0;
                            let dy = ((gy + 1) as f64) * ts + ts / 2.0 - shadow_size / 2.0;
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, 0.0, 0.0, shadow_size, shadow_size, dx, dy, shadow_size, shadow_size,
                            )?;
                        }
                    }
                }
                ctx.set_global_alpha(1.0);
            }
        }

        // Elevated surface + cliff
        let elev_tex_id = if level == 2 {
            loaded.tilemap_texture2
        } else {
            loaded.tilemap_texture
        };
        if let Some(tilemap_tex_id) = elev_tex_id {
            if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
                for gy in min_gy..max_gy {
                    for gx in min_gx..max_gx {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        let (col, row) =
                            autotile::elevated_top_src(&game.grid, gx, gy, level);
                        let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                        let dx = (gx as f64) * ts;
                        let dy = (gy as f64) * ts;
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts_draw, ts_draw,
                        )?;

                        if let Some((ccol, crow)) =
                            autotile::cliff_src(&game.grid, gx, gy, level)
                        {
                            let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
                            let cdy = ((gy + 1) as f64) * ts;
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, csx, csy, csw, csh, dx, cdy, ts_draw, ts_draw,
                            )?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn draw_grid_lines(
    ctx: &web_sys::CanvasRenderingContext2d,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;
    ctx.set_stroke_style_str("rgba(0,0,0,0.08)");
    ctx.set_line_width(0.5);
    ctx.begin_path();
    // Vertical lines
    for gx in min_gx..=max_gx {
        let x = (gx as f64) * ts;
        ctx.move_to(x, (min_gy as f64) * ts);
        ctx.line_to(x, (max_gy as f64) * ts);
    }
    // Horizontal lines
    for gy in min_gy..=max_gy {
        let y = (gy as f64) * ts;
        ctx.move_to((min_gx as f64) * ts, y);
        ctx.line_to((max_gx as f64) * ts, y);
    }
    ctx.stroke();
    Ok(())
}

fn draw_overlays(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    ts: f64,
    preview_path: &[(u32, u32)],
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // Highlight player tile
    if let Some(player) = game.player_unit() {
        let dx = (player.grid_x as f64) * ts + 1.0;
        let dy = (player.grid_y as f64) * ts + 1.0;
        let size = ts - 2.0;
        ctx.set_fill_style_str("rgba(255,255,51,0.3)");
        ctx.fill_rect(dx, dy, size, size);
    }

    // Draw auto-move path preview
    if game.auto_path_idx < game.auto_path.len() {
        for i in game.auto_path_idx..game.auto_path.len() {
            let (px, py) = game.auto_path[i];
            let dx = (px as f64) * ts + 2.0;
            let dy = (py as f64) * ts + 2.0;
            let size = ts - 4.0;
            let alpha = if i == game.auto_path.len() - 1 {
                0.5 // destination brighter
            } else {
                0.25
            };
            ctx.set_fill_style_str(&format!("rgba(100,149,237,{})", alpha));
            ctx.fill_rect(dx, dy, size, size);
        }
    }

    // Live swipe preview path (during gesture)
    if !preview_path.is_empty() {
        for (i, &(px, py)) in preview_path.iter().enumerate() {
            let dx = (px as f64) * ts + 2.0;
            let dy = (py as f64) * ts + 2.0;
            let size = ts - 4.0;
            let alpha = if i == preview_path.len() - 1 {
                0.45
            } else {
                0.2
            };
            ctx.set_fill_style_str(&format!("rgba(255,255,255,{})", alpha));
            ctx.fill_rect(dx, dy, size, size);
        }
    }

    // HP bars for alive units (use visual_alive during animation)
    for unit in &game.units {
        let show = if animator.is_playing() {
            animator.is_visually_alive(unit.id)
        } else {
            unit.alive
        };
        if !show {
            continue;
        }
        let (wx, wy) = (unit.visual_x, unit.visual_y);
        let bar_width = 48.0_f64;
        let bar_height = 6.0_f64;
        let bar_y = (wy as f64) - (TILE_SIZE as f64) * 0.45;
        let bar_x = (wx as f64) - bar_width / 2.0;

        ctx.set_global_alpha(0.8);
        ctx.set_fill_style_str("rgb(51,51,51)");
        ctx.fill_rect(bar_x, bar_y - bar_height / 2.0, bar_width, bar_height);

        let hp_ratio = unit.hp as f64 / unit.stats.max_hp as f64;
        let fill_width = bar_width * hp_ratio;
        let fill_color = if hp_ratio > 0.5 {
            "rgb(51,204,51)"
        } else if hp_ratio > 0.25 {
            "rgb(230,179,26)"
        } else {
            "rgb(230,51,26)"
        };
        ctx.set_global_alpha(0.9);
        ctx.set_fill_style_str(fill_color);
        ctx.fill_rect(
            bar_x,
            bar_y - (bar_height - 2.0) / 2.0,
            fill_width,
            bar_height - 2.0,
        );

        ctx.set_global_alpha(1.0);
    }

    Ok(())
}

fn draw_foreground(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // Unit sprites (sorted by Y for proper layering)
    let mut unit_indices: Vec<usize> = game
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| {
            if animator.is_playing() {
                animator.is_visually_alive(u.id) || u.death_fade > 0.0
            } else {
                u.alive || u.death_fade > 0.0
            }
        })
        .map(|(i, _)| i)
        .collect();
    unit_indices.sort_by(|&a, &b| {
        game.units[a]
            .visual_y
            .partial_cmp(&game.units[b].visual_y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                game.units[a]
                    .visual_x
                    .partial_cmp(&game.units[b].visual_x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });

    for &idx in &unit_indices {
        let unit = &game.units[idx];
        let key = UnitTextureKey {
            faction: unit.faction,
            kind: unit.kind,
            anim: unit.current_anim,
        };

        let tex_id = match loaded.unit_textures.get(&key) {
            Some(&id) => id,
            None => {
                let fallback_key = UnitTextureKey {
                    faction: unit.faction,
                    kind: unit.kind,
                    anim: UnitAnim::Idle,
                };
                match loaded.unit_textures.get(&fallback_key) {
                    Some(&id) => id,
                    None => continue,
                }
            }
        };

        if let Some((img, frame_w, frame_h, _)) = tm.get_image(tex_id) {
            let sheet = SpriteSheet {
                frame_width: frame_w,
                frame_height: frame_h,
                frame_count: unit.animation.frame_count,
            };
            let (sx, sy, sw, sh) = sheet.frame_src_rect(unit.animation.current_frame);
            let (wx, wy) = (unit.visual_x, unit.visual_y);
            let sprite_size = unit.kind.frame_size() as f64;

            let opacity = if unit.alive {
                1.0
            } else {
                (unit.death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0) as f64
            };

            let dx = (wx as f64) - sprite_size / 2.0;
            let dy = (wy as f64) - sprite_size / 2.0;

            draw_sprite(
                ctx,
                img,
                sx,
                sy,
                sw,
                sh,
                dx,
                dy,
                sprite_size,
                sprite_size,
                unit.facing == Facing::Left,
                opacity,
            )?;
        }
    }

    // Particle sprites
    for particle in &game.particles {
        let filename = particle.kind.asset_filename();
        let tex_id = match loaded.particle_textures.get(filename) {
            Some(&id) => id,
            None => continue,
        };

        if let Some((img, frame_w, frame_h, _)) = tm.get_image(tex_id) {
            let sheet = SpriteSheet {
                frame_width: frame_w,
                frame_height: frame_h,
                frame_count: particle.animation.frame_count,
            };
            let (sx, sy, sw, sh) = sheet.frame_src_rect(particle.animation.current_frame);
            let size = particle.kind.frame_size() as f64;
            let dx = (particle.world_x as f64) - size / 2.0;
            let dy = (particle.world_y as f64) - size / 2.0;

            draw_sprite(ctx, img, sx, sy, sw, sh, dx, dy, size, size, false, 1.0)?;
        }
    }

    // Arrow projectiles
    if let Some(&arrow_tex_id) = loaded.arrow_texture.as_ref() {
        if let Some((img, _, _, _)) = tm.get_image(arrow_tex_id) {
            for proj in &game.projectiles {
                let flip = proj.angle.abs() > std::f32::consts::FRAC_PI_2;
                let draw_angle = if flip {
                    (proj.angle as f64) + std::f64::consts::PI
                } else {
                    proj.angle as f64
                };

                ctx.save();
                ctx.translate(proj.current_x as f64, proj.current_y as f64)?;
                ctx.rotate(draw_angle)?;
                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img, 0.0, 0.0, 64.0, 64.0, -32.0, -32.0, 64.0, 64.0,
                )?;
                ctx.restore();
            }
        }
    }

    Ok(())
}

fn draw_trees(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    _elapsed: f64,
) -> Result<(), JsValue> {
    if loaded.tree_textures.is_empty() {
        return Ok(());
    }

    let ts = TILE_SIZE as f64;

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Forest {
                continue;
            }

            // Only draw on every other tile to avoid overlap
            if (gx + gy) % 2 != 0 {
                continue;
            }

            let variant_idx =
                (gx.wrapping_mul(31).wrapping_add(gy.wrapping_mul(17))) as usize
                    % loaded.tree_textures.len();
            let (tex_id, frame_w, frame_h) = loaded.tree_textures[variant_idx];

            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let fw = frame_w as f64;
                let fh = frame_h as f64;

                // Static: always use frame 0, draw at 2-tile width scaled proportionally
                let draw_w = ts * 2.0;
                let draw_h = draw_w * (fh / fw);
                let dx = (gx as f64) * ts + ts / 2.0 - draw_w / 2.0;
                let dy = (gy as f64) * ts + ts - draw_h;

                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img, 0.0, 0.0, fw, fh, dx, dy, draw_w, draw_h,
                )?;
            }
        }
    }

    Ok(())
}

/// Update the offscreen fog canvas (1px per tile) using direct pixel manipulation.
/// Only called when game.fog_dirty is true (i.e. after player moves).
fn update_fog_canvas(
    fog_ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
) -> Result<(), JsValue> {
    let w = game.grid.width;
    let h = game.grid.height;
    let len = (w * h * 4) as usize;
    let mut pixels = vec![0u8; len];

    for gy in 0..h {
        for gx in 0..w {
            let idx = (gy * w + gx) as usize;
            let po = idx * 4; // pixel offset (RGBA)

            let alpha = if game.visible[idx] {
                // Visible tile — add soft edge if near fog
                let fog_n = 8 - visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                if fog_n >= 3 {
                    ((fog_n as u32 - 2) * 10).min(255) as u8
                } else {
                    0
                }
            } else if game.revealed[idx] {
                // Previously seen — dim fog, softer near visible tiles
                let vis_n = visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                let base = 140i32; // ~0.55 * 255
                (base - (vis_n as i32) * 15).max(38) as u8
            } else {
                // Never seen — full darkness
                let vis_n = visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                let base = 235i32; // ~0.92 * 255
                (base - (vis_n as i32) * 20).max(102) as u8
            };

            // Black with computed alpha
            pixels[po] = 0;
            pixels[po + 1] = 0;
            pixels[po + 2] = 0;
            pixels[po + 3] = alpha;
        }
    }

    let clamped = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h)?;
    fog_ctx.put_image_data(&image_data, 0.0, 0.0)?;

    Ok(())
}

/// Count visible neighbors (8-directional) using direct array access. No allocations.
fn visible_neighbor_count_fast(visible: &[bool], gx: u32, gy: u32, w: u32, h: u32) -> u32 {
    let mut count = 0u32;
    let x = gx as i32;
    let y = gy as i32;
    let wi = w as i32;
    let hi = h as i32;
    for &(ndx, ndy) in &[
        (-1, -1), (0, -1), (1, -1),
        (-1, 0),           (1, 0),
        (-1, 1),  (0, 1),  (1, 1),
    ] {
        let nx = x + ndx;
        let ny = y + ndy;
        if nx >= 0 && ny >= 0 && nx < wi && ny < hi {
            if visible[(ny as u32 * w + nx as u32) as usize] {
                count += 1;
            }
        }
    }
    count
}

fn request_animation_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}

struct LoopState {
    canvas2d: Canvas2d,
    game: Game,
    texture_manager: TextureManager,
    last_time: Option<f64>,
    elapsed: f64,
    fog_canvas: web_sys::HtmlCanvasElement,
    fog_ctx: web_sys::CanvasRenderingContext2d,
    animator: TurnAnimator,
}
