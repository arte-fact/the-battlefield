use super::texture_manager::TextureId;
use wasm_bindgen::prelude::*;

/// Metadata about a loaded texture's sprite sheet layout.
pub struct TextureInfo {
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
}

/// Abstract 2D renderer. Canvas 2D is the current implementation;
/// future backends (WebGL, native) can implement this trait.
#[allow(dead_code)]
pub trait Renderer {
    // --- Viewport ---
    fn width(&self) -> f64;
    fn height(&self) -> f64;
    fn dpr(&self) -> f64;

    // --- State stack ---
    fn save(&self);
    fn restore(&self);

    // --- Transforms ---
    fn translate(&self, x: f64, y: f64) -> Result<(), JsValue>;
    fn scale(&self, sx: f64, sy: f64) -> Result<(), JsValue>;
    fn rotate(&self, angle: f64) -> Result<(), JsValue>;

    // --- Style ---
    fn set_fill_color(&self, color: &str);
    fn set_stroke_color(&self, color: &str);
    fn set_alpha(&self, alpha: f64);
    fn set_line_width(&self, width: f64);
    fn set_line_dash(&self, segments: &[f64]);
    fn set_composite_op(&self, op: &str);
    fn set_image_smoothing(&self, enabled: bool);

    // --- Rectangles ---
    fn fill_rect(&self, x: f64, y: f64, w: f64, h: f64);
    fn stroke_rect(&self, x: f64, y: f64, w: f64, h: f64);

    // --- Paths ---
    fn begin_path(&self);
    fn close_path(&self);
    fn move_to(&self, x: f64, y: f64);
    fn line_to(&self, x: f64, y: f64);
    fn arc(&self, cx: f64, cy: f64, r: f64, start: f64, end: f64) -> Result<(), JsValue>;
    fn arc_to(&self, x1: f64, y1: f64, x2: f64, y2: f64, r: f64) -> Result<(), JsValue>;
    fn round_rect(&self, x: f64, y: f64, w: f64, h: f64, r: f64) -> Result<(), JsValue>;
    fn fill(&self);
    fn stroke(&self);

    // --- Text ---
    fn set_font(&self, font: &str);
    fn set_text_align(&self, align: &str);
    fn set_text_baseline(&self, baseline: &str);
    fn fill_text(&self, text: &str, x: f64, y: f64);
    fn stroke_text(&self, text: &str, x: f64, y: f64);
    fn measure_text_width(&self, text: &str) -> f64;

    // --- Textures ---
    fn draw_texture(
        &self,
        id: TextureId,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) -> Result<(), JsValue>;

    fn draw_texture_flipped(
        &self,
        id: TextureId,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) -> Result<(), JsValue>;

    fn draw_sprite(
        &self,
        id: TextureId,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
        flip_x: bool,
        opacity: f64,
    ) -> Result<(), JsValue>;

    fn texture_info(&self, id: TextureId) -> Option<TextureInfo>;
}
