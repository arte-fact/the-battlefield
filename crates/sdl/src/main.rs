#![allow(clippy::too_many_arguments)]

#[cfg(target_os = "emscripten")]
mod emscripten;
mod input;
mod renderer;

use battlefield_core::game::Game;
use battlefield_core::grid::{GRID_SIZE, TILE_SIZE};
use battlefield_core::particle::Particle;
use battlefield_core::ui::ButtonAction;
use battlefield_core::unit::Faction;
use input::InputState;
use renderer::GameScreen;
use sdl2::controller::Button;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use std::time::Instant;

const WINDOW_W: u32 = 960;
const WINDOW_H: u32 = 640;

// ---------------------------------------------------------------------------
// GameLoop: holds all per-frame state so it can be driven by either a
// blocking loop (native) or a callback (Emscripten).
// ---------------------------------------------------------------------------

struct GameLoop {
    canvas: Canvas<Window>,
    texture_creator: &'static TextureCreator<WindowContext>,
    event_pump: sdl2::EventPump,
    game_controller_subsystem: sdl2::GameControllerSubsystem,
    active_controller: Option<sdl2::controller::GameController>,

    game: Game,
    assets: renderer::Assets<'static>,
    input_state: InputState,
    screen: GameScreen,
    seed: u32,
    player_was_alive: bool,
    dpi_scale: f64,
    /// Actual device pixel ratio (for touch button sizing on mobile).
    touch_dpr: f32,

    last_time: Instant,
    start_time: Instant,

    // Frame profiler (enabled by PERF_PROFILE=1 on native)
    profiling: bool,
    prof_tick_us: Vec<u128>,
    prof_update_us: Vec<u128>,
    prof_render_us: Vec<u128>,
    prof_frame_us: Vec<u128>,
    prof_last_report: Instant,
    prof_interval: std::time::Duration,
}

