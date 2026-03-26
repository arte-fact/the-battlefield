use battlefield_core::player_input::PlayerInput;
use sdl2::controller::{Axis, Button};
use sdl2::event::Event;
use sdl2::keyboard::Scancode;
use sdl2::mouse::MouseButton;
use sdl2::mouse::MouseWheelDirection;
use std::collections::{HashMap, HashSet};

// ---- Virtual Joystick ----

const JOYSTICK_DEAD_ZONE: f32 = 5.0;
const JOYSTICK_MAX_RADIUS: f32 = 60.0;

/// Virtual joystick for touch input.
pub struct VirtualJoystick {
    pub active: bool,
    touch_id: i64,
    pub center_x: f32,
    pub center_y: f32,
    pub stick_x: f32,
    pub stick_y: f32,
    pub max_radius: f32,
    pub dead_zone: f32,
    pub dx: f32,
    pub dy: f32,
}

impl VirtualJoystick {
    pub fn new() -> Self {
        Self {
            active: false,
            touch_id: -1,
            center_x: 0.0,
            center_y: 0.0,
            stick_x: 0.0,
            stick_y: 0.0,
            max_radius: JOYSTICK_MAX_RADIUS,
            dead_zone: JOYSTICK_DEAD_ZONE,
            dx: 0.0,
            dy: 0.0,
        }
    }

    pub fn activate(&mut self, touch_id: i64, x: f32, y: f32) {
        self.active = true;
        self.touch_id = touch_id;
        self.center_x = x;
        self.center_y = y;
        self.stick_x = x;
        self.stick_y = y;
        self.dx = 0.0;
        self.dy = 0.0;
    }

    pub fn update(&mut self, touch_id: i64, x: f32, y: f32) {
        if !self.active || touch_id != self.touch_id {
            return;
        }
        let raw_dx = x - self.center_x;
        let raw_dy = y - self.center_y;
        let dist = (raw_dx * raw_dx + raw_dy * raw_dy).sqrt();
        if dist > self.max_radius {
            self.stick_x = self.center_x + raw_dx / dist * self.max_radius;
            self.stick_y = self.center_y + raw_dy / dist * self.max_radius;
            self.dx = raw_dx / dist;
            self.dy = raw_dy / dist;
        } else {
            self.stick_x = x;
            self.stick_y = y;
            if dist > self.dead_zone {
                self.dx = raw_dx / self.max_radius;
                self.dy = raw_dy / self.max_radius;
            } else {
                self.dx = 0.0;
                self.dy = 0.0;
            }
        }
    }

    pub fn deactivate(&mut self, touch_id: i64) {
        if touch_id == self.touch_id {
            self.active = false;
            self.dx = 0.0;
            self.dy = 0.0;
        }
    }
}

// ---- Action Button ----

/// Circular touch button for attack and order actions.
pub struct ActionButton {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
    pub pressed: bool,
    touch_id: Option<i64>,
}

impl ActionButton {
    pub fn new(cx: f32, cy: f32, radius: f32) -> Self {
        Self {
            center_x: cx,
            center_y: cy,
            radius,
            pressed: false,
            touch_id: None,
        }
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        let dx = x - self.center_x;
        let dy = y - self.center_y;
        (dx * dx + dy * dy).sqrt() <= self.radius
    }

    pub fn press(&mut self, touch_id: i64) {
        self.pressed = true;
        self.touch_id = Some(touch_id);
    }

    pub fn release(&mut self, touch_id: i64) {
        if self.touch_id == Some(touch_id) {
            self.pressed = false;
            self.touch_id = None;
        }
    }
}

// ---- Input State ----

/// Tracks keyboard, mouse, gamepad, and touch state; produces PlayerInput each frame.
pub struct InputState {
    // Keyboard
    pressed_this_frame: HashSet<Scancode>,
    pub scroll_delta: f32,
    pub aim_dir: f32,

    // Mouse
    pub mouse_x: i32,
    pub mouse_y: i32,
    pub mouse_clicked: bool,

    // Gamepad
    pub gamepad_connected: bool,
    left_stick_x: f32,
    left_stick_y: f32,
    trigger_left: f32,
    trigger_right: f32,
    gamepad_buttons_down: HashSet<Button>,
    gamepad_pressed_this_frame: HashSet<Button>,
    pub focused_button: usize,

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
    camera_drag_id: Option<i64>,
    camera_drag_prev: (f32, f32),
    camera_drag_dx: f32,
    camera_drag_dy: f32,

