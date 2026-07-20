//! winit event → InputState → PlayerInput translation.

use battlefield_core::player_input::PlayerInput;
pub use battlefield_core::touch_input::{ActionButton, TouchControls, VirtualJoystick};
use battlefield_core::unit::OrderRequest;
use std::collections::HashSet;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, Touch, TouchPhase};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Keyboard key type for this backend.
type Key = KeyCode;

/// Platform-agnostic gamepad button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpButton {
    South, // A (Xbox) / Cross (PS)
    East,  // B / Circle
    West,  // X / Square
    North, // Y / Triangle
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Tracks keyboard, mouse, touch, and gamepad state; produces PlayerInput each frame.
pub struct InputState {
    defend_held_secs: f32,
    defend_hold_fired: bool,
    // Keyboard
    keys_down: HashSet<Key>,
    pressed_this_frame: HashSet<Key>,
    pub scroll_delta: f32,
    pub aim_dir: f32,

    // Mouse
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_clicked: bool,

    // Gamepad
    pub gamepad_connected: bool,
    pub focused_button: usize,
    left_stick_x: f32,
    left_stick_y: f32,
    trigger_left: f32,
    trigger_right: f32,
    gp_buttons_down: HashSet<GpButton>,
    gp_pressed_this_frame: HashSet<GpButton>,

    pub touch: TouchControls,

    // Scale factor from raw device pixels → GPU surface pixels.
    // Needed when the GPU surface is clamped below the raw viewport.
    coord_scale_x: f32,
    coord_scale_y: f32,
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

impl InputState {
    pub fn new() -> Self {
        Self {
            defend_held_secs: 0.0,
            defend_hold_fired: false,
            keys_down: HashSet::new(),
            pressed_this_frame: HashSet::new(),
            scroll_delta: 0.0,
            aim_dir: 0.0,
            mouse_x: 0,
            mouse_y: 0,
            mouse_clicked: false,
            gamepad_connected: false,
            focused_button: 0,
            left_stick_x: 0.0,
            left_stick_y: 0.0,
            trigger_left: 0.0,
            trigger_right: 0.0,
            gp_buttons_down: HashSet::new(),
            gp_pressed_this_frame: HashSet::new(),
            touch: TouchControls::new(),
            coord_scale_x: 1.0,
            coord_scale_y: 1.0,
        }
    }

    pub fn begin_frame(&mut self) {
        self.pressed_this_frame.clear();
        self.gp_pressed_this_frame.clear();
        self.scroll_delta = 0.0;
        self.mouse_clicked = false;
    }

    pub fn set_coordinate_scale(&mut self, sx: f32, sy: f32) {
        self.coord_scale_x = sx;
        self.coord_scale_y = sy;
    }

    pub fn pressed_this_frame(&self, key: Key) -> bool {
        self.pressed_this_frame.contains(&key)
    }

    pub fn is_key_down(&self, key: Key) -> bool {
        self.keys_down.contains(&key)
    }

    // ── Gamepad ─────────────────────────────────────────────────────────

    pub fn gp_pressed(&self, btn: GpButton) -> bool {
        self.gp_pressed_this_frame.contains(&btn)
    }

    pub fn gp_held(&self, btn: GpButton) -> bool {
        self.gp_buttons_down.contains(&btn)
    }

    pub fn gp_button_down(&mut self, btn: GpButton) {
        self.gp_buttons_down.insert(btn);
        self.gp_pressed_this_frame.insert(btn);
    }

    pub fn gp_button_up(&mut self, btn: GpButton) {
        self.gp_buttons_down.remove(&btn);
    }

    pub fn gp_set_axis(&mut self, stick_x: f32, stick_y: f32) {
        // gilrs already applies circular dead zone (0.1 threshold)
        self.left_stick_x = stick_x;
        self.left_stick_y = stick_y;
    }

    pub fn gp_set_triggers(&mut self, left: f32, right: f32) {
        self.trigger_left = left.max(0.0);
        self.trigger_right = right.max(0.0);
    }

    pub fn gp_zoom_delta(&self) -> f32 {
        self.trigger_right
    }

