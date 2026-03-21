use std::collections::HashSet;

/// 8-directional swipe direction (used by legacy turn-based test shim).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDir {
    N,
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
}

impl SwipeDir {
    /// Grid delta (dx, dy) for this direction.
    pub fn delta(self) -> (i32, i32) {
        match self {
            SwipeDir::N => (0, -1),
            SwipeDir::NE => (1, -1),
            SwipeDir::E => (1, 0),
            SwipeDir::SE => (1, 1),
            SwipeDir::S => (0, 1),
            SwipeDir::SW => (-1, 1),
            SwipeDir::W => (-1, 0),
            SwipeDir::NW => (-1, -1),
        }
    }

    /// Normalized float direction vector (diagonals ≈ 0.707).
    pub fn delta_f32(self) -> (f32, f32) {
        let (dx, dy) = self.delta();
        let (fx, fy) = (dx as f32, dy as f32);
        let len = (fx * fx + fy * fy).sqrt();
        if len > 0.0 {
            (fx / len, fy / len)
        } else {
            (0.0, 0.0)
        }
    }

    /// Classify a screen-space delta into one of 8 directions.
    /// Returns None if the distance is below threshold.
    pub fn from_delta(dx: f32, dy: f32, threshold: f32) -> Option<Self> {
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < threshold {
            return None;
        }
        let angle = dy.atan2(dx);
        let angle = if angle < 0.0 {
            angle + std::f32::consts::TAU
        } else {
            angle
        };
        let sector =
            ((angle + std::f32::consts::FRAC_PI_8) / std::f32::consts::FRAC_PI_4) as i32 % 8;
        Some(match sector {
            0 => SwipeDir::E,
            1 => SwipeDir::SE,
            2 => SwipeDir::S,
            3 => SwipeDir::SW,
            4 => SwipeDir::W,
            5 => SwipeDir::NW,
            6 => SwipeDir::N,
            7 => SwipeDir::NE,
            _ => unreachable!(),
        })
    }

    /// Classify integer grid-space delta into a direction.
    pub fn from_grid_delta(dx: i32, dy: i32) -> Option<Self> {
        if dx == 0 && dy == 0 {
            return None;
        }
        Self::from_delta(dx as f32, dy as f32, 0.5)
    }
}

// ---- Virtual Joystick ----

/// Canvas-rendered virtual joystick for mobile touch input.
pub struct VirtualJoystick {
    /// Whether the joystick is active (finger is down).
    pub active: bool,
    /// Touch ID tracking this joystick.
    touch_id: i32,
    /// Center position of the joystick base (screen coords).
    pub center_x: f32,
    pub center_y: f32,
    /// Current stick knob position (screen coords).
    pub stick_x: f32,
    pub stick_y: f32,
    /// Maximum stick displacement radius.
    pub max_radius: f32,
    /// Dead zone radius (below this distance, output is zero).
    pub dead_zone: f32,
    /// Normalized output direction (-1..1 each axis).
    pub dx: f32,
    pub dy: f32,
}

const JOYSTICK_DEAD_ZONE: f32 = 5.0;
const JOYSTICK_MAX_RADIUS: f32 = 60.0;

impl Default for VirtualJoystick {
    fn default() -> Self {
        Self::new()
    }
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

    /// Activate joystick on touch start in the left zone.
    pub fn activate(&mut self, touch_id: i32, x: f32, y: f32) {
        self.active = true;
        self.touch_id = touch_id;
        self.center_x = x;
        self.center_y = y;
        self.stick_x = x;
        self.stick_y = y;
        self.dx = 0.0;
        self.dy = 0.0;
    }

    /// Update joystick on touch move.
    pub fn update(&mut self, touch_id: i32, x: f32, y: f32) {
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

    /// Deactivate joystick on touch end.
    pub fn deactivate(&mut self, touch_id: i32) {
        if touch_id == self.touch_id {
            self.active = false;
            self.dx = 0.0;
            self.dy = 0.0;
        }
    }
}

// ---- Action Button ----

/// Canvas-rendered action button for mobile touch input.
pub struct ActionButton {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
    pub pressed: bool,
    touch_id: Option<i32>,
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

    pub fn press(&mut self, touch_id: i32) {
        self.pressed = true;
        self.touch_id = Some(touch_id);
    }

    pub fn release(&mut self, touch_id: i32) {
        if self.touch_id == Some(touch_id) {
            self.pressed = false;
            self.touch_id = None;
        }
    }
}

// ---- Input ----

/// Tracks keyboard and touch input state.
pub struct Input {
    pub keys_down: HashSet<String>,
    pub scroll_delta: f32,

