//! Platform-agnostic touch input.
//!
//! [`VirtualJoystick`] and [`ActionButton`] are pure geometry.
//! [`TouchControls`] owns the full touch stack (joystick, buttons, camera
//! gestures); renderers translate platform events into its methods.

use crate::unit::OrderRequest;
use std::collections::HashMap;

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

// ---- Touch Controls ----

/// Seconds the Dismiss button must be held before it fires.
pub const DISMISS_HOLD_SECS: f32 = 0.4;

/// Full touch control state shared by all renderers.
pub struct TouchControls {
    pub joystick: VirtualJoystick,
    pub attack: ActionButton,
    pub charge: ActionButton,
    pub defend: ActionButton,
    pub dismiss: ActionButton,
    pub is_touch_device: bool,
    pub has_used_joystick: bool,
    pending_order: Option<OrderRequest>,
    dismiss_held_secs: f32,
    dismiss_fired: bool,
    camera_drag_id: Option<u64>,
    camera_drag_prev: (f32, f32),
    camera_drag_dx: f32,
    camera_drag_dy: f32,
    two_finger_prev_dist: Option<f32>,
    two_finger_prev_mid: Option<(f32, f32)>,
    pinch_zoom: f32,
    touch_pan_x: f32,
    touch_pan_y: f32,
    active_fingers: HashMap<u64, (f32, f32)>,
    canvas_w: f32,
}

impl Default for TouchControls {
    fn default() -> Self {
        Self::new()
    }
}

impl TouchControls {
    pub fn new() -> Self {
        Self {
            joystick: VirtualJoystick::new(),
            attack: ActionButton::new(0.0, 0.0, 56.0),
            charge: ActionButton::new(0.0, 0.0, 28.0),
            defend: ActionButton::new(0.0, 0.0, 28.0),
            dismiss: ActionButton::new(0.0, 0.0, 28.0),
            is_touch_device: false,
            has_used_joystick: false,
            pending_order: None,
            dismiss_held_secs: 0.0,
            dismiss_fired: false,
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
        }
    }

    pub fn update_layout(&mut self, canvas_w: f32, canvas_h: f32, dpr: f32) {
        self.canvas_w = canvas_w;
        let atk_radius = 56.0 * dpr;
        let atk_margin = 90.0 * dpr;
        let atk_cx = canvas_w - atk_margin;
        let atk_cy = canvas_h - atk_margin;
        self.attack.center_x = atk_cx;
        self.attack.center_y = atk_cy;
        self.attack.radius = atk_radius;

        let ord_radius = 36.0 * dpr;
        let spacing = atk_radius + ord_radius + 12.0 * dpr;
        let diag = spacing * 0.707;

        self.charge.center_x = atk_cx - spacing;
        self.charge.center_y = atk_cy;
        self.charge.radius = ord_radius;

        self.defend.center_x = atk_cx - diag;
        self.defend.center_y = atk_cy - diag;
        self.defend.radius = ord_radius;

        self.dismiss.center_x = atk_cx;
        self.dismiss.center_y = atk_cy - spacing;
        self.dismiss.radius = ord_radius;

        self.joystick.max_radius = 40.0 * dpr;
        self.joystick.dead_zone = 4.0 * dpr;
    }

    /// Advance hold timers. Call once per frame.
    pub fn tick(&mut self, dt: f32) {
        if self.dismiss.pressed {
            self.dismiss_held_secs += dt;
            if self.dismiss_held_secs >= DISMISS_HOLD_SECS && !self.dismiss_fired {
                self.dismiss_fired = true;
                self.pending_order = Some(OrderRequest::Dismiss);
            }
        } else {
            self.dismiss_held_secs = 0.0;
            self.dismiss_fired = false;
        }
    }

    /// Dismiss hold progress 0..1 for the button fill ring.
    pub fn dismiss_hold_frac(&self) -> f32 {
        (self.dismiss_held_secs / DISMISS_HOLD_SECS).min(1.0)
    }

    /// Consume the pending order (edge-triggered, once per press).
    pub fn take_order(&mut self) -> Option<OrderRequest> {
        self.pending_order.take()
    }

    fn try_buttons(&mut self, tid: i64, x: f32, y: f32) -> bool {
        if self.charge.contains(x, y) {
            self.charge.press(tid);
            self.pending_order = Some(OrderRequest::Charge);
            return true;
        }
        if self.defend.contains(x, y) {
            self.defend.press(tid);
            self.pending_order = Some(OrderRequest::Defend);
            return true;
        }
        if self.dismiss.contains(x, y) {
            self.dismiss.press(tid);
            return true;
        }
        if !self.attack.pressed && self.attack.contains(x, y) {
            self.attack.press(tid);
            return true;
        }
        false
    }

    pub fn on_touch_start(&mut self, touch_id: u64, x: f32, y: f32, total_touches: u32) {
        self.is_touch_device = true;
        self.active_fingers.insert(touch_id, (x, y));
        let tid = touch_id as i64;
        if total_touches >= 2 {
            if self.joystick.active || self.attack.pressed {
                if !self.joystick.active && x < self.canvas_w * 0.4 {
                    self.joystick.activate(tid, x, y);
                    self.has_used_joystick = true;
                    return;
                }
                self.try_buttons(tid, x, y);
            }
            return;
        }

        if x < self.canvas_w * 0.4 {
            self.joystick.activate(tid, x, y);
            self.has_used_joystick = true;
            return;
        }

        if self.try_buttons(tid, x, y) {
            return;
        }

        if self.camera_drag_id.is_none() {
            self.camera_drag_id = Some(touch_id);
            self.camera_drag_prev = (x, y);
        }
    }

