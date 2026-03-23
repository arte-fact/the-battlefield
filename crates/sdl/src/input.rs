use battlefield_core::player_input::PlayerInput;
use sdl2::controller::{Axis, Button};
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::mouse::MouseButton;
use sdl2::mouse::MouseWheelDirection;
use std::collections::HashSet;

/// Tracks keyboard and gamepad state and produces PlayerInput each frame.
pub struct InputState {
    /// Keys that were pressed this frame (edge-triggered).
    pressed_this_frame: HashSet<Scancode>,
    /// Accumulated mouse wheel scroll since last frame.
    pub scroll_delta: f32,
    /// Last known aim direction (preserved when not moving).
    pub aim_dir: f32,
    /// Current mouse X position in window coordinates.
    pub mouse_x: i32,
    /// Current mouse Y position in window coordinates.
    pub mouse_y: i32,
    /// Whether the left mouse button was clicked this frame.
    pub mouse_clicked: bool,

    // Gamepad
    pub gamepad_connected: bool,
    left_stick_x: f32,
    left_stick_y: f32,
    trigger_left: f32,
    trigger_right: f32,
    gamepad_buttons_down: HashSet<Button>,
    gamepad_pressed_this_frame: HashSet<Button>,
    /// Menu button focus index for D-Pad navigation.
    pub focused_button: usize,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            pressed_this_frame: HashSet::new(),
            scroll_delta: 0.0,
            aim_dir: 0.0,
            mouse_x: 0,
            mouse_y: 0,
            mouse_clicked: false,
            gamepad_connected: false,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            trigger_left: 0.0,
            trigger_right: 0.0,
            gamepad_buttons_down: HashSet::new(),
            gamepad_pressed_this_frame: HashSet::new(),
            focused_button: 0,
        }
    }

    /// Call at the start of each frame to reset per-frame state.
    pub fn begin_frame(&mut self) {
        self.pressed_this_frame.clear();
        self.gamepad_pressed_this_frame.clear();
        self.scroll_delta = 0.0;
        self.mouse_clicked = false;
    }

    /// Returns true if the given scancode was pressed this frame (edge-triggered).
    pub fn pressed_this_frame(&self, sc: Scancode) -> bool {
        self.pressed_this_frame.contains(&sc)
    }

    /// Process an SDL event to record key presses and scroll.
    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::KeyDown {
                scancode: Some(sc),
                repeat: false,
                ..
            } => {
                self.pressed_this_frame.insert(*sc);
            }
            Event::MouseButtonDown {
                mouse_btn: MouseButton::Left,
                ..
            } => {
                self.mouse_clicked = true;
            }
            Event::MouseMotion { x, y, .. } => {
                self.mouse_x = *x;
                self.mouse_y = *y;
            }
            Event::MouseWheel { y, direction, .. } => {
                let dir = if *direction == MouseWheelDirection::Flipped {
                    -1.0
                } else {
                    1.0
                };
                self.scroll_delta += *y as f32 * dir;
            }
            _ => {}
        }
    }

    /// Process an SDL controller event to record gamepad state.
    pub fn handle_controller_event(&mut self, event: &Event) {
        match event {
            Event::ControllerAxisMotion { axis, value, .. } => {
                let normalized = *value as f32 / 32767.0;
                let dead_zone = 0.25;
                let filtered = if normalized.abs() < dead_zone {
                    0.0
                } else {
                    normalized
                };
                match axis {
                    Axis::LeftX => self.left_stick_x = filtered,
                    Axis::LeftY => self.left_stick_y = filtered,
                    Axis::TriggerLeft => self.trigger_left = normalized.max(0.0),
                    Axis::TriggerRight => self.trigger_right = normalized.max(0.0),
                    _ => {}
                }
            }
            Event::ControllerButtonDown { button, .. } => {
                self.gamepad_buttons_down.insert(*button);
                self.gamepad_pressed_this_frame.insert(*button);
            }
            Event::ControllerButtonUp { button, .. } => {
                self.gamepad_buttons_down.remove(button);
            }
            _ => {}
        }
    }

    /// Returns true if the given gamepad button was pressed this frame (edge-triggered).
    pub fn gamepad_pressed(&self, btn: Button) -> bool {
        self.gamepad_pressed_this_frame.contains(&btn)
    }

    /// Returns true if the given gamepad button is currently held down.
    pub fn gamepad_held(&self, btn: Button) -> bool {
        self.gamepad_buttons_down.contains(&btn)
    }

    /// Returns the left stick position as (x, y), each in -1.0..1.0 after dead zone.
    pub fn gamepad_movement(&self) -> (f32, f32) {
        (self.left_stick_x, self.left_stick_y)
    }

    /// Returns zoom delta from triggers: positive = zoom in, negative = zoom out.
    pub fn gamepad_zoom_delta(&self) -> f32 {
        self.trigger_right - self.trigger_left
    }

    /// Build a PlayerInput from the current keyboard and gamepad state.
    pub fn build_player_input(&mut self, keyboard: &sdl2::keyboard::KeyboardState) -> PlayerInput {
        // Keyboard movement
        let kb_left = keyboard.is_scancode_pressed(Scancode::A)
            || keyboard.is_scancode_pressed(Scancode::Left);
        let kb_right = keyboard.is_scancode_pressed(Scancode::D)
            || keyboard.is_scancode_pressed(Scancode::Right);
        let kb_up =
            keyboard.is_scancode_pressed(Scancode::W) || keyboard.is_scancode_pressed(Scancode::Up);
        let kb_down = keyboard.is_scancode_pressed(Scancode::S)
            || keyboard.is_scancode_pressed(Scancode::Down);
        let kb_any = kb_left || kb_right || kb_up || kb_down;

        let kb_raw_x = (kb_right as i32 - kb_left as i32) as f32;
        let kb_raw_y = (kb_down as i32 - kb_up as i32) as f32;

        // Gamepad movement: left stick + D-Pad
        let (stick_x, stick_y) = self.gamepad_movement();
        let dpad_x = self.gamepad_held(Button::DPadRight) as i32
            - self.gamepad_held(Button::DPadLeft) as i32;
        let dpad_y =
            self.gamepad_held(Button::DPadDown) as i32 - self.gamepad_held(Button::DPadUp) as i32;
        let gp_raw_x = stick_x + dpad_x as f32;
        let gp_raw_y = stick_y + dpad_y as f32;

        // Use keyboard if any WASD/arrow pressed, otherwise use gamepad
        let (raw_x, raw_y) = if kb_any {
            (kb_raw_x, kb_raw_y)
        } else {
            (gp_raw_x, gp_raw_y)
        };

        let len = (raw_x * raw_x + raw_y * raw_y).sqrt();
        let (move_x, move_y) = if len > 0.0 {
            (raw_x / len, raw_y / len)
        } else {
            (0.0, 0.0)
        };

        // Update aim direction from movement (preserve last direction when idle)
        if move_x != 0.0 || move_y != 0.0 {
            self.aim_dir = move_y.atan2(move_x);
        }

        let kb_attack_held = keyboard.is_scancode_pressed(Scancode::Space);
        let gp_attack_held = self.gamepad_held(Button::A);
        let attack_held = kb_attack_held || gp_attack_held;

        let kb_attack_pressed = self.pressed_this_frame.contains(&Scancode::Space);
        let gp_attack_pressed = self.gamepad_pressed(Button::A);

        PlayerInput {
            move_x,
            move_y,
            attack: kb_attack_pressed || gp_attack_pressed || attack_held,
            aim_dir: self.aim_dir,
            attack_held,
            order_hold: self.pressed_this_frame.contains(&Scancode::H)
                || self.gamepad_pressed(Button::LeftShoulder),
            order_go: self.pressed_this_frame.contains(&Scancode::G)
                || self.gamepad_pressed(Button::RightShoulder),
            order_retreat: self.pressed_this_frame.contains(&Scancode::R)
                || self.gamepad_pressed(Button::Y),
            order_follow: self.pressed_this_frame.contains(&Scancode::F)
                || self.gamepad_pressed(Button::X),
        }
    }
}