    // Touch: two-finger gestures (pinch/pan)
    /// Previous two-finger distance (for pinch detection)
    two_finger_prev_dist: Option<f32>,
    /// Previous two-finger midpoint (for pan detection)
    two_finger_prev_mid: Option<(f32, f32)>,
    /// Accumulated pinch zoom delta
    pub pinch_zoom: f32,
    /// Accumulated two-finger pan delta
    pub touch_pan_x: f32,
    pub touch_pan_y: f32,

    // Touch controls
    pub joystick: VirtualJoystick,
    pub attack_button: ActionButton,
    /// Order buttons for touch (Hold, Go, Retreat, Follow).
    pub order_hold_btn: ActionButton,
    pub order_go_btn: ActionButton,
    pub order_retreat_btn: ActionButton,
    pub order_follow_btn: ActionButton,
    /// Set on first touch event; enables touch control rendering.
    pub is_touch_device: bool,
    /// True after the player has used the joystick at least once (hides ghost hint).
    pub has_used_joystick: bool,
    /// Attack button was pressed this frame (consumed on read).
    pub attack_pressed: bool,
    /// Keyboard attack key (space) was pressed this frame (consumed on read).
    attack_key_pressed: bool,
    /// Order keys pressed this frame (consumed on read).
    order_hold_pressed: bool,
    order_go_pressed: bool,
    order_retreat_pressed: bool,
    order_follow_pressed: bool,

    // Single-finger camera drag (right side of screen, not on any button)
    /// Touch ID for camera drag, if active.
    camera_drag_id: Option<i32>,
    /// Previous position for camera drag delta.
    camera_drag_prev: (f32, f32),
    /// Accumulated camera drag delta (consumed each frame).
    pub camera_drag_dx: f32,
    pub camera_drag_dy: f32,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        Self {
            keys_down: HashSet::new(),
            scroll_delta: 0.0,
            two_finger_prev_dist: None,
            two_finger_prev_mid: None,
            pinch_zoom: 0.0,
            touch_pan_x: 0.0,
            touch_pan_y: 0.0,
            joystick: VirtualJoystick::new(),
            attack_button: ActionButton::new(0.0, 0.0, 56.0),
            order_hold_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_go_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_retreat_btn: ActionButton::new(0.0, 0.0, 28.0),
            order_follow_btn: ActionButton::new(0.0, 0.0, 28.0),
            is_touch_device: false,
            has_used_joystick: false,
            attack_pressed: false,
            attack_key_pressed: false,
            order_hold_pressed: false,
            order_go_pressed: false,
            order_retreat_pressed: false,
            order_follow_pressed: false,
            camera_drag_id: None,
            camera_drag_prev: (0.0, 0.0),
            camera_drag_dx: 0.0,
            camera_drag_dy: 0.0,
        }
    }

    /// Update touch control positions and sizes based on canvas size and DPR.
    pub fn update_layout(&mut self, canvas_w: f32, canvas_h: f32, dpr: f32) {
        // Attack button: bottom-right with comfortable margin
        let atk_radius = 56.0 * dpr;
        let atk_margin = 90.0 * dpr;
        let atk_cx = canvas_w - atk_margin;
        let atk_cy = canvas_h - atk_margin;
        self.attack_button.center_x = atk_cx;
        self.attack_button.center_y = atk_cy;
        self.attack_button.radius = atk_radius;

        // Order buttons: arranged around the attack button (left, top-left, top, top-right)
        let ord_radius = 36.0 * dpr;
        let spacing = atk_radius + ord_radius + 12.0 * dpr;

        // Left of ATK
        self.order_hold_btn.center_x = atk_cx - spacing;
        self.order_hold_btn.center_y = atk_cy;
        self.order_hold_btn.radius = ord_radius;

        // Top-left of ATK
        let diag = spacing * 0.707;
        self.order_go_btn.center_x = atk_cx - diag;
        self.order_go_btn.center_y = atk_cy - diag;
        self.order_go_btn.radius = ord_radius;

        // Top of ATK
        self.order_retreat_btn.center_x = atk_cx;
        self.order_retreat_btn.center_y = atk_cy - spacing;
        self.order_retreat_btn.radius = ord_radius;

        // Top-right of ATK
        self.order_follow_btn.center_x = atk_cx + diag;
        self.order_follow_btn.center_y = atk_cy - diag;
        self.order_follow_btn.radius = ord_radius;

        // Joystick: smaller radius for higher sensitivity
        self.joystick.max_radius = 40.0 * dpr;
        self.joystick.dead_zone = 4.0 * dpr;
    }

