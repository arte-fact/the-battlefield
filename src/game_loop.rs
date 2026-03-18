use crate::animation::TurnAnimator;
use crate::autotile;
use crate::building::BuildingKind;
use crate::game::{Game, ATTACK_CONE_HALF_ANGLE};
use crate::grid::{self, Decoration, TileKind, GRID_SIZE, TILE_SIZE};
use crate::input::Input;
use crate::particle::Particle;
use crate::renderer::{draw_sprite, load_image, Canvas2d, TextureId, TextureManager};
use crate::sprite::SpriteSheet;
use crate::unit::{Facing, Faction, UnitAnim, UnitKind, DEATH_FADE_DURATION};
use crate::zone::ZoneState;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// Deterministic pseudo-random flip based on grid position.
/// Returns true for ~50% of tiles in a spatially uniform pattern.
fn tile_flip(gx: u32, gy: u32) -> bool {
    gx.wrapping_mul(48271).wrapping_add(gy.wrapping_mul(16807)) & 1 == 0
}

/// Draw a tile-sized image horizontally flipped.
fn draw_tile_flipped(
    ctx: &web_sys::CanvasRenderingContext2d,
    img: &web_sys::HtmlImageElement,
    sx: f64, sy: f64, sw: f64, sh: f64,
    dx: f64, dy: f64, dw: f64, dh: f64,
) -> Result<(), JsValue> {
    ctx.save();
    ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
    ctx.scale(-1.0, 1.0)?;
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        img, sx, sy, sw, sh, -dw / 2.0, -dh / 2.0, dw, dh,
    )?;
    ctx.restore();
    Ok(())
}

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
    /// Rock decoration textures: single 64x64 sprites for 4 variants
    rock_textures: Vec<TextureId>,
    /// Pre-flipped tree canvases: (canvas, frame_w, frame_h) for each variant
    tree_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Pre-flipped rock canvases: one per variant
    rock_textures_flipped: Vec<web_sys::HtmlCanvasElement>,
    /// Bush sprite sheets: (texture_id, frame_w, frame_h) for 4 variants (128x128 frames)
    bush_textures: Vec<(TextureId, u32, u32)>,
    /// Pre-flipped bush canvases: (canvas, frame_w, frame_h) for each variant
    bush_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Water rock sprite sheets: (texture_id, frame_w, frame_h) for 4 variants (64x64 frames)
    water_rock_textures: Vec<(TextureId, u32, u32)>,
    /// Pre-flipped water rock canvases: (canvas, frame_w, frame_h) for each variant
    water_rock_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Tower building textures: index 0=Black(neutral), 1=Blue, 2=Red (128x256 each)
    tower_textures: Vec<TextureId>,
    /// Base building textures: indexed by kind_index * 2 + faction_index
    /// kind_index: 0=Barracks, 1=Archery, 2=Monastery
    /// faction_index: 0=Blue, 1=Red
    /// Each entry: (texture_id, sprite_width, sprite_height)
    building_textures: Vec<(TextureId, u32, u32)>,
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
            rock_textures: Vec::new(),
            tree_textures_flipped: Vec::new(),
            rock_textures_flipped: Vec::new(),
            bush_textures: Vec::new(),
            bush_textures_flipped: Vec::new(),
            water_rock_textures: Vec::new(),
            water_rock_textures_flipped: Vec::new(),
            tower_textures: Vec::new(),
            building_textures: Vec::new(),
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

    // Set initial touch control layout based on canvas size and DPR
    let dpr = canvas.width() as f32 / canvas.client_width().max(1) as f32;
    {
        let mut inp = input.borrow_mut();
        inp.update_layout(canvas.width() as f32, canvas.height() as f32, dpr);
    }

    // Create offscreen fog canvas (1 pixel per tile, bilinear-interpolated when drawn)
    let fog_canvas = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    fog_canvas.set_width(game.grid.width);
    fog_canvas.set_height(game.grid.height);
    let fog_ctx = fog_canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;

    // Create offscreen terrain cache canvas (full grid pre-rendered once)
    let terrain_size = game.grid.width * (TILE_SIZE as u32);
    let terrain_canvas = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    terrain_canvas.set_width(terrain_size);
    terrain_canvas.set_height(terrain_size);
    let terrain_ctx = terrain_canvas
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
        terrain_canvas,
        terrain_ctx,
        terrain_dirty: true,
        animator: TurnAnimator::new(),
    }));

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

            // Process input and real-time game logic
            {
                let game = &mut state_guard.game;
                let mut inp = input.borrow_mut();

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

                // Clamp camera to world bounds after pan/zoom
                let world_size = GRID_SIZE as f32 * TILE_SIZE;
                game.camera.clamp_to_world(world_size, world_size);

                // Only run game logic if no winner yet
                if game.winner.is_none() {
                    // Snapshot positions for movement animations
                    let old_positions: Vec<(f32, f32)> = game.units.iter().map(|u| (u.x, u.y)).collect();

                    // Tick cooldowns for all units
                    game.tick_cooldowns(dt as f32);

                    // AI acts independently each frame
                    game.tick_ai(dt as f32);

                    // Capture zone progression and base production
                    game.tick_zones(dt as f32);
                    game.tick_production(dt as f32);

                    // Attack held: lock aim direction and facing
                    let attack_held = inp.is_key_down(" ") || inp.attack_button.pressed;

                    // Keyboard movement (WASD/ZQSD + arrows)
                    let (move_dx, move_dy) = inp.movement_direction();
                    if move_dx != 0.0 || move_dy != 0.0 {
                        if !attack_held {
                            game.player_aim_dir = move_dy.atan2(move_dx);
                        }
                        game.try_player_move(move_dx, move_dy, dt as f32);
                    }

                    // Virtual joystick movement (mobile)
                    let (joy_dx, joy_dy) = (inp.joystick.dx, inp.joystick.dy);
                    if joy_dx.abs() > 0.01 || joy_dy.abs() > 0.01 {
                        if !attack_held {
                            game.player_aim_dir = joy_dy.atan2(joy_dx);
                        }
                        game.try_player_move(joy_dx, joy_dy, dt as f32);
                    }

                    // Update player facing from aim direction (skip when attacking)
                    if !attack_held {
                        let aim_cos = game.player_aim_dir.cos();
                        if let Some(player) = game.player_unit_mut() {
                            if aim_cos > 0.01 {
                                player.facing = Facing::Right;
                            } else if aim_cos < -0.01 {
                                player.facing = Facing::Left;
                            }
                        }
                    }

                    // Attack: keyboard (space held or pressed) or touch button
                    let attack_input = inp.take_attack_key()
                        || inp.take_attack_pressed()
                        || attack_held;
                    if attack_input {
                        game.player_attack();
                    }

                    // Resolve circle-circle collisions
                    game.resolve_collisions();

                    // Update run/idle animations based on movement
                    game.update_movement_anims(&old_positions);
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

            // Advance all animations in parallel and collect spawned effects
            {
                let LoopState { ref mut animator, ref mut game, .. } = *state_guard;
                if animator.is_playing() {
                    let anim_output = animator.update(dt as f32, &mut game.units);
                    for (kind, x, y) in anim_output.particles {
                        game.particles.push(Particle::new(kind, x, y));
                    }
                }
            }

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
                let inp = input.borrow();
                let mut state_guard = state.borrow_mut();
                if let Err(e) = render_frame(&mut state_guard, &loaded, &inp) {
                    log::error!("Render error: {:?}", e);
                }
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
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
                let canvas_w = canvas_clone.width() as f32;
                input_clone
                    .borrow_mut()
                    .on_touch_start(t.identifier(), cx, cy, count, canvas_w);
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
            let has_control = input_clone.borrow().has_active_control();

            if touches.length() >= 2 && !has_control {
                // Pure camera gesture (no controls active)
                let t0 = touches.get(0).unwrap();
                let t1 = touches.get(1).unwrap();
                let (x0, y0) = canvas_touch_coords(&canvas_clone, &t0);
                let (x1, y1) = canvas_touch_coords(&canvas_clone, &t1);
                input_clone
                    .borrow_mut()
                    .on_touch_move_two_finger(x0, y0, x1, y1);
            } else {
                // Update each touch individually (joystick filters by touch_id)
                for i in 0..touches.length() {
                    let t = touches.get(i).unwrap();
                    let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                    input_clone.borrow_mut().on_touch_move_single(t.identifier(), cx, cy);
                }
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
                    (UnitKind::Monk, UnitAnim::Idle) => ("Idle.png", 6),
                    (UnitKind::Monk, UnitAnim::Run) => ("Run.png", 4),
                    (UnitKind::Monk, UnitAnim::Attack) => ("Heal.png", 11),
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

    // Load rock decoration sprites (4 variants, 64x64 each)
    for i in 1..=4 {
        let url = format!("{}/Terrain/Decorations/Rocks/Rock{}.png", ASSET_BASE, i);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().rock_textures.push(tex_id);
    }

    // Load bush sprites (4 variants, 1024x128, 8 frames of 128x128)
    for i in 1..=4 {
        let url = format!("{}/Terrain/Decorations/Bushes/Bushe{}.png", ASSET_BASE, i);
        let tex_id = load_texture(state, &url, 128, 128, 8).await?;
        loaded.borrow_mut().bush_textures.push((tex_id, 128, 128));
    }

    // Load water rock sprites (4 variants, 1024x64, 16 frames of 64x64)
    for i in 1..=4 {
        let url = format!(
            "{}/Terrain/Decorations/Rocks in the Water/Water Rocks_0{}.png",
            ASSET_BASE, i
        );
        let tex_id = load_texture(state, &url, 64, 64, 16).await?;
        loaded
            .borrow_mut()
            .water_rock_textures
            .push((tex_id, 64, 64));
    }

    // Load tower building sprites (3 color variants: Black=neutral, Blue, Red)
    {
        let tower_colors = ["Black Buildings", "Blue Buildings", "Red Buildings"];
        for color_folder in &tower_colors {
            let url = format!("{}/Buildings/{}/Tower.png", ASSET_BASE, color_folder);
            let tex_id = load_texture(state, &url, 128, 256, 1).await?;
            loaded.borrow_mut().tower_textures.push(tex_id);
        }
    }

    // Load base building sprites (3 kinds × 2 factions)
    // Index: kind_index * 2 + faction_index
    {
        let kinds = [
            (BuildingKind::Barracks, 192u32, 256u32),
            (BuildingKind::Archery, 192, 256),
            (BuildingKind::Monastery, 192, 320),
        ];
        let faction_folders = ["Blue Buildings", "Red Buildings"];
        for &(kind, sw, sh) in &kinds {
            for faction_folder in &faction_folders {
                let url = format!(
                    "{}/Buildings/{}/{}",
                    ASSET_BASE, faction_folder, kind.asset_filename()
                );
                let tex_id = load_texture(state, &url, sw, sh, 1).await?;
                loaded.borrow_mut().building_textures.push((tex_id, sw, sh));
            }
        }
    }

    // Pre-flip tree and rock textures at load time (eliminates per-frame save/translate/scale/restore)
    {
        let document = web_sys::window().unwrap().document().unwrap();
        let guard = state.borrow();
        let tm = &guard.texture_manager;
        let loaded_ref = loaded.borrow();

        let mut tree_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.tree_textures {
            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(fw);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(fw as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, fw as f64, fh as f64, 0.0, 0.0, fw as f64, fh as f64,
                    );
                tree_flipped.push((c, fw, fh));
            }
        }

        let mut rock_flipped = Vec::new();
        for &tex_id in &loaded_ref.rock_textures {
            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(64);
                c.set_height(64);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(64.0, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, 64.0, 64.0, 0.0, 0.0, 64.0, 64.0,
                    );
                rock_flipped.push(c);
            }
        }

        let mut bush_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.bush_textures {
            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(fw);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(fw as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, fw as f64, fh as f64, 0.0, 0.0, fw as f64, fh as f64,
                    );
                bush_flipped.push((c, fw, fh));
            }
        }

        let mut water_rock_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.water_rock_textures {
            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(fw);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(fw as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, fw as f64, fh as f64, 0.0, 0.0, fw as f64, fh as f64,
                    );
                water_rock_flipped.push((c, fw, fh));
            }
        }

        drop(loaded_ref);
        drop(guard);

        let mut loaded_mut = loaded.borrow_mut();
        loaded_mut.tree_textures_flipped = tree_flipped;
        loaded_mut.rock_textures_flipped = rock_flipped;
        loaded_mut.bush_textures_flipped = bush_flipped;
        loaded_mut.water_rock_textures_flipped = water_rock_flipped;
    }

    Ok(())
}

