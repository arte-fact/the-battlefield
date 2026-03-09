use wasm_bindgen::JsValue;

/// Draw a sprite from a sprite sheet onto the canvas.
/// `src_x, src_y, src_w, src_h` define the source rectangle in the image.
/// `dx, dy, dw, dh` define the destination rectangle in world space (already transformed by camera).
/// The canvas context should already have the camera transform applied.
pub fn draw_sprite(
    ctx: &web_sys::CanvasRenderingContext2d,
    img: &web_sys::HtmlImageElement,
    src_x: f64,
    src_y: f64,
    src_w: f64,
    src_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    flip_x: bool,
    opacity: f64,
) -> Result<(), JsValue> {
    let needs_state = flip_x || (opacity - 1.0).abs() > 0.001;

    if needs_state {
        ctx.save();
        if (opacity - 1.0).abs() > 0.001 {
            ctx.set_global_alpha(opacity);
        }
        if flip_x {
            ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
            ctx.scale(-1.0, 1.0)?;
            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                img,
                src_x,
                src_y,
                src_w,
                src_h,
                -dw / 2.0,
                -dh / 2.0,
                dw,
                dh,
            )?;
            ctx.restore();
        } else {
            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                img, src_x, src_y, src_w, src_h, dx, dy, dw, dh,
            )?;
            ctx.restore();
        }
    } else {
        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            img, src_x, src_y, src_w, src_h, dx, dy, dw, dh,
        )?;
    }
    Ok(())
}