    // Touch: two-finger gestures
    two_finger_prev_dist: Option<f32>,
    two_finger_prev_mid: Option<(f32, f32)>,
    pinch_zoom: f32,
    touch_pan_x: f32,
    touch_pan_y: f32,

    // Active finger tracking (finger_id → last pixel position)
    active_fingers: HashMap<i64, (f32, f32)>,

    // Canvas dimensions for coordinate conversion (updated each frame)
    canvas_w: f32,
    canvas_h: f32,
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
        }
    }

    /// Call at the start of each frame to reset per-frame state.
    pub fn begin_frame(&mut self) {
        self.pressed_this_frame.clear();
        self.gamepad_pressed_this_frame.clear();
        self.scroll_delta = 0.0;
        self.mouse_clicked = false;
    }

    /// Update canvas dimensions (for touch coordinate conversion).
    pub fn set_canvas_size(&mut self, w: f32, h: f32) {
        self.canvas_w = w;
        self.canvas_h = h;
    }

    /// Returns true if the given scancode was pressed this frame (edge-triggered).
    pub fn pressed_this_frame(&self, sc: Scancode) -> bool {
        self.pressed_this_frame.contains(&sc)
    }

    /// Process an SDL event to record key presses, mouse, and touch.
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
            Event::FingerDown {
                finger_id, x, y, ..
            } => {
                let px = *x * self.canvas_w;
                let py = *y * self.canvas_h;
                self.is_touch_device = true;
                self.active_fingers.insert(*finger_id, (px, py));
                let total = self.active_fingers.len() as u32;
                self.on_touch_start(*finger_id, px, py, total, self.canvas_w);
            }
            Event::FingerMotion {
                finger_id, x, y, ..
            } => {
                let px = *x * self.canvas_w;
                let py = *y * self.canvas_h;
                self.active_fingers.insert(*finger_id, (px, py));
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
                    self.on_touch_move_single(*finger_id, px, py);
                }
            }
            Event::FingerUp {
                finger_id, x, y, ..
            } => {
                let px = *x * self.canvas_w;
                let py = *y * self.canvas_h;
                self.active_fingers.remove(finger_id);
                let remaining = self.active_fingers.len() as u32;
                self.on_touch_end(*finger_id, px, py, remaining);
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

    pub fn gamepad_pressed(&self, btn: Button) -> bool {
        self.gamepad_pressed_this_frame.contains(&btn)
    }

    pub fn gamepad_held(&self, btn: Button) -> bool {
        self.gamepad_buttons_down.contains(&btn)
    }

    pub fn gamepad_movement(&self) -> (f32, f32) {
        (self.left_stick_x, self.left_stick_y)
    }

    pub fn gamepad_zoom_delta(&self) -> f32 {
        self.trigger_right
    }

    pub fn gamepad_aim_lock(&self) -> bool {
        self.trigger_left > 0.5
    }

    // ---- Touch layout ----

    /// Update touch control positions and sizes based on canvas size and DPR.
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

    // ---- Touch routing ----

    fn try_order_buttons(&mut self, touch_id: i64, x: f32, y: f32) -> bool {
        if self.order_follow_btn.contains(x, y) {
            self.order_follow_btn.press(touch_id);
            self.order_follow_pressed = true;
            return true;
        }
        if self.order_charge_btn.contains(x, y) {
            self.order_charge_btn.press(touch_id);
            self.order_charge_pressed = true;
            return true;
        }
        if self.order_defend_btn.contains(x, y) {
            self.order_defend_btn.press(touch_id);
            self.order_defend_pressed = true;
            return true;
        }
        false
    }

    fn on_touch_start(
        &mut self,
        touch_id: i64,
        x: f32,
        y: f32,
        total_touches: u32,
        canvas_width: f32,
    ) {
        if total_touches >= 2 {
            if self.joystick.active || self.attack_button.pressed {
                if !self.joystick.active && x < canvas_width * 0.4 {
                    self.joystick.activate(touch_id, x, y);
                    self.has_used_joystick = true;
                    return;
                }
                if self.try_order_buttons(touch_id, x, y) {
                    return;
                }
                if !self.attack_button.pressed && self.attack_button.contains(x, y) {
                    self.attack_button.press(touch_id);
                    self.attack_pressed = true;
                    return;
                }
            }
            return;
        }

        if x < canvas_width * 0.4 {
            self.joystick.activate(touch_id, x, y);
            self.has_used_joystick = true;
            return;
        }

        if self.try_order_buttons(touch_id, x, y) {
            return;
        }

        if self.attack_button.contains(x, y) {
            self.attack_button.press(touch_id);
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

    fn on_touch_move_single(&mut self, touch_id: i64, x: f32, y: f32) {
        self.joystick.update(touch_id, x, y);
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

    fn on_touch_end(&mut self, touch_id: i64, _x: f32, _y: f32, remaining_touches: u32) {
        if remaining_touches == 0 {
            self.two_finger_prev_dist = None;
            self.two_finger_prev_mid = None;
        }
        self.joystick.deactivate(touch_id);
        self.attack_button.release(touch_id);
        self.recruit_btn.release(touch_id);
        self.order_follow_btn.release(touch_id);
        self.order_charge_btn.release(touch_id);
        self.order_defend_btn.release(touch_id);
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_id = None;
        }
    }

    // ---- Consumption methods ----

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

    #[allow(dead_code)]
    pub fn take_attack_pressed(&mut self) -> bool {
        let r = self.attack_pressed;
        self.attack_pressed = false;
        r
    }

    // ---- Build player input ----

    /// Build a PlayerInput from the current keyboard, gamepad, and touch state.
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

        let kb_aim_lock = keyboard.is_scancode_pressed(Scancode::LCtrl)
            || keyboard.is_scancode_pressed(Scancode::RCtrl);
        let gp_aim_lock = self.gamepad_aim_lock();
        let aim_lock = kb_aim_lock || gp_aim_lock;

        let kb_attack = keyboard.is_scancode_pressed(Scancode::Space);
        let gp_attack = self.gamepad_held(Button::A) || self.gamepad_pressed(Button::A);
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
            order_follow: self.pressed_this_frame.contains(&Scancode::J)
                || self.gamepad_pressed(Button::X)
                || touch_follow,
            order_charge: self.pressed_this_frame.contains(&Scancode::L)
                || self.gamepad_pressed(Button::B)
                || touch_charge,
            order_defend: self.pressed_this_frame.contains(&Scancode::K)
                || self.gamepad_pressed(Button::Y)
                || touch_defend,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joystick_activate_deactivate() {
        let mut joy = VirtualJoystick::new();
        assert!(!joy.active);
        joy.activate(42, 100.0, 200.0);
        assert!(joy.active);
        assert!(joy.dx.abs() < f32::EPSILON);
        joy.deactivate(42);
        assert!(!joy.active);
    }

    #[test]
    fn joystick_update_within_radius() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 130.0, 100.0);
        assert!((joy.dx - 0.5).abs() < 0.01);
        assert!(joy.dy.abs() < 0.01);
    }

    #[test]
    fn joystick_update_beyond_radius() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 200.0, 100.0);
        assert!((joy.dx - 1.0).abs() < 0.01);
        assert!((joy.stick_x - 160.0).abs() < 0.01);
    }

    #[test]
    fn joystick_dead_zone() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 103.0, 101.0);
        assert!(joy.dx.abs() < f32::EPSILON);
        assert!(joy.dy.abs() < f32::EPSILON);
    }

    #[test]
    fn joystick_ignores_wrong_touch_id() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(99, 200.0, 200.0);
        assert!(joy.dx.abs() < f32::EPSILON);
        joy.deactivate(99);
        assert!(joy.active);
    }

    #[test]
    fn action_button_hit_test() {
        let btn = ActionButton::new(100.0, 100.0, 40.0);
        assert!(btn.contains(100.0, 100.0));
        assert!(btn.contains(130.0, 100.0));
        assert!(!btn.contains(150.0, 100.0));
    }

    #[test]
    fn action_button_press_release() {
        let mut btn = ActionButton::new(100.0, 100.0, 40.0);
        assert!(!btn.pressed);
        btn.press(5);
        assert!(btn.pressed);
        btn.release(5);
        assert!(!btn.pressed);
    }

    #[test]
    fn pinch_zoom_accumulates() {
        let mut input = InputState::new();
        input.on_touch_move_two_finger(0.0, 0.0, 100.0, 0.0);
        input.on_touch_move_two_finger(0.0, 0.0, 150.0, 0.0);
        let zoom = input.take_pinch_zoom();
        assert!((zoom - 0.5).abs() < 0.01);
        assert!(input.take_pinch_zoom().abs() < f32::EPSILON);
    }
}
