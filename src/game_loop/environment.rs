use super::assets::LoadedTextures;
use super::helpers::compute_wave_frame;
use super::terrain::tile_flip;
use crate::game::Game;
use crate::grid::{Decoration, TileKind, TILE_SIZE};
use crate::renderer::{Canvas2dRenderer, Renderer};
use wasm_bindgen::prelude::*;

/// Draw water background tiles (visible range only, per-frame).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_water(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    if let Some(water_tex_id) = loaded.water_texture {
        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                // Draw water on water tiles AND on land tiles adjacent to water
                // (foam sprites are 192x192 centered on land, so they need water behind them)
                let is_water = !game.grid.get(gx, gy).is_land();
                let has_foam = game
                    .water_adjacency
                    .get((gy * game.grid.width + gx) as usize)
                    .copied()
                    .unwrap_or(false);
                if !is_water && !has_foam {
                    continue;
                }
                let dx = (gx as f64) * ts;
                let dy = (gy as f64) * ts;
                r.draw_texture(water_tex_id, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts)?;
            }
        }
    }

    Ok(())
}

/// Draw animated water foam (the only per-frame terrain layer).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_foam(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    if let Some(foam_tex_id) = loaded.foam_texture {
        let foam_fps = 8.0;
        let global_frame = (elapsed * foam_fps) as u32;

        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                let idx = (gy * game.grid.width + gx) as usize;
                if !game.water_adjacency.get(idx).copied().unwrap_or(false) {
                    continue;
                }
                let tile_offset = (gx.wrapping_mul(7).wrapping_add(gy.wrapping_mul(13))) % 16;
                let frame = (global_frame + tile_offset) % 16;
                let foam_size = 192.0_f64;
                let sx = (frame as f64) * foam_size;
                let dx = (gx as f64) * ts + ts / 2.0 - foam_size / 2.0;
                let dy = (gy as f64) * ts + ts / 2.0 - foam_size / 2.0;
                r.draw_texture(
                    foam_tex_id,
                    sx,
                    0.0,
                    foam_size,
                    foam_size,
                    dx,
                    dy,
                    foam_size,
                    foam_size,
                )?;
            }
        }
    }

    Ok(())
}

/// Draw animated bush decorations (ground level, always behind units).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_bushes(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) -> Result<(), JsValue> {
    if loaded.bush_textures.is_empty() {
        return Ok(());
    }
    let ts = TILE_SIZE as f64;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                continue;
            }
            let variant_idx = (gx.wrapping_mul(41).wrapping_add(gy.wrapping_mul(23))) as usize
                % loaded.bush_textures.len();
            let (tex_id, frame_w, frame_h) = loaded.bush_textures[variant_idx];

            if let Some(info) = r.texture_info(tex_id) {
                let fw = frame_w as f64;
                let fh = frame_h as f64;

                let frame = compute_wave_frame(elapsed, gx, gy, info.frame_count, 0.15);
                let sx = frame as f64 * fw;

                let dx = (gx as f64) * ts;
                let dy = (gy as f64) * ts;

                if tile_flip(gx, gy) {
                    if let Some((flipped, _, _)) = loaded.bush_textures_flipped.get(variant_idx) {
                        let sheet_w = info.frame_count as f64 * fw;
                        let flipped_sx = sheet_w - sx - fw;
                        r.draw_canvas_region(
                            flipped, flipped_sx, 0.0, fw, fh, dx, dy, ts, ts,
                        )?;
                    }
                } else {
                    r.draw_texture(tex_id, sx, 0.0, fw, fh, dx, dy, ts, ts)?;
                }
            }
        }
    }
    Ok(())
}

/// Draw rocks as a ground-level pass (always behind units).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_rocks(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    if loaded.rock_textures.is_empty() {
        return Ok(());
    }
    let ts = TILE_SIZE as f64;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let variant_idx = (gx.wrapping_mul(13).wrapping_add(gy.wrapping_mul(29))) as usize
                % loaded.rock_textures.len();
            let tex_id = loaded.rock_textures[variant_idx];

            let dx = (gx as f64) * ts;
            let dy = (gy as f64) * ts;

            if tile_flip(gx, gy) {
                if let Some(flipped) = loaded.rock_textures_flipped.get(variant_idx) {
                    r.draw_canvas_region(
                        flipped, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts,
                    )?;
                }
            } else {
                r.draw_texture(tex_id, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts)?;
            }
        }
    }
    Ok(())
}
