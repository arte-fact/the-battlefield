use rusttype::{point, Font, Scale};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::surface::Surface;
use sdl2::video::{Window, WindowContext};

pub struct TextRenderer {
    font: Font<'static>,
}

impl TextRenderer {
    pub fn new() -> Self {
        let font_data = battlefield_assets::get("assets/MedievalSharp.ttf")
            .expect("Font not found in embedded assets");
        let font = Font::try_from_vec(font_data.to_vec()).expect("Failed to parse font");
        Self { font }
    }

    /// Draw text centered at `(cx, cy)` with given color and size.
    pub fn draw_text_centered(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        text: &str,
        cx: i32,
        cy: i32,
        size: f32,
        color: Color,
    ) {
        let (text_w, text_h, mut pixels) = match self.rasterize(text, size, color) {
            Some(result) => result,
            None => return,
        };
        let dx = cx - text_w as i32 / 2;
        let dy = cy - text_h as i32 / 2;
        blit_pixels(canvas, tc, &mut pixels, text_w, text_h, dx, dy);
    }

    /// Draw text left-aligned at `(x, y)` (top-left corner).
    #[allow(dead_code)]
    pub fn draw_text(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        text: &str,
        x: i32,
        y: i32,
        size: f32,
        color: Color,
    ) {
        let (text_w, text_h, mut pixels) = match self.rasterize(text, size, color) {
            Some(result) => result,
            None => return,
        };
        blit_pixels(canvas, tc, &mut pixels, text_w, text_h, x, y);
    }

    /// Measure text dimensions without rendering, returning `(width, height)`.
    pub fn measure_text(&self, text: &str, size: f32) -> (u32, u32) {
        let scale = Scale::uniform(size);
        let v_metrics = self.font.v_metrics(scale);
        let glyphs: Vec<_> = self
            .font
            .layout(text, scale, point(0.0, v_metrics.ascent))
            .collect();

        if glyphs.is_empty() {
            return (0, 0);
        }

        let min_x = glyphs
            .first()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.min.x)
            .unwrap_or(0);
        let max_x = glyphs
            .last()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.max.x)
            .unwrap_or(0);
        let text_w = (max_x - min_x) as u32;
        let text_h = (v_metrics.ascent - v_metrics.descent).ceil() as u32;
        (text_w, text_h)
    }

    /// Draw text centered at `(cx, cy)` with a dark semi-transparent background for readability.
    #[allow(dead_code)]
    pub fn draw_text_centered_with_bg(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        text: &str,
        cx: i32,
        cy: i32,
        size: f32,
        color: Color,
        bg_color: Color,
        pad_x: i32,
        pad_y: i32,
    ) {
        let (tw, th) = self.measure_text(text, size);
        if tw > 0 && th > 0 {
            let bg_w = tw as i32 + pad_x * 2;
            let bg_h = th as i32 + pad_y * 2;
            let bg_x = cx - bg_w / 2;
            let bg_y = cy - bg_h / 2;
            canvas.set_blend_mode(BlendMode::Blend);
            canvas.set_draw_color(bg_color);
            let _ = canvas.fill_rect(Rect::new(bg_x, bg_y, bg_w as u32, bg_h as u32));
        }
        self.draw_text_centered(canvas, tc, text, cx, cy, size, color);
    }

    /// Rasterize text into an RGBA pixel buffer, returning `(width, height, pixels)`.
    fn rasterize(&self, text: &str, size: f32, color: Color) -> Option<(u32, u32, Vec<u8>)> {
        let scale = Scale::uniform(size);
        let v_metrics = self.font.v_metrics(scale);
        let glyphs: Vec<_> = self
            .font
            .layout(text, scale, point(0.0, v_metrics.ascent))
            .collect();

        if glyphs.is_empty() {
            return None;
        }

        let min_x = glyphs
            .first()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.min.x)
            .unwrap_or(0);
        let max_x = glyphs
            .last()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.max.x)
            .unwrap_or(0);
        let text_w = (max_x - min_x) as u32;
        let text_h = (v_metrics.ascent - v_metrics.descent).ceil() as u32;

        if text_w == 0 || text_h == 0 {
            return None;
        }

        let mut pixels = vec![0u8; (text_w * text_h * 4) as usize];
        for glyph in &glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px = (bb.min.x - min_x) as u32 + gx;
                    let py = bb.min.y as u32 + gy;
                    if px < text_w && py < text_h {
                        let idx = ((py * text_w + px) * 4) as usize;
                        let alpha = (v * color.a as f32) as u8;
                        pixels[idx] = color.r;
                        pixels[idx + 1] = color.g;
                        pixels[idx + 2] = color.b;
                        pixels[idx + 3] = alpha;
                    }
                });
            }
        }

        Some((text_w, text_h, pixels))
    }
}

/// Create an SDL surface from RGBA pixel data, convert to texture, and blit to canvas.
fn blit_pixels(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    pixels: &mut [u8],
    w: u32,
    h: u32,
    x: i32,
    y: i32,
) {
    let surface = match Surface::from_data(pixels, w, h, w * 4, PixelFormatEnum::ABGR8888) {
        Ok(s) => s,
        Err(_) => return,
    };
    let tex = match tc.create_texture_from_surface(&surface) {
        Ok(t) => t,
        Err(_) => return,
    };
    let _ = canvas.copy(&tex, None, Rect::new(x, y, w, h));
}
