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

const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 4.0;

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

    /// Zoom in/out by delta (positive = zoom in).
    /// Snaps to nearest 1/64 step so TILE_SIZE * zoom is always integer (pixel-clean).
    pub fn zoom_by(&mut self, delta: f32) {
        let raw = self.zoom * (1.0 + delta * 0.1);
        let snapped = (raw * 64.0).round() / 64.0;
        self.zoom = snapped.clamp(MIN_ZOOM, MAX_ZOOM);
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

    /// Clamp camera so the viewport never extends past the world bounds (0,0)-(world_w, world_h).
    pub fn clamp_to_world(&mut self, world_w: f32, world_h: f32) {
        let half_w = self.viewport_w / (2.0 * self.zoom);
        let half_h = self.viewport_h / (2.0 * self.zoom);
        self.x = self.x.clamp(half_w, (world_w - half_w).max(half_w));
        self.y = self.y.clamp(half_h, (world_h - half_h).max(half_h));
    }

    /// Calculate an ideal zoom level based on viewport dimensions.
    /// Targets ~14 tiles visible along the shortest axis for a close,
    /// action-focused view on both portrait and landscape mobile.
    pub fn ideal_zoom(&self) -> f32 {
        let tile = crate::grid::TILE_SIZE;
        let short = self.viewport_w.min(self.viewport_h);
        let target_tiles = if self.viewport_h > self.viewport_w {
            14.0 // portrait
        } else {
            14.0 // landscape
        };
        let raw = short / (target_tiles * tile);
        let snapped = (raw * 64.0).round() / 64.0;
        snapped.clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// Update viewport dimensions (e.g., on resize/orientation change).
    pub fn resize(&mut self, viewport_w: f32, viewport_h: f32) {
        self.viewport_w = viewport_w;
        self.viewport_h = viewport_h;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
