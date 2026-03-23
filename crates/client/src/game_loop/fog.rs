use battlefield_core::game::Game;
use battlefield_core::render_util;
use wasm_bindgen::prelude::*;

/// Update the offscreen fog canvas (1px per tile) using direct pixel manipulation.
/// Only called when game.fog_dirty is true (i.e. after player moves).
pub(super) fn update_fog_canvas(
    fog_ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
) -> Result<(), JsValue> {
    let w = game.grid.width;
    let h = game.grid.height;
    let pixels = render_util::build_fog_pixels(&game.visible, w, h);

    let clamped = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h)?;
    fog_ctx.put_image_data(&image_data, 0.0, 0.0)?;

    Ok(())
}
