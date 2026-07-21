use battlefield_core::game::Game;
use battlefield_core::grid::TILE_SIZE;
use battlefield_core::ui;

use crate::input::InputState;
use crate::renderer::GameScreen;

use sdl2::controller::Button;
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const WINDOW_W: u32 = 960;
pub const WINDOW_H: u32 = 640;

/// Configuration for constructing a [`GameLoop`].
pub struct GameLoopConfig {
    /// Initial HUD DPI scale factor.
    pub dpi_scale: f64,
    /// Initial touch-button DPR multiplier.
    pub touch_dpr: f32,
    /// If `true`, Escape / Quit events exit the application.
    /// If `false`, they return to the main menu (browser behaviour).
    pub quit_on_escape: bool,
    /// If `true`, `step()` recalculates DPI from SDL output_size each frame.
    /// Set to `false` on Emscripten where the caller sets DPI directly.
    pub compute_dpi: bool,
    /// Enable frame-timing profiler output.
    pub profiling: bool,
}

// ---------------------------------------------------------------------------
// GameLoop: holds all per-frame state so it can be driven by either a
// blocking loop (native) or a callback (Emscripten).
// ---------------------------------------------------------------------------

pub struct GameLoop {
    pub canvas: Canvas<Window>,
    texture_creator: &'static TextureCreator<WindowContext>,
    pub event_pump: sdl2::EventPump,
    game_controller_subsystem: sdl2::GameControllerSubsystem,
    active_controller: Option<sdl2::controller::GameController>,

    pub game: Game,
    assets: crate::renderer::Assets<'static>,
    pub input_state: InputState,
    pub screen: GameScreen,
    pub seed: u32,
    player_was_alive: bool,
    pub ui: ui::UiState,
    pub dpi_scale: f64,
    /// Actual device pixel ratio (for touch button sizing on mobile).
    pub touch_dpr: f32,

    quit_on_escape: bool,
    compute_dpi: bool,

    last_time: Instant,
    start_time: Instant,

    // Frame profiler
    profiling: bool,
    prof_tick_us: Vec<u128>,
    prof_update_us: Vec<u128>,
    prof_render_us: Vec<u128>,
    prof_frame_us: Vec<u128>,
    prof_last_report: Instant,
    prof_interval: std::time::Duration,
}

impl GameLoop {
    /// Create a new game loop with the given SDL resources and configuration.
    pub fn new(
        canvas: Canvas<Window>,
        texture_creator: &'static TextureCreator<WindowContext>,
        event_pump: sdl2::EventPump,
        game_controller_subsystem: sdl2::GameControllerSubsystem,
        config: GameLoopConfig,
    ) -> Self {
        let assets = crate::renderer::Assets::load(texture_creator);
        log::info!("Assets loaded");

        let (output_w, output_h) = canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
        let mut game = Game::new(output_w as f32, output_h as f32);
        let seed = generate_seed();
        game.setup_demo_battle_with_seed(seed);
        log::info!(
            "Game initialized ({}x{} grid)",
            game.grid.width,
            game.grid.height
        );

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

        log::info!(
            "DPI scale: {} (output {}x{})",
            config.dpi_scale,
            output_w,
            output_h
        );

        let now = Instant::now();

        GameLoop {
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
            ui: ui::UiState::default(),
            dpi_scale: config.dpi_scale,
            touch_dpr: config.touch_dpr,
            quit_on_escape: config.quit_on_escape,
            compute_dpi: config.compute_dpi,
            last_time: now,
            start_time: now,
            profiling: config.profiling,
            prof_tick_us: Vec::new(),
            prof_update_us: Vec::new(),
            prof_render_us: Vec::new(),
            prof_frame_us: Vec::new(),
            prof_last_report: now,
            prof_interval: std::time::Duration::from_secs(3),
        }
    }

    /// Resize the SDL canvas if the dimensions have changed.
    /// Call this before [`step()`] on Emscripten to sync with the browser viewport.
    pub fn resize_if_needed(&mut self, w: u32, h: u32) {
        let (cur_w, _) = self.canvas.window().size();
        if w != cur_w {
            let _ = self.canvas.window_mut().set_size(w, h);
        }
    }

