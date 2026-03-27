//! Platform-agnostic touch input primitives.
//!
//! [`VirtualJoystick`] and [`ActionButton`] are pure geometry — no graphics
//! or platform types.  Renderers embed them in their own input state.

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
}
