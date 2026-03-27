use battlefield_core::autotile;
use battlefield_core::camera::Camera;
use battlefield_core::game::Game;
use battlefield_core::grid::{self, Decoration, TileKind, TILE_SIZE};
use battlefield_core::render_util;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::video::Window;

use super::assets::Assets;
use super::{src_rect, world_to_screen};

pub(super) fn draw_water(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let tsi = ts.ceil() as u32;
    let water_tex = match assets.water_texture.as_ref() {
        Some(t) => t,
        None => {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if !game.grid.get(gx, gy).is_land() {
                        let wx = gx as f32 * TILE_SIZE;
                        let wy = gy as f32 * TILE_SIZE;
                        let (sx, sy) = world_to_screen(wx, wy, cam);
                        canvas.set_draw_color(Color::RGB(48, 96, 160));
                        let _ = canvas.fill_rect(Rect::new(sx, sy, tsi, tsi));
                    }
                }
            }
            return;
        }
    };

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let is_water = !game.grid.get(gx, gy).is_land();
            let has_foam = game
                .water_adjacency
                .get((gy * game.grid.width + gx) as usize)
                .copied()
                .unwrap_or(false);
            if !is_water && !has_foam {
                continue;
            }
            let wx = gx as f32 * TILE_SIZE;
            let wy = gy as f32 * TILE_SIZE;
            let (sx, sy) = world_to_screen(wx, wy, cam);
            let src = Rect::new(0, 0, 64, 64);
            let dst = Rect::new(sx, sy, tsi, tsi);
            let _ = canvas.copy(water_tex, src, dst);
        }
    }
}

pub(super) fn draw_foam(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let foam_tex = match assets.foam_texture.as_ref() {
        Some(t) => t,
        None => return,
    };
    let foam_sprite_size = 192.0_f32;
    let foam_draw = foam_sprite_size * cam.zoom;

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let idx = (gy * game.grid.width + gx) as usize;
            if !game.water_adjacency.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let frame = match render_util::foam_frame(elapsed, gx, gy) {
                Some(f) => f,
                None => continue,
            };
            let foam_sx = frame as i32 * foam_sprite_size as i32;
            let src = Rect::new(foam_sx, 0, foam_sprite_size as u32, foam_sprite_size as u32);

            let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let center_wy = gy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let (scx, scy) = world_to_screen(center_wx, center_wy, cam);
            let half = (foam_draw * 0.5) as i32;
            let dst = Rect::new(scx - half, scy - half, foam_draw as u32, foam_draw as u32);
            let _ = canvas.copy(foam_tex, src, dst);
        }
    }
}

pub(super) fn draw_terrain(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let tsi = ts.ceil() as u32;
    let w = game.grid.width;
    let h = game.grid.height;

    // Road surface under road tiles and their grass neighbors (so transparent
    // fringe in grass edge tiles reveals a proper road texture, not flat color).
    if let Some(ref tilemap_tex) = assets.tilemap_texture {
        let (rc, rr) = autotile::FLAT_CENTER;
        let (rtsx, rtsy, rtsw, rtsh) = grid::tilemap_src_rect(rc, rr);
        let rsrc = src_rect(rtsx, rtsy, rtsw, rtsh);

        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                let tile = game.grid.get(gx, gy);
                let is_road_neighbor = tile != TileKind::Road
                    && tile.is_land()
                    && ((gx > 0 && game.grid.get(gx - 1, gy) == TileKind::Road)
                        || (gx + 1 < w && game.grid.get(gx + 1, gy) == TileKind::Road)
                        || (gy > 0 && game.grid.get(gx, gy - 1) == TileKind::Road)
                        || (gy + 1 < h && game.grid.get(gx, gy + 1) == TileKind::Road));
                if tile != TileKind::Road && !is_road_neighbor {
                    continue;
                }
                let wx = gx as f32 * TILE_SIZE;
                let wy = gy as f32 * TILE_SIZE;
                let (sx, sy) = world_to_screen(wx, wy, cam);
                let dst = Rect::new(sx, sy, tsi, tsi);
                // Use autotile for water borders on both road tiles and road neighbors
                let (col, row) = {
                    let mask = autotile::cardinal_land_mask(&game.grid, gx, gy);
                    autotile::flat_ground_entry(mask)
                };
                let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
                let src = src_rect(tsx, tsy, tsw, tsh);
                let flip_h = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
                let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);
                // Sand tint overlay
                canvas.set_draw_color(Color::RGBA(212, 176, 112, 140));
                let _ = canvas.fill_rect(dst);
            }
        }
    }

    // Flat ground (autotiled)
    if let Some(ref tilemap_tex) = assets.tilemap_texture {
        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                let tile = game.grid.get(gx, gy);
                if !tile.is_land() || tile == TileKind::Road {
                    continue;
                }
                let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
                let wx = gx as f32 * TILE_SIZE;
                let wy = gy as f32 * TILE_SIZE;
                let (sx, sy) = world_to_screen(wx, wy, cam);
                let src = src_rect(tsx, tsy, tsw, tsh);
                let dst = Rect::new(sx, sy, tsi, tsi);

                let flip_h = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
                let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);
            }
        }
    }

    // Elevation
    let elev_min_gy = min_gy.saturating_sub(1);
    for level in 2..=2u8 {
        // Shadow below elevated edges
        if let Some(ref mut shadow_tex) = assets.shadow_texture {
            shadow_tex.set_alpha_mod(128);
            for gy in elev_min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if game.grid.elevation(gx, gy) < level {
                        continue;
                    }
                    if gy + 1 < h && game.grid.elevation(gx, gy + 1) < level {
                        let shadow_world = 192.0_f32;
                        let shadow_draw = shadow_world * cam.zoom;
                        let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                        let center_wy = (gy + 1) as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                        let (scx, scy) = world_to_screen(center_wx, center_wy, cam);
                        let half = (shadow_draw * 0.5) as i32;
                        let dst = Rect::new(
                            scx - half,
                            scy - half,
                            shadow_draw as u32,
                            shadow_draw as u32,
                        );
                        let src = Rect::new(0, 0, 192, 192);
                        let _ = canvas.copy(shadow_tex, src, dst);
                    }
                }
            }
            shadow_tex.set_alpha_mod(255);
        }

    }
}

