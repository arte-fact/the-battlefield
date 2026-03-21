use crate::game::Game;
use wasm_bindgen::prelude::*;

/// Update the offscreen fog canvas (1px per tile) using direct pixel manipulation.
/// Only called when game.fog_dirty is true (i.e. after player moves).
pub(super) fn update_fog_canvas(
    fog_ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
) -> Result<(), JsValue> {
    let w = game.grid.width;
    let h = game.grid.height;
    let len = (w * h * 4) as usize;
    let mut pixels = vec![0u8; len];

    for gy in 0..h {
        for gx in 0..w {
            let idx = (gy * w + gx) as usize;
            let po = idx * 4; // pixel offset (RGBA)

            let alpha = if game.visible[idx] {
                // Visible tile -- add soft edge if near fog
                let fog_n = 8 - visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                if fog_n >= 3 {
                    ((fog_n as u32 - 2) * 10).min(255) as u8
                } else {
                    0
                }
            } else {
                // Outside line of sight -- dim fog, softer near visible tiles
                let vis_n = visible_neighbor_count_fast(&game.visible, gx, gy, w, h);
                let base = 140i32; // ~0.55 * 255
                (base - (vis_n as i32) * 15).max(38) as u8
            };

            // Black with computed alpha
            pixels[po] = 0;
            pixels[po + 1] = 0;
            pixels[po + 2] = 0;
            pixels[po + 3] = alpha;
        }
    }

    let clamped = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h)?;
    fog_ctx.put_image_data(&image_data, 0.0, 0.0)?;

    Ok(())
}

/// Count visible neighbors (8-directional) using direct array access. No allocations.
pub(super) fn visible_neighbor_count_fast(
    visible: &[bool],
    gx: u32,
    gy: u32,
    w: u32,
    h: u32,
) -> u32 {
    let mut count = 0u32;
    let x = gx as i32;
    let y = gy as i32;
    let wi = w as i32;
    let hi = h as i32;
    for &(ndx, ndy) in &[
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        let nx = x + ndx;
        let ny = y + ndy;
        if nx >= 0 && ny >= 0 && nx < wi && ny < hi {
            if visible[(ny as u32 * w + nx as u32) as usize] {
                count += 1;
            }
        }
    }
    count
}
