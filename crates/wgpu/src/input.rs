//! winit event → InputState → PlayerInput translation.

use battlefield_core::player_input::PlayerInput;
pub use battlefield_core::touch_input::{ActionButton, VirtualJoystick};
use std::collections::{HashMap, HashSet};
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

    // Touch controls
    pub joystick: VirtualJoystick,
    pub attack_button: ActionButton,
    pub recruit_btn: ActionButton,
    pub order_follow_btn: ActionButton,
    pub order_charge_btn: ActionButton,
    pub order_defend_btn: ActionButton,
    pub is_touch_device: bool,
    pub has_used_joystick: bool,
    attack_pressed: bool,
    order_follow_pressed: bool,
    order_charge_pressed: bool,
    order_defend_pressed: bool,

    // Touch: camera drag
    camera_drag_id: Option<u64>,
    camera_drag_prev: (f32, f32),
    camera_drag_dx: f32,
    camera_drag_dy: f32,

    // Touch: two-finger gestures
    two_finger_prev_dist: Option<f32>,
    two_finger_prev_mid: Option<(f32, f32)>,
    pinch_zoom: f32,
    touch_pan_x: f32,
    touch_pan_y: f32,

    active_fingers: HashMap<u64, (f32, f32)>,
    canvas_w: f32,
    canvas_h: f32,
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
            joystick: VirtualJoystick::new(),
            attack_button: ActionButton::new(0.0, 0.0, 56.0),
            recruit_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_follow_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_charge_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_defend_btn: ActionButton::new(0.0, 0.0, 28.0),
            is_touch_device: false,
            has_used_joystick: false,
            attack_pressed: false,
            order_follow_pressed: false,
            order_charge_pressed: false,
            order_defend_pressed: false,
            camera_drag_id: None,
            camera_drag_prev: (0.0, 0.0),
            camera_drag_dx: 0.0,
            camera_drag_dy: 0.0,
            two_finger_prev_dist: None,
            two_finger_prev_mid: None,
            pinch_zoom: 0.0,
            touch_pan_x: 0.0,
            touch_pan_y: 0.0,
            active_fingers: HashMap::new(),
            canvas_w: 960.0,
            canvas_h: 640.0,
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

    pub fn set_canvas_size(&mut self, w: f32, h: f32) {
        self.canvas_w = w;
        self.canvas_h = h;
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
                        self.is_touch_device = true;
                        self.active_fingers.insert(*id, (px, py));
                        let total = self.active_fingers.len() as u32;
                        // Also register as a mouse click for menu button hit testing
                        self.mouse_x = px as i32;
                        self.mouse_y = py as i32;
                        self.mouse_clicked = true;
                        self.on_touch_start(*id, px, py, total);
                    }
                    TouchPhase::Moved => {
                        self.active_fingers.insert(*id, (px, py));
                        if self.active_fingers.len() >= 2 && !self.has_active_control() {
                            let positions: Vec<(f32, f32)> =
                                self.active_fingers.values().copied().collect();
                            if positions.len() >= 2 {
                                self.on_touch_move_two_finger(
                                    positions[0].0,
                                    positions[0].1,
                                    positions[1].0,
                                    positions[1].1,
                                );
                            }
                        } else {
                            self.on_touch_move_single(*id, px, py);
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.active_fingers.remove(id);
                        let remaining = self.active_fingers.len() as u32;
                        self.on_touch_end(*id, px, py, remaining);
                    }
                }
            }
            _ => {}
        }
    }

    // ── Touch layout ────────────────────────────────────────────────────

    pub fn update_layout(&mut self, canvas_w: f32, canvas_h: f32, dpr: f32) {
        let atk_radius = 56.0 * dpr;
        let atk_margin = 90.0 * dpr;
        let atk_cx = canvas_w - atk_margin;
        let atk_cy = canvas_h - atk_margin;
        self.attack_button.center_x = atk_cx;
        self.attack_button.center_y = atk_cy;
        self.attack_button.radius = atk_radius;

        let ord_radius = 36.0 * dpr;
        let spacing = atk_radius + ord_radius + 12.0 * dpr;

        self.order_follow_btn.center_x = atk_cx - spacing;
        self.order_follow_btn.center_y = atk_cy;
        self.order_follow_btn.radius = ord_radius;

        let diag = spacing * 0.707;
        self.order_charge_btn.center_x = atk_cx - diag;
        self.order_charge_btn.center_y = atk_cy - diag;
        self.order_charge_btn.radius = ord_radius;

        self.order_defend_btn.center_x = atk_cx;
        self.order_defend_btn.center_y = atk_cy - spacing;
        self.order_defend_btn.radius = ord_radius;

        self.joystick.max_radius = 40.0 * dpr;
        self.joystick.dead_zone = 4.0 * dpr;
    }

    // ── Touch routing ───────────────────────────────────────────────────

    fn try_order_buttons(&mut self, touch_id: u64, x: f32, y: f32) -> bool {
        if self.order_follow_btn.contains(x, y) {
            self.order_follow_btn.press(touch_id as i64);
            self.order_follow_pressed = true;
            return true;
        }
        if self.order_charge_btn.contains(x, y) {
            self.order_charge_btn.press(touch_id as i64);
            self.order_charge_pressed = true;
            return true;
        }
        if self.order_defend_btn.contains(x, y) {
            self.order_defend_btn.press(touch_id as i64);
            self.order_defend_pressed = true;
            return true;
        }
        false
    }

    fn on_touch_start(&mut self, touch_id: u64, x: f32, y: f32, total_touches: u32) {
        let tid = touch_id as i64;
        if total_touches >= 2 {
            if self.joystick.active || self.attack_button.pressed {
                if !self.joystick.active && x < self.canvas_w * 0.4 {
                    self.joystick.activate(tid, x, y);
                    self.has_used_joystick = true;
                    return;
                }
                if self.try_order_buttons(touch_id, x, y) {
                    return;
                }
                if !self.attack_button.pressed && self.attack_button.contains(x, y) {
                    self.attack_button.press(tid);
                    self.attack_pressed = true;
                    return;
                }
            }
            return;
        }

        if x < self.canvas_w * 0.4 {
            self.joystick.activate(tid, x, y);
            self.has_used_joystick = true;
            return;
        }

        if self.try_order_buttons(touch_id, x, y) {
            return;
        }

        if self.attack_button.contains(x, y) {
            self.attack_button.press(tid);
            self.attack_pressed = true;
            return;
        }

        if self.camera_drag_id.is_none() {
            self.camera_drag_id = Some(touch_id);
            self.camera_drag_prev = (x, y);
        }
    }

    fn has_active_control(&self) -> bool {
        self.joystick.active
            || self.attack_button.pressed
            || self.recruit_btn.pressed
            || self.order_follow_btn.pressed
            || self.order_charge_btn.pressed
            || self.order_defend_btn.pressed
            || self.camera_drag_id.is_some()
    }

    fn on_touch_move_single(&mut self, touch_id: u64, x: f32, y: f32) {
        self.joystick.update(touch_id as i64, x, y);
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_dx += x - self.camera_drag_prev.0;
            self.camera_drag_dy += y - self.camera_drag_prev.1;
            self.camera_drag_prev = (x, y);
        }
    }

    fn on_touch_move_two_finger(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
        let dx = x2 - x1;
        let dy = y2 - y1;
        let dist = (dx * dx + dy * dy).sqrt();
        let mid_x = (x1 + x2) / 2.0;
        let mid_y = (y1 + y2) / 2.0;

        if let Some(prev_dist) = self.two_finger_prev_dist {
            self.pinch_zoom += (dist - prev_dist) / 100.0;
        }
        if let Some((prev_mx, prev_my)) = self.two_finger_prev_mid {
            self.touch_pan_x += mid_x - prev_mx;
            self.touch_pan_y += mid_y - prev_my;
        }

        self.two_finger_prev_dist = Some(dist);
        self.two_finger_prev_mid = Some((mid_x, mid_y));
    }

    fn on_touch_end(&mut self, touch_id: u64, _x: f32, _y: f32, remaining_touches: u32) {
        let tid = touch_id as i64;
        if remaining_touches == 0 {
            self.two_finger_prev_dist = None;
            self.two_finger_prev_mid = None;
        }
        self.joystick.deactivate(tid);
        self.attack_button.release(tid);
        self.recruit_btn.release(tid);
        self.order_follow_btn.release(tid);
        self.order_charge_btn.release(tid);
        self.order_defend_btn.release(tid);
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_id = None;
        }
    }

    // ── Consumption methods ─────────────────────────────────────────────

    pub fn take_pinch_zoom(&mut self) -> f32 {
        let z = self.pinch_zoom;
        self.pinch_zoom = 0.0;
        z
    }

    pub fn take_touch_pan(&mut self) -> (f32, f32) {
        let r = (self.touch_pan_x, self.touch_pan_y);
        self.touch_pan_x = 0.0;
        self.touch_pan_y = 0.0;
        r
    }

    pub fn take_camera_drag(&mut self) -> (f32, f32) {
        let r = (self.camera_drag_dx, self.camera_drag_dy);
        self.camera_drag_dx = 0.0;
        self.camera_drag_dy = 0.0;
        r
    }

    // ── Build player input ──────────────────────────────────────────────

    pub fn build_player_input(&mut self) -> PlayerInput {
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
        let joy_active = self.joystick.active
            && (self.joystick.dx.abs() > 0.01 || self.joystick.dy.abs() > 0.01);

        // Priority: keyboard > touch joystick > gamepad
        let (raw_x, raw_y) = if kb_any {
            (kb_raw_x, kb_raw_y)
        } else if joy_active {
            (self.joystick.dx, self.joystick.dy)
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
        let touch_attack = self.attack_button.pressed;

        // Consume touch order flags
        let touch_follow = self.order_follow_pressed;
        self.order_follow_pressed = false;
        let touch_charge = self.order_charge_pressed;
        self.order_charge_pressed = false;
        let touch_defend = self.order_defend_pressed;
        self.order_defend_pressed = false;

        PlayerInput {
            move_x,
            move_y,
            attack: kb_attack || gp_attack || touch_attack,
            aim_dir: self.aim_dir,
            aim_lock,
            recruit: false,
            order_follow: self.pressed_this_frame(KeyCode::KeyJ)
                || self.gp_pressed(GpButton::West)
                || touch_follow,
            order_charge: self.pressed_this_frame(KeyCode::KeyL)
                || self.gp_pressed(GpButton::East)
                || touch_charge,
            order_defend: self.pressed_this_frame(KeyCode::KeyK)
                || self.gp_pressed(GpButton::North)
                || touch_defend,
        }
    }
}
