//! Frame stepping, timing, screen state machine — mirrors SDL game_loop.rs.

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(target_arch = "wasm32")]
use web_time::{Instant, SystemTime, UNIX_EPOCH};

use battlefield_core::game::Game;
use battlefield_core::grid::{GRID_SIZE, TILE_SIZE};
use battlefield_core::ui::{self, GameScreen};
use battlefield_core::unit::Faction;
use winit::keyboard::KeyCode;

use crate::gpu::GpuContext;
use crate::input::{GpButton, InputState};
use crate::renderer;
use crate::renderer::assets::Assets;

#[cfg(not(target_arch = "wasm32"))]
use gilrs::Gilrs;

pub const WINDOW_W: u32 = 960;
pub const WINDOW_H: u32 = 640;

pub struct GameLoop {
    pub gpu: GpuContext,
    pub game: Game,
    assets: Assets,
    pub input_state: InputState,
    pub screen: GameScreen,
    pub seed: u32,
    player_was_alive: bool,
    pub ui: ui::UiState,
    pub dpi_scale: f64,
    pub touch_dpr: f32,

    last_time: Instant,
    start_time: Instant,

    #[cfg(not(target_arch = "wasm32"))]
    gilrs: Option<Gilrs>,
    #[cfg(not(target_arch = "wasm32"))]
    active_gamepad: Option<gilrs::GamepadId>,
}

