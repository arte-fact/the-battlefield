/// Orthographic camera for viewing the battlefield.
/// Tracks position in world space and zoom level.
pub struct Camera {
    /// Center of the camera in world-space pixels.
    pub x: f32,
    pub y: f32,
    /// Zoom factor (1.0 = 1 world pixel = 1 screen pixel).
    pub zoom: f32,
    /// Viewport dimensions in screen pixels.
    pub viewport_w: f32,
    pub viewport_h: f32,
}

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 4.0;
const PAN_SPEED: f32 = 400.0; // pixels per second at zoom 1.0

impl Camera {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            viewport_w,
            viewport_h,
        }
    }

    /// Pan the camera by (dx, dy) in screen-relative direction, scaled by dt.
    pub fn pan(&mut self, dx: f32, dy: f32, dt: f32) {
        let speed = PAN_SPEED / self.zoom;
        self.x += dx * speed * dt;
        self.y += dy * speed * dt;
    }

    /// Zoom in/out by delta (positive = zoom in).
    pub fn zoom_by(&mut self, delta: f32) {
        self.zoom = (self.zoom * (1.0 + delta * 0.1)).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// Visible world-space rectangle: (left, top, right, bottom).
    pub fn visible_rect(&self) -> (f32, f32, f32, f32) {
        let half_w = self.viewport_w / (2.0 * self.zoom);
        let half_h = self.viewport_h / (2.0 * self.zoom);
        (
            self.x - half_w,
            self.y - half_h,
            self.x + half_w,
            self.y + half_h,
        )
    }

    /// Convert screen pixel position to world-space position.
    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        let wx = self.x + (sx - self.viewport_w * 0.5) / self.zoom;
        let wy = self.y + (sy - self.viewport_h * 0.5) / self.zoom;
        (wx, wy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_center_maps_to_camera_center() {
        let mut cam = Camera::new(960.0, 640.0);
        cam.x = 100.0;
        cam.y = 200.0;
        let (wx, wy) = cam.screen_to_world(480.0, 320.0);
        assert!((wx - 100.0).abs() < 1e-5);
        assert!((wy - 200.0).abs() < 1e-5);
    }

    #[test]
    fn zoom_clamps() {
        let mut cam = Camera::new(960.0, 640.0);
        for _ in 0..100 {
            cam.zoom_by(10.0);
        }
        assert!(cam.zoom <= MAX_ZOOM);
        for _ in 0..200 {
            cam.zoom_by(-10.0);
        }
        assert!(cam.zoom >= MIN_ZOOM);
    }

    #[test]
    fn visible_rect_at_zoom_1() {
        let cam = Camera::new(960.0, 640.0);
        let (l, t, r, b) = cam.visible_rect();
        assert!((l - (-480.0)).abs() < 1e-5);
        assert!((r - 480.0).abs() < 1e-5);
        assert!((t - (-320.0)).abs() < 1e-5);
        assert!((b - 320.0).abs() < 1e-5);
    }

    #[test]
    fn pan_moves_camera() {
        let mut cam = Camera::new(960.0, 640.0);
        cam.pan(1.0, 0.0, 1.0);
        assert!(cam.x > 0.0);
        assert!((cam.y).abs() < 1e-5);
    }
}