    pub fn key_down(&mut self, key: String) {
        // Normalize single-char keys to lowercase for consistent matching
        // across keyboard layouts and modifier states.
        let key = if key.len() == 1 {
            key.to_lowercase()
        } else {
            key
        };
        if key == " " && !self.keys_down.contains(" ") {
            self.attack_key_pressed = true;
        }
        if key == "h" && !self.keys_down.contains("h") {
            self.order_hold_pressed = true;
        }
        if key == "g" && !self.keys_down.contains("g") {
            self.order_go_pressed = true;
        }
        if key == "r" && !self.keys_down.contains("r") {
            self.order_retreat_pressed = true;
        }
        if key == "f" && !self.keys_down.contains("f") {
            self.order_follow_pressed = true;
        }
        self.keys_down.insert(key);
    }

    pub fn key_up(&mut self, key: &str) {
        if key.len() == 1 {
            self.keys_down.remove(&key.to_lowercase());
        } else {
            self.keys_down.remove(key);
        }
    }

    pub fn is_key_down(&self, key: &str) -> bool {
        self.keys_down.contains(key)
    }

    /// Compute movement direction from WASD, ZQSD (AZERTY), and Arrow keys.
    /// Returns a normalized (dx, dy) direction vector, or (0, 0) if no keys held.
    /// Note: single-char keys are stored lowercase by key_down().
    pub fn movement_direction(&self) -> (f32, f32) {
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;

        // Left: A (QWERTY) / Q (AZERTY) / Arrow
        if self.is_key_down("a") || self.is_key_down("q") || self.is_key_down("ArrowLeft") {
            dx -= 1.0;
        }
        // Right: D / Arrow
        if self.is_key_down("d") || self.is_key_down("ArrowRight") {
            dx += 1.0;
        }
        // Up: W (QWERTY) / Z (AZERTY) / Arrow
        if self.is_key_down("w") || self.is_key_down("z") || self.is_key_down("ArrowUp") {
            dy -= 1.0;
        }
        // Down: S / Arrow
        if self.is_key_down("s") || self.is_key_down("ArrowDown") {
            dy += 1.0;
        }

        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.0 {
            (dx / len, dy / len)
        } else {
            (0.0, 0.0)
        }
    }

    /// Consume scroll delta.
    pub fn take_scroll(&mut self) -> f32 {
        let d = self.scroll_delta;
        self.scroll_delta = 0.0;
        d
    }

    /// Consume keyboard attack key press (space bar).
    pub fn take_attack_key(&mut self) -> bool {
        let r = self.attack_key_pressed;
        self.attack_key_pressed = false;
        r
    }

    /// Consume Hold order key press (H).
    pub fn take_order_hold(&mut self) -> bool {
        let r = self.order_hold_pressed;
        self.order_hold_pressed = false;
        r
    }

    /// Consume Go order key press (G).
    pub fn take_order_go(&mut self) -> bool {
        let r = self.order_go_pressed;
        self.order_go_pressed = false;
        r
    }

    /// Consume Retreat order key press (R).
    pub fn take_order_retreat(&mut self) -> bool {
        let r = self.order_retreat_pressed;
        self.order_retreat_pressed = false;
        r
    }

    /// Consume Follow order key press (F).
    pub fn take_order_follow(&mut self) -> bool {
        let r = self.order_follow_pressed;
        self.order_follow_pressed = false;
        r
    }

    /// Consume a specific key press (returns true if that key was just pressed).
    pub fn take_key(&mut self, key: &str) -> bool {
        self.keys_down.remove(key)
    }

    /// Clear all input state (used on screen transitions).
    pub fn clear_all(&mut self) {
        self.keys_down.clear();
        self.scroll_delta = 0.0;
        self.attack_key_pressed = false;
        self.order_hold_pressed = false;
        self.order_go_pressed = false;
        self.order_retreat_pressed = false;
        self.order_follow_pressed = false;
        self.joystick.active = false;
        self.joystick.dx = 0.0;
        self.joystick.dy = 0.0;
        self.attack_button.pressed = false;
    }

    // -- Touch methods --

