mod assets;
mod environment;
mod fog;
mod foreground;
mod helpers;
mod hud;
mod render;
mod screens;
mod terrain;
mod touch;

use crate::input::Input;
use crate::renderer::{Canvas2dRenderer, Renderer};
use battlefield_core::animation::TurnAnimator;
use battlefield_core::game::Game;
use battlefield_core::grid::{GRID_SIZE, TILE_SIZE};
use battlefield_core::particle::Particle;
use battlefield_core::unit::{Facing, Faction};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use assets::{load_textures, LoadedTextures};
use helpers::{haptic, HudElements};
use hud::render_minimap_terrain;
use render::render_frame;
use screens::{GameScreen, OverlayAction, OverlayButton};
use terrain::TerrainChunks;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

pub(in crate::game_loop) struct LoopState {
    pub(in crate::game_loop) renderer: Canvas2dRenderer,
    pub(in crate::game_loop) canvas_element: web_sys::HtmlCanvasElement,
    pub(in crate::game_loop) game: Game,
    pub(in crate::game_loop) last_time: Option<f64>,
    pub(in crate::game_loop) elapsed: f64,
    pub(in crate::game_loop) fog_canvas: web_sys::HtmlCanvasElement,
    pub(in crate::game_loop) fog_ctx: web_sys::CanvasRenderingContext2d,
    pub(in crate::game_loop) terrain_chunks: TerrainChunks,
    pub(in crate::game_loop) terrain_dirty: bool,
    pub(in crate::game_loop) animator: TurnAnimator,
    /// Pre-rendered minimap terrain (1 pixel per tile, static after generation).
    pub(in crate::game_loop) minimap_terrain: web_sys::HtmlCanvasElement,
    /// Last known CSS size for resize detection.
    pub(in crate::game_loop) last_css_w: u32,
    pub(in crate::game_loop) last_css_h: u32,
    /// Current game screen / state machine node.
    pub(in crate::game_loop) screen: GameScreen,
    /// Seed used to generate the current map (for retry).
    pub(in crate::game_loop) current_seed: u32,
    /// Clickable buttons populated by overlay draw functions each frame.
    pub(in crate::game_loop) overlay_buttons: Vec<OverlayButton>,
    /// Delay timer before showing death/end overlays (lets effects finish).
    pub(in crate::game_loop) overlay_delay: f32,
    /// Cached HUD container element for show/hide toggling.
    pub(in crate::game_loop) hud_container: Option<web_sys::HtmlElement>,
}

pub fn run(
    renderer: Canvas2dRenderer,
    game: Game,
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
        renderer,
        canvas_element: canvas.clone(),
        game,
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
    type RafClosure = Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>>;
    let f: RafClosure = Rc::new(RefCell::new(None));
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
                    let dpr = state_guard.renderer.dpr() as f32;
                    let canvas_w = (css_w as f32 * dpr) as u32;
                    let canvas_h = (css_h as f32 * dpr) as u32;
                    state_guard.canvas_element.set_width(canvas_w);
                    state_guard.canvas_element.set_height(canvas_h);
                    state_guard.renderer.set_width(canvas_w as f64);
                    state_guard.renderer.set_height(canvas_h as f64);
                    state_guard.renderer.set_image_smoothing(false);
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
                                request_fullscreen();
                            }
                            OverlayAction::Retry => {
                                let seed = state_guard.current_seed;
                                let vw = state_guard.renderer.width() as f32;
                                let vh = state_guard.renderer.height() as f32;
                                restart_game(&mut state_guard, seed, vw, vh);
                                input.borrow_mut().clear_all();
                            }
                            OverlayAction::NewGame => {
                                let seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
                                let vw = state_guard.renderer.width() as f32;
                                let vh = state_guard.renderer.height() as f32;
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
                            request_fullscreen();
                        }
                    }
                    GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                        // Enter = retry same map, Space = new game
                        if enter {
                            let seed = state_guard.current_seed;
                            let vw = state_guard.renderer.width() as f32;
                            let vh = state_guard.renderer.height() as f32;
                            inp.clear_all();
                            drop(inp);
                            restart_game(&mut state_guard, seed, vw, vh);
                            input.borrow_mut().clear_all();
                        } else if space {
                            let seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
                            let vw = state_guard.renderer.width() as f32;
                            let vh = state_guard.renderer.height() as f32;
                            inp.clear_all();
                            drop(inp);
                            restart_game(&mut state_guard, seed, vw, vh);
                            input.borrow_mut().clear_all();
                        }
                    }
                    GameScreen::Playing => unreachable!(),
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

                    // Aim lock: Ctrl key locks aim direction and facing
                    let aim_lock = inp.is_key_down("Control");

                    // Keyboard movement (WASD/ZQSD + arrows)
                    let (move_dx, move_dy) = inp.movement_direction();
                    if move_dx != 0.0 || move_dy != 0.0 {
                        if !aim_lock {
                            game.player_aim_dir = move_dy.atan2(move_dx);
                        }
                        game.try_player_move(move_dx, move_dy, dt as f32);
                    }

                    // Virtual joystick movement (mobile)
                    let (joy_dx, joy_dy) = (inp.joystick.dx, inp.joystick.dy);
                    if joy_dx.abs() > 0.01 || joy_dy.abs() > 0.01 {
                        if !aim_lock {
                            game.player_aim_dir = joy_dy.atan2(joy_dx);
                        }
                        game.try_player_move(joy_dx, joy_dy, dt as f32);
                    }

                    // Update player facing from aim direction (skip when attacking)
                    if !aim_lock {
                        let aim_cos = game.player_aim_dir.cos();
                        if let Some(player) = game.player_unit_mut() {
                            if aim_cos > 0.01 {
                                player.facing = Facing::Right;
                            } else if aim_cos < -0.01 {
                                player.facing = Facing::Left;
                            }
                        }
                    }

                    // Attack: keyboard (space) or touch button
                    let attack_input =
                        inp.is_key_down(" ") || inp.take_attack_key() || inp.take_attack_pressed();
                    if attack_input && game.player_attack() && inp.is_touch_device {
                        haptic(25);
                    }

                    // Recruit: R key or touch button
                    if inp.take_recruit() && game.recruit_units() > 0 && inp.is_touch_device {
                        haptic(15);
                    }

                    // Player orders: F=Follow, C=Charge, V=Defend
                    if inp.take_order_follow()
                        && game.issue_order("follow") > 0
                        && inp.is_touch_device
                    {
                        haptic(15);
                    }
                    if inp.take_order_charge()
                        && game.issue_order("charge") > 0
                        && inp.is_touch_device
                    {
                        haptic(15);
                    }
                    if inp.take_order_defend()
                        && game.issue_order("defend") > 0
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

/// Request fullscreen + wake lock via the JS helper exposed on window.
fn request_fullscreen() {
    if let Some(window) = web_sys::window() {
        let window_js: &JsValue = window.as_ref();
        if let Ok(func) = js_sys::Reflect::get(window_js, &JsValue::from_str("__requestFullscreen"))
        {
            if func.is_function() {
                let _ = js_sys::Function::from(func).call0(window_js);
            }
        }
    }
}

fn request_animation_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}
