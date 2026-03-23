use super::texture_manager::{TextureId, TextureManager};
use super::traits::{Renderer, TextureInfo};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

pub struct Canvas2dRenderer {
    ctx: web_sys::CanvasRenderingContext2d,
    width: f64,
    height: f64,
    dpr: f64,
    tm: TextureManager,
}

impl Canvas2dRenderer {
    pub fn new(canvas: &HtmlCanvasElement, dpr: f64) -> Result<Self, JsValue> {
        let ctx = canvas
            .get_context("2d")?
            .ok_or("no 2d context")?
            .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
        ctx.set_image_smoothing_enabled(false);
        Ok(Self {
            ctx,
            width: canvas.width() as f64,
            height: canvas.height() as f64,
            dpr,
            tm: TextureManager::new(),
        })
    }

    // --- Mutable viewport updates (used on resize) ---

    pub fn set_width(&mut self, w: f64) {
        self.width = w;
    }

    pub fn set_height(&mut self, h: f64) {
        self.height = h;
    }

    // --- Texture management (loading is backend-specific) ---

    pub fn texture_manager(&self) -> &TextureManager {
        &self.tm
    }

    pub fn texture_manager_mut(&mut self) -> &mut TextureManager {
        &mut self.tm
    }

    // --- Canvas-specific: offscreen canvas blitting ---

    pub fn draw_canvas(&self, canvas: &HtmlCanvasElement, x: f64, y: f64) -> Result<(), JsValue> {
        self.ctx.draw_image_with_html_canvas_element(canvas, x, y)?;
        Ok(())
    }

    pub fn draw_canvas_scaled(
        &self,
        canvas: &HtmlCanvasElement,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<(), JsValue> {
        self.ctx
            .draw_image_with_html_canvas_element_and_dw_and_dh(canvas, x, y, w, h)?;
        Ok(())
    }

    pub fn draw_canvas_region(
        &self,
        canvas: &HtmlCanvasElement,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) -> Result<(), JsValue> {
        self.ctx
            .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                canvas, sx, sy, sw, sh, dx, dy, dw, dh,
            )?;
        Ok(())
    }

    // --- Raw context access (escape hatch) ---

    #[allow(dead_code)]
    pub fn ctx(&self) -> &web_sys::CanvasRenderingContext2d {
        &self.ctx
    }
}

impl Renderer for Canvas2dRenderer {
    fn width(&self) -> f64 {
        self.width
    }

    fn height(&self) -> f64 {
        self.height
    }

    fn dpr(&self) -> f64 {
        self.dpr
    }

    fn save(&self) {
        self.ctx.save();
    }

    fn restore(&self) {
        self.ctx.restore();
    }

    fn translate(&self, x: f64, y: f64) -> Result<(), JsValue> {
        self.ctx.translate(x, y)
    }

    fn scale(&self, sx: f64, sy: f64) -> Result<(), JsValue> {
        self.ctx.scale(sx, sy)
    }

    fn rotate(&self, angle: f64) -> Result<(), JsValue> {
        self.ctx.rotate(angle)
    }

    fn set_fill_color(&self, color: &str) {
        self.ctx.set_fill_style_str(color);
    }

    fn set_stroke_color(&self, color: &str) {
        self.ctx.set_stroke_style_str(color);
    }

    fn set_alpha(&self, alpha: f64) {
        self.ctx.set_global_alpha(alpha);
    }

    fn set_line_width(&self, width: f64) {
        self.ctx.set_line_width(width);
    }

    fn set_line_dash(&self, segments: &[f64]) {
        let arr = js_sys::Array::new();
        for &s in segments {
            arr.push(&JsValue::from(s));
        }
        let _ = self.ctx.set_line_dash(&arr);
    }

    fn set_composite_op(&self, op: &str) {
        let _ = self.ctx.set_global_composite_operation(op);
    }

    fn set_image_smoothing(&self, enabled: bool) {
        self.ctx.set_image_smoothing_enabled(enabled);
    }

    fn fill_rect(&self, x: f64, y: f64, w: f64, h: f64) {
        self.ctx.fill_rect(x, y, w, h);
    }

