use std::collections::HashSet;

/// Live swipe tracking state for preview rendering.
#[derive(Debug, Clone, Copy)]
pub struct SwipeState {
    pub start_x: f32,
    pub start_y: f32,
    pub current_x: f32,
    pub current_y: f32,
}

/// 8-directional swipe direction.
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

    /// Classify a screen-space delta into one of 8 directions.
    /// Returns None if the distance is below threshold.
    pub fn from_delta(dx: f32, dy: f32, threshold: f32) -> Option<Self> {
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < threshold {
            return None;
        }
        let angle = dy.atan2(dx);
        // Normalize to [0, 2π)
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

/// Tracks keyboard, mouse, and touch input state.
pub struct Input {
    pub keys_down: HashSet<String>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_clicked: bool,
    pub scroll_delta: f32,

    // Touch state
    /// Single-touch start: (touch_id, start_x, start_y)
    single_touch_start: Option<(i32, f32, f32)>,
    /// Whether multi-touch occurred during this gesture (suppresses swipe)
    was_multi_touch: bool,
    /// Previous two-finger distance (for pinch detection)
    two_finger_prev_dist: Option<f32>,
    /// Previous two-finger midpoint (for pan detection)
    two_finger_prev_mid: Option<(f32, f32)>,
    /// Completed swipe direction to consume
    pub swipe: Option<SwipeDir>,
    /// Accumulated pinch zoom delta
    pub pinch_zoom: f32,
    /// Accumulated two-finger pan delta
    pub touch_pan_x: f32,
    pub touch_pan_y: f32,
    /// End turn requested (from on-screen button)
    pub end_turn_requested: bool,
    /// Live swipe tracking for preview rendering
    live_swipe: Option<SwipeState>,
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
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_clicked: false,
            scroll_delta: 0.0,
            single_touch_start: None,
            was_multi_touch: false,
            two_finger_prev_dist: None,
            two_finger_prev_mid: None,
            swipe: None,
            pinch_zoom: 0.0,
            touch_pan_x: 0.0,
            touch_pan_y: 0.0,
            end_turn_requested: false,
            live_swipe: None,
        }
    }

    pub fn key_down(&mut self, key: String) {
        self.keys_down.insert(key);
    }

    pub fn key_up(&mut self, key: &str) {
        self.keys_down.remove(key);
    }

    pub fn is_key_down(&self, key: &str) -> bool {
        self.keys_down.contains(key)
    }

    /// Compute camera pan direction from WASD/arrow keys.
    pub fn camera_pan(&self) -> (f32, f32) {
        let mut dx = 0.0f32;
        let mut dy = 0.0f32;

        if self.is_key_down("ArrowLeft") || self.is_key_down("a") || self.is_key_down("A") {
            dx -= 1.0;
        }
        if self.is_key_down("ArrowRight") || self.is_key_down("d") || self.is_key_down("D") {
            dx += 1.0;
        }
        if self.is_key_down("ArrowUp") || self.is_key_down("w") || self.is_key_down("W") {
            dy -= 1.0;
        }
        if self.is_key_down("ArrowDown") || self.is_key_down("s") || self.is_key_down("S") {
            dy += 1.0;
        }

        (dx, dy)
    }

    /// Consume the mouse click (returns true once per click).
    pub fn take_click(&mut self) -> Option<(f32, f32)> {
        if self.mouse_clicked {
            self.mouse_clicked = false;
            Some((self.mouse_x, self.mouse_y))
        } else {
            None
        }
    }

    /// Consume scroll delta.
    pub fn take_scroll(&mut self) -> f32 {
        let d = self.scroll_delta;
        self.scroll_delta = 0.0;
        d
    }

    // -- Touch methods --

    /// Called on touchstart. Coordinates are canvas-relative.
    pub fn on_touch_start(&mut self, touch_id: i32, x: f32, y: f32, total_touches: u32) {
        if total_touches == 1 {
            self.single_touch_start = Some((touch_id, x, y));
            self.was_multi_touch = false;
            self.live_swipe = Some(SwipeState {
                start_x: x,
                start_y: y,
                current_x: x,
                current_y: y,
            });
        } else {
            // Multi-touch: cancel single-touch tracking
            self.was_multi_touch = true;
            self.single_touch_start = None;
            self.live_swipe = None;
        }
    }

    /// Called on touchmove when a single finger is active.
    pub fn on_touch_move_single(&mut self, x: f32, y: f32) {
        if let Some(ref mut swipe) = self.live_swipe {
            swipe.current_x = x;
            swipe.current_y = y;
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

    /// Called on touchend. Coordinates are final position of the ended touch.
    pub fn on_touch_end(&mut self, touch_id: i32, x: f32, y: f32, remaining_touches: u32) {
        if remaining_touches == 0 {
            self.two_finger_prev_dist = None;
            self.two_finger_prev_mid = None;
        }

        self.live_swipe = None;

        if let Some((start_id, start_x, start_y)) = self.single_touch_start {
            if start_id == touch_id && !self.was_multi_touch {
                let dx = x - start_x;
                let dy = y - start_y;
                self.swipe = SwipeDir::from_delta(dx, dy, 30.0);
            }
            self.single_touch_start = None;
        }
    }

    /// Get current live swipe state (for preview rendering).
    pub fn swipe_state(&self) -> Option<SwipeState> {
        self.live_swipe
    }

    /// Consume completed swipe.
    pub fn take_swipe(&mut self) -> Option<SwipeDir> {
        self.swipe.take()
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

    /// Consume end-turn button press.
    pub fn take_end_turn(&mut self) -> bool {
        let r = self.end_turn_requested;
        self.end_turn_requested = false;
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
    fn camera_pan_direction() {
        let mut input = Input::new();
        input.key_down("ArrowRight".to_string());
        input.key_down("ArrowUp".to_string());
        let (dx, dy) = input.camera_pan();
        assert!((dx - 1.0).abs() < f32::EPSILON);
        assert!((dy - (-1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn take_click_consumes() {
        let mut input = Input::new();
        input.mouse_clicked = true;
        input.mouse_x = 100.0;
        input.mouse_y = 200.0;
        let click = input.take_click();
        assert_eq!(click, Some((100.0, 200.0)));
        assert!(input.take_click().is_none());
    }

    // SwipeDir tests

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
        assert_eq!(
            SwipeDir::from_delta(100.0, 100.0, 30.0),
            Some(SwipeDir::SE)
        );
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

    #[test]
    fn touch_swipe_lifecycle() {
        let mut input = Input::new();
        input.on_touch_start(0, 100.0, 100.0, 1);
        input.on_touch_end(0, 250.0, 100.0, 0); // swipe right
        assert_eq!(input.take_swipe(), Some(SwipeDir::E));
        assert_eq!(input.take_swipe(), None); // consumed
    }

    #[test]
    fn touch_short_swipe_is_none() {
        let mut input = Input::new();
        input.on_touch_start(0, 100.0, 100.0, 1);
        input.on_touch_end(0, 105.0, 102.0, 0); // too short
        assert_eq!(input.take_swipe(), None);
    }

    #[test]
    fn multi_touch_suppresses_swipe() {
        let mut input = Input::new();
        input.on_touch_start(0, 100.0, 100.0, 1);
        input.on_touch_start(1, 200.0, 200.0, 2); // second finger
        input.on_touch_end(0, 300.0, 100.0, 1);
        assert_eq!(input.take_swipe(), None);
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

    #[test]
    fn end_turn_button() {
        let mut input = Input::new();
        assert!(!input.take_end_turn());
        input.end_turn_requested = true;
        assert!(input.take_end_turn());
        assert!(!input.take_end_turn()); // consumed
    }
}