fn render_frame(state: &mut LoopState, loaded: &LoadedTextures, input: &Input) -> Result<(), JsValue> {
    // Update fog offscreen canvas if FOV changed
    if state.game.fog_dirty {
        update_fog_canvas(&state.fog_ctx, &state.game)?;
        state.game.fog_dirty = false;
    }

    // Render terrain cache once (all static layers pre-rendered to offscreen canvas)
    if state.terrain_dirty {
        render_terrain_cache(
            &state.terrain_ctx,
            &state.game,
            loaded,
            &state.texture_manager,
        )?;
        state.terrain_dirty = false;
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
    // Snap the screen-space offset to integer pixels so all tile edges align perfectly.
    // screen_x = zoom * (world_x - cam) + viewport/2, so snapping
    // (viewport/2 - zoom * cam) to integer ensures every tile corner is pixel-exact
    // (since zoom is snapped to 1/64 steps, zoom * 64 is always integer).
    let zoom = game.camera.zoom as f64;
    let offset_x = (canvas_w / 2.0 - zoom * (game.camera.x as f64)).round();
    let offset_y = (canvas_h / 2.0 - zoom * (game.camera.y as f64)).round();
    ctx.save();
    ctx.translate(offset_x, offset_y)?;
    ctx.scale(zoom, zoom)?;

    // Visible tile range
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(game.grid.width as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(game.grid.height as i32) as u32;

    // 3. Water → foam → cached land/elevation (foam must layer between water and grass)
    draw_water(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy)?;
    draw_foam(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy, state.elapsed)?;
    ctx.draw_image_with_html_canvas_element(&state.terrain_canvas, 0.0, 0.0)?;

    // 4. Capture zone overlays (colored fill, dashed border, labels, progress bars)
    draw_zone_overlays(ctx, game, min_gx, min_gy, max_gx, max_gy)?;

    // 5. Draw overlays (player highlight, HP bars, path line, attack target)
    draw_overlays(ctx, game, min_gx, min_gy, max_gx, max_gy, ts, &state.animator)?;

    // 6. Draw foreground sprites (units, particles, projectiles, trees) — Y-sorted together
    draw_foreground(ctx, game, loaded, tm, &state.animator, min_gx, min_gy, max_gx, max_gy)?;

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

    // 9. Fill solid black outside the grid to hide background when zoomed out.
    // Use a large margin so it covers any visible area beyond the grid.
    let margin = grid_world_size;
    ctx.set_fill_style_str("#000");
    ctx.fill_rect(-margin, -margin, margin, grid_world_size + 2.0 * margin); // left
    ctx.fill_rect(grid_world_size, -margin, margin, grid_world_size + 2.0 * margin); // right
    ctx.fill_rect(0.0, -margin, grid_world_size, margin); // top
    ctx.fill_rect(0.0, grid_world_size, grid_world_size, margin); // bottom

    ctx.restore();

    // Draw zone HUD pips in screen space (top-right corner)
    draw_zone_hud(ctx, game, canvas_w, state.canvas2d.dpr)?;

    // Draw victory progress bar (when a faction holds all zones)
    draw_victory_progress(ctx, game, canvas_w, canvas_h, state.canvas2d.dpr)?;

    // Draw victory overlay (when a faction has won)
    draw_victory_overlay(ctx, game, canvas_w, canvas_h, state.canvas2d.dpr)?;

    // Draw touch controls in screen space (after camera transform is restored)
    draw_touch_controls(ctx, input, state.canvas2d.dpr)?;

    Ok(())
}

/// Render all static terrain layers to the offscreen terrain cache canvas.
/// Called once after textures load; the result is blitted each frame.
fn render_terrain_cache(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;
    let w = game.grid.width;
    let h = game.grid.height;

    // Water is NOT cached here — it's drawn per-frame so foam can layer between water and land.
    // This canvas stays transparent where there's no land, letting water+foam show through.

    // Layer 3: Flat ground (auto-tiled, flips amortized since drawn once)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in 0..h {
                for gx in 0..w {
                    if !game.grid.get(gx, gy).is_land() {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts;
                    let dy = (gy as f64) * ts;
                    if col == 1 && row == 1 && tile_flip(gx, gy) {
                        draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts, ts,
                        )?;
                    }
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
                for gy in 0..h {
                    for gx in 0..w {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        if gy + 1 < h && game.grid.elevation(gx, gy + 1) < level {
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
                for gy in 0..h {
                    for gx in 0..w {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        let (col, row) =
                            autotile::elevated_top_src(&game.grid, gx, gy, level);
                        let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                        let dx = (gx as f64) * ts;
                        let dy = (gy as f64) * ts;
                        if col == 6 && row == 1 && tile_flip(gx, gy) {
                            draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                        } else {
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, sx, sy, sw, sh, dx, dy, ts, ts,
                            )?;
                        }

                        if let Some((ccol, crow)) =
                            autotile::cliff_src(&game.grid, gx, gy, level)
                        {
                            let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
                            let cdy = ((gy + 1) as f64) * ts;
                            if tile_flip(gx, gy.wrapping_add(1000)) {
                                draw_tile_flipped(ctx, img, csx, csy, csw, csh, dx, cdy, ts, ts)?;
                            } else {
                                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                    img, csx, csy, csw, csh, dx, cdy, ts, ts,
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }

    // Layer 5: Bush decorations (rendered under units, on terrain)
    if !loaded.bush_textures.is_empty() {
        for gy in 0..h {
            for gx in 0..w {
                if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                    continue;
                }
                let variant_idx =
                    (gx.wrapping_mul(41).wrapping_add(gy.wrapping_mul(23))) as usize
                        % loaded.bush_textures.len();
                let (tex_id, frame_w, frame_h) = loaded.bush_textures[variant_idx];

                if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                    let fw = frame_w as f64;
                    let fh = frame_h as f64;
                    let dx = (gx as f64) * ts;
                    let dy = (gy as f64) * ts;

                    if tile_flip(gx, gy) {
                        if let Some((flipped, _, _)) = loaded.bush_textures_flipped.get(variant_idx) {
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, 0.0, 0.0, fw, fh, dx, dy, ts, ts,
                            )?;
                        }
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, fw, fh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Draw capture zone overlays in world space (fill, dashed border, label, progress bar).
fn draw_zone_overlays(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    for zone in &game.zone_manager.zones {
        // Skip zones entirely outside the visible range (bounding box of circle)
        let zone_min_gx = zone.center_gx.saturating_sub(zone.radius);
        let zone_min_gy = zone.center_gy.saturating_sub(zone.radius);
        let zone_max_gx = zone.center_gx + zone.radius + 1;
        let zone_max_gy = zone.center_gy + zone.radius + 1;

        if zone_max_gx < min_gx || zone_min_gx > max_gx
            || zone_max_gy < min_gy || zone_min_gy > max_gy
        {
            continue;
        }

        let cx = zone.center_wx as f64;
        let cy = zone.center_wy as f64;
        let r = zone.radius_world as f64;

        // Fill + border color by state
        let (fill_color, border_color) = match zone.state {
            ZoneState::Neutral => ("rgba(200,200,200,0.06)", "rgba(200,200,200,0.25)"),
            ZoneState::Contested => ("rgba(255,200,0,0.08)", "rgba(255,200,0,0.4)"),
            ZoneState::Capturing(Faction::Blue) => ("rgba(60,120,255,0.08)", "rgba(60,120,255,0.4)"),
            ZoneState::Capturing(Faction::Red) => ("rgba(255,60,60,0.08)", "rgba(255,60,60,0.4)"),
            ZoneState::Controlled(Faction::Blue) => ("rgba(60,120,255,0.12)", "rgba(60,120,255,0.5)"),
            ZoneState::Controlled(Faction::Red) => ("rgba(255,60,60,0.12)", "rgba(255,60,60,0.5)"),
            _ => ("rgba(200,200,200,0.06)", "rgba(200,200,200,0.25)"),
        };

        // Semi-transparent circular fill
        ctx.set_fill_style_str(fill_color);
        ctx.begin_path();
        ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU)?;
        ctx.fill();

        // Dashed circular border
        ctx.set_stroke_style_str(border_color);
        ctx.set_line_width(2.0);
        let dash = js_sys::Array::new();
        dash.push(&JsValue::from(8.0));
        dash.push(&JsValue::from(4.0));
        ctx.set_line_dash(&dash)?;
        ctx.begin_path();
        ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU)?;
        ctx.stroke();
        ctx.set_line_dash(&js_sys::Array::new())?;

        // Zone name label (above circle)
        let label_y = cy - r - 14.0;
        ctx.set_font("bold 11px monospace");
        ctx.set_text_align("center");
        ctx.set_text_baseline("bottom");
        ctx.set_fill_style_str("rgba(255,255,255,0.7)");
        ctx.fill_text(zone.name, cx, label_y)?;

        // Progress bar (just below the label, above circle)
        let bar_w = r;
        let bar_h = 4.0;
        let bar_x = cx - bar_w / 2.0;
        let bar_y = cy - r - 6.0;

        // Bar background
        ctx.set_fill_style_str("rgba(0,0,0,0.4)");
        ctx.fill_rect(bar_x, bar_y, bar_w, bar_h);

        // Blue fills right from center, Red fills left from center
        let progress = zone.progress as f64;
        if progress > 0.01 {
            ctx.set_fill_style_str("rgba(60,120,255,0.85)");
            let fill_w = bar_w * 0.5 * progress;
            ctx.fill_rect(bar_x + bar_w * 0.5, bar_y, fill_w, bar_h);
        } else if progress < -0.01 {
            ctx.set_fill_style_str("rgba(255,60,60,0.85)");
            let fill_w = bar_w * 0.5 * (-progress);
            ctx.fill_rect(bar_x + bar_w * 0.5 - fill_w, bar_y, fill_w, bar_h);
        }

        // Center divider tick
        ctx.set_fill_style_str("rgba(255,255,255,0.5)");
        ctx.fill_rect(bar_x + bar_w * 0.5 - 0.5, bar_y - 1.0, 1.0, bar_h + 2.0);
    }

    Ok(())
}

/// Draw capture zone HUD pips in screen space (top-right corner).
fn draw_zone_hud(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    canvas_w: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    let zones = &game.zone_manager.zones;
    if zones.is_empty() {
        return Ok(());
    }

    let pip_size = 14.0 * dpr;
    let gap = 3.0 * dpr;
    let margin = 10.0 * dpr;
    let total_w = zones.len() as f64 * (pip_size + gap) - gap;
    let start_x = canvas_w - margin - total_w;
    let y = margin;

    for (i, zone) in zones.iter().enumerate() {
        let x = start_x + i as f64 * (pip_size + gap);

        // Background pip
        ctx.set_fill_style_str("rgba(0,0,0,0.5)");
        ctx.fill_rect(x, y, pip_size, pip_size);

        // Color and fill ratio by state
        let (color, fill_ratio) = match zone.state {
            ZoneState::Neutral => ("rgba(150,150,150,0.5)", 0.0),
            ZoneState::Contested => ("rgba(255,200,0,0.7)", zone.progress.abs() as f64),
            ZoneState::Capturing(Faction::Blue) | ZoneState::Controlled(Faction::Blue) => {
                ("rgba(60,120,255,0.8)", zone.progress.abs() as f64)
            }
            ZoneState::Capturing(Faction::Red) | ZoneState::Controlled(Faction::Red) => {
                ("rgba(255,60,60,0.8)", zone.progress.abs() as f64)
            }
            _ => ("rgba(150,150,150,0.5)", 0.0),
        };

        if fill_ratio > 0.01 {
            ctx.set_fill_style_str(color);
            let fill_h = pip_size * fill_ratio;
            ctx.fill_rect(x, y + pip_size - fill_h, pip_size, fill_h);
        }

        // Border
        ctx.set_stroke_style_str("rgba(255,255,255,0.35)");
        ctx.set_line_width(1.0);
        ctx.stroke_rect(x, y, pip_size, pip_size);
    }

    ctx.set_global_alpha(1.0);
    Ok(())
}

/// Draw water background tiles (visible range only, per-frame).
fn draw_water(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    if let Some(water_tex_id) = loaded.water_texture {
        if let Some((img, _, _, _)) = tm.get_image(water_tex_id) {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    // Draw water on water tiles AND on land tiles adjacent to water
                    // (foam sprites are 192x192 centered on land, so they need water behind them)
                    let is_water = !game.grid.get(gx, gy).is_land();
                    let has_foam = game
                        .water_adjacency
                        .get((gy * game.grid.width + gx) as usize)
                        .copied()
                        .unwrap_or(false);
                    if !is_water && !has_foam {
                        continue;
                    }
                    let dx = (gx as f64) * ts;
                    let dy = (gy as f64) * ts;
                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts,
                    )?;
                }
            }
        }
    }

    Ok(())
}

/// Draw animated water foam (the only per-frame terrain layer).
fn draw_foam(
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

    Ok(())
}

fn draw_overlays(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    _min_gx: u32,
    _min_gy: u32,
    _max_gx: u32,
    _max_gy: u32,
    _ts: f64,
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // Player position indicator (circle under player)
    if let Some(player) = game.player_unit() {
        ctx.set_fill_style_str("rgba(255,255,51,0.2)");
        ctx.begin_path();
        ctx.arc(player.x as f64, player.y as f64, 24.0, 0.0, std::f64::consts::TAU)?;
        ctx.fill();

        // Aim direction indicator (wedge showing attack cone)
        let aim = game.player_aim_dir as f64;
        let half = ATTACK_CONE_HALF_ANGLE as f64;
        let radius = 40.0_f64;
        let px = player.x as f64;
        let py = player.y as f64;

        ctx.set_fill_style_str("rgba(255,255,100,0.12)");
        ctx.begin_path();
        ctx.move_to(px, py);
        ctx.arc(px, py, radius, aim - half, aim + half)?;
        ctx.close_path();
        ctx.fill();

        ctx.set_stroke_style_str("rgba(255,255,100,0.35)");
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.move_to(px, py);
        ctx.arc(px, py, radius, aim - half, aim + half)?;
        ctx.close_path();
        ctx.stroke();
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
        let (wx, wy) = (unit.x, unit.y);
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

/// Draw a progress bar when a faction is holding all zones toward victory.
fn draw_victory_progress(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    canvas_w: f64,
    _canvas_h: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    let progress = game.zone_manager.victory_progress();
    if progress < f32::EPSILON || game.winner.is_some() {
        return Ok(());
    }

    let faction = match game.zone_manager.victory_candidate {
        Some(f) => f,
        None => return Ok(()),
    };

    let bar_w = 300.0 * dpr;
    let bar_h = 24.0 * dpr;
    let bar_x = (canvas_w - bar_w) / 2.0;
    let bar_y = 60.0 * dpr;
    let radius = 6.0 * dpr;

    // Background
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.6)");
    ctx.begin_path();
    ctx.round_rect_with_f64(bar_x, bar_y, bar_w, bar_h, radius)?;
    ctx.fill();

    // Fill
    let color = match faction {
        Faction::Blue => "rgba(70, 130, 230, 0.9)",
        _ => "rgba(220, 60, 60, 0.9)",
    };
    ctx.set_fill_style_str(color);
    let fill_w = bar_w * progress as f64;
    if fill_w > 0.5 {
        ctx.begin_path();
        ctx.round_rect_with_f64(bar_x, bar_y, fill_w, bar_h, radius)?;
        ctx.fill();
    }

    // Label
    let font_size = 14.0 * dpr;
    ctx.set_font(&format!("bold {font_size}px sans-serif"));
    ctx.set_fill_style_str("white");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    let remaining = ((1.0 - progress) * crate::zone::VICTORY_HOLD_TIME) as u32;
    let label = match faction {
        Faction::Blue => format!("Blue holds all zones - Victory in {remaining}s"),
        _ => format!("Red holds all zones - Victory in {remaining}s"),
    };
    ctx.fill_text(&label, canvas_w / 2.0, bar_y + bar_h / 2.0)?;

    Ok(())
}

/// Draw victory overlay when a faction has won.
fn draw_victory_overlay(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    let faction = match game.winner {
        Some(f) => f,
        None => return Ok(()),
    };

    // Semi-transparent overlay
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.5)");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    // Victory text
    let big_font = 48.0 * dpr;
    let small_font = 20.0 * dpr;
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    let (title, color) = match faction {
        Faction::Blue => ("BLUE VICTORY", "rgba(70, 150, 255, 1.0)"),
        _ => ("RED VICTORY", "rgba(255, 80, 80, 1.0)"),
    };

    // Shadow
    ctx.set_font(&format!("bold {big_font}px sans-serif"));
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.7)");
    ctx.fill_text(title, canvas_w / 2.0 + 2.0 * dpr, canvas_h / 2.0 + 2.0 * dpr)?;

    // Title
    ctx.set_fill_style_str(color);
    ctx.fill_text(title, canvas_w / 2.0, canvas_h / 2.0)?;

    // Subtitle
    ctx.set_font(&format!("{small_font}px sans-serif"));
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.8)");
    ctx.fill_text(
        "All capture zones held for 2 minutes",
        canvas_w / 2.0,
        canvas_h / 2.0 + big_font * 0.8,
    )?;

    Ok(())
}

/// Draw touch controls in screen space (virtual joystick + attack button).
fn draw_touch_controls(
    ctx: &web_sys::CanvasRenderingContext2d,
    input: &Input,
    dpr: f64,
) -> Result<(), JsValue> {
    if !input.is_touch_device {
        return Ok(());
    }

    // Virtual joystick (only when active)
    if input.joystick.active {
        // Base circle
        ctx.set_global_alpha(0.25);
        ctx.set_fill_style_str("rgba(255,255,255,0.3)");
        ctx.begin_path();
        ctx.arc(
            input.joystick.center_x as f64,
            input.joystick.center_y as f64,
            input.joystick.max_radius as f64,
            0.0,
            std::f64::consts::TAU,
        )?;
        ctx.fill();

        // Stick knob
        ctx.set_global_alpha(0.6);
        ctx.set_fill_style_str("rgba(255,255,255,0.6)");
        ctx.begin_path();
        ctx.arc(
            input.joystick.stick_x as f64,
            input.joystick.stick_y as f64,
            20.0 * dpr,
            0.0,
            std::f64::consts::TAU,
        )?;
        ctx.fill();
    }

    // Attack button (always visible on touch device)
    let btn = &input.attack_button;
    let alpha = if btn.pressed { 0.7 } else { 0.4 };
    ctx.set_global_alpha(alpha);
    ctx.set_fill_style_str("rgba(220,50,50,0.6)");
    ctx.begin_path();
    ctx.arc(
        btn.center_x as f64,
        btn.center_y as f64,
        btn.radius as f64,
        0.0,
        std::f64::consts::TAU,
    )?;
    ctx.fill();

    // Button label
    ctx.set_global_alpha(0.9);
    ctx.set_fill_style_str("white");
    ctx.set_font(&format!("bold {}px monospace", (14.0 * dpr) as u32));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.fill_text("ATK", btn.center_x as f64, btn.center_y as f64)?;

    ctx.set_global_alpha(1.0);
    Ok(())
}

/// A drawable entity for Y-sorted rendering.
enum Drawable {
    Unit(usize),            // index into game.units
    Tree(u32, u32),         // (gx, gy)
    Rock(u32, u32),         // (gx, gy)
    WaterRock(u32, u32),    // (gx, gy)
    Building(u8),           // zone index (tower)
    BaseBuilding(usize, usize), // (base_index, building_index)
    Particle(usize),        // index into game.particles
}

fn draw_foreground(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    animator: &TurnAnimator,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    // Player position for tree transparency
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));

    // Collect all drawable entities with their Y-sort key (foot position)
    let mut drawables: Vec<(f64, Drawable)> = Vec::new();

    // Units
    for (i, u) in game.units.iter().enumerate() {
        let visible = if animator.is_playing() {
            animator.is_visually_alive(u.id) || u.death_fade > 0.0
        } else {
            u.alive || u.death_fade > 0.0
        };
        if visible {
            drawables.push((u.y as f64, Drawable::Unit(i)));
        }
    }

    // Trees and rocks (visible range only)
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts;
            match tile {
                TileKind::Forest if !loaded.tree_textures.is_empty() => {
                    drawables.push((foot_y, Drawable::Tree(gx, gy)));
                }
                TileKind::Rock if !loaded.rock_textures.is_empty() => {
                    drawables.push((foot_y, Drawable::Rock(gx, gy)));
                }
                _ => {}
            }
            // Decorations (bushes rendered in terrain cache, not here)
            if game.grid.decoration(gx, gy) == Some(Decoration::WaterRock)
                && !loaded.water_rock_textures.is_empty()
            {
                drawables.push((foot_y, Drawable::WaterRock(gx, gy)));
            }
        }
    }

    // Tower buildings at zone centers
    for (i, zone) in game.zone_manager.zones.iter().enumerate() {
        let foot_y = (zone.center_gy as f64 + 1.0) * ts;
        drawables.push((foot_y, Drawable::Building(i as u8)));
    }

    // Base buildings
    for (bi, base) in game.bases.iter().enumerate() {
        for (bj, building) in base.buildings.iter().enumerate() {
            let foot_y = (building.grid_y as f64 + 1.0) * ts;
            drawables.push((foot_y, Drawable::BaseBuilding(bi, bj)));
        }
    }

    // Particles
    for (i, _) in game.particles.iter().enumerate() {
        drawables.push((game.particles[i].world_y as f64, Drawable::Particle(i)));
    }

    // Sort by Y (foot position), then by X for stability
    drawables.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Draw in Y order
    for (_, drawable) in &drawables {
        match drawable {
            Drawable::Unit(idx) => {
                let unit = &game.units[*idx];
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
                    let sprite_size = unit.kind.frame_size() as f64;

                    let opacity = if !unit.alive {
                        (unit.death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0) as f64
                    } else if unit.hit_flash > 0.0
                        && (unit.hit_flash * 30.0) as i32 % 2 == 0
                    {
                        0.3
                    } else {
                        1.0
                    };

                    let dx = (unit.x as f64) - sprite_size / 2.0;
                    let dy = (unit.y as f64) - sprite_size / 2.0;

                    draw_sprite(
                        ctx, img, sx, sy, sw, sh, dx, dy,
                        sprite_size, sprite_size,
                        unit.facing == Facing::Left, opacity,
                    )?;
                }
            }

            Drawable::Tree(gx, gy) => {
                let variant_idx =
                    (gx.wrapping_mul(31).wrapping_add(gy.wrapping_mul(17))) as usize
                        % loaded.tree_textures.len();
                let (tex_id, frame_w, frame_h) = loaded.tree_textures[variant_idx];

                if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                    let fw = frame_w as f64;
                    let fh = frame_h as f64;

                    let draw_w = ts * 3.0;
                    let draw_h = draw_w * (fh / fw);
                    let dx = (*gx as f64) * ts + ts / 2.0 - draw_w / 2.0;
                    let dy = (*gy as f64) * ts + ts - draw_h;

                    // Tree center in world coords
                    let tree_cx = (*gx as f64) * ts + ts / 2.0;
                    let tree_cy = (*gy as f64) * ts + ts / 2.0;

                    // Semi-transparent when near the player to avoid hiding them
                    let alpha = if let Some((px, py)) = player_pos {
                        let dist_x = (tree_cx - px).abs();
                        let dist_y = (tree_cy - py).abs();
                        let dist = (dist_x * dist_x + dist_y * dist_y).sqrt();
                        let fade_start = ts * 2.5; // start fading at 2.5 tiles
                        let fade_end = ts * 1.0;   // fully transparent at 1 tile
                        if dist < fade_end {
                            0.3
                        } else if dist < fade_start {
                            let t = (dist - fade_end) / (fade_start - fade_end);
                            0.3 + t * 0.7
                        } else {
                            1.0
                        }
                    } else {
                        1.0
                    };

                    if (alpha - 1.0).abs() > 0.001 {
                        ctx.set_global_alpha(alpha);
                    }

                    if tile_flip(*gx, *gy) {
                        if let Some((flipped, _, _)) = loaded.tree_textures_flipped.get(variant_idx) {
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, 0.0, 0.0, fw, fh, dx, dy, draw_w, draw_h,
                            )?;
                        }
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, fw, fh, dx, dy, draw_w, draw_h,
                        )?;
                    }

                    if (alpha - 1.0).abs() > 0.001 {
                        ctx.set_global_alpha(1.0);
                    }
                }
            }

            Drawable::Rock(gx, gy) => {
                let variant_idx =
                    (gx.wrapping_mul(13).wrapping_add(gy.wrapping_mul(29))) as usize
                        % loaded.rock_textures.len();
                let tex_id = loaded.rock_textures[variant_idx];

                if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                    let dx = (*gx as f64) * ts;
                    let dy = (*gy as f64) * ts;

                    if tile_flip(*gx, *gy) {
                        if let Some(flipped) = loaded.rock_textures_flipped.get(variant_idx) {
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts,
                            )?;
                        }
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts,
                        )?;
                    }
                }
            }

            Drawable::WaterRock(gx, gy) => {
                let variant_idx =
                    (gx.wrapping_mul(37).wrapping_add(gy.wrapping_mul(19))) as usize
                        % loaded.water_rock_textures.len();
                let (tex_id, frame_w, frame_h) = loaded.water_rock_textures[variant_idx];

                if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                    let fw = frame_w as f64;
                    let fh = frame_h as f64;
                    let dx = (*gx as f64) * ts;
                    let dy = (*gy as f64) * ts;

                    if tile_flip(*gx, *gy) {
                        if let Some((flipped, _, _)) = loaded.water_rock_textures_flipped.get(variant_idx) {
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, 0.0, 0.0, fw, fh, dx, dy, ts, ts,
                            )?;
                        }
                    } else {
                        // Use frame 0 only (static)
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, fw, fh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }

            Drawable::Building(zone_idx) => {
                if loaded.tower_textures.is_empty() {
                    continue;
                }
                let zone = &game.zone_manager.zones[*zone_idx as usize];

                // Select tower color based on zone state
                let color_idx = match zone.state {
                    ZoneState::Controlled(Faction::Blue)
                    | ZoneState::Capturing(Faction::Blue) => 1,
                    ZoneState::Controlled(Faction::Red)
                    | ZoneState::Capturing(Faction::Red) => 2,
                    _ => 0, // Black (neutral / contested)
                };

                let tex_id = loaded.tower_textures[color_idx];
                if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                    let draw_w = ts * 2.0;
                    let draw_h = ts * 4.0;
                    let dx = (zone.center_gx as f64) * ts + ts / 2.0 - draw_w / 2.0;
                    let dy = (zone.center_gy as f64) * ts + ts - draw_h;

                    // Pulse opacity during capturing to show in-progress
                    let alpha = match zone.state {
                        ZoneState::Capturing(_) => {
                            (zone.progress.abs() as f64 * 0.5 + 0.5).clamp(0.5, 1.0)
                        }
                        _ => 1.0,
                    };

                    if (alpha - 1.0).abs() > 0.001 {
                        ctx.set_global_alpha(alpha);
                    }

                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, 128.0, 256.0, dx, dy, draw_w, draw_h,
                    )?;

                    if (alpha - 1.0).abs() > 0.001 {
                        ctx.set_global_alpha(1.0);
                    }
                }
            }

            Drawable::BaseBuilding(base_idx, building_idx) => {
                if loaded.building_textures.is_empty() {
                    continue;
                }
                let base = &game.bases[*base_idx];
                let building = &base.buildings[*building_idx];

                // Texture index: kind_index * 2 + faction_index
                let kind_index = match building.kind {
                    BuildingKind::Barracks => 0,
                    BuildingKind::Archery => 1,
                    BuildingKind::Monastery => 2,
                };
                let faction_index = match base.faction {
                    Faction::Blue => 0,
                    _ => 1,
                };
                let tex_idx = kind_index * 2 + faction_index;

                if tex_idx < loaded.building_textures.len() {
                    let (tex_id, sprite_w, sprite_h) = loaded.building_textures[tex_idx];
                    if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                        let sw = sprite_w as f64;
                        let sh = sprite_h as f64;
                        let draw_w = ts * 3.0;
                        let draw_h = draw_w * (sh / sw);
                        let dx = (building.grid_x as f64) * ts + ts / 2.0 - draw_w / 2.0;
                        let dy = (building.grid_y as f64) * ts + ts - draw_h;

                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, sw, sh, dx, dy, draw_w, draw_h,
                        )?;
                    }
                }
            }

            Drawable::Particle(idx) => {
                let particle = &game.particles[*idx];
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
        }
    }

    // Arrow projectiles (drawn last — they fly above everything)
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
    terrain_canvas: web_sys::HtmlCanvasElement,
    terrain_ctx: web_sys::CanvasRenderingContext2d,
    terrain_dirty: bool,
    animator: TurnAnimator,
}
