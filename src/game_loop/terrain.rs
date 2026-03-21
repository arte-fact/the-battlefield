use super::assets::LoadedTextures;
use crate::autotile;
use crate::game::Game;
use crate::grid::{self, TileKind, TILE_SIZE};
use crate::renderer::Canvas2dRenderer;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Chunk size in tiles. Each chunk is CHUNK_TILES x CHUNK_TILES tiles,
/// rendered to a (CHUNK_TILES * 64) x (CHUNK_TILES * 64) pixel canvas.
/// 32 tiles -> 2048x2048 px (~4MP, well within browser canvas limits).
pub(super) const CHUNK_TILES: u32 = 32;

/// Chunk-based terrain cache: the grid is divided into chunks, each with
/// its own small offscreen canvas. Only visible chunks are drawn per frame.
pub(super) struct TerrainChunks {
    /// Chunk canvases stored row-major: chunks[cy * cols + cx]
    pub(super) canvases: Vec<web_sys::HtmlCanvasElement>,
    pub(super) contexts: Vec<web_sys::CanvasRenderingContext2d>,
    /// Whether each chunk needs re-rendering
    pub(super) dirty: Vec<bool>,
    /// Number of chunks in each dimension
    pub(super) cols: u32,
    pub(super) rows: u32,
}

impl TerrainChunks {
    pub(super) fn new(
        document: &web_sys::Document,
        grid_w: u32,
        grid_h: u32,
    ) -> Result<Self, JsValue> {
        let cols = (grid_w + CHUNK_TILES - 1) / CHUNK_TILES;
        let rows = (grid_h + CHUNK_TILES - 1) / CHUNK_TILES;
        let count = (cols * rows) as usize;
        let chunk_px = CHUNK_TILES * (TILE_SIZE as u32);

        let mut canvases = Vec::with_capacity(count);
        let mut contexts = Vec::with_capacity(count);

        for _ in 0..count {
            let c = document
                .create_element("canvas")?
                .dyn_into::<web_sys::HtmlCanvasElement>()?;
            c.set_width(chunk_px);
            c.set_height(chunk_px);
            let ctx = c
                .get_context("2d")?
                .unwrap()
                .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
            canvases.push(c);
            contexts.push(ctx);
        }

        Ok(Self {
            canvases,
            contexts,
            dirty: vec![true; count],
            cols,
            rows,
        })
    }

    pub(super) fn mark_all_dirty(&mut self) {
        for d in &mut self.dirty {
            *d = true;
        }
    }
}

/// Deterministic pseudo-random flip based on grid position.
/// Returns true for ~50% of tiles in a spatially uniform pattern.
pub(super) fn tile_flip(gx: u32, gy: u32) -> bool {
    gx.wrapping_mul(48271).wrapping_add(gy.wrapping_mul(16807)) & 1 == 0
}

/// Draw a tile-sized image horizontally flipped.
fn draw_tile_flipped(
    ctx: &web_sys::CanvasRenderingContext2d,
    img: &web_sys::HtmlImageElement,
    sx: f64,
    sy: f64,
    sw: f64,
    sh: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
) -> Result<(), JsValue> {
    ctx.save();
    ctx.translate(dx + dw / 2.0, dy + dh / 2.0)?;
    ctx.scale(-1.0, 1.0)?;
    ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
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
    ctx.restore();
    Ok(())
}

