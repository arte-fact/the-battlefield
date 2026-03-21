use crate::animation::TurnAnimator;
use crate::autotile;
use crate::building::BuildingKind;
use crate::game::{Game, ATTACK_CONE_HALF_ANGLE, ORDER_FLASH_DURATION};
use crate::grid::{self, Decoration, TileKind, GRID_SIZE, TILE_SIZE};
use crate::input::Input;
use crate::particle::Particle;
use crate::renderer::{draw_sprite, load_image, Canvas2d, TextureId, TextureManager};
use crate::sprite::SpriteSheet;
use crate::unit::{Facing, Faction, OrderKind, UnitAnim, UnitKind, DEATH_FADE_DURATION};
use crate::zone::ZoneState;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// Which screen the game is currently showing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum GameScreen {
    MainMenu,
    Playing,
    PlayerDeath,
    GameWon,
    GameLost,
}

/// Action triggered by clicking an overlay button.
#[derive(Clone, Copy)]
enum OverlayAction {
    Play,
    Retry,
    NewGame,
}

/// A clickable button on an overlay screen.
struct OverlayButton {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    action: OverlayAction,
}

/// Deterministic pseudo-random flip based on grid position.
/// Returns true for ~50% of tiles in a spatially uniform pattern.
fn tile_flip(gx: u32, gy: u32) -> bool {
    gx.wrapping_mul(48271).wrapping_add(gy.wrapping_mul(16807)) & 1 == 0
}