    pub fn on_touch_move(&mut self, touch_id: u64, x: f32, y: f32) {
        self.active_fingers.insert(touch_id, (x, y));
        if self.active_fingers.len() >= 2 && !self.has_active_control() {
            let positions: Vec<(f32, f32)> = self.active_fingers.values().copied().collect();
            self.two_finger_move(
                positions[0].0,
                positions[0].1,
                positions[1].0,
                positions[1].1,
            );
        } else {
            self.joystick.update(touch_id as i64, x, y);
            if self.camera_drag_id == Some(touch_id) {
                self.camera_drag_dx += x - self.camera_drag_prev.0;
                self.camera_drag_dy += y - self.camera_drag_prev.1;
                self.camera_drag_prev = (x, y);
            }
        }
    }

    pub fn on_touch_end(&mut self, touch_id: u64) {
        self.active_fingers.remove(&touch_id);
        if self.active_fingers.is_empty() {
            self.two_finger_prev_dist = None;
            self.two_finger_prev_mid = None;
        }
        let tid = touch_id as i64;
        self.joystick.deactivate(tid);
        self.attack.release(tid);
        self.charge.release(tid);
        self.defend.release(tid);
        self.dismiss.release(tid);
        if !self.dismiss.pressed {
            self.dismiss_held_secs = 0.0;
            self.dismiss_fired = false;
        }
        if self.camera_drag_id == Some(touch_id) {
            self.camera_drag_id = None;
        }
    }

    fn has_active_control(&self) -> bool {
        self.joystick.active
            || self.attack.pressed
            || self.charge.pressed
            || self.defend.pressed
            || self.dismiss.pressed
            || self.camera_drag_id.is_some()
    }

    fn two_finger_move(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) {
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

    pub fn finger_count(&self) -> u32 {
        self.active_fingers.len() as u32
    }

    pub fn take_pinch_zoom(&mut self) -> f32 {
        std::mem::take(&mut self.pinch_zoom)
    }

    pub fn take_touch_pan(&mut self) -> (f32, f32) {
        (
            std::mem::take(&mut self.touch_pan_x),
            std::mem::take(&mut self.touch_pan_y),
        )
    }

    pub fn take_camera_drag(&mut self) -> (f32, f32) {
        (
            std::mem::take(&mut self.camera_drag_dx),
            std::mem::take(&mut self.camera_drag_dy),
        )
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

    fn controls() -> TouchControls {
        let mut tc = TouchControls::new();
        tc.update_layout(960.0, 640.0, 1.0);
        tc
    }

    #[test]
    fn charge_fires_on_press() {
        let mut tc = controls();
        let (x, y) = (tc.charge.center_x, tc.charge.center_y);
        tc.on_touch_start(1, x, y, 1);
        assert_eq!(tc.take_order(), Some(OrderRequest::Charge));
        assert_eq!(tc.take_order(), None);
    }

    #[test]
    fn dismiss_requires_hold() {
        let mut tc = controls();
        let (x, y) = (tc.dismiss.center_x, tc.dismiss.center_y);
        tc.on_touch_start(1, x, y, 1);
        tc.tick(0.2);
        assert_eq!(tc.take_order(), None);
        tc.tick(0.3);
        assert_eq!(tc.take_order(), Some(OrderRequest::Dismiss));
        tc.tick(1.0);
        assert_eq!(tc.take_order(), None, "fires once per press");
        tc.on_touch_end(1);
        assert_eq!(tc.dismiss_hold_frac(), 0.0);
    }

    #[test]
    fn dismiss_release_before_hold_cancels() {
        let mut tc = controls();
        let (x, y) = (tc.dismiss.center_x, tc.dismiss.center_y);
        tc.on_touch_start(1, x, y, 1);
        tc.tick(0.2);
        tc.on_touch_end(1);
        tc.tick(0.5);
        assert_eq!(tc.take_order(), None);
    }

    #[test]
    fn left_zone_activates_joystick() {
        let mut tc = controls();
        tc.on_touch_start(1, 100.0, 500.0, 1);
        assert!(tc.joystick.active);
        tc.on_touch_end(1);
        assert!(!tc.joystick.active);
    }

    #[test]
    fn right_zone_off_buttons_drags_camera() {
        let mut tc = controls();
        tc.on_touch_start(1, 700.0, 100.0, 1);
        tc.on_touch_move(1, 710.0, 90.0);
        let (dx, dy) = tc.take_camera_drag();
        assert!((dx - 10.0).abs() < 0.01);
        assert!((dy + 10.0).abs() < 0.01);
    }

    #[test]
    fn two_finger_pinch_and_pan() {
        let mut tc = controls();
        tc.on_touch_start(1, 400.0, 300.0, 1);
        // First finger grabbed camera drag; end it to free control state
        tc.on_touch_end(1);
        tc.on_touch_start(1, 400.0, 300.0, 2);
        tc.on_touch_start(2, 500.0, 300.0, 2);
        tc.on_touch_move(1, 390.0, 300.0);
        tc.on_touch_move(1, 380.0, 300.0);
        assert!(tc.take_pinch_zoom() > 0.0);
    }
}