    /// Check if a touch hits any order button; if so, press it and set the flag.
    fn try_order_buttons(&mut self, touch_id: i32, x: f32, y: f32) -> bool {
        if self.order_hold_btn.contains(x, y) {
            self.order_hold_btn.press(touch_id);
            self.order_hold_pressed = true;
            return true;
        }
        if self.order_go_btn.contains(x, y) {
            self.order_go_btn.press(touch_id);
            self.order_go_pressed = true;
            return true;
        }
        if self.order_retreat_btn.contains(x, y) {
            self.order_retreat_btn.press(touch_id);
            self.order_retreat_pressed = true;
            return true;
        }
        if self.order_follow_btn.contains(x, y) {
            self.order_follow_btn.press(touch_id);
            self.order_follow_pressed = true;
            return true;
        }
        false
    }

    /// Called on touchstart. Routes to joystick, attack button, or order buttons.
    pub fn on_touch_start(
        &mut self,
        touch_id: i32,
        x: f32,
        y: f32,
        total_touches: u32,
        canvas_width: f32,
    ) {
        self.is_touch_device = true;

        if total_touches >= 2 {
            // If a control is already active, try routing the new touch to the other control
            // instead of treating it as a camera gesture. This allows joystick + attack simultaneously.
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
            // No control hit → camera gesture
            return;
        }

        // Left 40% of screen → joystick
        if x < canvas_width * 0.4 {
            self.joystick.activate(touch_id, x, y);
            self.has_used_joystick = true;
            return;
        }

        // Check order buttons first (they sit above attack button)
        if self.try_order_buttons(touch_id, x, y) {
            return;
        }

        // Check attack button
        if self.attack_button.contains(x, y) {
            self.attack_button.press(touch_id);
            self.attack_pressed = true;
            return;
        }

        // Right side, no button hit → camera drag
        if self.camera_drag_id.is_none() {
            self.camera_drag_id = Some(touch_id);
            self.camera_drag_prev = (x, y);
        }
    }

    /// Returns true if any touch control (joystick, attack, order, or camera drag) is currently active.
    pub fn has_active_control(&self) -> bool {
        self.joystick.active
            || self.attack_button.pressed
            || self.order_hold_btn.pressed
            || self.order_go_btn.pressed
            || self.order_retreat_btn.pressed
            || self.order_follow_btn.pressed
            || self.camera_drag_id.is_some()
    }

