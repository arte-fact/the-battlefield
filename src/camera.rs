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

    /// Convert world-space position to NDC (-1..1).
    pub fn world_to_ndc(&self, wx: f32, wy: f32) -> (f32, f32) {
        let ndc_x = (wx - self.x) * self.zoom * 2.0 / self.viewport_w;
        // Y is flipped: in world space Y increases downward, in NDC Y increases upward
        let ndc_y = -(wy - self.y) * self.zoom * 2.0 / self.viewport_h;
        (ndc_x, ndc_y)
    }

    /// Convert screen pixel position to world-space position.
    pub fn screen_to_world(&self, sx: f32, sy: f32) -> (f32, f32) {
        let wx = self.x + (sx - self.viewport_w * 0.5) / self.zoom;
        let wy = self.y + (sy - self.viewport_h * 0.5) / self.zoom;
        (wx, wy)
    }

    /// Build a 4x4 orthographic view-projection matrix for the shader.
    /// Maps world coords to NDC, with Y flipped (world Y-down to NDC Y-up).
    pub fn view_proj_matrix(&self) -> [f32; 16] {
        let half_w = self.viewport_w / (2.0 * self.zoom);
        let half_h = self.viewport_h / (2.0 * self.zoom);

        let left = self.x - half_w;
        let right = self.x + half_w;
        let top = self.y - half_h;
        let bottom = self.y + half_h;

        // Orthographic projection, row-major stored as column-major for WGSL
        let sx = 2.0 / (right - left);
        let sy = -2.0 / (bottom - top); // flip Y
        let tx = -(right + left) / (right - left);
        let ty = (bottom + top) / (bottom - top); // adjusted for Y flip

        // Column-major layout for WGSL mat4x4
        [
            sx, 0.0, 0.0, 0.0, // column 0
            0.0, sy, 0.0, 0.0, // column 1
            0.0, 0.0, 1.0, 0.0, // column 2
            tx, ty, 0.0, 1.0, // column 3
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_center_maps_to_ndc_origin() {
        let cam = Camera::new(960.0, 640.0);
        let (nx, ny) = cam.world_to_ndc(0.0, 0.0);
        assert!((nx).abs() < 1e-5);
        assert!((ny).abs() < 1e-5);
    }

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

    #[test]
    fn view_proj_center_maps_to_origin() {
        let cam = Camera::new(960.0, 640.0);
        let m = cam.view_proj_matrix();
        // Transform (0,0,0,1) by column-major matrix
        let x = m[0] * 0.0 + m[4] * 0.0 + m[8] * 0.0 + m[12];
        let y = m[1] * 0.0 + m[5] * 0.0 + m[9] * 0.0 + m[13];
        assert!(x.abs() < 1e-5, "x = {x}");
        assert!(y.abs() < 1e-5, "y = {y}");
    }
}
