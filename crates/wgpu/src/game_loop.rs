//! Frame stepping, timing, screen state machine — mirrors SDL game_loop.rs.

use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use battlefield_core::game::Game;
use battlefield_core::grid::{GRID_SIZE, TILE_SIZE};
use battlefield_core::ui::{self, GameScreen};
use battlefield_core::unit::Faction;
use winit::keyboard::KeyCode;

use crate::gpu::GpuContext;
use crate::input::InputState;
use crate::renderer;
use crate::renderer::assets::Assets;

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
    pub dpi_scale: f64,
    pub touch_dpr: f32,

    last_time: Instant,
    start_time: Instant,
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

        Self {
            gpu,
            game,
            assets,
            input_state: InputState::new(),
            screen: GameScreen::MainMenu,
            seed,
            player_was_alive: true,
            dpi_scale: sf,
            touch_dpr: sf as f32,
            last_time: now,
            start_time: now,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.gpu.resize(width, height);
        if width > 0 && height > 0 {
            self.game.camera.resize(width as f32, height as f32);
            self.game.camera.zoom = self.game.camera.ideal_zoom();
        }
    }

    pub fn handle_event(&mut self, event: &winit::event::WindowEvent) {
        self.input_state.handle_window_event(event);
    }

    /// Clear per-frame input state. Call AFTER step() so that events
    /// accumulated between frames are consumed before being cleared.
    pub fn end_frame(&mut self) {
        self.input_state.begin_frame();
    }

    /// Run one frame. Returns false if the application should exit.
    /// Input events have already been dispatched via handle_event() before this.
    pub fn step(&mut self) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_time).as_secs_f64().min(0.1);
        let elapsed = now.duration_since(self.start_time).as_secs_f64();
        self.last_time = now;

        let vw = self.gpu.surface_config.width;
        let vh = self.gpu.surface_config.height;

        // Update touch layout
        self.input_state.set_canvas_size(vw as f32, vh as f32);
        self.input_state
            .update_layout(vw as f32, vh as f32, self.touch_dpr);

        // ── Screen transition logic ──────────────────────────────────────
        match self.screen {
            GameScreen::MainMenu => {
                // Keyboard start (Enter/Space) — buttons also handled after render
                if self.input_state.pressed_this_frame(KeyCode::Enter)
                    || self.input_state.pressed_this_frame(KeyCode::Space)
                {
                    self.screen = GameScreen::Playing;
                    log::info!("Game started");
                }
            }
            GameScreen::Playing => {
                if self.input_state.pressed_this_frame(KeyCode::Escape) {
                    return false;
                }

                // Camera zoom from scroll wheel
                let scroll = self.input_state.scroll_delta;
                if scroll.abs() > f32::EPSILON {
                    self.game.camera.zoom_by(scroll);
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
                let player_input = self.input_state.build_player_input();

                if self.game.winner.is_none() {
                    self.game.tick(&player_input, dt as f32);

                    if player_input.attack {
                        self.game.player_attack();
                    }
                    if player_input.order_follow {
                        self.game.recruit_units();
                        self.game.issue_order("follow");
                    }
                    if player_input.order_charge {
                        self.game.recruit_units();
                        self.game.issue_order("charge");
                    }
                    if player_input.order_defend {
                        self.game.recruit_units();
                        self.game.issue_order("defend");
                    }
                }

                self.game.process_turn_events();
                self.game.update(dt);

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
                if self.input_state.pressed_this_frame(KeyCode::Escape) {
                    return false;
                }

                self.game.update(dt);

                if self.input_state.pressed_this_frame(KeyCode::Enter) {
                    // Retry same seed
                    self.game = Game::new(vw as f32, vh as f32);
                    self.game.setup_demo_battle_with_seed(self.seed);
                    self.screen = GameScreen::Playing;
                    self.player_was_alive = true;
                    log::info!("Retrying with seed {}", self.seed);
                } else if self.input_state.pressed_this_frame(KeyCode::Space) {
                    // New game
                    self.seed = generate_seed();
                    self.game = Game::new(vw as f32, vh as f32);
                    self.game.setup_demo_battle_with_seed(self.seed);
                    self.screen = GameScreen::Playing;
                    self.player_was_alive = true;
                    log::info!("New game with seed {}", self.seed);
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
        );

        // Handle mouse click on overlay buttons
        if self.input_state.mouse_clicked {
            for btn in &buttons {
                if btn.contains(self.input_state.mouse_x, self.input_state.mouse_y) {
                    match btn.action {
                        ui::ButtonAction::Play => {
                            self.screen = GameScreen::Playing;
                            log::info!("Game started (click)");
                        }
                        ui::ButtonAction::Retry => {
                            self.game = Game::new(vw as f32, vh as f32);
                            self.game.setup_demo_battle_with_seed(self.seed);
                            self.screen = GameScreen::Playing;
                            self.player_was_alive = true;
                            log::info!("Retrying with seed {} (click)", self.seed);
                        }
                        ui::ButtonAction::NewGame => {
                            self.seed = generate_seed();
                            self.game = Game::new(vw as f32, vh as f32);
                            self.game.setup_demo_battle_with_seed(self.seed);
                            self.screen = GameScreen::Playing;
                            self.player_was_alive = true;
                            log::info!("New game with seed {} (click)", self.seed);
                        }
                    }
                    break;
                }
            }
        }

        true
    }
}

/// Generate a seed using Instant (works on both native and web via web-time).
fn generate_seed() -> u32 {
    // Use elapsed time from a zero instant as entropy source.
    // On native this is process uptime; on web it's performance.now().
    let elapsed = Instant::now().elapsed();
    elapsed.as_nanos() as u32
}