    /// Process a winit WindowEvent.
    pub fn handle_window_event(&mut self, event: &winit::event::WindowEvent) {
        use winit::event::WindowEvent;
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if !event.repeat {
                                self.pressed_this_frame.insert(key);
                            }
                            self.keys_down.insert(key);
                        }
                        ElementState::Released => {
                            self.keys_down.remove(&key);
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.mouse_clicked = true;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_x = (position.x as f32 * self.coord_scale_x) as i32;
                self.mouse_y = (position.y as f32 * self.coord_scale_y) as i32;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 40.0,
                };
                self.scroll_delta += dy;
            }
            WindowEvent::Touch(Touch {
                phase,
                location,
                id,
                ..
            }) => {
                let px = location.x as f32 * self.coord_scale_x;
                let py = location.y as f32 * self.coord_scale_y;
                match phase {
                    TouchPhase::Started => {
                        // Also register as a mouse click for menu button hit testing
                        self.mouse_x = px as i32;
                        self.mouse_y = py as i32;
                        self.mouse_clicked = true;
                        let total = self.touch.finger_count() + 1;
                        self.touch.on_touch_start(*id, px, py, total);
                    }
                    TouchPhase::Moved => {
                        self.touch.on_touch_move(*id, px, py);
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.touch.on_touch_end(*id);
                    }
                }
            }
            _ => {}
        }
    }

    // ── Build player input ──────────────────────────────────────────────

    pub fn build_player_input(&mut self, dt: f32) -> PlayerInput {
        // Keyboard movement
        let kb_left = self.is_key_down(KeyCode::KeyA) || self.is_key_down(KeyCode::ArrowLeft);
        let kb_right = self.is_key_down(KeyCode::KeyD) || self.is_key_down(KeyCode::ArrowRight);
        let kb_up = self.is_key_down(KeyCode::KeyW) || self.is_key_down(KeyCode::ArrowUp);
        let kb_down = self.is_key_down(KeyCode::KeyS) || self.is_key_down(KeyCode::ArrowDown);
        let kb_any = kb_left || kb_right || kb_up || kb_down;

        let kb_raw_x = (kb_right as i32 - kb_left as i32) as f32;
        let kb_raw_y = (kb_down as i32 - kb_up as i32) as f32;

        // Gamepad movement: left stick + D-pad
        let dpad_x =
            self.gp_held(GpButton::DPadRight) as i32 - self.gp_held(GpButton::DPadLeft) as i32;
        let dpad_y =
            self.gp_held(GpButton::DPadDown) as i32 - self.gp_held(GpButton::DPadUp) as i32;
        let gp_raw_x = self.left_stick_x + dpad_x as f32;
        let gp_raw_y = self.left_stick_y + dpad_y as f32;

        // Touch joystick
        let joy = &self.touch.joystick;
        let joy_active = joy.active && (joy.dx.abs() > 0.01 || joy.dy.abs() > 0.01);

        // Priority: keyboard > touch joystick > gamepad
        let (raw_x, raw_y) = if kb_any {
            (kb_raw_x, kb_raw_y)
        } else if joy_active {
            (joy.dx, joy.dy)
        } else {
            (gp_raw_x, gp_raw_y)
        };

        let len = (raw_x * raw_x + raw_y * raw_y).sqrt();
        let (move_x, move_y) = if len > 0.0 {
            (raw_x / len, raw_y / len)
        } else {
            (0.0, 0.0)
        };

        if move_x != 0.0 || move_y != 0.0 {
            self.aim_dir = move_y.atan2(move_x);
        }

        let kb_aim_lock =
            self.is_key_down(KeyCode::ControlLeft) || self.is_key_down(KeyCode::ControlRight);
        let aim_lock = kb_aim_lock || self.trigger_left > 0.5;

        let kb_attack = self.is_key_down(KeyCode::Space);
        let gp_attack = self.gp_held(GpButton::South);
        let touch_attack = self.touch.attack.pressed;

        // Defend is tap-vs-hold: a short press orders the formation, a
        // long press stations the retinue at the nearest zone.
        let defend_down = self.is_key_down(KeyCode::KeyK) || self.gp_held(GpButton::North);
        let mut defend_order = None;
        if defend_down {
            self.defend_held_secs += dt;
            if self.defend_held_secs >= battlefield_core::touch_input::HOLD_ZONE_HOLD_SECS
                && !self.defend_hold_fired
            {
                self.defend_hold_fired = true;
                defend_order = Some(OrderRequest::HoldZone);
            }
        } else {
            if self.defend_held_secs > 0.0 && !self.defend_hold_fired {
                defend_order = Some(OrderRequest::Defend);
            }
            self.defend_held_secs = 0.0;
            self.defend_hold_fired = false;
        }

        let order = self.touch.take_order().or(
            if self.pressed_this_frame(KeyCode::KeyJ) || self.gp_pressed(GpButton::West) {
                Some(OrderRequest::Charge)
            } else if self.pressed_this_frame(KeyCode::KeyL) || self.gp_pressed(GpButton::East) {
                Some(OrderRequest::Dismiss)
            } else {
                defend_order
            },
        );

        PlayerInput {
            move_x,
            move_y,
            attack: kb_attack || gp_attack || touch_attack,
            aim_dir: self.aim_dir,
            aim_lock,
            order,
        }
    }
}
