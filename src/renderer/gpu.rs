use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

pub struct Canvas2d {
    pub ctx: web_sys::CanvasRenderingContext2d,
    pub width: f64,
    pub height: f64,
    /// Device pixel ratio (backing pixels / CSS pixels).
    pub dpr: f64,
}

impl Canvas2d {
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
        })
    }
}
