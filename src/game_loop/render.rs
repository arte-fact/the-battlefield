use super::assets::LoadedTextures;
use super::environment::{draw_bushes, draw_foam, draw_rocks, draw_water};
use super::fog::update_fog_canvas;
use super::foreground::draw_foreground;
use super::hud::{
    draw_minimap, draw_overlays, draw_unit_bars, draw_victory_progress, draw_zone_hud,
    draw_zone_overlays,
};
use super::screens::{draw_death_screen, draw_main_menu, draw_result_screen, GameScreen};
use super::terrain::{render_terrain_chunk, CHUNK_TILES};
use super::touch::draw_touch_controls;
use super::LoopState;
use crate::grid::TILE_SIZE;
use crate::input::Input;
use crate::renderer::Renderer;
use wasm_bindgen::prelude::*;

pub(super) fn render_frame(
    state: &mut LoopState,
    loaded: &LoadedTextures,
    input: &Input,
) -> Result<(), JsValue> {
    // Update fog offscreen canvas if FOV changed
    if state.game.fog_dirty {
        update_fog_canvas(&state.fog_ctx, &state.game)?;
        state.game.fog_dirty = false;
    }

    // Mark all terrain chunks dirty on first render or when terrain changes
    if state.terrain_dirty {
        state.terrain_chunks.mark_all_dirty();
        state.terrain_dirty = false;
    }

    let r = &state.renderer;
    let canvas_w = r.width();
    let canvas_h = r.height();
    let game = &state.game;

    let ts = TILE_SIZE as f64;

    // 1. Clear canvas
    r.set_fill_color("#1a1a26");
    r.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    // 2. Apply camera transform
    // Snap the screen-space offset to integer pixels so all tile edges align perfectly.
    let zoom = game.camera.zoom as f64;
    let offset_x = (canvas_w / 2.0 - zoom * (game.camera.x as f64)).round();
    let offset_y = (canvas_h / 2.0 - zoom * (game.camera.y as f64)).round();
    r.save();
    r.translate(offset_x, offset_y)?;
    r.scale(zoom, zoom)?;

    // Visible tile range
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(game.grid.width as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(game.grid.height as i32) as u32;

    // 3. Water -> foam -> cached terrain chunks (only visible chunks drawn)
    draw_water(r, game, loaded, min_gx, min_gy, max_gx, max_gy)?;
    draw_foam(
        r,
        game,
        loaded,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        state.elapsed,
    )?;

    // Render and draw only the terrain chunks that overlap the visible area
    {
        let min_cx = min_gx / CHUNK_TILES;
        let min_cy = min_gy / CHUNK_TILES;
        let max_cx = ((max_gx + CHUNK_TILES - 1) / CHUNK_TILES).min(state.terrain_chunks.cols);
        let max_cy = ((max_gy + CHUNK_TILES - 1) / CHUNK_TILES).min(state.terrain_chunks.rows);

        for cy in min_cy..max_cy {
            for cx in min_cx..max_cx {
                let ci = (cy * state.terrain_chunks.cols + cx) as usize;

                // Render chunk if dirty
                if state.terrain_chunks.dirty[ci] {
                    let chunk_gx = cx * CHUNK_TILES;
                    let chunk_gy = cy * CHUNK_TILES;
                    let chunk_end_gx = (chunk_gx + CHUNK_TILES).min(game.grid.width);
                    let chunk_end_gy = (chunk_gy + CHUNK_TILES).min(game.grid.height);
                    render_terrain_chunk(
                        &state.terrain_chunks.contexts[ci],
                        game,
                        loaded,
                        &state.renderer,
                        chunk_gx,
                        chunk_gy,
                        chunk_end_gx,
                        chunk_end_gy,
                    )?;
                    state.terrain_chunks.dirty[ci] = false;
                }

                // Draw chunk to main canvas at its world position
                let wx = (cx * CHUNK_TILES) as f64 * ts;
                let wy = (cy * CHUNK_TILES) as f64 * ts;
                state
                    .renderer
                    .draw_canvas(&state.terrain_chunks.canvases[ci], wx, wy)?;
            }
        }
    }

    // 4. Capture zone overlays (colored fill, dashed border, labels, progress bars)
    draw_zone_overlays(r, game, min_gx, min_gy, max_gx, max_gy)?;

    // 5. Draw overlays (player highlight, HP bars, path line, attack target)
    draw_overlays(r, game, min_gx, min_gy, max_gx, max_gy, ts, &state.animator)?;

    // 6a. Draw ground-level rocks (always behind units)
    draw_rocks(r, game, loaded, min_gx, min_gy, max_gx, max_gy)?;

    // 6a2. Draw bush decorations (animated, ground level)
    draw_bushes(
        r,
        game,
        loaded,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        state.elapsed,
    )?;

    // 6b. Draw foreground sprites (units, particles, projectiles, trees) -- Y-sorted together
    draw_foreground(
        r,
        game,
        loaded,
        &state.animator,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        state.elapsed,
    )?;

    // 7. HP bars and order labels (drawn on top of units)
    draw_unit_bars(r, game, &state.animator)?;

    // 8. Draw fog of war -- only the visible portion of the fog canvas
    let grid_world_size = (game.grid.width as f64) * ts;
    r.set_image_smoothing(true);
    {
        // Source rect in fog canvas (1 pixel per tile)
        let sx = min_gx as f64;
        let sy = min_gy as f64;
        let sw = (max_gx - min_gx) as f64;
        let sh = (max_gy - min_gy) as f64;
        // Dest rect in world space
        let dx = min_gx as f64 * ts;
        let dy = min_gy as f64 * ts;
        let dw = sw * ts;
        let dh = sh * ts;
        state
            .renderer
            .draw_canvas_region(&state.fog_canvas, sx, sy, sw, sh, dx, dy, dw, dh)?;
    }

    // 9. Fill solid black outside the grid to hide background when zoomed out.
    let margin = grid_world_size;
    r.set_fill_color("#000");
    r.fill_rect(-margin, -margin, margin, grid_world_size + 2.0 * margin); // left
    r.fill_rect(
        grid_world_size,
        -margin,
        margin,
        grid_world_size + 2.0 * margin,
    ); // right
    r.fill_rect(0.0, -margin, grid_world_size, margin); // top
    r.fill_rect(0.0, grid_world_size, grid_world_size, margin); // bottom

    r.restore();

    // Draw zone HUD pips
    draw_zone_hud(r, game, canvas_w, r.dpr(), input.is_touch_device)?;

    // Draw minimap (top-left on touch to avoid joystick, bottom-left on desktop)
    draw_minimap(
        r,
        game,
        &state.minimap_terrain,
        canvas_w,
        canvas_h,
        r.dpr(),
        input.is_touch_device,
    )?;

    // Draw victory progress bar (only during gameplay)
    if state.screen == GameScreen::Playing {
        draw_victory_progress(r, game, canvas_w, canvas_h, r.dpr())?;
    }

    // Draw touch controls (only during gameplay)
    if state.screen == GameScreen::Playing {
        draw_touch_controls(r, input, canvas_h, r.dpr())?;
    }

    // Draw overlay screens (menu, death, win, lose)
    state.overlay_buttons.clear();
    match state.screen {
        GameScreen::MainMenu => {
            draw_main_menu(r, canvas_w, canvas_h, r.dpr(), &mut state.overlay_buttons)?;
        }
        GameScreen::PlayerDeath => {
            draw_death_screen(r, canvas_w, canvas_h, r.dpr(), &mut state.overlay_buttons)?;
        }
        GameScreen::GameWon => {
            draw_result_screen(
                r,
                canvas_w,
                canvas_h,
                r.dpr(),
                true,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::GameLost => {
            draw_result_screen(
                r,
                canvas_w,
                canvas_h,
                r.dpr(),
                false,
                &mut state.overlay_buttons,
            )?;
        }
        GameScreen::Playing => {}
    }

    Ok(())
}