/// Render a single terrain chunk covering tiles [gx0..gx1) x [gy0..gy1).
/// All drawing uses chunk-local coordinates (tile position minus chunk origin).
///
/// The `chunk_ctx` is the offscreen canvas context for this chunk.
/// The `renderer` is used only for texture lookups (not for drawing).
#[allow(clippy::too_many_arguments)]
pub(super) fn render_terrain_chunk(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    renderer: &Canvas2dRenderer,
    gx0: u32,
    gy0: u32,
    gx1: u32,
    gy1: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;
    let w = game.grid.width;
    let h = game.grid.height;
    let ox = gx0 as f64 * ts; // world-space origin of this chunk
    let oy = gy0 as f64 * ts;
    let tm = renderer.texture_manager();

    // Clear the chunk canvas (transparent)
    let chunk_w = (gx1 - gx0) as f64 * ts;
    let chunk_h = (gy1 - gy0) as f64 * ts;
    ctx.clear_rect(0.0, 0.0, chunk_w, chunk_h);

    // Layer 2.5: Road sand fill (extends 1 tile into neighbors)
    {
        ctx.set_fill_style_str("#C4A265");
        for gy in gy0..gy1 {
            for gx in gx0..gx1 {
                if game.grid.get(gx, gy) != TileKind::Road {
                    continue;
                }
                let dx = (gx as f64) * ts - ox;
                let dy = (gy as f64) * ts - oy;
                ctx.fill_rect(dx, dy, ts, ts);
                if gx > 0 && game.grid.get(gx - 1, gy) != TileKind::Road {
                    ctx.fill_rect(dx - ts, dy, ts, ts);
                }
                if gx + 1 < w && game.grid.get(gx + 1, gy) != TileKind::Road {
                    ctx.fill_rect(dx + ts, dy, ts, ts);
                }
                if gy > 0 && game.grid.get(gx, gy - 1) != TileKind::Road {
                    ctx.fill_rect(dx, dy - ts, ts, ts);
                }
                if gy + 1 < h && game.grid.get(gx, gy + 1) != TileKind::Road {
                    ctx.fill_rect(dx, dy + ts, ts, ts);
                }
            }
        }
    }

    // Layer 3: Flat ground (auto-tiled)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    let tile = game.grid.get(gx, gy);
                    if !tile.is_land() || tile == TileKind::Road {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts - ox;
                    let dy = (gy as f64) * ts - oy;
                    if col == 1 && row == 1 && tile_flip(gx, gy) {
                        draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
        }
    }

    // Layer 3.5: Road surface (grass autotile tinted to sand)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    if game.grid.get(gx, gy) != TileKind::Road {
                        continue;
                    }
                    let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                    let dx = (gx as f64) * ts - ox;
                    let dy = (gy as f64) * ts - oy;
                    if col == 1 && row == 1 && tile_flip(gx, gy) {
                        draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                    } else {
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
            ctx.set_global_composite_operation("multiply")?;
            ctx.set_fill_style_str("#D4B070");
            for gy in gy0..gy1 {
                for gx in gx0..gx1 {
                    if game.grid.get(gx, gy) == TileKind::Road {
                        ctx.fill_rect((gx as f64) * ts - ox, (gy as f64) * ts - oy, ts, ts);
                    }
                }
            }
            ctx.set_global_composite_operation("source-over")?;
        }
    }

    // Layer 4: Elevation (shadow + elevated surface + cliff)
    for level in 2..=2u8 {
        if let Some(shadow_tex_id) = loaded.shadow_texture {
            if let Some((img, _, _, _)) = tm.get_image(shadow_tex_id) {
                ctx.set_global_alpha(0.5);
                for gy in gy0..gy1 {
                    for gx in gx0..gx1 {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        if gy + 1 < h && game.grid.elevation(gx, gy + 1) < level {
                            let shadow_size = 192.0_f64;
                            let dx = (gx as f64) * ts + ts / 2.0 - shadow_size / 2.0 - ox;
                            let dy = ((gy + 1) as f64) * ts + ts / 2.0 - shadow_size / 2.0 - oy;
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, 0.0, 0.0, shadow_size, shadow_size, dx, dy, shadow_size, shadow_size,
                            )?;
                        }
                    }
                }
                ctx.set_global_alpha(1.0);
            }
        }

        let elev_tex_id = if level == 2 {
            loaded.tilemap_texture2
        } else {
            loaded.tilemap_texture
        };
        if let Some(tilemap_tex_id) = elev_tex_id {
            if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
                for gy in gy0..gy1 {
                    for gx in gx0..gx1 {
                        if game.grid.elevation(gx, gy) < level {
                            continue;
                        }
                        let (col, row) = autotile::elevated_top_src(&game.grid, gx, gy, level);
                        let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                        let dx = (gx as f64) * ts - ox;
                        let dy = (gy as f64) * ts - oy;
                        if col == 6 && row == 1 && tile_flip(gx, gy) {
                            draw_tile_flipped(ctx, img, sx, sy, sw, sh, dx, dy, ts, ts)?;
                        } else {
                            ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                img, sx, sy, sw, sh, dx, dy, ts, ts,
                            )?;
                        }

                        if let Some((ccol, crow)) =
                            autotile::cliff_src(&game.grid, gx, gy, level)
                        {
                            let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
                            let cdy = ((gy + 1) as f64) * ts - oy;
                            if tile_flip(gx, gy.wrapping_add(1000)) {
                                draw_tile_flipped(
                                    ctx, img, csx, csy, csw, csh, dx, cdy, ts, ts,
                                )?;
                            } else {
                                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                                    img, csx, csy, csw, csh, dx, cdy, ts, ts,
                                )?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