    /// Called on touchmove when a single finger is active.
    pub fn on_touch_move_single(&mut self, touch_id: i32, x: f32, y: f32) {
        self.joystick.update(touch_id, x, y);
        // Camera drag
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_dx += x - self.camera_drag_prev.0;
            self.camera_drag_dy += y - self.camera_drag_prev.1;
            self.camera_drag_prev = (x, y);
        }
    }

    /// Called on touchmove when two fingers are active.
    pub fn on_touch_move_two_finger(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
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

    /// Called on touchend.
    pub fn on_touch_end(&mut self, touch_id: i32, _x: f32, _y: f32, remaining_touches: u32) {
        if remaining_touches == 0 {
            self.two_finger_prev_dist = None;
            self.two_finger_prev_mid = None;
        }

        self.joystick.deactivate(touch_id);
        self.attack_button.release(touch_id);
        self.order_hold_btn.release(touch_id);
        self.order_go_btn.release(touch_id);
        self.order_retreat_btn.release(touch_id);
        self.order_follow_btn.release(touch_id);
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_id = None;
        }
    }

    /// Consume pinch zoom delta.
    pub fn take_pinch_zoom(&mut self) -> f32 {
        let z = self.pinch_zoom;
        self.pinch_zoom = 0.0;
        z
    }

    /// Consume two-finger pan delta.
    pub fn take_touch_pan(&mut self) -> (f32, f32) {
        let x = self.touch_pan_x;
        let y = self.touch_pan_y;
        self.touch_pan_x = 0.0;
        self.touch_pan_y = 0.0;
        (x, y)
    }

    /// Consume single-finger camera drag delta.
    pub fn take_camera_drag(&mut self) -> (f32, f32) {
        let dx = self.camera_drag_dx;
        let dy = self.camera_drag_dy;
        self.camera_drag_dx = 0.0;
        self.camera_drag_dy = 0.0;
        (dx, dy)
    }

    /// Consume attack button press.
    pub fn take_attack_pressed(&mut self) -> bool {
        let r = self.attack_pressed;
        self.attack_pressed = false;
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_tracking() {
        let mut input = Input::new();
        assert!(!input.is_key_down("a"));
        input.key_down("a".to_string());
        assert!(input.is_key_down("a"));
        input.key_up("a");
        assert!(!input.is_key_down("a"));
    }

    #[test]
    fn uppercase_keys_normalized_to_lowercase() {
        let mut input = Input::new();
        // Pressing Shift+S sends "S" — should be stored as "s"
        input.key_down("S".to_string());
        assert!(input.is_key_down("s"));
        // Releasing with key="S" should remove "s"
        input.key_up("S");
        assert!(!input.is_key_down("s"));
    }

    #[test]
    fn movement_direction_wasd() {
        let mut input = Input::new();
        input.key_down("d".to_string());
        input.key_down("w".to_string());
        let (dx, dy) = input.movement_direction();
        // Diagonal: should be normalized
        assert!((dx - 0.7071).abs() < 0.01);
        assert!((dy - (-0.7071)).abs() < 0.01);
    }

    #[test]
    fn movement_direction_arrows() {
        let mut input = Input::new();
        input.key_down("ArrowRight".to_string());
        let (dx, dy) = input.movement_direction();
        assert!((dx - 1.0).abs() < f32::EPSILON);
        assert!(dy.abs() < f32::EPSILON);
    }

    #[test]
    fn movement_direction_wasd_and_arrows_combine() {
        let mut input = Input::new();
        input.key_down("d".to_string());
        input.key_down("ArrowDown".to_string());
        let (dx, dy) = input.movement_direction();
        // d → right, ArrowDown → down = diagonal SE
        assert!((dx - 0.7071).abs() < 0.01);
        assert!((dy - 0.7071).abs() < 0.01);
    }

    #[test]
    fn movement_direction_no_keys() {
        let input = Input::new();
        let (dx, dy) = input.movement_direction();
        assert!(dx.abs() < f32::EPSILON);
        assert!(dy.abs() < f32::EPSILON);
    }

    #[test]
    fn attack_key_consumed() {
        let mut input = Input::new();
        assert!(!input.take_attack_key());
        input.key_down(" ".to_string());
        assert!(input.take_attack_key());
        assert!(!input.take_attack_key()); // consumed
    }

    #[test]
    fn azerty_movement() {
        let mut input = Input::new();
        input.key_down("z".to_string()); // AZERTY up
        input.key_down("q".to_string()); // AZERTY left
        let (dx, dy) = input.movement_direction();
        assert!((dx - (-0.7071)).abs() < 0.01);
        assert!((dy - (-0.7071)).abs() < 0.01);
    }

    // SwipeDir tests (kept for legacy test shim)

    #[test]
    fn swipe_dir_from_delta_cardinal() {
        assert_eq!(SwipeDir::from_delta(100.0, 0.0, 30.0), Some(SwipeDir::E));
        assert_eq!(SwipeDir::from_delta(-100.0, 0.0, 30.0), Some(SwipeDir::W));
        assert_eq!(SwipeDir::from_delta(0.0, 100.0, 30.0), Some(SwipeDir::S));
        assert_eq!(SwipeDir::from_delta(0.0, -100.0, 30.0), Some(SwipeDir::N));
    }

    #[test]
    fn swipe_dir_from_delta_diagonal() {
        assert_eq!(
            SwipeDir::from_delta(100.0, -100.0, 30.0),
            Some(SwipeDir::NE)
        );
        assert_eq!(SwipeDir::from_delta(100.0, 100.0, 30.0), Some(SwipeDir::SE));
        assert_eq!(
            SwipeDir::from_delta(-100.0, 100.0, 30.0),
            Some(SwipeDir::SW)
        );
        assert_eq!(
            SwipeDir::from_delta(-100.0, -100.0, 30.0),
            Some(SwipeDir::NW)
        );
    }

    #[test]
    fn swipe_dir_below_threshold_returns_none() {
        assert_eq!(SwipeDir::from_delta(5.0, 5.0, 30.0), None);
    }

    #[test]
    fn swipe_dir_grid_delta() {
        assert_eq!(SwipeDir::from_grid_delta(3, 0), Some(SwipeDir::E));
        assert_eq!(SwipeDir::from_grid_delta(-2, -2), Some(SwipeDir::NW));
        assert_eq!(SwipeDir::from_grid_delta(0, 0), None);
    }

    #[test]
    fn swipe_dir_deltas() {
        assert_eq!(SwipeDir::N.delta(), (0, -1));
        assert_eq!(SwipeDir::SE.delta(), (1, 1));
        assert_eq!(SwipeDir::W.delta(), (-1, 0));
    }

    // Joystick tests

    #[test]
    fn joystick_activate_deactivate() {
        let mut joy = VirtualJoystick::new();
        assert!(!joy.active);
        joy.activate(42, 100.0, 200.0);
        assert!(joy.active);
        assert!((joy.dx).abs() < f32::EPSILON);
        assert!((joy.dy).abs() < f32::EPSILON);
        joy.deactivate(42);
        assert!(!joy.active);
    }

    #[test]
    fn joystick_update_within_radius() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 130.0, 100.0); // 30px right, within 60px max_radius
        assert!((joy.dx - 0.5).abs() < 0.01); // 30/60 = 0.5
        assert!(joy.dy.abs() < 0.01);
    }

    #[test]
    fn joystick_update_beyond_radius() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 200.0, 100.0); // 100px right, beyond 60px max_radius
        assert!((joy.dx - 1.0).abs() < 0.01); // clamped to 1.0
                                              // Stick should be clamped to max_radius from center
        assert!((joy.stick_x - 160.0).abs() < 0.01);
    }

    #[test]
    fn joystick_dead_zone() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(1, 103.0, 101.0); // within dead zone (5px)
        assert!(joy.dx.abs() < f32::EPSILON);
        assert!(joy.dy.abs() < f32::EPSILON);
    }

    #[test]
    fn joystick_ignores_wrong_touch_id() {
        let mut joy = VirtualJoystick::new();
        joy.activate(1, 100.0, 100.0);
        joy.update(99, 200.0, 200.0); // wrong touch
        assert!(joy.dx.abs() < f32::EPSILON);
        joy.deactivate(99); // wrong touch
        assert!(joy.active); // still active
    }

    // Action button tests

    #[test]
    fn action_button_hit_test() {
        let btn = ActionButton::new(100.0, 100.0, 40.0);
        assert!(btn.contains(100.0, 100.0)); // center
        assert!(btn.contains(130.0, 100.0)); // within radius
        assert!(!btn.contains(150.0, 100.0)); // outside radius
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

    // Touch routing tests

    #[test]
    fn touch_left_zone_activates_joystick() {
        let mut input = Input::new();
        // Touch on left 40% of 960px canvas = x < 384
        input.on_touch_start(1, 100.0, 300.0, 1, 960.0);
        assert!(input.is_touch_device);
        assert!(input.joystick.active);
    }

    #[test]
    fn touch_right_zone_no_action() {
        let mut input = Input::new();
        // Touch on right side, outside attack button — no action
        input.on_touch_start(1, 600.0, 300.0, 1, 960.0);
        assert!(!input.joystick.active);
        assert!(!input.attack_pressed);
    }

    #[test]
    fn touch_attack_button() {
        let mut input = Input::new();
        input.update_layout(960.0, 640.0, 1.0);
        // Touch on the attack button (center at 870, 550, radius 56)
        input.on_touch_start(1, 870.0, 550.0, 1, 960.0);
        assert!(input.attack_pressed);
        assert!(input.attack_button.pressed);
    }

    #[test]
    fn touch_order_hold_button() {
        let mut input = Input::new();
        input.update_layout(960.0, 640.0, 1.0);
        // Touch the hold button (top of stack above attack)
        let hx = input.order_hold_btn.center_x;
        let hy = input.order_hold_btn.center_y;
        input.on_touch_start(1, hx, hy, 1, 960.0);
        assert!(input.take_order_hold());
        assert!(!input.take_order_hold()); // consumed
    }

    #[test]
    fn touch_order_go_button() {
        let mut input = Input::new();
        input.update_layout(960.0, 640.0, 1.0);
        let gx = input.order_go_btn.center_x;
        let gy = input.order_go_btn.center_y;
        input.on_touch_start(1, gx, gy, 1, 960.0);
        assert!(input.take_order_go());
    }

    #[test]
    fn pinch_zoom_accumulates() {
        let mut input = Input::new();
        input.on_touch_move_two_finger(0.0, 0.0, 100.0, 0.0); // dist=100, first reading
        input.on_touch_move_two_finger(0.0, 0.0, 150.0, 0.0); // dist=150, delta=+50
        let zoom = input.take_pinch_zoom();
        assert!((zoom - 0.5).abs() < 0.01); // 50/100
        assert!((input.take_pinch_zoom()).abs() < f32::EPSILON); // consumed
    }
}