impl GameLoop {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        let gpu = GpuContext::new(window.clone());
        Self::init(gpu, window)
    }

    /// Async constructor for web.
    pub async fn new_async(window: Arc<winit::window::Window>) -> Self {
        let gpu = GpuContext::new_async(window.clone()).await;
        Self::init(gpu, window)
    }

    fn init(gpu: GpuContext, window: Arc<winit::window::Window>) -> Self {
        let assets = Assets::load(&gpu);
        log::info!("Assets loaded");

        let size = window.inner_size();
        let w = size.width.max(1);
        let h = size.height.max(1);

        let mut game = Game::new(w as f32, h as f32);
        let seed = generate_seed();
        game.setup_demo_battle_with_seed(seed);
        log::info!("Game initialized ({}x{} grid)", GRID_SIZE, GRID_SIZE);

        let sf = window.scale_factor();
        log::info!("DPI scale: {sf} (window {w}x{h})");

        let now = Instant::now();

        // Initialize gamepad support
        #[cfg(not(target_arch = "wasm32"))]
        let (gilrs, active_gamepad, gamepad_connected) = {
            match Gilrs::new() {
                Ok(mut g) => {
                    let active = g.gamepads().next().map(|(id, gp)| {
                        let name = gp.name().to_owned();
                        log::info!("Controller connected: {name}");
                        id
                    });
                    // Drain any startup events
                    while g.next_event().is_some() {}
                    (Some(g), active, active.is_some())
                }
                Err(e) => {
                    log::warn!("Failed to initialize gilrs: {e}");
                    (None, None, false)
                }
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        let mut input_state = InputState::new();
        #[cfg(target_arch = "wasm32")]
        let input_state = InputState::new();
        #[cfg(not(target_arch = "wasm32"))]
        {
            input_state.gamepad_connected = gamepad_connected;
        }

        Self {
            gpu,
            game,
            assets,
            input_state,
            screen: GameScreen::MainMenu,
            seed,
            player_was_alive: true,
            ui: ui::UiState::default(),
            dpi_scale: sf,
            touch_dpr: sf as f32,
            last_time: now,
            start_time: now,
            #[cfg(not(target_arch = "wasm32"))]
            gilrs,
            #[cfg(not(target_arch = "wasm32"))]
            active_gamepad,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height); // proportionally clamped inside
        if width > 0 && height > 0 {
            // Use GPU surface dimensions (possibly clamped) for all coordinate
            // spaces: camera, input layout, and HUD rendering all match.
            let sw = self.gpu.surface_config.width as f32;
            let sh = self.gpu.surface_config.height as f32;
            self.game.camera.resize(sw, sh);
            self.game.camera.zoom = self.game.camera.ideal_zoom_for_dpi(self.touch_dpr);
            self.input_state.touch.update_layout(sw, sh, self.touch_dpr);
            // Scale touch/mouse coords from raw device pixels to surface pixels
            let sx = sw / width as f32;
            let sy = sh / height as f32;
            self.input_state.set_coordinate_scale(sx, sy);
        }
    }

    pub fn set_dpi(&mut self, scale: f64) {
        self.dpi_scale = scale;
        self.touch_dpr = scale as f32;
    }

    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) {
        self.input_state.handle_window_event(event);
    }

    /// Clear per-frame input state. Call AFTER step() so that events
    /// accumulated between frames are consumed before being cleared.
    pub fn end_frame(&mut self) {
        self.input_state.begin_frame();
    }

    /// Poll gamepad events and update input state.
    #[cfg(not(target_arch = "wasm32"))]
    fn poll_gamepad(&mut self) {
        let Some(ref mut gilrs) = self.gilrs else {
            return;
        };

        // Process events for buttons and connection state
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event() {
            use gilrs::ev::EventType;
            match event {
                EventType::Connected => {
                    if self.active_gamepad.is_none() {
                        let gp = gilrs.gamepad(id);
                        log::info!("Controller connected: {}", gp.name());
                        self.active_gamepad = Some(id);
                        self.input_state.gamepad_connected = true;
                    }
                }
                EventType::Disconnected => {
                    if self.active_gamepad == Some(id) {
                        log::info!("Controller disconnected");
                        self.active_gamepad = None;
                        self.input_state.gamepad_connected = false;
                    }
                }
                EventType::ButtonPressed(btn, _) if self.active_gamepad == Some(id) => {
                    if let Some(gp) = gilrs_to_gp(btn) {
                        self.input_state.gp_button_down(gp);
                    }
                }
                EventType::ButtonReleased(btn, _) if self.active_gamepad == Some(id) => {
                    if let Some(gp) = gilrs_to_gp(btn) {
                        self.input_state.gp_button_up(gp);
                    }
                }
                _ => {}
            }
        }

        // Read axis state directly from gamepad each frame (more robust than events)
        if let Some(id) = self.active_gamepad {
            let gp = gilrs.gamepad(id);

            // Sticks — negate Y: gilrs up=+1, game up=negative Y (screen coords)
            let sx = gp
                .axis_data(gilrs::Axis::LeftStickX)
                .map(|d| d.value())
                .unwrap_or(0.0);
            let sy = gp
                .axis_data(gilrs::Axis::LeftStickY)
                .map(|d| d.value())
                .unwrap_or(0.0);
            self.input_state.gp_set_axis(sx, -sy);

            // Triggers — gilrs reports [-1.0, 1.0], remap to [0.0, 1.0]
            let lt = gp
                .axis_data(gilrs::Axis::LeftZ)
                .map(|d| (d.value() + 1.0) / 2.0)
                .unwrap_or(0.0);
            let rt = gp
                .axis_data(gilrs::Axis::RightZ)
                .map(|d| (d.value() + 1.0) / 2.0)
                .unwrap_or(0.0);
            self.input_state.gp_set_triggers(lt, rt);
        }
    }

    /// Run one frame. Returns false if the application should exit.
    /// Input events have already been dispatched via handle_event() before this.
    pub fn step(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64().min(0.1);
        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        self.last_time = now;

        #[cfg(not(target_arch = "wasm32"))]
        self.poll_gamepad();

        // Use GPU surface dimensions — all coordinate spaces (HUD, camera,
        // touch input) are unified to surface-config pixels.
        let vw = self.gpu.surface_config.width;
        let vh = self.gpu.surface_config.height;

        // ── Screen transition logic ──────────────────────────────────────
        match self.screen {
            GameScreen::MainMenu => {
                // Keyboard start (Enter/Space) — buttons also handled after render
                if self.input_state.pressed_this_frame(KeyCode::Enter)
                    || self.input_state.pressed_this_frame(KeyCode::Space)
                {
                    self.activate_button(ui::ButtonAction::Play, vw, vh, "key");
                }
            }
            GameScreen::Playing => {
                if self.input_state.pressed_this_frame(KeyCode::Escape) {
                    return false;
                }

                // Camera zoom from scroll wheel or gamepad right trigger
                let scroll = self.input_state.scroll_delta;
                if scroll.abs() > f32::EPSILON {
                    self.game.camera.zoom_by(scroll);
                }
                let gp_zoom = self.input_state.gp_zoom_delta();
                if gp_zoom > f32::EPSILON {
                    self.game.camera.zoom_by(gp_zoom * dt as f32);
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
                let player_input = self.input_state.build_player_input();

                if self.game.winner.is_none() {
                    self.game.tick(&player_input, dt as f32);

                    if player_input.attack {
                        self.game.player_attack();
                    }
                    if let Some(req) = player_input.order {
                        self.game.issue_order(req);
                    }
                }

                self.game.process_turn_events();
                self.game.update(dt);

                let world_size = GRID_SIZE as f32 * TILE_SIZE;
                self.game.camera.clamp_to_world(world_size, world_size);

                // Check for player death
                let player_alive = self.game.player_unit().is_some();
                if self.player_was_alive && !player_alive {
                    self.screen = self.end_screen(false, GameScreen::PlayerDeath);
                    log::info!("Player died");
                }
                self.player_was_alive = player_alive;

                // Check for game end
                if let Some(winner) = self.game.winner {
                    if winner == Faction::Blue {
                        self.screen = self.end_screen(true, GameScreen::GameWon);
                    } else {
                        self.screen = self.end_screen(false, GameScreen::GameLost);
                    }
                    log::info!("Game ended: {:?} wins", winner);
                }
            }
            GameScreen::SkirmishSetup => {
                let up = self.input_state.pressed_this_frame(KeyCode::ArrowUp)
                    || self.input_state.gp_pressed(GpButton::DPadUp);
                let down = self.input_state.pressed_this_frame(KeyCode::ArrowDown)
                    || self.input_state.gp_pressed(GpButton::DPadDown);
                let left = self.input_state.pressed_this_frame(KeyCode::ArrowLeft)
                    || self.input_state.gp_pressed(GpButton::DPadLeft);
                let right = self.input_state.pressed_this_frame(KeyCode::ArrowRight)
                    || self.input_state.gp_pressed(GpButton::DPadRight);
                let rows = ui::SkirmishConfig::ROWS;
                if up {
                    self.ui.focused_row = (self.ui.focused_row + rows - 1) % rows;
                }
                if down {
                    self.ui.focused_row = (self.ui.focused_row + 1) % rows;
                }
                if left || right {
                    let dir = if left { -1 } else { 1 };
                    let row = self.ui.focused_row;
                    self.ui.skirmish.adjust(row, dir, generate_seed());
                }
                if self.input_state.pressed_this_frame(KeyCode::Enter)
                    || self.input_state.gp_pressed(GpButton::South)
                {
                    self.activate_button(ui::ButtonAction::StartSkirmish, vw, vh, "key");
                }
                if self.input_state.pressed_this_frame(KeyCode::Escape)
                    || self.input_state.gp_pressed(GpButton::East)
                {
                    self.screen = GameScreen::MainMenu;
                }
            }
            GameScreen::ScoreEntry => {
                if self.input_state.pressed_this_frame(KeyCode::ArrowUp)
                    || self.input_state.gp_pressed(GpButton::DPadUp)
                {
                    self.ui.initials.cycle(1);
                }
                if self.input_state.pressed_this_frame(KeyCode::ArrowDown)
                    || self.input_state.gp_pressed(GpButton::DPadDown)
                {
                    self.ui.initials.cycle(-1);
                }
                if self.input_state.pressed_this_frame(KeyCode::ArrowLeft)
                    || self.input_state.gp_pressed(GpButton::DPadLeft)
                {
                    self.ui.initials.move_slot(-1);
                }
                if self.input_state.pressed_this_frame(KeyCode::ArrowRight)
                    || self.input_state.gp_pressed(GpButton::DPadRight)
                {
                    self.ui.initials.move_slot(1);
                }
                if self.input_state.pressed_this_frame(KeyCode::Enter)
                    || self.input_state.gp_pressed(GpButton::South)
                {
                    self.activate_button(ui::ButtonAction::ConfirmInitials, vw, vh, "key");
                }
            }
            GameScreen::ScoreBoard => {
                if self.input_state.pressed_this_frame(KeyCode::Enter)
                    || self.input_state.pressed_this_frame(KeyCode::Escape)
                    || self.input_state.gp_pressed(GpButton::South)
                    || self.input_state.gp_pressed(GpButton::East)
                {
                    self.screen = GameScreen::MainMenu;
                }
            }
            GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                if self.input_state.pressed_this_frame(KeyCode::Escape) {
                    return false;
                }

                self.game.update(dt);

                if self.input_state.pressed_this_frame(KeyCode::Enter) {
                    self.activate_button(ui::ButtonAction::Retry, vw, vh, "key");
                } else if self.input_state.pressed_this_frame(KeyCode::Space) {
                    self.activate_button(ui::ButtonAction::NewGame, vw, vh, "key");
                }
            }
        }

        // ── Render ───────────────────────────────────────────────────────
        let buttons = renderer::render_frame(
            &self.gpu,
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
            self.game.order_pulse,
            self.game.order_pulse_radius,
            &self.ui,
        );

        // Handle mouse click on overlay buttons
        if self.input_state.mouse_clicked {
            for btn in &buttons {
                if btn.contains(self.input_state.mouse_x, self.input_state.mouse_y) {
                    self.activate_button(btn.action, vw, vh, "click");
                    break;
                }
            }
        }

        // Gamepad: D-pad menu navigation + A-button confirm
        if !buttons.is_empty() {
            if self.input_state.gp_pressed(GpButton::DPadRight)
                || self.input_state.gp_pressed(GpButton::DPadDown)
            {
                self.input_state.focused_button =
                    (self.input_state.focused_button + 1) % buttons.len();
            }
            if self.input_state.gp_pressed(GpButton::DPadLeft)
                || self.input_state.gp_pressed(GpButton::DPadUp)
            {
                self.input_state.focused_button = if self.input_state.focused_button == 0 {
                    buttons.len() - 1
                } else {
                    self.input_state.focused_button - 1
                };
            }
            self.input_state.focused_button =
                self.input_state.focused_button.min(buttons.len() - 1);

            if self.screen != GameScreen::Playing && self.input_state.gp_pressed(GpButton::South) {
                let action = buttons[self.input_state.focused_button].action;
                self.activate_button(action, vw, vh, "gamepad");
            }
        }

        true
    }

    fn activate_button(&mut self, action: ui::ButtonAction, vw: u32, vh: u32, source: &str) {
        ui::handle_button_action(
            action,
            &mut self.screen,
            &mut self.game,
            &mut self.seed,
            &mut self.player_was_alive,
            vw,
            vh,
            &mut self.ui,
            source,
        );
        if matches!(action, ui::ButtonAction::ConfirmInitials) {
            self.save_scores();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_scores(&mut self) {
        if let Ok(json) = std::fs::read_to_string("battlefield_scores.json") {
            if let Some(b) = ui::ScoreBoard::from_json(&json) {
                self.ui.scoreboard = b;
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_scores(&self) {
        let _ = std::fs::write("battlefield_scores.json", self.ui.scoreboard.to_json());
    }

    #[cfg(target_arch = "wasm32")]
    fn save_scores(&self) {}

    /// Arcade runs route into the score flow; skirmish keeps result screens.
    fn end_screen(&mut self, victory: bool, fallback: GameScreen) -> GameScreen {
        if self.ui.mode != ui::GameMode::Arcade {
            return fallback;
        }
        let score = ui::RunScore::from_game(&self.game, victory);
        if self.ui.scoreboard.rank_for(score.total()).is_some() {
            self.ui.pending_score = Some(score);
            self.ui.initials = ui::InitialsEntry::default();
            GameScreen::ScoreEntry
        } else {
            GameScreen::ScoreBoard
        }
    }
}

/// Time-based seed; web-time backs SystemTime with Date.now on wasm.
fn generate_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
        .unwrap_or(42)
}

/// Convert a gilrs button to our platform-agnostic GpButton.
#[cfg(not(target_arch = "wasm32"))]
fn gilrs_to_gp(btn: gilrs::Button) -> Option<GpButton> {
    use gilrs::Button;
    match btn {
        Button::South => Some(GpButton::South),
        Button::East => Some(GpButton::East),
        Button::West => Some(GpButton::West),
        Button::North => Some(GpButton::North),
        Button::DPadUp => Some(GpButton::DPadUp),
        Button::DPadDown => Some(GpButton::DPadDown),
        Button::DPadLeft => Some(GpButton::DPadLeft),
        Button::DPadRight => Some(GpButton::DPadRight),
        _ => None,
    }
}