impl GameLoop {
    /// Run one frame. Returns `false` when the game should exit (native only).
    fn step(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64().min(0.1);
        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        self.last_time = now;

        self.input_state.begin_frame();

        // Poll events
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    #[cfg(not(target_os = "emscripten"))]
                    {
                        return false;
                    }
                }
                Event::ControllerDeviceAdded { which, .. } => {
                    if self.active_controller.is_none() {
                        if let Ok(gc) = self.game_controller_subsystem.open(which) {
                            log::info!("Controller connected: {}", gc.name());
                            self.input_state.gamepad_connected = true;
                            self.active_controller = Some(gc);
                        }
                    }
                }
                Event::ControllerDeviceRemoved { .. } => {
                    self.active_controller = None;
                    self.input_state.gamepad_connected = false;
                    log::info!("Controller disconnected");
                }
                Event::ControllerAxisMotion { .. }
                | Event::ControllerButtonDown { .. }
                | Event::ControllerButtonUp { .. } => {
                    self.input_state.handle_controller_event(&event);
                }
                _ => {}
            }
            self.input_state.handle_event(&event);
        }

        // Handle window resize and DPI changes
        #[cfg(target_os = "emscripten")]
        {
            let (vw, vh, dpr) = emscripten::viewport_size_device_pixels();
            let (cur_sdl_w, _) = self.canvas.window().size();
            if vw != cur_sdl_w {
                let _ = self.canvas.window_mut().set_size(vw, vh);
            }
            // On Emscripten, the canvas is already at device-pixel resolution.
            // Text/HUD sizes are designed for the canvas pixel space, so dpi_scale = 1.0.
            // Touch buttons need actual DPR to be finger-sized in CSS pixels.
            self.dpi_scale = 1.0;
            self.touch_dpr = dpr as f32;
        }
        let (cur_w, cur_h) = self.canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
        if cur_w as f32 != self.game.camera.viewport_w
            || cur_h as f32 != self.game.camera.viewport_h
        {
            self.game.camera.resize(cur_w as f32, cur_h as f32);
            self.game.camera.zoom = self.game.camera.ideal_zoom();
            #[cfg(not(target_os = "emscripten"))]
            {
                let (lw, _lh) = self.canvas.window().size();
                if lw > 0 {
                    self.dpi_scale = cur_w as f64 / lw as f64;
                    self.touch_dpr = self.dpi_scale as f32;
                }
            }
        }

        // Update touch input with current canvas dimensions and layout
        self.input_state.set_canvas_size(cur_w as f32, cur_h as f32);
        self.input_state
            .update_layout(cur_w as f32, cur_h as f32, self.touch_dpr);

        // Screen transition logic
        let keyboard = self.event_pump.keyboard_state();
        match self.screen {
            GameScreen::MainMenu => {
                if keyboard.is_scancode_pressed(Scancode::Return)
                    || keyboard.is_scancode_pressed(Scancode::Space)
                    || self.input_state.gamepad_pressed(Button::A)
                    || self.input_state.gamepad_pressed(Button::Start)
                {
                    self.screen = GameScreen::Playing;
                    log::info!("Game started");
                }
            }
            GameScreen::Playing => {
                if self.input_state.pressed_this_frame(Scancode::Escape)
                    || self.input_state.gamepad_pressed(Button::Back)
                {
                    #[cfg(not(target_os = "emscripten"))]
                    {
                        return false;
                    }
                    #[cfg(target_os = "emscripten")]
                    {
                        self.screen = GameScreen::MainMenu;
                        return true;
                    }
                }

                // Camera zoom from scroll wheel
                let scroll = self.input_state.scroll_delta;
                if scroll.abs() > f32::EPSILON {
                    self.game.camera.zoom_by(scroll);
                }

                // Camera zoom from gamepad triggers
                let gp_zoom = self.input_state.gamepad_zoom_delta();
                if gp_zoom.abs() > 0.01 {
                    self.game.camera.zoom_by(gp_zoom * dt as f32 * 3.0);
                }

                // Touch: pinch-to-zoom
                let pinch = self.input_state.take_pinch_zoom();
                if pinch.abs() > f32::EPSILON {
                    self.game.camera.zoom_by(pinch);
                }

                // Touch: two-finger pan
                let (pan_tx, pan_ty) = self.input_state.take_touch_pan();
                if pan_tx.abs() > f32::EPSILON || pan_ty.abs() > f32::EPSILON {
                    self.game.camera.x -= pan_tx / self.game.camera.zoom;
                    self.game.camera.y -= pan_ty / self.game.camera.zoom;
                }

                // Touch: single-finger camera drag
                let (drag_dx, drag_dy) = self.input_state.take_camera_drag();
                if drag_dx.abs() > f32::EPSILON || drag_dy.abs() > f32::EPSILON {
                    self.game.camera.x -= drag_dx / self.game.camera.zoom;
                    self.game.camera.y -= drag_dy / self.game.camera.zoom;
                }

                // Build input and tick game
                let player_input = self.input_state.build_player_input(&keyboard);

                if self.game.winner.is_none() {
                    let t0 = Instant::now();
                    self.game.tick(&player_input, dt as f32);
                    if self.profiling {
                        self.prof_tick_us.push(t0.elapsed().as_micros());
                    }

                    if player_input.attack && self.game.player_attack() {
                        if let Some(ref mut gc) = self.active_controller {
                            let _ = gc.set_rumble(0x4000, 0x8000, 80);
                        }
                    }
                    if player_input.order_follow {
                        self.game.recruit_units();
                        self.game.issue_order("follow");
                        if let Some(ref mut gc) = self.active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                    if player_input.order_charge {
                        self.game.recruit_units();
                        self.game.issue_order("charge");
                        if let Some(ref mut gc) = self.active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                    if player_input.order_defend {
                        self.game.recruit_units();
                        self.game.issue_order("defend");
                        if let Some(ref mut gc) = self.active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                }

                // Process turn events -> spawn particles
                if !self.game.turn_events.is_empty() {
                    let events = self.game.turn_events.drain(..).collect::<Vec<_>>();
                    for event in &events {
                        if let battlefield_core::animation::TurnEvent::Move { from, .. } = event {
                            self.game.particles.push(Particle::new(
                                battlefield_core::particle::ParticleKind::Dust,
                                from.0,
                                from.1,
                            ));
                        }
                    }
                }

                // Update animations, particles, camera follow
                let t0 = Instant::now();
                self.game.update(dt);
                if self.profiling {
                    self.prof_update_us.push(t0.elapsed().as_micros());
                }

                // Clamp camera
                let world_size = GRID_SIZE as f32 * TILE_SIZE;
                self.game.camera.clamp_to_world(world_size, world_size);

                // Check for player death
                let player_alive = self.game.player_unit().is_some();
                if self.player_was_alive && !player_alive {
                    self.screen = GameScreen::PlayerDeath;
                    log::info!("Player died");
                }
                self.player_was_alive = player_alive;

                // Check for game end
                if let Some(winner) = self.game.winner {
                    if winner == Faction::Blue {
                        self.screen = GameScreen::GameWon;
                    } else {
                        self.screen = GameScreen::GameLost;
                    }
                    log::info!("Game ended: {:?} wins", winner);
                }
            }
            GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                if self.input_state.pressed_this_frame(Scancode::Escape)
                    || self.input_state.gamepad_pressed(Button::Back)
                {
                    #[cfg(not(target_os = "emscripten"))]
                    {
                        return false;
                    }
                    #[cfg(target_os = "emscripten")]
                    {
                        self.screen = GameScreen::MainMenu;
                        return true;
                    }
                }

                // Still update animations/particles for visual continuity
                self.game.update(dt);

                // Enter/A = retry (same seed), Space/X = new game
                if self.input_state.pressed_this_frame(Scancode::Return)
                    || self.input_state.gamepad_pressed(Button::A)
                {
                    self.game = Game::new(cur_w as f32, cur_h as f32);
                    self.game.setup_demo_battle_with_seed(self.seed);
                    self.screen = GameScreen::Playing;
                    self.player_was_alive = true;
                    log::info!("Retrying with seed {}", self.seed);
                } else if self.input_state.pressed_this_frame(Scancode::Space)
                    || self.input_state.gamepad_pressed(Button::X)
                {
                    self.seed = generate_seed();
                    self.game = Game::new(cur_w as f32, cur_h as f32);
                    self.game.setup_demo_battle_with_seed(self.seed);
                    self.screen = GameScreen::Playing;
                    self.player_was_alive = true;
                    log::info!("New game with seed {}", self.seed);
                }
            }
        }

        // Render
        let t0 = Instant::now();
        let buttons = renderer::render_frame(
            &mut self.canvas,
            self.texture_creator,
            &self.game,
            &mut self.assets,
            self.screen,
            elapsed,
            self.input_state.mouse_x,
            self.input_state.mouse_y,
            self.input_state.focused_button,
            self.input_state.gamepad_connected,
            self.dpi_scale,
            &self.input_state,
            self.touch_dpr as f64,
        );

        if self.profiling {
            self.prof_render_us.push(t0.elapsed().as_micros());
        }

        // Frame profiling report
        if self.profiling {
            self.prof_frame_us.push(now.elapsed().as_micros());
            if self.prof_last_report.elapsed() >= self.prof_interval
                && !self.prof_frame_us.is_empty()
            {
                let n = self.prof_frame_us.len();
                let avg = |v: &[u128]| -> f64 {
                    if v.is_empty() {
                        0.0
                    } else {
                        v.iter().sum::<u128>() as f64 / v.len() as f64
                    }
                };
                let p95 = |v: &mut Vec<u128>| -> u128 {
                    v.sort();
                    v.get(v.len() * 95 / 100).copied().unwrap_or(0)
                };
                let max = |v: &[u128]| -> u128 { v.iter().copied().max().unwrap_or(0) };
                let fps = 1_000_000.0 / avg(&self.prof_frame_us);
                eprintln!("--- PERF ({n} frames, {fps:.0} FPS) ---");
                eprintln!(
                    "  tick:   avg {:.0}us  p95 {}us  max {}us",
                    avg(&self.prof_tick_us),
                    p95(&mut self.prof_tick_us),
                    max(&self.prof_tick_us)
                );
                eprintln!(
                    "  update: avg {:.0}us  p95 {}us  max {}us",
                    avg(&self.prof_update_us),
                    p95(&mut self.prof_update_us),
                    max(&self.prof_update_us)
                );
                eprintln!(
                    "  render: avg {:.0}us  p95 {}us  max {}us",
                    avg(&self.prof_render_us),
                    p95(&mut self.prof_render_us),
                    max(&self.prof_render_us)
                );
                eprintln!(
                    "  frame:  avg {:.0}us  p95 {}us  max {}us",
                    avg(&self.prof_frame_us),
                    p95(&mut self.prof_frame_us),
                    max(&self.prof_frame_us)
                );
                let budget = 16670.0;
                let pct = avg(&self.prof_frame_us) / budget * 100.0;
                eprintln!("  budget: {pct:.1}% of 16.67ms (60fps)");
                eprintln!(
                    "  units:  {} alive",
                    self.game.units.iter().filter(|u| u.alive).count()
                );
                self.prof_tick_us.clear();
                self.prof_update_us.clear();
                self.prof_render_us.clear();
                self.prof_frame_us.clear();
                self.prof_last_report = Instant::now();
            }
        }

        // Handle mouse click on overlay buttons
        if self.input_state.mouse_clicked {
            for btn in &buttons {
                if btn.contains(self.input_state.mouse_x, self.input_state.mouse_y) {
                    handle_button_action(
                        btn.action,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        "click",
                    );
                    break;
                }
            }
        }

        // D-Pad navigation for menu button focus
        if !buttons.is_empty() {
            if self.input_state.gamepad_pressed(Button::DPadRight)
                || self.input_state.gamepad_pressed(Button::DPadDown)
            {
                self.input_state.focused_button =
                    (self.input_state.focused_button + 1) % buttons.len();
            }
            if self.input_state.gamepad_pressed(Button::DPadLeft)
                || self.input_state.gamepad_pressed(Button::DPadUp)
            {
                self.input_state.focused_button = if self.input_state.focused_button == 0 {
                    buttons.len() - 1
                } else {
                    self.input_state.focused_button - 1
                };
            }
            self.input_state.focused_button =
                self.input_state.focused_button.min(buttons.len() - 1);

            if self.screen != GameScreen::Playing && self.input_state.gamepad_pressed(Button::A) {
                handle_button_action(
                    buttons[self.input_state.focused_button].action,
                    &mut self.screen,
                    &mut self.game,
                    &mut self.seed,
                    &mut self.player_was_alive,
                    cur_w,
                    cur_h,
                    "gamepad",
                );
            }
        }

        true
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    #[cfg(not(target_os = "emscripten"))]
    env_logger::init();

    log::info!("The Battlefield -- SDL2 starting up");

    let sdl = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video init failed");
    let game_controller_subsystem = sdl.game_controller().expect("controller subsystem failed");

    // On Emscripten, use device-pixel-resolution (CSS size * DPR) for sharp rendering.
    #[cfg(target_os = "emscripten")]
    let (init_w, init_h, em_dpr) = emscripten::viewport_size_device_pixels();
    #[cfg(not(target_os = "emscripten"))]
    let (init_w, init_h) = (WINDOW_W, WINDOW_H);

    let window = {
        let mut wb = video.window("The Battlefield", init_w, init_h);
        wb.resizable();
        #[cfg(not(target_os = "emscripten"))]
        {
            wb.position_centered();
            wb.allow_highdpi();
        }
        wb.build().expect("Window creation failed")
    };

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .expect("Canvas creation failed");

    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);

    // Nearest-neighbor scaling for pixel art sprites (no blurring)
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");

    // Leak the TextureCreator so Assets can have 'static lifetime,
    // needed for the Emscripten callback pattern. The TextureCreator
    // lives for the entire program duration anyway.
    let texture_creator: &'static TextureCreator<WindowContext> =
        Box::leak(Box::new(canvas.texture_creator()));
    let assets = renderer::Assets::load(texture_creator);
    log::info!("Assets loaded");

    // Compute DPI scale factor
    let (output_w, output_h) = canvas.output_size().unwrap_or((init_w, init_h));
    #[cfg(not(target_os = "emscripten"))]
    let (dpi_scale, touch_dpr) = {
        let (logical_w, _logical_h) = canvas.window().size();
        let s = if logical_w > 0 {
            output_w as f64 / logical_w as f64
        } else {
            1.0
        };
        (s, s as f32)
    };
    // On Emscripten, the canvas is already at device-pixel resolution.
    // Text/HUD rendering uses dpi_scale=1.0; touch buttons use actual DPR.
    #[cfg(target_os = "emscripten")]
    let (dpi_scale, touch_dpr) = (1.0_f64, em_dpr as f32);
    log::info!("DPI scale: {dpi_scale} (output {output_w}x{output_h})");

    // Initialize game
    let (w, h) = (output_w, output_h);
    let mut game = Game::new(w as f32, h as f32);
    let seed = generate_seed();
    game.setup_demo_battle_with_seed(seed);
    log::info!("Game initialized ({}x{} grid)", GRID_SIZE, GRID_SIZE);

    let mut input_state = InputState::new();

    // Try to open the first available game controller
    let mut active_controller: Option<sdl2::controller::GameController> = None;
    for i in 0..game_controller_subsystem.num_joysticks().unwrap_or(0) {
        if game_controller_subsystem.is_game_controller(i) {
            if let Ok(gc) = game_controller_subsystem.open(i) {
                log::info!("Controller connected: {}", gc.name());
                input_state.gamepad_connected = true;
                active_controller = Some(gc);
                break;
            }
        }
    }

    let event_pump = sdl.event_pump().expect("Event pump failed");

    // Frame profiler (enabled by PERF_PROFILE=1 on native)
    #[cfg(not(target_os = "emscripten"))]
    let profiling = std::env::var("PERF_PROFILE").is_ok();
    #[cfg(target_os = "emscripten")]
    let profiling = false;

    let now = Instant::now();

    #[allow(unused_mut)]
    let mut game_loop = GameLoop {
        canvas,
        texture_creator,
        event_pump,
        game_controller_subsystem,
        active_controller,
        game,
        assets,
        input_state,
        screen: GameScreen::MainMenu,
        seed,
        player_was_alive: true,
        dpi_scale,
        touch_dpr,
        last_time: now,
        start_time: now,
        profiling,
        prof_tick_us: Vec::new(),
        prof_update_us: Vec::new(),
        prof_render_us: Vec::new(),
        prof_frame_us: Vec::new(),
        prof_last_report: now,
        prof_interval: std::time::Duration::from_secs(3),
    };

    #[cfg(not(target_os = "emscripten"))]
    {
        loop {
            if !game_loop.step() {
                break;
            }
        }
        log::info!("Shutting down");
    }

    #[cfg(target_os = "emscripten")]
    {
        let raw = Box::into_raw(Box::new(game_loop));
        unsafe {
            emscripten::emscripten_set_main_loop_arg(
                em_frame_callback,
                raw as *mut std::ffi::c_void,
                0, // let the browser use requestAnimationFrame
                1, // simulate_infinite_loop (never returns)
            );
        }
    }
}