    /// Run one frame. Returns `false` when the game should exit.
    pub fn step(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64().min(0.1);
        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        self.last_time = now;

        self.input_state.begin_frame();

        // Poll events
        for event in self.event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    if self.quit_on_escape {
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
        let (cur_w, cur_h) = self.canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
        if cur_w as f32 != self.game.camera.viewport_w
            || cur_h as f32 != self.game.camera.viewport_h
        {
            self.game.camera.resize(cur_w as f32, cur_h as f32);
            if self.compute_dpi {
                let (lw, _lh) = self.canvas.window().size();
                if lw > 0 {
                    self.dpi_scale = cur_w as f64 / lw as f64;
                    self.touch_dpr = self.dpi_scale as f32;
                }
            }
            self.game.camera.zoom = self.game.camera.ideal_zoom_for_dpi(self.touch_dpr);
        }

        // Update touch input with current canvas dimensions and layout
        self.input_state.set_canvas_size(cur_w as f32, cur_h as f32);
        self.input_state
            .touch
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
                    ui::handle_button_action(
                        ui::ButtonAction::PlayFree,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "key",
                    );
                }
            }
            GameScreen::Loading => {
                // Pump budgeted generation inside a frame-time slice.
                let t0 = Instant::now();
                loop {
                    if !self.game.setup_step() {
                        ui::finish_loading(&mut self.game, &mut self.screen, &self.ui);
                        break;
                    }
                    if t0.elapsed().as_secs_f64() > 0.010 {
                        break;
                    }
                }
                self.ui.loading_progress = self.game.setup_progress();
            }
            GameScreen::Playing => {
                if self.input_state.pressed_this_frame(Scancode::Escape)
                    || self.input_state.gamepad_pressed(Button::Back)
                {
                    if self.quit_on_escape {
                        return false;
                    } else {
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
                let pinch = self.input_state.touch.take_pinch_zoom();
                if pinch.abs() > f32::EPSILON {
                    self.game.camera.zoom_by(pinch);
                }

                // Touch: two-finger pan
                let (pan_tx, pan_ty) = self.input_state.touch.take_touch_pan();
                if pan_tx.abs() > f32::EPSILON || pan_ty.abs() > f32::EPSILON {
                    self.game.camera.x -= pan_tx / self.game.camera.zoom;
                    self.game.camera.y -= pan_ty / self.game.camera.zoom;
                }

                // Touch: single-finger camera drag
                let (drag_dx, drag_dy) = self.input_state.touch.take_camera_drag();
                if drag_dx.abs() > f32::EPSILON || drag_dy.abs() > f32::EPSILON {
                    self.game.camera.x -= drag_dx / self.game.camera.zoom;
                    self.game.camera.y -= drag_dy / self.game.camera.zoom;
                }

                // Build input and tick game
                self.input_state.touch.tick(dt as f32);
                let player_input = self.input_state.build_player_input(&keyboard, dt as f32);

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
                    if let Some(req) = player_input.order {
                        self.game.issue_order(req);
                        if let Some(ref mut gc) = self.active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                }

                // Process turn events -> spawn particles
                self.game.process_turn_events();

                // Update animations, particles, camera follow
                let t0 = Instant::now();
                self.game.update(dt);
                if self.profiling {
                    self.prof_update_us.push(t0.elapsed().as_micros());
                }

                // Clamp camera
                let world_w = self.game.grid.width as f32 * TILE_SIZE;
                let world_h = self.game.grid.height as f32 * TILE_SIZE;
                self.game.camera.clamp_to_world(world_w, world_h);

                // Check for player death
                let player_alive = self.game.player_unit().is_some();
                if self.player_was_alive && !player_alive {
                    self.screen = self.end_screen(false, GameScreen::PlayerDeath);
                    log::info!("Player died");
                }
                self.player_was_alive = player_alive;

                // Check for game end
                if let Some(winner) = self.game.winner {
                    if Some(winner) == self.game.player_faction {
                        self.screen = self.end_screen(true, GameScreen::GameWon);
                    } else {
                        self.screen = self.end_screen(false, GameScreen::GameLost);
                    }
                    log::info!("Game ended: {:?} wins", winner);
                }
            }
            GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                if self.input_state.pressed_this_frame(Scancode::Escape)
                    || self.input_state.gamepad_pressed(Button::Back)
                {
                    if self.quit_on_escape {
                        return false;
                    } else {
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
                    ui::handle_button_action(
                        ui::ButtonAction::Retry,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "key",
                    );
                } else if self.input_state.pressed_this_frame(Scancode::Space)
                    || self.input_state.gamepad_pressed(Button::X)
                {
                    ui::handle_button_action(
                        ui::ButtonAction::NewGame,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "key",
                    );
                }
            }
            GameScreen::SkirmishSetup => {
                let rows = ui::SkirmishConfig::ROWS;
                if self.input_state.pressed_this_frame(Scancode::Up) {
                    self.ui.focused_row = (self.ui.focused_row + rows - 1) % rows;
                }
                if self.input_state.pressed_this_frame(Scancode::Down) {
                    self.ui.focused_row = (self.ui.focused_row + 1) % rows;
                }
                let left = self.input_state.pressed_this_frame(Scancode::Left);
                let right = self.input_state.pressed_this_frame(Scancode::Right);
                if left || right {
                    let dir = if left { -1 } else { 1 };
                    let row = self.ui.focused_row;
                    self.ui.skirmish.adjust(row, dir, generate_seed());
                }
                if self.input_state.pressed_this_frame(Scancode::Return) {
                    ui::handle_button_action(
                        ui::ButtonAction::StartSkirmish,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "key",
                    );
                }
                if self.input_state.pressed_this_frame(Scancode::Escape) {
                    self.screen = GameScreen::MainMenu;
                }
            }
            GameScreen::ScoreEntry => {
                if self.input_state.pressed_this_frame(Scancode::Up) {
                    self.ui.initials.cycle(1);
                }
                if self.input_state.pressed_this_frame(Scancode::Down) {
                    self.ui.initials.cycle(-1);
                }
                if self.input_state.pressed_this_frame(Scancode::Left) {
                    self.ui.initials.move_slot(-1);
                }
                if self.input_state.pressed_this_frame(Scancode::Right) {
                    self.ui.initials.move_slot(1);
                }
                if self.input_state.pressed_this_frame(Scancode::Return) {
                    ui::handle_button_action(
                        ui::ButtonAction::ConfirmInitials,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "key",
                    );
                    let _ = std::fs::write("battlefield_scores.json", self.ui.scoreboard.to_json());
                }
            }
            GameScreen::ScoreBoard => {
                if self.input_state.pressed_this_frame(Scancode::Return)
                    || self.input_state.pressed_this_frame(Scancode::Escape)
                {
                    self.screen = GameScreen::MainMenu;
                }
            }
        }

        // Render
        let t0 = Instant::now();
        let buttons = crate::renderer::render_frame(
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
            &self.ui,
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
                    ui::handle_button_action(
                        btn.action,
                        &mut self.screen,
                        &mut self.game,
                        &mut self.seed,
                        &mut self.player_was_alive,
                        cur_w,
                        cur_h,
                        &mut self.ui,
                        "click",
                    );
                    if matches!(btn.action, ui::ButtonAction::ConfirmInitials) {
                        let _ =
                            std::fs::write("battlefield_scores.json", self.ui.scoreboard.to_json());
                    }
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
                ui::handle_button_action(
                    buttons[self.input_state.focused_button].action,
                    &mut self.screen,
                    &mut self.game,
                    &mut self.seed,
                    &mut self.player_was_alive,
                    cur_w,
                    cur_h,
                    &mut self.ui,
                    "gamepad",
                );
            }
        }

        true
    }

    /// Arcade runs route into the score flow; skirmish keeps result screens.
    fn end_screen(&mut self, victory: bool, fallback: GameScreen) -> GameScreen {
        if self.ui.mode != ui::GameMode::Arcade {
            return fallback;
        }
        let score = ui::RunScore::from_game(&self.game, victory);
        self.ui.finish_arcade_run(victory);
        let _ = std::fs::write("battlefield_scores.json", self.ui.scoreboard.to_json());
        if self.ui.scoreboard.rank_for(score.total()).is_some() {
            self.ui.pending_score = Some(score);
            self.ui.initials = ui::InitialsEntry::default();
            GameScreen::ScoreEntry
        } else {
            GameScreen::ScoreBoard
        }
    }

    pub fn load_scores(&mut self) {
        if let Ok(json) = std::fs::read_to_string("battlefield_scores.json") {
            if let Some(b) = ui::ScoreBoard::from_json(&json) {
                self.ui.scoreboard = b;
            }
        }
    }
}

fn generate_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(42)
}
