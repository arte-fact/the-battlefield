use std::collections::HashSet;

/// Tracks keyboard and mouse input state.
pub struct Input {
    pub keys_down: HashSet<String>,
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_clicked: bool,
    pub scroll_delta: f32,
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
}