/// Draw an elevated tile (surface + cliff face) in the Y-sorted foreground pass.
pub(super) fn draw_elevated_tile(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &Assets,
    cam: &Camera,
    ts: f32,
    gx: u32,
    gy: u32,
) {
    let tsi = ts.ceil() as u32;
    let level = game.grid.elevation(gx, gy);
    if level < 2 {
        return;
    }

    let elev_tex = if level == 2 {
        assets.tilemap_texture2.as_ref()
    } else {
        assets.tilemap_texture.as_ref()
    };
    if let Some(tilemap_tex) = elev_tex {
        let (col, row) = autotile::elevated_top_src(&game.grid, gx, gy, level);
        let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
        let wx = gx as f32 * TILE_SIZE;
        let wy = gy as f32 * TILE_SIZE;
        let (sx, sy) = world_to_screen(wx, wy, cam);
        let src = src_rect(tsx, tsy, tsw, tsh);
        let dst = Rect::new(sx, sy, tsi, tsi);

        let flip_h = col == 6 && row == 1 && render_util::tile_flip(gx, gy);
        let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);

        // Cliff face (drawn on the tile below)
        if let Some((ccol, crow)) = autotile::cliff_src(&game.grid, gx, gy, level) {
            let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
            let cliff_wy = (gy + 1) as f32 * TILE_SIZE;
            let (_, cliff_sy) = world_to_screen(wx, cliff_wy, cam);
            let cliff_src = src_rect(csx, csy, csw, csh);
            let cliff_dst = Rect::new(sx, cliff_sy, tsi, tsi);
            let cliff_flip = render_util::tile_flip(gx, gy.wrapping_add(1000));
            let _ = canvas.copy_ex(
                tilemap_tex,
                cliff_src,
                cliff_dst,
                0.0,
                None,
                cliff_flip,
                false,
            );
        }
    }
}

pub(super) fn draw_bushes(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    if assets.bush_textures.is_empty() {
        return;
    }

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                continue;
            }
            let variant_idx =
                render_util::variant_index(gx, gy, assets.bush_textures.len(), 41, 23);
            let (ref tex, fw, fh, frame_count) = assets.bush_textures[variant_idx];

            let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15);
            let sx = frame * fw;

            let draw_w = (fw as f32 * cam.zoom) as u32;
            let draw_h = (fh as f32 * cam.zoom) as u32;
            let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let center_wy = gy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let (scx, scy) = world_to_screen(center_wx, center_wy, cam);

            let src = Rect::new(sx as i32, 0, fw, fh);
            let dst = Rect::new(
                scx - draw_w as i32 / 2,
                scy - draw_h as i32 / 2,
                draw_w,
                draw_h,
            );
            let flip_h = render_util::tile_flip(gx, gy);
            let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
        }
    }
}

pub(super) fn draw_rocks(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    if assets.rock_textures.is_empty() {
        return;
    }
    let tsi = ts.ceil() as u32;

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let variant_idx =
                render_util::variant_index(gx, gy, assets.rock_textures.len(), 13, 29);
            let tex = &assets.rock_textures[variant_idx];

            let wx = gx as f32 * TILE_SIZE;
            let wy = gy as f32 * TILE_SIZE;
            let (screen_x, screen_y) = world_to_screen(wx, wy, cam);

            let src = Rect::new(0, 0, 64, 64);
            let dst = Rect::new(screen_x, screen_y, tsi, tsi);
            let flip_h = render_util::tile_flip(gx, gy);
            let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
        }
    }
}
