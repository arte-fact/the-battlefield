#![allow(clippy::too_many_arguments)]

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
use std::time::Instant;

const WINDOW_W: u32 = 960;
const WINDOW_H: u32 = 640;

fn main() {
    env_logger::init();
    log::info!("The Battlefield -- SDL2 starting up");

    let sdl = sdl2::init().expect("SDL2 init failed");
    let video = sdl.video().expect("SDL2 video init failed");
    let game_controller_subsystem = sdl.game_controller().expect("controller subsystem failed");

    let window = video
        .window("The Battlefield", WINDOW_W, WINDOW_H)
        .position_centered()
        .resizable()
        .build()
        .expect("Window creation failed");

    let mut canvas = window
        .into_canvas()
        .accelerated()
        .present_vsync()
        .build()
        .expect("Canvas creation failed");

    canvas.set_blend_mode(sdl2::render::BlendMode::Blend);

    // Nearest-neighbor scaling for pixel art sprites (no blurring)
    sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");

    let texture_creator = canvas.texture_creator();
    let mut assets = renderer::Assets::load(&texture_creator);
    log::info!("Assets loaded");

    // Initialize game
    let (w, h) = canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
    let mut game = Game::new(w as f32, h as f32);
    let mut seed = generate_seed();
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

    let mut event_pump = sdl.event_pump().expect("Event pump failed");
    let mut last_time = Instant::now();
    let start_time = Instant::now();
    let mut screen = GameScreen::MainMenu;
    let mut player_was_alive = true;

    'main_loop: loop {
        let now = Instant::now();
        let dt = now.duration_since(last_time).as_secs_f64().min(0.1);
        let elapsed = now.duration_since(start_time).as_secs_f64();
        last_time = now;

        input_state.begin_frame();

        // Poll events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => break 'main_loop,
                Event::ControllerDeviceAdded { which, .. } => {
                    if active_controller.is_none() {
                        if let Ok(gc) = game_controller_subsystem.open(which) {
                            log::info!("Controller connected: {}", gc.name());
                            input_state.gamepad_connected = true;
                            active_controller = Some(gc);
                        }
                    }
                }
                Event::ControllerDeviceRemoved { .. } => {
                    active_controller = None;
                    input_state.gamepad_connected = false;
                    log::info!("Controller disconnected");
                }
                Event::ControllerAxisMotion { .. }
                | Event::ControllerButtonDown { .. }
                | Event::ControllerButtonUp { .. } => {
                    input_state.handle_controller_event(&event);
                }
                _ => {}
            }
            input_state.handle_event(&event);
        }

        // Handle window resize
        let (cur_w, cur_h) = canvas.output_size().unwrap_or((WINDOW_W, WINDOW_H));
        if cur_w as f32 != game.camera.viewport_w || cur_h as f32 != game.camera.viewport_h {
            game.camera.resize(cur_w as f32, cur_h as f32);
            game.camera.zoom = game.camera.ideal_zoom();
        }

        // Screen transition logic
        let keyboard = event_pump.keyboard_state();
        match screen {
            GameScreen::MainMenu => {
                if keyboard.is_scancode_pressed(Scancode::Return)
                    || keyboard.is_scancode_pressed(Scancode::Space)
                    || input_state.gamepad_pressed(Button::A)
                    || input_state.gamepad_pressed(Button::Start)
                {
                    screen = GameScreen::Playing;
                    log::info!("Game started");
                }
            }
            GameScreen::Playing => {
                // Check for Escape / Back to quit
                if input_state.pressed_this_frame(Scancode::Escape)
                    || input_state.gamepad_pressed(Button::Back)
                {
                    break 'main_loop;
                }

                // Camera zoom from scroll wheel
                let scroll = input_state.scroll_delta;
                if scroll.abs() > f32::EPSILON {
                    game.camera.zoom_by(scroll);
                }

                // Camera zoom from gamepad triggers
                let gp_zoom = input_state.gamepad_zoom_delta();
                if gp_zoom.abs() > 0.01 {
                    game.camera.zoom_by(gp_zoom * dt as f32 * 3.0);
                }

                // Build input and tick game
                let player_input = input_state.build_player_input(&keyboard);

                if game.winner.is_none() {
                    game.tick(&player_input, dt as f32);

                    if (player_input.attack || player_input.attack_held) && game.player_attack() {
                        // Rumble on successful hit
                        if let Some(ref mut gc) = active_controller {
                            let _ = gc.set_rumble(0x4000, 0x8000, 80);
                        }
                    }
                    if player_input.order_hold && game.issue_order("hold") > 0 {
                        if let Some(ref mut gc) = active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                    if player_input.order_go && game.issue_order("go") > 0 {
                        if let Some(ref mut gc) = active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                    if player_input.order_retreat && game.issue_order("retreat") > 0 {
                        if let Some(ref mut gc) = active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                    if player_input.order_follow && game.issue_order("follow") > 0 {
                        if let Some(ref mut gc) = active_controller {
                            let _ = gc.set_rumble(0x2000, 0x4000, 50);
                        }
                    }
                }

                // Process turn events -> spawn particles
                if !game.turn_events.is_empty() {
                    let events = game.turn_events.drain(..).collect::<Vec<_>>();
                    for event in &events {
                        if let battlefield_core::animation::TurnEvent::Move { from, .. } = event {
                            game.particles.push(Particle::new(
                                battlefield_core::particle::ParticleKind::Dust,
                                from.0,
                                from.1,
                            ));
                        }
                    }
                }

                // Update animations, particles, camera follow
                game.update(dt);

                // Clamp camera
                let world_size = GRID_SIZE as f32 * TILE_SIZE;
                game.camera.clamp_to_world(world_size, world_size);

                // Check for player death
                let player_alive = game.player_unit().is_some();
                if player_was_alive && !player_alive {
                    screen = GameScreen::PlayerDeath;
                    log::info!("Player died");
                }
                player_was_alive = player_alive;

                // Check for game end
                if let Some(winner) = game.winner {
                    if winner == Faction::Blue {
                        screen = GameScreen::GameWon;
                    } else {
                        screen = GameScreen::GameLost;
                    }
                    log::info!("Game ended: {:?} wins", winner);
                }
            }
            GameScreen::PlayerDeath | GameScreen::GameWon | GameScreen::GameLost => {
                if input_state.pressed_this_frame(Scancode::Escape)
                    || input_state.gamepad_pressed(Button::Back)
                {
                    break 'main_loop;
                }

                // Still update animations/particles for visual continuity
                game.update(dt);

                // Enter/A = retry (same seed), Space/X = new game
                if input_state.pressed_this_frame(Scancode::Return)
                    || input_state.gamepad_pressed(Button::A)
                {
                    game = Game::new(cur_w as f32, cur_h as f32);
                    game.setup_demo_battle_with_seed(seed);
                    screen = GameScreen::Playing;
                    player_was_alive = true;
                    log::info!("Retrying with seed {seed}");
                } else if input_state.pressed_this_frame(Scancode::Space)
                    || input_state.gamepad_pressed(Button::X)
                {
                    seed = generate_seed();
                    game = Game::new(cur_w as f32, cur_h as f32);
                    game.setup_demo_battle_with_seed(seed);
                    screen = GameScreen::Playing;
                    player_was_alive = true;
                    log::info!("New game with seed {seed}");
                }
            }
        }

        // Render
        let buttons = renderer::render_frame(
            &mut canvas,
            &texture_creator,
            &game,
            &mut assets,
            screen,
            elapsed,
            input_state.mouse_x,
            input_state.mouse_y,
            input_state.focused_button,
            input_state.gamepad_connected,
        );

        // Handle mouse click on overlay buttons
        if input_state.mouse_clicked {
            for btn in &buttons {
                if btn.contains(input_state.mouse_x, input_state.mouse_y) {
                    handle_button_action(
                        btn.action,
                        &mut screen,
                        &mut game,
                        &mut seed,
                        &mut player_was_alive,
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
            if input_state.gamepad_pressed(Button::DPadRight)
                || input_state.gamepad_pressed(Button::DPadDown)
            {
                input_state.focused_button = (input_state.focused_button + 1) % buttons.len();
            }
            if input_state.gamepad_pressed(Button::DPadLeft)
                || input_state.gamepad_pressed(Button::DPadUp)
            {
                input_state.focused_button = if input_state.focused_button == 0 {
                    buttons.len() - 1
                } else {
                    input_state.focused_button - 1
                };
            }
            // Clamp focus index in case button count changed between screens
            input_state.focused_button = input_state.focused_button.min(buttons.len() - 1);

            // Confirm focused button with A (only on non-Playing screens to avoid conflict)
            if screen != GameScreen::Playing && input_state.gamepad_pressed(Button::A) {
                handle_button_action(
                    buttons[input_state.focused_button].action,
                    &mut screen,
                    &mut game,
                    &mut seed,
                    &mut player_was_alive,
                    cur_w,
                    cur_h,
                    "gamepad",
                );
            }
        }
    }

    log::info!("Shutting down");
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