/// Draw a tile-sized image horizontally flipped.
fn draw_tile_flipped(
    ctx: &web_sys::CanvasRenderingContext2d,
    img: &web_sys::HtmlImageElement,
    sx: f64,
    sy: f64,
    sw: f64,
    sh: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
) -> Result<(), JsValue> {
    ctx.save();
    ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
    ctx.scale(-1.0, 1.0)?;
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
        img,
        sx,
        sy,
        sw,
        sh,
        -dw / 2.0,
        -dh / 2.0,
        dw,
        dh,
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
    /// kind: 0=Barracks, 1=Archery, 2=Monastery; faction: 0=Blue, 1=Red
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
    initial_seed: u32,
) -> Result<(), JsValue> {
    let input = Rc::new(RefCell::new(Input::new()));
    let loaded_textures = Rc::new(RefCell::new(LoadedTextures::new()));
    let textures_loading = Rc::new(RefCell::new(false));

    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let hud = HudElements::from_document(&document);
    let hud = Rc::new(hud);

    // Shared pending click position (set by mousedown / touchstart, consumed by game loop)
    let pending_click: Rc<RefCell<Option<(f32, f32)>>> = Rc::new(RefCell::new(None));

    // Grab HUD container for show/hide toggling
    let hud_container = document
        .get_element_by_id("hud")
        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok());

    setup_input_listeners(canvas, &input, &pending_click)?;

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

    // Create offscreen minimap terrain canvas (1 pixel per tile, rendered once)
    let minimap_terrain = document
        .create_element("canvas")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    minimap_terrain.set_width(game.grid.width);
    minimap_terrain.set_height(game.grid.height);
    render_minimap_terrain(&minimap_terrain, &game)?;

    // Create chunk-based terrain cache (CHUNK_TILES x CHUNK_TILES tiles per chunk)
    let terrain_chunks = TerrainChunks::new(&document, game.grid.width, game.grid.height)?;

    let last_css_w = canvas.client_width().max(1) as u32;
    let last_css_h = canvas.client_height().max(1) as u32;
    let state = Rc::new(RefCell::new(LoopState {
        canvas2d,
        canvas_element: canvas.clone(),
        game,
        texture_manager,
        last_time: None,
        elapsed: 0.0,
        fog_canvas,
        fog_ctx,
        terrain_chunks,
        terrain_dirty: true,
        animator: TurnAnimator::new(),
        minimap_terrain,
        last_css_w,
        last_css_h,
        screen: GameScreen::MainMenu,
        current_seed: initial_seed,
        overlay_buttons: Vec::new(),
        overlay_delay: 0.0,
        hud_container,
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

            // Detect resize / orientation change
            {
                let css_w = state_guard.canvas_element.client_width().max(1) as u32;
                let css_h = state_guard.canvas_element.client_height().max(1) as u32;
                if css_w != state_guard.last_css_w || css_h != state_guard.last_css_h {
                    state_guard.last_css_w = css_w;
                    state_guard.last_css_h = css_h;
                    let dpr = state_guard.canvas2d.dpr as f32;
                    let canvas_w = (css_w as f32 * dpr) as u32;
                    let canvas_h = (css_h as f32 * dpr) as u32;
                    state_guard.canvas_element.set_width(canvas_w);
                    state_guard.canvas_element.set_height(canvas_h);
                    state_guard.canvas2d.width = canvas_w as f64;
                    state_guard.canvas2d.height = canvas_h as f64;
                    state_guard.canvas2d.ctx.set_image_smoothing_enabled(false);
                    state_guard
                        .game
                        .camera
                        .resize(canvas_w as f32, canvas_h as f32);
                    state_guard.game.camera.zoom = state_guard.game.camera.ideal_zoom();
                    let mut inp = input.borrow_mut();
                    inp.update_layout(canvas_w as f32, canvas_h as f32, dpr);
                }
            }

            // --- Overlay button click handling ---
            {
                let click = pending_click.borrow_mut().take();
                if let Some((cx, cy)) = click {
                    let mut matched_action = None;
                    for btn in &state_guard.overlay_buttons {
                        if (cx as f64) >= btn.x
                            && (cx as f64) <= btn.x + btn.w
                            && (cy as f64) >= btn.y
                            && (cy as f64) <= btn.y + btn.h
                        {
                            matched_action = Some(btn.action);
                            break;
                        }
                    }
                    if let Some(action) = matched_action {
                        match action {
                            OverlayAction::Play => {
                                state_guard.screen = GameScreen::Playing;
                                if let Some(ref el) = state_guard.hud_container {
                                    let _ = el.style().set_property("display", "flex");
                                }
                                input.borrow_mut().clear_all();
                            }
                            OverlayAction::Retry => {
                                let seed = state_guard.current_seed;
                                let vw = state_guard.canvas2d.width as f32;
                                let vh = state_guard.canvas2d.height as f32;
                                restart_game(&mut state_guard, seed, vw, vh);
                                input.borrow_mut().clear_all();
                            }
                            OverlayAction::NewGame => {
                                let seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
                                let vw = state_guard.canvas2d.width as f32;
                                let vh = state_guard.canvas2d.height as f32;
                                restart_game(&mut state_guard, seed, vw, vh);
                                input.borrow_mut().clear_all();
                            }
                        }
                    }
                }
            }

            // --- Keyboard shortcuts for menu / overlays ---
            if state_guard.screen != GameScreen::Playing {
                let mut inp = input.borrow_mut();
                let enter = inp.take_key("Enter");
                let space = inp.take_key(" ");
                match state_guard.screen {
                    GameScreen::MainMenu => {
                        if enter || space {
                            state_guard.screen = GameScreen::Playing;
                            if let Some(ref el) = state_guard.hud_container {
                                let _ = el.style().set_property("display", "flex");
                            }
                            inp.clear_all();
                        }
                    }
                    GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                        // Enter = retry same map, Space = new game
                        if enter {
                            let seed = state_guard.current_seed;
                            let vw = state_guard.canvas2d.width as f32;
                            let vh = state_guard.canvas2d.height as f32;
                            inp.clear_all();
                            drop(inp);
                            restart_game(&mut state_guard, seed, vw, vh);
                            input.borrow_mut().clear_all();
                        } else if space {
                            let seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
                            let vw = state_guard.canvas2d.width as f32;
                            let vh = state_guard.canvas2d.height as f32;
                            inp.clear_all();
                            drop(inp);
                            restart_game(&mut state_guard, seed, vw, vh);
                            input.borrow_mut().clear_all();
                        }
                    }
                    _ => {}
                }
            }

            // Process input and real-time game logic (only when Playing)
            if state_guard.screen == GameScreen::Playing {
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

                // Touch: single-finger camera drag (right side)
                let (drag_dx, drag_dy) = inp.take_camera_drag();
                if drag_dx.abs() > f32::EPSILON || drag_dy.abs() > f32::EPSILON {
                    game.camera.x -= drag_dx / game.camera.zoom;
                    game.camera.y -= drag_dy / game.camera.zoom;
                }

                // Clamp camera to world bounds after pan/zoom
                let world_size = GRID_SIZE as f32 * TILE_SIZE;
                game.camera.clamp_to_world(world_size, world_size);

                // Only run game logic if no winner yet
                if game.winner.is_none() {
                    // Snapshot positions for movement animations
                    let old_positions: Vec<(f32, f32)> =
                        game.units.iter().map(|u| (u.x, u.y)).collect();

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
                    let attack_input =
                        inp.take_attack_key() || inp.take_attack_pressed() || attack_held;
                    if attack_input && game.player_attack() && inp.is_touch_device {
                        haptic(25);
                    }

                    // Player orders: H=Hold, G=Go, R=Retreat, F=Follow
                    if inp.take_order_hold() && game.issue_order("hold") > 0 && inp.is_touch_device
                    {
                        haptic(15);
                    }
                    if inp.take_order_go() && game.issue_order("go") > 0 && inp.is_touch_device {
                        haptic(15);
                    }
                    if inp.take_order_retreat()
                        && game.issue_order("retreat") > 0
                        && inp.is_touch_device
                    {
                        haptic(15);
                    }
                    if inp.take_order_follow()
                        && game.issue_order("follow") > 0
                        && inp.is_touch_device
                    {
                        haptic(15);
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
                let alive_ids: Vec<_> = state_guard
                    .game
                    .units
                    .iter()
                    .filter(|u| u.alive)
                    .map(|u| u.id)
                    .collect();
                state_guard
                    .animator
                    .init_visual_alive(alive_ids.into_iter());
                let anim_output = state_guard.animator.enqueue(events);
                // Immediately spawn dust particles from move events
                for (kind, x, y) in anim_output.particles {
                    state_guard.game.particles.push(Particle::new(kind, x, y));
                }
            }

            // Advance all animations in parallel and collect spawned effects
            {
                let LoopState {
                    ref mut animator,
                    ref mut game,
                    ..
                } = *state_guard;
                if animator.is_playing() {
                    let anim_output = animator.update(dt as f32, &mut game.units);
                    for (kind, x, y) in anim_output.particles {
                        game.particles.push(Particle::new(kind, x, y));
                    }
                }
            }

            // Update game state (animations, particles, camera follow)
            state_guard.game.update(dt);

            // --- Detect state transitions ---
            if state_guard.screen == GameScreen::Playing {
                if let Some(winner) = state_guard.game.winner {
                    if winner == Faction::Blue {
                        state_guard.screen = GameScreen::GameWon;
                    } else {
                        state_guard.screen = GameScreen::GameLost;
                    }
                    state_guard.overlay_delay = 0.0;
                    if let Some(ref el) = state_guard.hud_container {
                        let _ = el.style().set_property("display", "none");
                    }
                } else if !state_guard.game.is_player_alive() {
                    // Wait for death fade to complete + 0.5s delay
                    let player_fade = state_guard
                        .game
                        .units
                        .iter()
                        .find(|u| u.is_player)
                        .map(|u| u.death_fade)
                        .unwrap_or(0.0);
                    if player_fade <= 0.0 {
                        state_guard.overlay_delay += dt as f32;
                        if state_guard.overlay_delay >= 0.5 {
                            state_guard.screen = GameScreen::PlayerDeath;
                            if let Some(ref el) = state_guard.hud_container {
                                let _ = el.style().set_property("display", "none");
                            }
                        }
                    }
                }
            }

            // Update HUD (only when Playing)
            if state_guard.screen == GameScreen::Playing {
                if let Some(ref hud) = *hud.as_ref() {
                    hud.update(&state_guard.game);
                }
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
    pending_click: &Rc<RefCell<Option<(f32, f32)>>>,
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

    // Mousedown (for overlay button clicks)
    {
        let click_clone = pending_click.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
            let rect = canvas_clone.get_bounding_client_rect();
            let scale_x = canvas_clone.width() as f64 / rect.width();
            let scale_y = canvas_clone.height() as f64 / rect.height();
            let cx = (e.client_x() as f64 - rect.left()) * scale_x;
            let cy = (e.client_y() as f64 - rect.top()) * scale_y;
            *click_clone.borrow_mut() = Some((cx as f32, cy as f32));
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("mousedown", closure.as_ref().unchecked_ref())?;
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
        let click_clone = pending_click.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            let count = touches.length();
            if count >= 1 {
                let t = e.changed_touches().get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                let canvas_w = canvas_clone.width() as f32;
                // Also set pending click for overlay button handling
                if count == 1 {
                    *click_clone.borrow_mut() = Some((cx, cy));
                }
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
                    input_clone
                        .borrow_mut()
                        .on_touch_move_single(t.identifier(), cx, cy);
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
fn canvas_touch_coords(canvas: &web_sys::HtmlCanvasElement, touch: &web_sys::Touch) -> (f32, f32) {
    let rect = canvas.get_bounding_client_rect();
    let scale_x = canvas.width() as f64 / rect.width();
    let scale_y = canvas.height() as f64 / rect.height();
    let cx = (touch.client_x() as f64 - rect.left()) * scale_x;
    let cy = (touch.client_y() as f64 - rect.top()) * scale_y;
    (cx as f32, cy as f32)
}

/// Trigger haptic feedback (vibration) if supported.
fn haptic(duration_ms: u32) {
    if let Some(window) = web_sys::window() {
        let navigator = window.navigator();
        let nav_js: &JsValue = navigator.as_ref();
        if let Ok(vibrate_fn) = js_sys::Reflect::get(nav_js, &JsValue::from_str("vibrate")) {
            if vibrate_fn.is_function() {
                let _ =
                    js_sys::Function::from(vibrate_fn).call1(nav_js, &JsValue::from(duration_ms));
            }
        }
    }
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

                let tex_id = load_texture(state, &url, frame_size, frame_size, frame_count).await?;

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
        let url = format!("{}/Terrain/Tileset/Water Background color.png", ASSET_BASE);
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

    // Load base production building sprites (3 kinds × 2 factions)
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
                    ASSET_BASE,
                    faction_folder,
                    kind.asset_filename()
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
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
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
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                    );
                bush_flipped.push((c, fw, fh));
            }
        }

        let mut water_rock_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.water_rock_textures {
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
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

fn render_frame(
    state: &mut LoopState,
    loaded: &LoadedTextures,
    input: &Input,
) -> Result<(), JsValue> {
    // Update fog offscreen canvas if FOV changed
    if state.game.fog_dirty {
        update_fog_canvas(&state.fog_ctx, &state.game)?;
        state.game.fog_dirty = false;
    }

    // Mark all terrain chunks dirty on first render or when terrain changes
    if state.terrain_dirty {
        state.terrain_chunks.mark_all_dirty();
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

    // 3. Water → foam → cached terrain chunks (only visible chunks drawn)
    draw_water(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy)?;
    draw_foam(
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

    // Render and draw only the terrain chunks that overlap the visible area
    {
        let min_cx = min_gx / CHUNK_TILES;
        let min_cy = min_gy / CHUNK_TILES;
        let max_cx = ((max_gx + CHUNK_TILES - 1) / CHUNK_TILES).min(state.terrain_chunks.cols);
        let max_cy = ((max_gy + CHUNK_TILES - 1) / CHUNK_TILES).min(state.terrain_chunks.rows);

        for cy in min_cy..max_cy {
            for cx in min_cx..max_cx {
                let ci = (cy * state.terrain_chunks.cols + cx) as usize;

                // Render chunk if dirty
                if state.terrain_chunks.dirty[ci] {
                    let chunk_gx = cx * CHUNK_TILES;
                    let chunk_gy = cy * CHUNK_TILES;
                    let chunk_end_gx = (chunk_gx + CHUNK_TILES).min(game.grid.width);
                    let chunk_end_gy = (chunk_gy + CHUNK_TILES).min(game.grid.height);
                    render_terrain_chunk(
                        &state.terrain_chunks.contexts[ci],
                        game,
                        loaded,
                        &state.texture_manager,
                        chunk_gx,
                        chunk_gy,
                        chunk_end_gx,
                        chunk_end_gy,
                    )?;
                    state.terrain_chunks.dirty[ci] = false;
                }

                // Draw chunk to main canvas at its world position
                let wx = (cx * CHUNK_TILES) as f64 * ts;
                let wy = (cy * CHUNK_TILES) as f64 * ts;
                ctx.draw_image_with_html_canvas_element(
                    &state.terrain_chunks.canvases[ci],
                    wx,
                    wy,
                )?;
            }
        }
    }

    // 4. Capture zone overlays (colored fill, dashed border, labels, progress bars)
    draw_zone_overlays(ctx, game, min_gx, min_gy, max_gx, max_gy)?;

    // 5. Draw overlays (player highlight, HP bars, path line, attack target)
    draw_overlays(
        ctx,
        game,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        ts,
        &state.animator,
    )?;

    // 6a. Draw ground-level rocks (always behind units)
    draw_rocks(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy)?;

    // 6a2. Draw bush decorations (animated, ground level)
    draw_bushes(
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

    // 6b. Draw foreground sprites (units, particles, projectiles, trees) — Y-sorted together
    draw_foreground(
        ctx,
        game,
        loaded,
        tm,
        &state.animator,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        state.elapsed,
    )?;

    // 7. HP bars and order labels (drawn on top of units)
    draw_unit_bars(ctx, game, &state.animator)?;

    // 8. Draw fog of war — only the visible portion of the fog canvas
    let grid_world_size = (game.grid.width as f64) * ts;
    ctx.set_image_smoothing_enabled(true);
    {
        // Source rect in fog canvas (1 pixel per tile)
        let sx = min_gx as f64;
        let sy = min_gy as f64;
        let sw = (max_gx - min_gx) as f64;
        let sh = (max_gy - min_gy) as f64;
        // Dest rect in world space
        let dx = min_gx as f64 * ts;
        let dy = min_gy as f64 * ts;
        let dw = sw * ts;
        let dh = sh * ts;
        ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            &state.fog_canvas,
            sx,
            sy,
            sw,
            sh,
            dx,
            dy,
            dw,
            dh,
        )?;
    }

    // 9. Fill solid black outside the grid to hide background when zoomed out.
    // Use a large margin so it covers any visible area beyond the grid.
    let margin = grid_world_size;
    ctx.set_fill_style_str("#000");
    ctx.fill_rect(-margin, -margin, margin, grid_world_size + 2.0 * margin); // left
    ctx.fill_rect(
        grid_world_size,
        -margin,
        margin,
        grid_world_size + 2.0 * margin,
    ); // right
    ctx.fill_rect(0.0, -margin, grid_world_size, margin); // top
    ctx.fill_rect(0.0, grid_world_size, grid_world_size, margin); // bottom

    ctx.restore();

    // Draw zone HUD pips
    draw_zone_hud(
        ctx,
        game,
        canvas_w,
        state.canvas2d.dpr,
        input.is_touch_device,
    )?;

    // Draw minimap (top-left on touch to avoid joystick, bottom-left on desktop)
    draw_minimap(
        ctx,
        game,
        &state.minimap_terrain,
        canvas_w,
        canvas_h,
        state.canvas2d.dpr,
        input.is_touch_device,
    )?;

    // Draw victory progress bar (only during gameplay)
    if state.screen == GameScreen::Playing {
        draw_victory_progress(ctx, game, canvas_w, canvas_h, state.canvas2d.dpr)?;
    }

    // Draw touch controls (only during gameplay)
    if state.screen == GameScreen::Playing {
        draw_touch_controls(ctx, input, canvas_h, state.canvas2d.dpr)?;
    }

    // Draw overlay screens (menu, death, win, lose)
    state.overlay_buttons.clear();
    match state.screen {
        GameScreen::MainMenu => {
            draw_main_menu(
                ctx,
                canvas_w,
                canvas_h,
                state.canvas2d.dpr,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::PlayerDeath => {
            draw_death_screen(
                ctx,
                canvas_w,
                canvas_h,
                state.canvas2d.dpr,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::GameWon => {
            draw_result_screen(
                ctx,
                canvas_w,
                canvas_h,
                state.canvas2d.dpr,
                true,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::GameLost => {
            draw_result_screen(
                ctx,
                canvas_w,
                canvas_h,
                state.canvas2d.dpr,
                false,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::Playing => {}
    }

    Ok(())
}

/// Render all static terrain layers to the offscreen terrain cache canvas.
/// Called once after textures load; the result is blitted each frame.
/// Render a single terrain chunk covering tiles [gx0..gx1) x [gy0..gy1).
/// All drawing uses chunk-local coordinates (tile position minus chunk origin).
#[allow(clippy::too_many_arguments)]
fn render_terrain_chunk(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    gx0: u32,
    gy0: u32,
    gx1: u32,
    gy1: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;
    let w = game.grid.width;
    let h = game.grid.height;
    let ox = gx0 as f64 * ts; // world-space origin of this chunk
    let oy = gy0 as f64 * ts;

    // Clear the chunk canvas (transparent)
    let chunk_w = (gx1 - gx0) as f64 * ts;
    let chunk_h = (gy1 - gy0) as f64 * ts;
    ctx.clear_rect(0.0, 0.0, chunk_w, chunk_h);

    // Layer 2.5: Road sand fill (extends 1 tile into neighbors)
    {
        ctx.set_fill_style_str("#C4A265");
        for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                if game.grid.get(gx, gy) != TileKind::Road {
                    continue;
                }
                let dx = (gx as f64) * ts - ox;
                let dy = (gy as f64) * ts - oy;
                ctx.fill_rect(dx, dy, ts, ts);
                if gx > 0 && game.grid.get(gx - 1, gy) != TileKind::Road {
                    ctx.fill_rect(dx - ts, dy, ts, ts);
                }
                if gx + 1 < w && game.grid.get(gx + 1, gy) != TileKind::Road {
                    ctx.fill_rect(dx + ts, dy, ts, ts);
                }
                if gy > 0 && game.grid.get(gx, gy - 1) != TileKind::Road {
                    ctx.fill_rect(dx, dy - ts, ts, ts);
                }
                if gy + 1 < h && game.grid.get(gx, gy + 1) != TileKind::Road {
                    ctx.fill_rect(dx, dy + ts, ts, ts);
                }
            }
        }
    }

    // Layer 3: Flat ground (auto-tiled)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    let tile = game.grid.get(gx, gy);
                    if !tile.is_land() || tile == TileKind::Road {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts - ox;
                    let dy = (gy as f64) * ts - oy;
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

    // Layer 3.5: Road surface (grass autotile tinted to sand)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    if game.grid.get(gx, gy) != TileKind::Road {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts - ox;
                    let dy = (gy as f64) * ts - oy;
                    if col == 1 && row == 1 && tile_flip(gx, gy) {
                        draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
            ctx.set_global_composite_operation("multiply")?;
            ctx.set_fill_style_str("#D4B070");
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    if game.grid.get(gx, gy) == TileKind::Road {
                        ctx.fill_rect((gx as f64) * ts - ox, (gy as f64) * ts - oy, ts, ts);
                    }
                }
            }
            ctx.set_global_composite_operation("source-over")?;
        }
    }

    // Layer 4: Elevation (shadow + elevated surface + cliff)
    for level in 2..=2u8 {
        if let Some(shadow_tex_id) = loaded.shadow_texture {
            if let Some((img, _, _, _)) = tm.get_image(shadow_tex_id) {
                ctx.set_global_alpha(0.5);
                for gy in gy0..gy1 {
                    for gx in gx0..gx1 {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        if gy + 1 < h && game.grid.elevation(gx, gy + 1) < level {
                            let shadow_size = 192.0_f64;
                            let dx = (gx as f64) * ts + ts / 2.0 - shadow_size / 2.0 - ox;
                            let dy = ((gy + 1) as f64) * ts + ts / 2.0 - shadow_size / 2.0 - oy;
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, 0.0, 0.0, shadow_size, shadow_size, dx, dy, shadow_size, shadow_size,
                            )?;
                        }
                    }
                }
                ctx.set_global_alpha(1.0);
            }
        }

        let elev_tex_id = if level == 2 {
            loaded.tilemap_texture2
        } else {
            loaded.tilemap_texture
        };
        if let Some(tilemap_tex_id) = elev_tex_id {
            if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
                for gy in gy0..gy1 {
                    for gx in gx0..gx1 {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        let (col, row) = autotile::elevated_top_src(&game.grid, gx, gy, level);
                        let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                        let dx = (gx as f64) * ts - ox;
                        let dy = (gy as f64) * ts - oy;
                        if col == 6 && row == 1 && tile_flip(gx, gy) {
                            draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                        } else {
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, sx, sy, sw, sh, dx, dy, ts, ts,
                            )?;
                        }

                        if let Some((ccol, crow)) = autotile::cliff_src(&game.grid, gx, gy, level) {
                            let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
                            let cdy = ((gy + 1) as f64) * ts - oy;
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

    // (Bushes are drawn in a separate animated pass, not in cached terrain chunks)

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

        if zone_max_gx < min_gx
            || zone_min_gx > max_gx
            || zone_max_gy < min_gy
            || zone_min_gy > max_gy
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
            ZoneState::Capturing(Faction::Blue) => {
                ("rgba(60,120,255,0.08)", "rgba(60,120,255,0.4)")
            }
            ZoneState::Capturing(Faction::Red) => ("rgba(255,60,60,0.08)", "rgba(255,60,60,0.4)"),
            ZoneState::Controlled(Faction::Blue) => {
                ("rgba(60,120,255,0.12)", "rgba(60,120,255,0.5)")
            }
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
        ctx.fill_text(&zone.name, cx, label_y)?;

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

/// Draw capture zone HUD pips in screen space (top-center on touch, top-right on desktop).
fn draw_zone_hud(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    canvas_w: f64,
    dpr: f64,
    is_touch: bool,
) -> Result<(), JsValue> {
    let zones = &game.zone_manager.zones;
    if zones.is_empty() {
        return Ok(());
    }

    let pip_size = if is_touch { 20.0 * dpr } else { 14.0 * dpr };
    let gap = if is_touch { 5.0 * dpr } else { 3.0 * dpr };
    let margin = 10.0 * dpr;
    let total_w = zones.len() as f64 * (pip_size + gap) - gap;
    // Center horizontally on touch, right-align on desktop
    let start_x = if is_touch {
        (canvas_w - total_w) / 2.0
    } else {
        canvas_w - margin - total_w
    };
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

/// Render the static terrain layer of the minimap (1 pixel per tile).
fn render_minimap_terrain(canvas: &web_sys::HtmlCanvasElement, game: &Game) -> Result<(), JsValue> {
    let w = game.grid.width;
    let h = game.grid.height;
    let len = (w * h * 4) as usize;
    let mut pixels = vec![0u8; len];

    for gy in 0..h {
        for gx in 0..w {
            let idx = (gy * w + gx) as usize;
            let po = idx * 4;

            let (r, g, b) = match game.grid.get(gx, gy) {
                TileKind::Water => (30, 60, 120),
                TileKind::Forest => (30, 80, 30),
                TileKind::Rock => (90, 85, 75),
                TileKind::Road => (160, 140, 100),
                TileKind::Grass => {
                    if game.grid.elevation(gx, gy) >= 2 {
                        (100, 95, 70)
                    } else if game.grid.decoration(gx, gy) == Some(Decoration::Bush) {
                        (55, 100, 45)
                    } else {
                        (70, 110, 50)
                    }
                }
            };
            pixels[po] = r;
            pixels[po + 1] = g;
            pixels[po + 2] = b;
            pixels[po + 3] = 255;
        }
    }

    let mm_ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    let clamped = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h)?;
    mm_ctx.put_image_data(&image_data, 0.0, 0.0)?;
    Ok(())
}

/// Draw the minimap HUD (top-left on touch devices, bottom-left on desktop).
fn draw_minimap(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    terrain_canvas: &web_sys::HtmlCanvasElement,
    _canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    is_touch: bool,
) -> Result<(), JsValue> {
    let mm_size = (160.0 * dpr).min(canvas_h * 0.25);
    let mm_margin = 8.0 * dpr;
    let mm_x = mm_margin;
    // Top-left on touch (below zone HUD), bottom-left on desktop
    let mm_y = if is_touch {
        mm_margin + 34.0 * dpr // below zone pips
    } else {
        canvas_h - mm_margin - mm_size
    };

    let grid_w = game.grid.width as f64;
    let grid_h = game.grid.height as f64;
    let scale_x = mm_size / grid_w;
    let scale_y = mm_size / grid_h;

    // Background border
    ctx.set_fill_style_str("rgba(0,0,0,0.7)");
    let border = 2.0 * dpr;
    ctx.fill_rect(
        mm_x - border,
        mm_y - border,
        mm_size + border * 2.0,
        mm_size + border * 2.0,
    );

    // Draw pre-rendered terrain (nearest-neighbor for crisp pixels)
    ctx.save();
    let _ = ctx.set_image_smoothing_enabled(false);
    ctx.draw_image_with_html_canvas_element_and_dw_and_dh(
        terrain_canvas,
        mm_x,
        mm_y,
        mm_size,
        mm_size,
    )?;
    ctx.restore();

    // Fog of war overlay (semi-transparent black for hidden tiles)
    // Use a coarse approach: sample every 2nd tile for performance
    let step = 2.0_f64;
    let rect_w = (scale_x * step).ceil().max(1.0);
    let rect_h = (scale_y * step).ceil().max(1.0);
    ctx.set_fill_style_str("rgba(0,0,0,0.55)");
    let mut gy = 0.0_f64;
    while gy < grid_h {
        let mut gx = 0.0_f64;
        while gx < grid_w {
            let idx = (gy as u32 * game.grid.width + gx as u32) as usize;
            if idx < game.visible.len() && !game.visible[idx] {
                let rx = mm_x + gx * scale_x;
                let ry = mm_y + gy * scale_y;
                ctx.fill_rect(rx, ry, rect_w, rect_h);
            }
            gx += step;
        }
        gy += step;
    }

    // Capture zones — colored circles
    for zone in &game.zone_manager.zones {
        let zx = mm_x + zone.center_gx as f64 * scale_x;
        let zy = mm_y + zone.center_gy as f64 * scale_y;
        let zr = (zone.radius as f64 * scale_x).max(2.0 * dpr);

        let color = match zone.state {
            ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => {
                "rgba(60,130,255,0.8)"
            }
            ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => {
                "rgba(255,60,60,0.8)"
            }
            ZoneState::Contested => "rgba(255,200,0,0.8)",
            _ => "rgba(180,180,180,0.6)",
        };
        ctx.set_fill_style_str(color);
        ctx.begin_path();
        ctx.arc(zx, zy, zr, 0.0, std::f64::consts::TAU)?;
        ctx.fill();
    }

    // Buildings — small faction-colored squares
    for b in &game.buildings {
        let bx = mm_x + b.grid_x as f64 * scale_x;
        let by = mm_y + b.grid_y as f64 * scale_y;
        let bs = (2.0 * dpr).max(2.0);
        let color = match b.faction {
            Faction::Blue => "rgba(80,150,255,0.9)",
            _ => "rgba(255,80,80,0.9)",
        };
        ctx.set_fill_style_str(color);
        ctx.fill_rect(bx - bs * 0.5, by - bs * 0.5, bs, bs);
    }

    // Units — small dots (enemies hidden in fog)
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = grid::world_to_grid(unit.x, unit.y);
        // Hide enemy units outside player visibility
        if unit.faction != Faction::Blue {
            let idx = (gy as u32 * game.grid.width + gx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] {
                continue;
            }
        }
        let ux = mm_x + gx as f64 * scale_x;
        let uy = mm_y + gy as f64 * scale_y;
        let ur = (1.2 * dpr).max(1.0);

        let color = match unit.faction {
            Faction::Blue => "#4a9eff",
            _ => "#ff4a4a",
        };
        ctx.set_fill_style_str(color);
        ctx.fill_rect(ux - ur, uy - ur, ur * 2.0, ur * 2.0);
    }

    // Camera viewport rectangle
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let world_size = grid_w * TILE_SIZE as f64;
    let vx = mm_x + (vl as f64 / world_size) * mm_size;
    let vy = mm_y + (vt as f64 / world_size) * mm_size;
    let vw = ((vr - vl) as f64 / world_size) * mm_size;
    let vh = ((vb - vt) as f64 / world_size) * mm_size;

    ctx.set_stroke_style_str("rgba(255,255,255,0.85)");
    ctx.set_line_width(1.5 * dpr);
    ctx.stroke_rect(
        vx.max(mm_x),
        vy.max(mm_y),
        vw.min(mm_size - (vx - mm_x).max(0.0)),
        vh.min(mm_size - (vy - mm_y).max(0.0)),
    );

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
                    let tile_offset = (gx.wrapping_mul(7).wrapping_add(gy.wrapping_mul(13))) % 16;
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
        ctx.arc(
            player.x as f64,
            player.y as f64,
            24.0,
            0.0,
            std::f64::consts::TAU,
        )?;
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

    ctx.set_global_alpha(1.0);

    Ok(())
}

/// Draw HP bars and order labels on top of unit sprites.
fn draw_unit_bars(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // HP bars
    for unit in &game.units {
        let show = if animator.is_playing() {
            animator.is_visually_alive(unit.id)
        } else {
            unit.alive
        };
        if !show {
            continue;
        }
        // Hide enemy HP bars outside friendly line of sight
        if unit.faction != Faction::Blue {
            let (gx, gy) = unit.grid_cell();
            let idx = (gy * game.grid.width + gx) as usize;
            if !game.visible[idx] {
                continue;
            }
        }
        let (wx, wy) = (unit.x, unit.y);
        let bar_width = 48.0_f64;
        let bar_height = 6.0_f64;
        let bar_y = (wy as f64) - (TILE_SIZE as f64) * 0.85;
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

    // Order word indicators ("HOLD", "GO", "RETREAT") above units
    for unit in &game.units {
        if !unit.alive || unit.order_flash <= 0.0 {
            continue;
        }
        let label = match unit.order {
            Some(OrderKind::Hold { .. }) => "HOLD",
            Some(OrderKind::Go { .. }) => "GO",
            Some(OrderKind::Retreat { .. }) => "RETREAT",
            Some(OrderKind::Follow) => "FOLLOW",
            None => continue,
        };

        let alpha = (unit.order_flash / ORDER_FLASH_DURATION) as f64;
        let wx = unit.x as f64;
        let wy = unit.y as f64 - (TILE_SIZE as f64) * 1.0;

        ctx.set_global_alpha(alpha);
        ctx.set_font("bold 14px sans-serif");
        ctx.set_text_align("center");
        ctx.set_text_baseline("bottom");
        ctx.set_stroke_style_str("rgba(0,0,0,0.9)");
        ctx.set_line_width(3.0);
        ctx.stroke_text(label, wx, wy)?;
        ctx.set_fill_style_str("rgb(255,215,0)");
        ctx.fill_text(label, wx, wy)?;
    }
    ctx.set_global_alpha(1.0);

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

/// Draw a rounded-rect overlay button and register it for hit testing.
fn draw_overlay_button(
    ctx: &web_sys::CanvasRenderingContext2d,
    label: &str,
    cx: f64,
    cy: f64,
    dpr: f64,
    fill_color: &str,
    action: OverlayAction,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let btn_w = 200.0 * dpr;
    let btn_h = 50.0 * dpr;
    let btn_x = cx - btn_w / 2.0;
    let btn_y = cy - btn_h / 2.0;
    let r = 10.0 * dpr;

    // Button background
    ctx.set_fill_style_str(fill_color);
    ctx.begin_path();
    // Rounded rect using arc_to
    ctx.move_to(btn_x + r, btn_y);
    ctx.arc_to(btn_x + btn_w, btn_y, btn_x + btn_w, btn_y + btn_h, r)?;
    ctx.arc_to(btn_x + btn_w, btn_y + btn_h, btn_x, btn_y + btn_h, r)?;
    ctx.arc_to(btn_x, btn_y + btn_h, btn_x, btn_y, r)?;
    ctx.arc_to(btn_x, btn_y, btn_x + btn_w, btn_y, r)?;
    ctx.close_path();
    ctx.fill();

    // Border
    ctx.set_stroke_style_str("rgba(255, 255, 255, 0.4)");
    ctx.set_line_width(2.0 * dpr);
    ctx.stroke();

    // Label
    let font_size = 20.0 * dpr;
    ctx.set_font(&format!("bold {font_size}px sans-serif"));
    ctx.set_fill_style_str("white");
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.fill_text(label, cx, cy)?;

    buttons.push(OverlayButton {
        x: btn_x,
        y: btn_y,
        w: btn_w,
        h: btn_h,
        action,
    });

    Ok(())
}

/// Draw the main menu screen.
fn draw_main_menu(
    ctx: &web_sys::CanvasRenderingContext2d,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Dark overlay
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.75)");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 52.0 * dpr;
    ctx.set_font(&format!("bold {title_font}px sans-serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    // Shadow
    ctx.set_fill_style_str("rgba(0, 0, 0, 0.7)");
    ctx.fill_text(
        "THE BATTLEFIELD",
        cx + 3.0 * dpr,
        cy - 80.0 * dpr + 3.0 * dpr,
    )?;

    // Gold title
    ctx.set_fill_style_str("#ffd700");
    ctx.fill_text("THE BATTLEFIELD", cx, cy - 80.0 * dpr)?;

    // Play button
    draw_overlay_button(
        ctx,
        "PLAY",
        cx,
        cy + 20.0 * dpr,
        dpr,
        "rgba(70, 150, 70, 0.85)",
        OverlayAction::Play,
        buttons,
    )?;

    // Controls hint
    let hint_font = 13.0 * dpr;
    ctx.set_font(&format!("{hint_font}px monospace"));
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.5)");
    ctx.fill_text(
        "WASD move \u{2022} SPACE attack \u{2022} H/G/R/F orders",
        cx,
        cy + 90.0 * dpr,
    )?;
    ctx.fill_text("Enter / Space to start", cx, cy + 110.0 * dpr)?;

    Ok(())
}

/// Draw the "YOU DIED" screen with red tint.
fn draw_death_screen(
    ctx: &web_sys::CanvasRenderingContext2d,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Red-tinted overlay
    ctx.set_fill_style_str("rgba(80, 0, 0, 0.6)");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 56.0 * dpr;
    ctx.set_font(&format!("bold {title_font}px sans-serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    ctx.set_fill_style_str("rgba(0, 0, 0, 0.7)");
    ctx.fill_text("YOU DIED", cx + 3.0 * dpr, cy - 70.0 * dpr + 3.0 * dpr)?;

    ctx.set_fill_style_str("#cc2222");
    ctx.fill_text("YOU DIED", cx, cy - 70.0 * dpr)?;

    // Buttons
    let btn_y = cy + 20.0 * dpr;
    let gap = 120.0 * dpr;
    draw_overlay_button(
        ctx,
        "RETRY",
        cx - gap / 2.0,
        btn_y,
        dpr,
        "rgba(180, 80, 40, 0.85)",
        OverlayAction::Retry,
        buttons,
    )?;
    draw_overlay_button(
        ctx,
        "NEW GAME",
        cx + gap / 2.0,
        btn_y,
        dpr,
        "rgba(60, 120, 180, 0.85)",
        OverlayAction::NewGame,
        buttons,
    )?;

    // Hint
    let hint_font = 12.0 * dpr;
    ctx.set_font(&format!("{hint_font}px monospace"));
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.4)");
    ctx.fill_text(
        "Enter = Retry \u{2022} Space = New Game",
        cx,
        btn_y + 45.0 * dpr,
    )?;

    Ok(())
}

/// Draw the victory / defeat result screen.
fn draw_result_screen(
    ctx: &web_sys::CanvasRenderingContext2d,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    is_victory: bool,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Overlay tint
    if is_victory {
        ctx.set_fill_style_str("rgba(0, 30, 60, 0.6)");
    } else {
        ctx.set_fill_style_str("rgba(40, 0, 0, 0.6)");
    }
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 52.0 * dpr;
    ctx.set_font(&format!("bold {title_font}px sans-serif"));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");

    let (title, color) = if is_victory {
        ("VICTORY", "#4ea8ff")
    } else {
        ("DEFEAT", "#ff5555")
    };

    ctx.set_fill_style_str("rgba(0, 0, 0, 0.7)");
    ctx.fill_text(title, cx + 3.0 * dpr, cy - 80.0 * dpr + 3.0 * dpr)?;
    ctx.set_fill_style_str(color);
    ctx.fill_text(title, cx, cy - 80.0 * dpr)?;

    // Subtitle
    let sub_font = 18.0 * dpr;
    ctx.set_font(&format!("{sub_font}px sans-serif"));
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.7)");
    let subtitle = if is_victory {
        "All capture zones held for 2 minutes"
    } else {
        "The enemy holds all capture zones"
    };
    ctx.fill_text(subtitle, cx, cy - 40.0 * dpr)?;

    // Buttons
    let btn_y = cy + 30.0 * dpr;
    let gap = 120.0 * dpr;
    let retry_label = if is_victory { "REPLAY" } else { "RETRY" };
    draw_overlay_button(
        ctx,
        retry_label,
        cx - gap / 2.0,
        btn_y,
        dpr,
        "rgba(180, 130, 40, 0.85)",
        OverlayAction::Retry,
        buttons,
    )?;
    draw_overlay_button(
        ctx,
        "NEW GAME",
        cx + gap / 2.0,
        btn_y,
        dpr,
        "rgba(60, 120, 180, 0.85)",
        OverlayAction::NewGame,
        buttons,
    )?;

    // Hint
    let hint_font = 12.0 * dpr;
    ctx.set_font(&format!("{hint_font}px monospace"));
    ctx.set_fill_style_str("rgba(255, 255, 255, 0.4)");
    ctx.fill_text(
        &format!("Enter = {} \u{2022} Space = New Game", retry_label),
        cx,
        btn_y + 45.0 * dpr,
    )?;

    Ok(())
}

/// Reset the game state for retry / new game.
fn restart_game(state: &mut LoopState, seed: u32, viewport_w: f32, viewport_h: f32) {
    state.game = Game::new(viewport_w, viewport_h);
    state.game.setup_demo_battle_with_seed(seed);
    state.current_seed = seed;
    state.terrain_dirty = true;
    state.animator = TurnAnimator::new();
    state.overlay_delay = 0.0;
    state.overlay_buttons.clear();
    state.screen = GameScreen::Playing;

    // Re-create fog canvas to match new grid dimensions
    state.fog_canvas.set_width(state.game.grid.width);
    state.fog_canvas.set_height(state.game.grid.height);

    // Re-render minimap terrain
    state.minimap_terrain.set_width(state.game.grid.width);
    state.minimap_terrain.set_height(state.game.grid.height);
    let _ = render_minimap_terrain(&state.minimap_terrain, &state.game);

    // Show HUD
    if let Some(ref el) = state.hud_container {
        let _ = el.style().set_property("display", "flex");
    }
}

/// Draw a circular touch button with label.
fn draw_touch_button(
    ctx: &web_sys::CanvasRenderingContext2d,
    cx: f64,
    cy: f64,
    radius: f64,
    fill_color: &str,
    label: &str,
    pressed: bool,
    dpr: f64,
) -> Result<(), JsValue> {
    let scale = if pressed { 1.12 } else { 1.0 };
    let r = radius * scale;

    // Dark background for contrast
    ctx.set_global_alpha(if pressed { 0.6 } else { 0.35 });
    ctx.set_fill_style_str("rgba(0,0,0,0.7)");
    ctx.begin_path();
    ctx.arc(cx, cy, r + 2.0 * dpr, 0.0, std::f64::consts::TAU)?;
    ctx.fill();

    // Colored fill
    ctx.set_global_alpha(if pressed { 0.85 } else { 0.55 });
    ctx.set_fill_style_str(fill_color);
    ctx.begin_path();
    ctx.arc(cx, cy, r, 0.0, std::f64::consts::TAU)?;
    ctx.fill();

    // Border ring
    ctx.set_stroke_style_str(if pressed {
        "rgba(255,255,255,0.8)"
    } else {
        "rgba(255,255,255,0.4)"
    });
    ctx.set_line_width(2.0 * dpr);
    ctx.stroke();

    // Label
    let font_size = (radius * 0.5).max(10.0 * dpr);
    ctx.set_global_alpha(0.95);
    ctx.set_fill_style_str("white");
    ctx.set_font(&format!("bold {}px monospace", font_size as u32));
    ctx.set_text_align("center");
    ctx.set_text_baseline("middle");
    ctx.fill_text(label, cx, cy)?;

    ctx.set_global_alpha(1.0);
    Ok(())
}

/// Draw touch controls in screen space (virtual joystick + attack + order buttons).
fn draw_touch_controls(
    ctx: &web_sys::CanvasRenderingContext2d,
    input: &Input,
    canvas_h: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    if !input.is_touch_device {
        return Ok(());
    }

    // Ghost joystick hint (before first use)
    if !input.has_used_joystick && !input.joystick.active {
        let ghost_x = 100.0 * dpr;
        let ghost_y = canvas_h - 120.0 * dpr;
        ctx.set_global_alpha(0.15);
        ctx.set_fill_style_str("rgba(255,255,255,0.3)");
        ctx.begin_path();
        ctx.arc(
            ghost_x,
            ghost_y,
            input.joystick.max_radius as f64,
            0.0,
            std::f64::consts::TAU,
        )?;
        ctx.fill();
        ctx.set_stroke_style_str("rgba(255,255,255,0.2)");
        ctx.set_line_width(2.0 * dpr);
        ctx.stroke();
        ctx.set_global_alpha(0.3);
        ctx.set_fill_style_str("white");
        ctx.set_font(&format!("bold {}px monospace", (12.0 * dpr) as u32));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        ctx.fill_text("MOVE", ghost_x, ghost_y)?;
        ctx.set_global_alpha(1.0);
    }

    // Virtual joystick (when active)
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
        ctx.set_fill_style_str("rgba(255,255,255,0.7)");
        ctx.begin_path();
        ctx.arc(
            input.joystick.stick_x as f64,
            input.joystick.stick_y as f64,
            22.0 * dpr,
            0.0,
            std::f64::consts::TAU,
        )?;
        ctx.fill();
        ctx.set_stroke_style_str("rgba(255,255,255,0.5)");
        ctx.set_line_width(2.0 * dpr);
        ctx.stroke();
    }

    // Attack button
    draw_touch_button(
        ctx,
        input.attack_button.center_x as f64,
        input.attack_button.center_y as f64,
        input.attack_button.radius as f64,
        "rgba(220,50,50,0.6)",
        "ATK",
        input.attack_button.pressed,
        dpr,
    )?;

    // Order buttons
    let order_btns: [(&crate::input::ActionButton, &str, &str); 4] = [
        (&input.order_hold_btn, "H", "rgba(200,170,50,0.5)"),
        (&input.order_go_btn, "G", "rgba(50,180,80,0.5)"),
        (&input.order_retreat_btn, "R", "rgba(50,120,200,0.5)"),
        (&input.order_follow_btn, "F", "rgba(160,80,200,0.5)"),
    ];
    for (btn, label, color) in &order_btns {
        draw_touch_button(
            ctx,
            btn.center_x as f64,
            btn.center_y as f64,
            btn.radius as f64,
            color,
            label,
            btn.pressed,
            dpr,
        )?;
    }

    ctx.set_global_alpha(1.0);
    Ok(())
}

/// Draw animated bush decorations (ground level, always behind units).
fn draw_bushes(
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
    if loaded.bush_textures.is_empty() {
        return Ok(());
    }
    let ts = TILE_SIZE as f64;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                continue;
            }
            let variant_idx = (gx.wrapping_mul(41).wrapping_add(gy.wrapping_mul(23))) as usize
                % loaded.bush_textures.len();
            let (tex_id, frame_w, frame_h) = loaded.bush_textures[variant_idx];

            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let fw = frame_w as f64;
                let fh = frame_h as f64;

                // Sine wave gate: 10 FPS when wave passes, frame 0 otherwise
                let wave_pos =
                    elapsed * 0.15 + gx as f64 * 0.06 + gy as f64 * 0.04 + (gx ^ gy) as f64 * 0.01;
                let frame = if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
                    ((elapsed * 10.0) as u32) % frame_count
                } else {
                    0
                };
                let sx = frame as f64 * fw;

                let dx = (gx as f64) * ts;
                let dy = (gy as f64) * ts;

                if tile_flip(gx, gy) {
                    if let Some((flipped, _, _)) = loaded.bush_textures_flipped.get(variant_idx) {
                        let sheet_w = frame_count as f64 * fw;
                        let flipped_sx = sheet_w - sx - fw;
                        ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            flipped, flipped_sx, 0.0, fw, fh, dx, dy, ts, ts,
                        )?;
                    }
                } else {
                    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, sx, 0.0, fw, fh, dx, dy, ts, ts,
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Draw rocks as a ground-level pass (always behind units).
fn draw_rocks(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    if loaded.rock_textures.is_empty() {
        return Ok(());
    }
    let ts = TILE_SIZE as f64;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let variant_idx = (gx.wrapping_mul(13).wrapping_add(gy.wrapping_mul(29))) as usize
                % loaded.rock_textures.len();
            let tex_id = loaded.rock_textures[variant_idx];

            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let dx = (gx as f64) * ts;
                let dy = (gy as f64) * ts;

                if tile_flip(gx, gy) {
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
    }
    Ok(())
}

/// A drawable entity for Y-sorted rendering.
enum Drawable {
    Unit(usize),         // index into game.units
    Tree(u32, u32),      // (gx, gy)
    WaterRock(u32, u32), // (gx, gy)
    Building(u8),        // zone index (tower)
    BaseBuilding(usize), // index into game.buildings
    Particle(usize),     // index into game.particles
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
    elapsed: f64,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    // Player position for tree transparency
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));

    // Collect all drawable entities with their Y-sort key (foot position)
    let mut drawables: Vec<(f64, Drawable)> = Vec::new();

    // Units (hide enemies outside friendly line of sight)
    for (i, u) in game.units.iter().enumerate() {
        let alive_or_fading = if animator.is_playing() {
            animator.is_visually_alive(u.id) || u.death_fade > 0.0
        } else {
            u.alive || u.death_fade > 0.0
        };
        if !alive_or_fading {
            continue;
        }
        // Hide enemy units on non-visible tiles
        if u.faction != Faction::Blue {
            let (gx, gy) = u.grid_cell();
            let idx = (gy * game.grid.width + gx) as usize;
            if !game.visible[idx] {
                continue;
            }
        }
        drawables.push((u.y as f64 + ts * 0.5, Drawable::Unit(i)));
    }

    // Trees and rocks (visible range only)
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts;
            if tile == TileKind::Forest && !loaded.tree_textures.is_empty() {
                drawables.push((foot_y, Drawable::Tree(gx, gy)));
            }
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

    // Production buildings at faction bases
    for (i, b) in game.buildings.iter().enumerate() {
        let foot_y = (b.grid_y as f64 + 1.0) * ts;
        drawables.push((foot_y, Drawable::BaseBuilding(i)));
    }

    // Particles
    for (i, _) in game.particles.iter().enumerate() {
        drawables.push((
            game.particles[i].world_y as f64 + ts * 0.5,
            Drawable::Particle(i),
        ));
    }

    // Sort by Y (foot position), then by X for stability
    drawables.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

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
                    // Archer idle uses wind wave pattern to sync with trees/bushes
                    let anim_frame =
                        if unit.kind == UnitKind::Archer && unit.current_anim == UnitAnim::Idle {
                            let (gx, gy) = unit.grid_cell();
                            let wave_pos = elapsed * 0.15
                                + gx as f64 * 0.06
                                + gy as f64 * 0.04
                                + (gx ^ gy) as f64 * 0.01;
                            if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
                                ((elapsed * 10.0) as u32) % unit.animation.frame_count
                            } else {
                                0
                            }
                        } else {
                            unit.animation.current_frame
                        };
                    let (sx, sy, sw, sh) = sheet.frame_src_rect(anim_frame);
                    let sprite_size = unit.kind.frame_size() as f64;

                    let opacity = if !unit.alive {
                        (unit.death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0) as f64
                    } else if unit.hit_flash > 0.0 && (unit.hit_flash * 30.0) as i32 % 2 == 0 {
                        0.3
                    } else {
                        1.0
                    };

                    let dx = (unit.x as f64) - sprite_size / 2.0;
                    let dy = (unit.y as f64) - sprite_size / 2.0;

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

            Drawable::Tree(gx, gy) => {
                let variant_idx = (gx.wrapping_mul(31).wrapping_add(gy.wrapping_mul(17))) as usize
                    % loaded.tree_textures.len();
                let (tex_id, frame_w, frame_h) = loaded.tree_textures[variant_idx];

                if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                    let fw = frame_w as f64;
                    let fh = frame_h as f64;

                    // Sine wave gate: 10 FPS when wave passes, frame 0 otherwise
                    let wave_pos = elapsed * 0.15
                        + *gx as f64 * 0.06
                        + *gy as f64 * 0.04
                        + (*gx ^ *gy) as f64 * 0.01;
                    let frame = if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
                        ((elapsed * 10.0) as u32) % frame_count
                    } else {
                        0
                    };
                    let sx = frame as f64 * fw;

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
                        let fade_end = ts * 1.0; // fully transparent at 1 tile
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
                        if let Some((flipped, _, _)) = loaded.tree_textures_flipped.get(variant_idx)
                        {
                            let sheet_w = frame_count as f64 * fw;
                            let flipped_sx = sheet_w - sx - fw;
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, flipped_sx, 0.0, fw, fh, dx, dy, draw_w, draw_h,
                            )?;
                        }
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, 0.0, fw, fh, dx, dy, draw_w, draw_h,
                        )?;
                    }

                    if (alpha - 1.0).abs() > 0.001 {
                        ctx.set_global_alpha(1.0);
                    }
                }
            }

            Drawable::WaterRock(gx, gy) => {
                let variant_idx = (gx.wrapping_mul(37).wrapping_add(gy.wrapping_mul(19))) as usize
                    % loaded.water_rock_textures.len();
                let (tex_id, frame_w, frame_h) = loaded.water_rock_textures[variant_idx];

                if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                    let fw = frame_w as f64;
                    let fh = frame_h as f64;

                    // Sine wave gate: 10 FPS when wave passes, frame 0 otherwise
                    let wave_pos = elapsed * 0.2
                        + *gx as f64 * 0.06
                        + *gy as f64 * 0.04
                        + (*gx ^ *gy) as f64 * 0.01;
                    let frame = if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
                        ((elapsed * 10.0) as u32) % frame_count
                    } else {
                        0
                    };
                    let sx = frame as f64 * fw;

                    let dx = (*gx as f64) * ts;
                    let dy = (*gy as f64) * ts;

                    if tile_flip(*gx, *gy) {
                        if let Some((flipped, _, _)) =
                            loaded.water_rock_textures_flipped.get(variant_idx)
                        {
                            let sheet_w = frame_count as f64 * fw;
                            let flipped_sx = sheet_w - sx - fw;
                            ctx.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                flipped, flipped_sx, 0.0, fw, fh, dx, dy, ts, ts,
                            )?;
                        }
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, 0.0, fw, fh, dx, dy, ts, ts,
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
                    ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
                    ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 2,
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

            Drawable::BaseBuilding(idx) => {
                if !loaded.building_textures.is_empty() {
                    let b = &game.buildings[*idx];
                    let kind_index = match b.kind {
                        BuildingKind::Barracks => 0,
                        BuildingKind::Archery => 1,
                        BuildingKind::Monastery => 2,
                    };
                    let faction_index = match b.faction {
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
                            let dx = (b.grid_x as f64) * ts + ts / 2.0 - draw_w / 2.0;
                            let dy = (b.grid_y as f64) * ts + ts - draw_h;
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, 0.0, 0.0, sw, sh, dx, dy, draw_w, draw_h,
                            )?;
                        }
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
            } else {
                // Outside line of sight — dim fog, softer near visible tiles
                let vis_n = visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                let base = 140i32; // ~0.55 * 255
                (base - (vis_n as i32) * 15).max(38) as u8
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
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
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

/// Chunk size in tiles. Each chunk is CHUNK_TILES × CHUNK_TILES tiles,
/// rendered to a (CHUNK_TILES * 64) × (CHUNK_TILES * 64) pixel canvas.
/// 32 tiles → 2048×2048 px (~4MP, well within browser canvas limits).
const CHUNK_TILES: u32 = 32;

/// Chunk-based terrain cache: the grid is divided into chunks, each with
/// its own small offscreen canvas. Only visible chunks are drawn per frame.
struct TerrainChunks {
    /// Chunk canvases stored row-major: chunks[cy * cols + cx]
    canvases: Vec<web_sys::HtmlCanvasElement>,
    contexts: Vec<web_sys::CanvasRenderingContext2d>,
    /// Whether each chunk needs re-rendering
    dirty: Vec<bool>,
    /// Number of chunks in each dimension
    cols: u32,
    rows: u32,
}

impl TerrainChunks {
    fn new(document: &web_sys::Document, grid_w: u32, grid_h: u32) -> Result<Self, JsValue> {
        let cols = (grid_w + CHUNK_TILES - 1) / CHUNK_TILES;
        let rows = (grid_h + CHUNK_TILES - 1) / CHUNK_TILES;
        let count = (cols * rows) as usize;
        let chunk_px = CHUNK_TILES * (TILE_SIZE as u32);

        let mut canvases = Vec::with_capacity(count);
        let mut contexts = Vec::with_capacity(count);

        for _ in 0..count {
            let c = document
                .create_element("canvas")?
                .dyn_into::<web_sys::HtmlCanvasElement>()?;
            c.set_width(chunk_px);
            c.set_height(chunk_px);
            let ctx = c
                .get_context("2d")?
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
            canvases.push(c);
            contexts.push(ctx);
        }

        Ok(Self {
            canvases,
            contexts,
            dirty: vec![true; count],
            cols,
            rows,
        })
    }

    fn mark_all_dirty(&mut self) {
        for d in &mut self.dirty {
            *d = true;
        }
    }
}

struct LoopState {
    canvas2d: Canvas2d,
    canvas_element: web_sys::HtmlCanvasElement,
    game: Game,
    texture_manager: TextureManager,
    last_time: Option<f64>,
    elapsed: f64,
    fog_canvas: web_sys::HtmlCanvasElement,
    fog_ctx: web_sys::CanvasRenderingContext2d,
    terrain_chunks: TerrainChunks,
    terrain_dirty: bool,
    animator: TurnAnimator,
    /// Pre-rendered minimap terrain (1 pixel per tile, static after generation).
    minimap_terrain: web_sys::HtmlCanvasElement,
    /// Last known CSS size for resize detection.
    last_css_w: u32,
    last_css_h: u32,
    /// Current game screen / state machine node.
    screen: GameScreen,
    /// Seed used to generate the current map (for retry).
    current_seed: u32,
    /// Clickable buttons populated by overlay draw functions each frame.
    overlay_buttons: Vec<OverlayButton>,
    /// Delay timer before showing death/end overlays (lets effects finish).
    overlay_delay: f32,
    /// Cached HUD container element for show/hide toggling.
    hud_container: Option<web_sys::HtmlElement>,
}