#[cfg(target_os = "emscripten")]
extern "C" fn em_frame_callback(arg: *mut std::ffi::c_void) {
    let game_loop = unsafe { &mut *(arg as *mut GameLoop) };
    game_loop.step();
}

fn generate_seed() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as u32)
        .unwrap_or(42)
}

fn handle_button_action(
    action: ButtonAction,
    screen: &mut GameScreen,
    game: &mut Game,
    seed: &mut u32,
    player_was_alive: &mut bool,
    viewport_w: u32,
    viewport_h: u32,
    source: &str,
) {
    match action {
        ButtonAction::Play => {
            *screen = GameScreen::Playing;
            log::info!("Game started ({source})");
        }
        ButtonAction::Retry => {
            *game = Game::new(viewport_w as f32, viewport_h as f32);
            game.setup_demo_battle_with_seed(*seed);
            *screen = GameScreen::Playing;
            *player_was_alive = true;
            log::info!("Retrying with seed {} ({source})", *seed);
        }
        ButtonAction::NewGame => {
            *seed = generate_seed();
            *game = Game::new(viewport_w as f32, viewport_h as f32);
            game.setup_demo_battle_with_seed(*seed);
            *screen = GameScreen::Playing;
            *player_was_alive = true;
            log::info!("New game with seed {} ({source})", *seed);
        }
    }
}