    fn stroke_rect(&self, x: f64, y: f64, w: f64, h: f64) {
        self.ctx.stroke_rect(x, y, w, h);
    }

    fn begin_path(&self) {
        self.ctx.begin_path();
    }

    fn close_path(&self) {
        self.ctx.close_path();
    }

    fn move_to(&self, x: f64, y: f64) {
        self.ctx.move_to(x, y);
    }

    fn line_to(&self, x: f64, y: f64) {
        self.ctx.line_to(x, y);
    }

    fn arc(&self, cx: f64, cy: f64, r: f64, start: f64, end: f64) -> Result<(), JsValue> {
        self.ctx.arc(cx, cy, r, start, end)
    }

    fn arc_to(&self, x1: f64, y1: f64, x2: f64, y2: f64, r: f64) -> Result<(), JsValue> {
        self.ctx.arc_to(x1, y1, x2, y2, r)
    }

    fn round_rect(&self, x: f64, y: f64, w: f64, h: f64, r: f64) -> Result<(), JsValue> {
        self.ctx.round_rect_with_f64(x, y, w, h, r)
    }

    fn fill(&self) {
        self.ctx.fill();
    }

    fn stroke(&self) {
        self.ctx.stroke();
    }

    fn set_font(&self, font: &str) {
        self.ctx.set_font(font);
    }

    fn set_text_align(&self, align: &str) {
        self.ctx.set_text_align(align);
    }

    fn set_text_baseline(&self, baseline: &str) {
        self.ctx.set_text_baseline(baseline);
    }

    fn fill_text(&self, text: &str, x: f64, y: f64) {
        let _ = self.ctx.fill_text(text, x, y);
    }

    fn stroke_text(&self, text: &str, x: f64, y: f64) {
        let _ = self.ctx.stroke_text(text, x, y);
    }

    fn measure_text_width(&self, text: &str) -> f64 {
        self.ctx
            .measure_text(text)
            .map(|m| m.width())
            .unwrap_or(0.0)
    }

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
    ) -> Result<(), JsValue> {
        if let Some((img, _, _, _)) = self.tm.get_image(id) {
            self.ctx
                .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img, sx, sy, sw, sh, dx, dy, dw, dh,
                )?;
        }
        Ok(())
    }

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
    ) -> Result<(), JsValue> {
        if let Some((img, _, _, _)) = self.tm.get_image(id) {
            self.ctx.save();
            self.ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
            self.ctx.scale(-1.0, 1.0)?;
            self.ctx
                .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img,
                    sx,
                    sy,
                    sw,
                    sh,
                    -dw / 2.0,
                    -dh / 2.0,
                    dw,
                    dh,
                )?;
            self.ctx.restore();
        }
        Ok(())
    }

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
    ) -> Result<(), JsValue> {
        let img = match self.tm.get_image(id) {
            Some((img, _, _, _)) => img,
            None => return Ok(()),
        };

        let needs_state = flip_x || (opacity - 1.0).abs() > 0.001;

        if needs_state {
            self.ctx.save();
            if (opacity - 1.0).abs() > 0.001 {
                self.ctx.set_global_alpha(opacity);
            }
            if flip_x {
                self.ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
                self.ctx.scale(-1.0, 1.0)?;
                self.ctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        sx,
                        sy,
                        sw,
                        sh,
                        -dw / 2.0,
                        -dh / 2.0,
                        dw,
                        dh,
                    )?;
                self.ctx.restore();
            } else {
                self.ctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, sx, sy, sw, sh, dx, dy, dw, dh,
                    )?;
                self.ctx.restore();
            }
        } else {
            self.ctx
                .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img, sx, sy, sw, sh, dx, dy, dw, dh,
                )?;
        }
        Ok(())
    }

    fn texture_info(&self, id: TextureId) -> Option<TextureInfo> {
        self.tm.get_image(id).map(|(_, fw, fh, fc)| TextureInfo {
            frame_width: fw,
            frame_height: fh,
            frame_count: fc,
        })
    }
}
