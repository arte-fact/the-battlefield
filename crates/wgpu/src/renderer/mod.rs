#![allow(clippy::too_many_arguments)]
//! Main render entry point and coordinate helpers.

pub mod assets;
pub mod draw_helpers;
pub mod primitive_batch;
pub mod sprite_batch;
pub mod text;

use assets::Assets;
use battlefield_core::autotile;
use battlefield_core::game::Game;
use battlefield_core::grid::{self, Decoration, TileKind, TILE_SIZE};
use battlefield_core::render_util;
use battlefield_core::rendering::{DrawBackend, SpriteInfo, SpriteKey};
use battlefield_core::ui::GameScreen;
use battlefield_core::unit::Faction;
use primitive_batch::PrimitiveBatch;
use sprite_batch::SpriteBatch;

use crate::gpu::{CameraUniform, GpuContext};

// ─────────────────────────────────────────────────────────────────────────────
// WgpuBackend — implements DrawBackend for core's shared foreground rendering
// ─────────────────────────────────────────────────────────────────────────────

pub struct WgpuBackend<'a> {
    pub batch: &'a mut SpriteBatch,
    pub prim: &'a mut PrimitiveBatch,
    pub assets: &'a Assets,
    pub game: &'a Game,
}

impl DrawBackend for WgpuBackend<'_> {
    fn draw_sprite(
        &mut self,
        key: SpriteKey,
        frame: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        flip: bool,
        alpha: f64,
    ) {
        let Some((tex_id, fw, fh, _fc)) = self.assets.sprite_lookup(key) else {
            return;
        };
        let tex = &self.assets.textures[tex_id];
        let src = [frame as f32 * fw as f32, 0.0, fw as f32, fh as f32];
        let dst = [x as f32, y as f32, w as f32, h as f32];
        self.batch.draw_sprite(
            tex_id,
            src,
            dst,
            (tex.width, tex.height),
            flip,
            [1.0, 1.0, 1.0, alpha as f32],
        );
    }

    fn draw_rotated(
        &mut self,
        key: SpriteKey,
        center_x: f64,
        center_y: f64,
        size: f64,
        angle: f64,
    ) {
        let Some((tex_id, fw, fh, _fc)) = self.assets.sprite_lookup(key) else {
            return;
        };
        let tex = &self.assets.textures[tex_id];
        let src = [0.0, 0.0, fw as f32, fh as f32];
        let s = size as f32;
        let dst = [center_x as f32 - s * 0.5, center_y as f32 - s * 0.5, s, s];
        self.batch.draw_sprite_rotated(
            tex_id,
            src,
            dst,
            (tex.width, tex.height),
            false,
            [1.0, 1.0, 1.0, 1.0],
            angle as f32,
        );
    }

    fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo> {
        self.assets.sprite_info(key)
    }

    fn draw_elevated_tile(&mut self, game: &Game, gx: u32, gy: u32) {
        draw_elevated_tile(self.batch, game, self.assets, gx, gy);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main render entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Render one frame. Returns clickable button regions for hit-testing.
pub fn render_frame(
    gpu: &GpuContext,
    game: &Game,
    assets: &mut Assets,
    screen: GameScreen,
    elapsed: f64,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    dpi_scale: f64,
    input_state: &crate::input::InputState,
) -> Vec<ClickableButton> {
    let cam = &game.camera;
    let (min_gx, min_gy, max_gx, max_gy) =
        render_util::visible_tile_range(cam, game.grid.width, game.grid.height);

    let surface_texture = match gpu.surface.get_current_texture() {
        Ok(t) => t,
        Err(wgpu::SurfaceError::Lost) => {
            gpu.surface.configure(&gpu.device, &gpu.surface_config);
            return Vec::new();
        }
        Err(wgpu::SurfaceError::OutOfMemory) => {
            log::error!("wgpu: out of memory");
            return Vec::new();
        }
        Err(e) => {
            log::warn!("wgpu surface error: {e:?}");
            return Vec::new();
        }
    };

    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    // ── World-space batches ──────────────────────────────────────────────

    let mut sprite_batch = SpriteBatch::new();
    let mut prim_batch = PrimitiveBatch::new();
    sprite_batch.begin();
    prim_batch.begin();

    // Pass 2: Water background
    draw_water(&mut sprite_batch, game, assets, min_gx, min_gy, max_gx, max_gy);

    // Pass 3: Foam animation
    draw_foam(&mut sprite_batch, game, assets, min_gx, min_gy, max_gx, max_gy, elapsed);

    // Pass 4: Terrain (roads + autotiled ground + elevation shadows)
    draw_terrain(&mut sprite_batch, &mut prim_batch, game, assets, min_gx, min_gy, max_gx, max_gy);

    // Pass 5: Zone overlays (world space) with labels and progress bars
    draw_zones(&mut prim_batch, &mut sprite_batch, gpu, game, assets, cam);

    // Pass 6: Bushes
    draw_bushes(&mut sprite_batch, game, assets, min_gx, min_gy, max_gx, max_gy, elapsed);

    // Pass 7: Rocks
    draw_rocks(&mut sprite_batch, game, assets, min_gx, min_gy, max_gx, max_gy);

    // Pass 8: Player aim overlay
    draw_player_overlay(&mut prim_batch, game);

    // Pass 9: Y-sorted foreground via core's shared draw_foreground
    {
        let mut backend = WgpuBackend {
            batch: &mut sprite_batch,
            prim: &mut prim_batch,
            assets,
            game,
        };
        battlefield_core::rendering::foreground::draw_foreground(
            &mut backend,
            game,
            (min_gx, min_gy, max_gx, max_gy),
            elapsed,
            |u| u.alive || u.death_fade > 0.0,
        );
    }

    // Pass 11: HP bars + unit markers + order labels (world-space)
    draw_unit_overlays(&mut prim_batch, &mut sprite_batch, gpu, game, assets, cam);

    // Pass 12: Fog of war
    draw_fog(&mut sprite_batch, gpu, game, assets);

    sprite_batch.finish(gpu);
    prim_batch.finish(gpu);

    // ── Screen-space HUD ───────────────────────────────────────────────

    let vw = gpu.surface_config.width as f32;
    let vh = gpu.surface_config.height as f32;

    assets.text.maybe_flush();

    let mut hud_prim = PrimitiveBatch::new();
    let mut hud_sprites = SpriteBatch::new();
    hud_prim.begin();
    hud_sprites.begin();

    draw_hud(&mut hud_prim, &mut hud_sprites, gpu, game, assets, vw, vh);
    draw_follower_panel(&mut hud_prim, &mut hud_sprites, gpu, game, assets, vw);
    draw_victory_progress(&mut hud_prim, &mut hud_sprites, gpu, game, assets, vw, dpi_scale);
    draw_minimap(&mut hud_prim, game, vw, vh);
    if screen == GameScreen::Playing {
        draw_touch_controls(&mut hud_prim, input_state, vw, vh);
    }

    let buttons = draw_screen_overlay(
        &mut hud_prim, &mut hud_sprites, gpu, assets, screen,
        vw, vh, mouse_x, mouse_y, focused_button, gamepad_connected, dpi_scale,
    );

    hud_prim.finish(gpu);
    hud_sprites.finish(gpu);

    // Upload BOTH camera matrices before the render pass
    gpu.set_camera(&CameraUniform::world_camera(cam));
    gpu.set_hud_camera(&CameraUniform::screen_ortho(vw, vh));

    // ── Render pass ──────────────────────────────────────────────────────

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

    let text_textures = assets.text.textures();

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.102, g: 0.102, b: 0.149, a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // World-space draws
        sprite_batch.render(&mut pass, gpu, &assets.textures, text_textures);
        prim_batch.render(&mut pass, gpu);

        // HUD draws (switch to HUD camera bind group)
        pass.set_bind_group(0, &gpu.hud_camera_bind_group, &[]);
        hud_prim.render_without_bind_group(&mut pass, gpu);
        hud_sprites.render_without_camera(&mut pass, gpu, &assets.textures, text_textures);
    }

    gpu.queue.submit(std::iter::once(encoder.finish()));
    surface_texture.present();

    buttons
}

// ─────────────────────────────────────────────────────────────────────────────
// Terrain — all coordinates in WORLD PIXELS
// ─────────────────────────────────────────────────────────────────────────────

fn draw_water(
    batch: &mut SpriteBatch, game: &Game, assets: &Assets,
    min_gx: u32, min_gy: u32, max_gx: u32, max_gy: u32,
) {
    let Some(tex_id) = assets.water_texture else { return };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Water
                && !game.water_adjacency.get((gy * game.grid.width + gx) as usize).copied().unwrap_or(false)
            {
                continue;
            }
            batch.draw_sprite(
                tex_id,
                [0.0, 0.0, tex.width as f32, tex.height as f32],
                [gx as f32 * ts, gy as f32 * ts, ts, ts],
                (tex.width, tex.height),
                false,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
}

fn draw_foam(
    batch: &mut SpriteBatch, game: &Game, assets: &Assets,
    min_gx: u32, min_gy: u32, max_gx: u32, max_gy: u32, elapsed: f64,
) {
    let Some(tex_id) = assets.foam_texture else { return };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    let fs = 192.0_f32;
    let fc = (tex.width as f32 / fs) as u32;
    if fc == 0 { return; }
    let draw_size = fs; // world pixels (covers 3 tiles)

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if !game.water_adjacency.get((gy * game.grid.width + gx) as usize).copied().unwrap_or(false) {
                continue;
            }
            if let Some(frame) = render_util::foam_frame(elapsed, gx, gy) {
                let frame = frame % fc;
                let cx = gx as f32 * ts + ts * 0.5;
                let cy = gy as f32 * ts + ts * 0.5;
                batch.draw_sprite(
                    tex_id,
                    [frame as f32 * fs, 0.0, fs, fs],
                    [cx - draw_size * 0.5, cy - draw_size * 0.5, draw_size, draw_size],
                    (tex.width, tex.height),
                    false,
                    [1.0, 1.0, 1.0, 1.0],
                );
            }
        }
    }
}

fn draw_terrain(
    batch: &mut SpriteBatch, _prim: &mut PrimitiveBatch, game: &Game, assets: &Assets,
    min_gx: u32, min_gy: u32, max_gx: u32, max_gy: u32,
) {
    let Some(tex_id) = assets.tilemap_texture else { return };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    let w = game.grid.width;
    let h = game.grid.height;

    // Sub-pass: Road surface with sand color filter (preserves tile transparency)
    let sand_tint = [1.0_f32, 0.88, 0.6, 1.0];
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
            let mask = autotile::cardinal_land_mask(&game.grid, gx, gy);
            let (col, row) = autotile::flat_ground_entry(mask);
            let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
            let flip = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
            batch.draw_sprite(
                tex_id,
                [sx as f32, sy as f32, sw as f32, sh as f32],
                [gx as f32 * ts, gy as f32 * ts, ts, ts],
                (tex.width, tex.height),
                flip,
                sand_tint,
            );
        }
    }

    // Sub-pass: Flat ground (non-road, non-water)
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            if !tile.is_land() || tile == TileKind::Road {
                continue;
            }
            let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
            let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
            let flip = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
            batch.draw_sprite(
                tex_id,
                [sx as f32, sy as f32, sw as f32, sh as f32],
                [gx as f32 * ts, gy as f32 * ts, ts, ts],
                (tex.width, tex.height),
                flip,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }

    // Sub-pass: Elevation shadows
    if let Some(shadow_id) = assets.shadow_texture {
        let stex = &assets.textures[shadow_id];
        let shadow_size = 192.0_f32;
        let elev_min_gy = min_gy.saturating_sub(1);
        for gy in elev_min_gy..max_gy {
            for gx in min_gx..max_gx {
                if game.grid.elevation(gx, gy) < 2 {
                    continue;
                }
                if gy + 1 < h && game.grid.elevation(gx, gy + 1) < 2 {
                    let cx = gx as f32 * ts + ts * 0.5;
                    let cy = (gy + 1) as f32 * ts + ts * 0.5;
                    batch.draw_sprite(
                        shadow_id,
                        [0.0, 0.0, 192.0, 192.0],
                        [cx - shadow_size * 0.5, cy - shadow_size * 0.5, shadow_size, shadow_size],
                        (stex.width, stex.height),
                        false,
                        [1.0, 1.0, 1.0, 0.5],
                    );
                }
            }
        }
    }
}

fn draw_elevated_tile(batch: &mut SpriteBatch, game: &Game, assets: &Assets, gx: u32, gy: u32) {
    let level = game.grid.elevation(gx, gy);
    if level < 2 {
        return;
    }
    let ts = TILE_SIZE;
    let tex_id = if level == 2 { assets.tilemap_texture2 } else { assets.tilemap_texture };
    let Some(tex_id) = tex_id else { return };
    let tex = &assets.textures[tex_id];

    let (col, row) = autotile::elevated_top_src(&game.grid, gx, gy, level);
    let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
    let flip = col == 6 && row == 1 && render_util::tile_flip(gx, gy);
    batch.draw_sprite(
        tex_id,
        [sx as f32, sy as f32, sw as f32, sh as f32],
        [gx as f32 * ts, gy as f32 * ts, ts, ts],
        (tex.width, tex.height),
        flip,
        [1.0, 1.0, 1.0, 1.0],
    );

    // Cliff face (drawn on the tile below)
    if let Some((ccol, crow)) = autotile::cliff_src(&game.grid, gx, gy, level) {
        let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
        let cliff_flip = render_util::tile_flip(gx, gy.wrapping_add(1000));
        batch.draw_sprite(
            tex_id,
            [csx as f32, csy as f32, csw as f32, csh as f32],
            [gx as f32 * ts, (gy + 1) as f32 * ts, ts, ts],
            (tex.width, tex.height),
            cliff_flip,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
}

fn draw_bushes(
    batch: &mut SpriteBatch, game: &Game, assets: &Assets,
    min_gx: u32, min_gy: u32, max_gx: u32, max_gy: u32, elapsed: f64,
) {
    if assets.textures.is_empty() {
        return;
    }
    let ts = TILE_SIZE;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                continue;
            }
            let Some(&(tex_id, fw, fh, fc)) = assets.bush_textures_ref()
                .get(render_util::variant_index(gx, gy, assets.bush_count(), 41, 23))
            else { continue };
            let tex = &assets.textures[tex_id];
            let frame = render_util::compute_wave_frame(elapsed, gx, gy, fc, 0.15);
            let cx = gx as f32 * ts + ts * 0.5;
            let cy = gy as f32 * ts + ts * 0.5;
            let ww = fw as f32;
            let wh = fh as f32;
            let flip = render_util::tile_flip(gx, gy);
            batch.draw_sprite(
                tex_id,
                [frame as f32 * fw as f32, 0.0, fw as f32, fh as f32],
                [cx - ww * 0.5, cy - wh * 0.5, ww, wh],
                (tex.width, tex.height),
                flip,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
}

fn draw_rocks(
    batch: &mut SpriteBatch, game: &Game, assets: &Assets,
    min_gx: u32, min_gy: u32, max_gx: u32, max_gy: u32,
) {
    let ts = TILE_SIZE;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let Some(&tex_id) = assets.rock_textures_ref()
                .get(render_util::variant_index(gx, gy, assets.rock_count(), 13, 29))
            else { continue };
            let tex = &assets.textures[tex_id];
            let flip = render_util::tile_flip(gx, gy);
            batch.draw_sprite(
                tex_id,
                [0.0, 0.0, 64.0, 64.0],
                [gx as f32 * ts, gy as f32 * ts, ts, ts],
                (tex.width, tex.height),
                flip,
                [1.0, 1.0, 1.0, 1.0],
            );
        }
    }
}

fn draw_zones(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, gpu: &GpuContext,
    game: &Game, assets: &mut Assets, cam: &battlefield_core::camera::Camera,
) {
    let ts = TILE_SIZE;
    let zoom = cam.zoom;

    for zone in &game.zone_manager.zones {
        let r = zone.radius as f32 * ts;

        // Zone circle fill + border
        let (fr, fg, fb, fa) = render_util::zone_fill_rgba(zone.state);
        prim.fill_circle(
            zone.center_wx, zone.center_wy, r,
            [fr as f32 / 255.0, fg as f32 / 255.0, fb as f32 / 255.0, fa as f32 / 255.0],
        );
        let (br, bg, bb, ba) = render_util::zone_border_rgba(zone.state);
        prim.stroke_circle(
            zone.center_wx, zone.center_wy, r, 2.0 / zoom,
            [br as f32 / 255.0, bg as f32 / 255.0, bb as f32 / 255.0, ba as f32 / 255.0],
        );

        // Zone name label (above circle)
        let name_font = 24.0 / zoom;  // constant screen size regardless of zoom
        let (tw, th) = assets.text.measure_text(zone.name, name_font);
        let label_w = tw as f32 + 8.0 / zoom;
        let label_h = th as f32 + 4.0 / zoom;
        let label_x = zone.center_wx - label_w * 0.5;
        let label_y = zone.center_wy - r - label_h - 40.0 / zoom;

        // Label background
        prim.fill_rect(label_x, label_y, label_w, label_h, [0.0, 0.0, 0.0, 0.5]);

        // Label text (world-space, scaled inversely with zoom for constant screen size)
        assets.text.draw_text_centered(
            sprites, gpu, zone.name,
            zone.center_wx, label_y + label_h * 0.5,
            name_font, 255, 255, 255, 220,
        );

        // Capture progress bar below name
        let bar_w = 160.0 / zoom;
        let bar_h = 12.0 / zoom;
        let bar_x = zone.center_wx - bar_w * 0.5;
        let bar_y = label_y + label_h + 2.0 / zoom;

        // Bar background
        prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.0, 0.0, 0.0, 0.4]);

        // Progress fill
        let progress = zone.progress as f64;
        if progress.abs() > 0.01 {
            let (cr, cg, cb) = if progress > 0.0 {
                (0.24_f32, 0.47, 1.0) // Blue
            } else {
                (1.0, 0.24, 0.24) // Red
            };
            let fill_w = bar_w * 0.5 * progress.abs() as f32;
            let fill_x = if progress > 0.0 {
                bar_x + bar_w * 0.5
            } else {
                bar_x + bar_w * 0.5 - fill_w
            };
            prim.fill_rect(fill_x, bar_y, fill_w, bar_h, [cr, cg, cb, 0.78]);
        }

        // Center line
        prim.fill_rect(
            zone.center_wx - 0.5 / zoom, bar_y, 1.0 / zoom, bar_h,
            [1.0, 1.0, 1.0, 0.3],
        );
    }
}

fn draw_player_overlay(prim: &mut PrimitiveBatch, game: &Game) {
    let Some(player) = game.player_unit() else { return };
    prim.fill_circle(player.x, player.y, 24.0, [1.0, 1.0, 0.2, 0.2]);
}

fn draw_unit_overlays(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, gpu: &GpuContext,
    game: &Game, assets: &mut Assets, cam: &battlefield_core::camera::Camera,
) {
    let ts = TILE_SIZE;
    let zoom = cam.zoom;

    for u in &game.units {
        if !u.alive { continue; }
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }

        // HP bar (world-space, fixed screen size)
        let ratio = u.hp as f64 / u.stats.max_hp as f64;
        let bar_w = 36.0 / zoom;
        let bar_h = 4.0 / zoom;
        let bar_x = u.x - bar_w * 0.5;
        let bar_y = u.y - ts * 0.7;

        prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.16, 0.16, 0.16, 0.78]);
        let (hr, hg, hb) = render_util::hp_bar_color(ratio);
        let fill_w = bar_w * ratio as f32;
        prim.fill_rect(bar_x, bar_y, fill_w, bar_h,
            [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 1.0]);

        // Player/recruited marker dot
        let dot_r = 4.0 / zoom;
        if u.is_player {
            prim.fill_circle(u.x, u.y - ts * 0.95, dot_r, [0.2, 0.86, 0.2, 0.86]);
        } else if game.recruited.contains(&u.id) {
            prim.fill_circle(u.x, u.y - ts * 0.95, dot_r, [1.0, 0.86, 0.2, 0.86]);
        }

        // Order label (flashing text above unit)
        if u.order_flash > 0.0 {
            if let Some(label) = render_util::order_label(u.order.as_ref()) {
                let alpha = (u.order_flash / battlefield_core::game::ORDER_FLASH_DURATION).min(1.0);
                let font_size = 18.0 / zoom;
                let (tw, th) = assets.text.measure_text(label, font_size);
                let label_w = tw as f32 + 8.0 / zoom;
                let label_h = th as f32 + 4.0 / zoom;
                let label_x = u.x - label_w * 0.5;
                let label_y = u.y - ts - label_h;

                // Dark background pill
                prim.fill_rect(label_x, label_y, label_w, label_h, [0.0, 0.0, 0.0, 0.6 * alpha]);

                // Label text
                let a = (alpha * 255.0) as u8;
                assets.text.draw_text_centered(
                    sprites, gpu, label,
                    u.x, label_y + label_h * 0.5,
                    font_size, 255, 215, 0, a,
                );
            }
        }
    }
}

fn draw_fog(batch: &mut SpriteBatch, gpu: &GpuContext, game: &Game, assets: &Assets) {
    let Some(tex_id) = assets.fog_texture else { return };
    let tex = &assets.textures[tex_id];
    let size = game.grid.width;
    let pixels = render_util::build_fog_pixels(&game.visible, size, game.grid.height);
    assets.update_fog(gpu, &pixels, size);
    let world_size = size as f32 * TILE_SIZE;
    batch.draw_sprite(
        tex_id,
        [0.0, 0.0, size as f32, size as f32],
        [0.0, 0.0, world_size, world_size],
        (tex.width, tex.height),
        false,
        [1.0, 1.0, 1.0, 1.0],
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Screen-space HUD
// ─────────────────────────────────────────────────────────────────────────────

fn draw_hud(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, _gpu: &GpuContext,
    game: &Game, assets: &mut Assets, vw: f32, _vh: f32,
) {
    use battlefield_core::zone::ZoneState;

    // Player HP bar at top-left
    if let Some(player) = game.player_unit() {
        let bar_x = 10.0_f32;
        let bar_y = 6.0_f32;
        let bar_w = 200.0_f32;
        let bar_h = 46.0_f32;

        // Bar base (3-slice texture or fallback)
        if let Some((tex_id, bw, bh)) = assets.ui_bar_base {
            draw_helpers::draw_bar_3slice(sprites, tex_id, bw, bh, bar_x, bar_y, bar_w, bar_h, 24.0);
        } else {
            prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.0, 0.0, 0.0, 0.7]);
        }

        // HP fill
        let ratio = player.hp as f64 / player.stats.max_hp as f64;
        let fill_left = 10.0_f32;
        let fill_top = 12.0_f32;
        let inner_w = bar_w - 20.0;
        let fill_h = (bar_h - 24.0).max(1.0);
        let fill_w = (inner_w * ratio as f32).max(0.0);
        if fill_w > 0.0 {
            let (hr, hg, hb) = render_util::hp_bar_color(ratio);
            if let Some(fill_id) = assets.ui_bar_fill {
                let tex = &assets.textures[fill_id];
                sprites.draw_sprite(fill_id, [0.0, 20.0, 64.0, 24.0],
                    [bar_x + fill_left, bar_y + fill_top, fill_w, fill_h],
                    (tex.width, tex.height), false,
                    [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 1.0]);
            } else {
                prim.fill_rect(bar_x + fill_left, bar_y + fill_top, fill_w, fill_h,
                    [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 0.9]);
            }
        }

        // Authority bar below HP
        let auth_y = bar_y + bar_h + 6.0;
        if let Some((tex_id, bw, bh)) = assets.ui_bar_base {
            draw_helpers::draw_bar_3slice(sprites, tex_id, bw, bh, bar_x, auth_y, bar_w, bar_h, 24.0);
        } else {
            prim.fill_rect(bar_x, auth_y, bar_w, bar_h, [0.0, 0.0, 0.0, 0.7]);
        }

        let auth_ratio = game.authority as f64 / 100.0;
        let auth_fill = (inner_w * auth_ratio as f32).max(0.0);
        if auth_fill > 0.0 {
            let (ar, ag, ab) = if game.authority >= 80.0 {
                (255u8, 200, 50)
            } else if game.authority >= 40.0 {
                (100, 200, 80)
            } else {
                (150, 150, 160)
            };
            if let Some(fill_id) = assets.ui_bar_fill {
                let tex = &assets.textures[fill_id];
                sprites.draw_sprite(fill_id, [0.0, 20.0, 64.0, 24.0],
                    [bar_x + fill_left, auth_y + fill_top, auth_fill, fill_h],
                    (tex.width, tex.height), false,
                    [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 1.0]);
            } else {
                prim.fill_rect(bar_x + fill_left, auth_y + fill_top, auth_fill, fill_h,
                    [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 0.9]);
            }
        }
    }

    // Zone control pips at top-center
    let zone_count = game.zone_manager.zones.len();
    if zone_count > 0 {
        let pip_r = 12.0_f32;
        let pip_gap = 32.0_f32;
        let total_w = zone_count as f32 * pip_gap - pip_gap + pip_r * 2.0;
        let start_x = vw * 0.5 - total_w * 0.5 + pip_r;
        let pip_y = 22.0_f32;

        // Background panel (9-slice paper or fallback)
        let panel_pad = 18.0_f32;
        let panel_x = start_x - pip_r - panel_pad;
        let panel_y_pos = pip_y - pip_r - panel_pad;
        let panel_w = total_w + panel_pad * 2.0;
        let panel_h = pip_r * 2.0 + panel_pad * 2.0;
        if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
            draw_helpers::draw_panel(sprites, tex_id, aw, ah,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                panel_x, panel_y_pos, panel_w, panel_h);
        } else {
            prim.fill_rect(panel_x, panel_y_pos, panel_w, panel_h, [0.16, 0.12, 0.06, 0.86]);
        }

        for (i, zone) in game.zone_manager.zones.iter().enumerate() {
            let cx = start_x + i as f32 * pip_gap;
            let progress = zone.progress.abs();

            // Background circle
            prim.fill_circle(cx, pip_y, pip_r, [0.12, 0.1, 0.08, 0.5]);

            let (cr, cg, cb, ca) = match zone.state {
                ZoneState::Neutral => (0.4, 0.4, 0.4, 0.3),
                ZoneState::Contested => (1.0, 0.78, 0.0, 0.63),
                ZoneState::Capturing(Faction::Blue) => (0.24, 0.51, 1.0, 0.7),
                ZoneState::Controlled(Faction::Blue) => (0.24, 0.51, 1.0, 1.0),
                ZoneState::Capturing(Faction::Red) => (1.0, 0.24, 0.24, 0.7),
                ZoneState::Controlled(Faction::Red) => (1.0, 0.24, 0.24, 1.0),
            };

            let inner_r = pip_r * progress.max(0.15);
            prim.fill_circle(cx, pip_y, inner_r, [cr, cg, cb, ca]);
            prim.stroke_circle(cx, pip_y, pip_r, 1.5, [cr, cg, cb, ca * 0.6]);
        }
    }
}

fn draw_minimap(prim: &mut PrimitiveBatch, game: &Game, _vw: f32, vh: f32) {
    let mm_size = 160.0_f32;
    let pad = 10.0_f32;
    let mm_x = pad;
    let mm_y = vh - pad - mm_size;

    let gw = game.grid.width as f32;
    let gh = game.grid.height as f32;
    let sx = mm_size / gw;
    let sy = mm_size / gh;

    // Background
    prim.fill_rect(mm_x - 2.0, mm_y - 2.0, mm_size + 4.0, mm_size + 4.0, [0.16, 0.12, 0.06, 0.86]);
    prim.fill_rect(mm_x, mm_y, mm_size, mm_size, [0.0, 0.0, 0.0, 0.63]);

    // Terrain dots (step 2 for performance)
    let step = 2u32;
    let rw = (sx * step as f32).ceil();
    let rh = (sy * step as f32).ceil();
    let mut gy = 0u32;
    while gy < game.grid.height {
        let mut gx = 0u32;
        while gx < game.grid.width {
            let (r, g, b) = match game.grid.get(gx, gy) {
                TileKind::Water => (0.12, 0.24, 0.47),
                TileKind::Forest => (0.12, 0.31, 0.12),
                TileKind::Rock => (0.35, 0.33, 0.29),
                TileKind::Road => (0.63, 0.55, 0.39),
                TileKind::Grass => {
                    if game.grid.elevation(gx, gy) >= 2 {
                        (0.39, 0.37, 0.27)
                    } else {
                        (0.27, 0.43, 0.20)
                    }
                }
            };
            let rx = mm_x + gx as f32 * sx;
            let ry = mm_y + gy as f32 * sy;
            prim.fill_rect(rx, ry, rw.max(1.0), rh.max(1.0), [r, g, b, 1.0]);
            gx += step;
        }
        gy += step;
    }

    // Fog overlay
    gy = 0;
    while gy < game.grid.height {
        let mut gx = 0u32;
        while gx < game.grid.width {
            let idx = (gy * game.grid.width + gx) as usize;
            if idx < game.visible.len() && !game.visible[idx] {
                let rx = mm_x + gx as f32 * sx;
                let ry = mm_y + gy as f32 * sy;
                prim.fill_rect(rx, ry, rw.max(1.0), rh.max(1.0), [0.0, 0.0, 0.0, 0.55]);
            }
            gx += step;
        }
        gy += step;
    }

    // Zone circles on minimap
    for zone in &game.zone_manager.zones {
        let zx = mm_x + zone.center_gx as f32 * sx;
        let zy = mm_y + zone.center_gy as f32 * sy;
        let zr = (zone.radius as f32 * sx).max(2.0);
        let (r, g, b) = render_util::zone_pip_rgb(zone.state);
        prim.fill_circle(zx, zy, zr, [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.78]);
    }

    // Unit dots
    for unit in &game.units {
        if !unit.alive { continue; }
        let (ugx, ugy) = grid::world_to_grid(unit.x, unit.y);
        if unit.faction != Faction::Blue {
            let idx = (ugy as u32 * game.grid.width + ugx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] { continue; }
        }
        let ux = mm_x + ugx as f32 * sx;
        let uy = mm_y + ugy as f32 * sy;
        let color = match unit.faction {
            Faction::Blue => [0.29, 0.62, 1.0, 1.0],
            Faction::Red => [1.0, 0.29, 0.29, 1.0],
        };
        prim.fill_rect(ux - 1.0, uy - 1.0, 2.0, 2.0, color);
    }

    // Camera viewport rect
    let cam = &game.camera;
    let (vl, vt, vr, vb) = cam.visible_rect();
    let world_size = gw * TILE_SIZE;
    let cvx = mm_x + (vl / world_size) * mm_size;
    let cvy = mm_y + (vt / world_size) * mm_size;
    let cvw = ((vr - vl) / world_size) * mm_size;
    let cvh = ((vb - vt) / world_size) * mm_size;
    prim.stroke_circle(cvx + cvw * 0.5, cvy + cvh * 0.5, 0.0, 0.0, [0.0; 4]); // dummy to avoid empty
    // Draw viewport rect as 4 lines
    prim.draw_line(cvx, cvy, cvx + cvw, cvy, 1.0, [1.0, 1.0, 1.0, 0.78]);
    prim.draw_line(cvx + cvw, cvy, cvx + cvw, cvy + cvh, 1.0, [1.0, 1.0, 1.0, 0.78]);
    prim.draw_line(cvx + cvw, cvy + cvh, cvx, cvy + cvh, 1.0, [1.0, 1.0, 1.0, 0.78]);
    prim.draw_line(cvx, cvy + cvh, cvx, cvy, 1.0, [1.0, 1.0, 1.0, 0.78]);
}

fn draw_follower_panel(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, gpu: &GpuContext,
    game: &Game, assets: &mut Assets, vw: f32,
) {
    use battlefield_core::asset_manifest;
    use battlefield_core::unit::UnitKind;

    let counts = game.recruited_counts();

    let icon_size = 48.0_f32;
    let pad = 12.0_f32;
    let gap = 6.0_f32;
    let header_h = 22.0_f32;
    let entry_w = icon_size + 4.0;
    let cols = 4.0_f32;
    let panel_w = cols * entry_w + (cols - 1.0) * gap + pad * 2.0;
    let panel_h = header_h + icon_size + 20.0 + pad * 2.0;
    let panel_x = vw - panel_w - 10.0;
    let panel_y = 8.0_f32;

    // Background (9-slice paper or fallback)
    if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
        draw_helpers::draw_panel(sprites, tex_id, aw, ah,
            &render_util::NINE_SLICE_SPECIAL_PAPER,
            panel_x, panel_y, panel_w, panel_h);
    } else {
        prim.fill_rect(panel_x, panel_y, panel_w, panel_h, [0.16, 0.12, 0.06, 0.86]);
    }

    // Header
    let rank = game.authority_rank_name();
    let followers = game.follower_count();
    let max_f = game.authority_max_followers();
    let header = format!("{rank}  {followers} of {max_f}");
    let hcx = panel_x + panel_w * 0.5;
    let hcy = panel_y + pad + header_h * 0.5;
    assets.text.draw_text_centered(sprites, gpu, &header, hcx, hcy, 22.0, 255, 255, 255, 240);

    // Portraits + counts
    let all_kinds = [
        (UnitKind::Warrior, counts[0]),
        (UnitKind::Archer, counts[1]),
        (UnitKind::Lancer, counts[2]),
        (UnitKind::Monk, counts[3]),
    ];
    let row_y = panel_y + pad + header_h + 2.0;

    for (i, &(kind, count)) in all_kinds.iter().enumerate() {
        let cx = panel_x + pad + i as f32 * (entry_w + gap) + entry_w * 0.5;
        let ix = cx - icon_size * 0.5;

        // Avatar sprite
        let avatar_idx = asset_manifest::avatar_index(kind);
        if let Some((tex_id, _, _, _)) = assets.sprite_lookup(
            battlefield_core::rendering::SpriteKey::Avatar(avatar_idx),
        ) {
            let tex = &assets.textures[tex_id];
            let alpha = if count == 0 { 0.35 } else { 1.0 };
            sprites.draw_sprite(
                tex_id,
                [0.0, 0.0, tex.width as f32, tex.height as f32],
                [ix, row_y, icon_size, icon_size],
                (tex.width, tex.height),
                false,
                [1.0, 1.0, 1.0, alpha],
            );
        }

        // Count
        let count_y = row_y + icon_size + 10.0;
        assets.text.draw_text_centered(
            sprites, gpu, &count.to_string(), cx, count_y, 20.0, 255, 255, 255, 240,
        );
    }
}

fn draw_victory_progress(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, gpu: &GpuContext,
    game: &Game, assets: &mut Assets, vw: f32, dpi_scale: f64,
) {
    use battlefield_core::zone::VICTORY_HOLD_TIME;

    let progress = game.zone_manager.victory_progress();
    if progress < f32::EPSILON || game.winner.is_some() {
        return;
    }
    let Some(faction) = game.zone_manager.victory_candidate else { return };

    let cx = vw * 0.5;
    let bar_w = 300.0_f32;
    let bar_h = 24.0_f32;
    let bar_x = cx - bar_w * 0.5;
    let bar_y = 50.0_f32;

    // Background
    prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.0, 0.0, 0.0, 0.7]);

    // Fill
    let fill_w = bar_w * progress;
    let fill_color = match faction {
        Faction::Blue => [0.27, 0.51, 0.9, 0.9],
        Faction::Red => [0.86, 0.24, 0.24, 0.9],
    };
    prim.fill_rect(bar_x, bar_y, fill_w, bar_h, fill_color);

    // Label
    let remaining = ((1.0 - progress) * VICTORY_HOLD_TIME) as u32;
    let name = if faction == Faction::Blue { "Blue" } else { "Red" };
    let msg = format!("{name} holds all zones. Victory in {remaining}s");
    let font_size = 20.0 * dpi_scale as f32;
    assets.text.draw_text_centered(
        sprites, gpu, &msg, cx, bar_y - 14.0, font_size, 255, 255, 255, 220,
    );
}

fn draw_touch_controls(
    prim: &mut PrimitiveBatch, input: &crate::input::InputState, _vw: f32, _vh: f32,
) {
    if !input.is_touch_device {
        return;
    }

    // Virtual joystick
    if input.joystick.active {
        let cx = input.joystick.center_x;
        let cy = input.joystick.center_y;
        let base_r = input.joystick.max_radius;
        prim.fill_circle(cx, cy, base_r, [1.0, 1.0, 1.0, 0.25]);
        let knob_r = 22.0;
        prim.fill_circle(input.joystick.stick_x, input.joystick.stick_y, knob_r, [1.0, 1.0, 1.0, 0.6]);
        prim.stroke_circle(input.joystick.stick_x, input.joystick.stick_y, knob_r, 1.5, [1.0, 1.0, 1.0, 0.5]);
    }

    // Attack button
    let atk = &input.attack_button;
    let atk_alpha = if atk.pressed { 0.6 } else { 0.35 };
    prim.fill_circle(atk.center_x, atk.center_y, atk.radius + 2.0, [0.0, 0.0, 0.0, atk_alpha * 0.6]);
    prim.fill_circle(atk.center_x, atk.center_y, atk.radius, [0.86, 0.2, 0.2, atk_alpha]);
    prim.stroke_circle(atk.center_x, atk.center_y, atk.radius, 1.5, [1.0, 1.0, 1.0, atk_alpha * 0.7]);

    // Order buttons
    let order_btns = [
        (&input.order_follow_btn, [0.63, 0.31, 0.78]),
        (&input.order_charge_btn, [0.86, 0.2, 0.2]),
        (&input.order_defend_btn, [0.2, 0.47, 0.78]),
    ];
    for (btn, rgb) in &order_btns {
        let a = if btn.pressed { 0.6 } else { 0.35 };
        prim.fill_circle(btn.center_x, btn.center_y, btn.radius + 2.0, [0.0, 0.0, 0.0, a * 0.5]);
        prim.fill_circle(btn.center_x, btn.center_y, btn.radius, [rgb[0], rgb[1], rgb[2], a]);
        prim.stroke_circle(btn.center_x, btn.center_y, btn.radius, 1.0, [1.0, 1.0, 1.0, a * 0.6]);
    }
}

fn draw_screen_overlay(
    prim: &mut PrimitiveBatch, sprites: &mut SpriteBatch, gpu: &GpuContext,
    assets: &mut Assets, screen: GameScreen,
    vw: f32, vh: f32, mouse_x: i32, mouse_y: i32,
    focused_button: usize, gamepad_connected: bool, dpi_scale: f64,
) -> Vec<ClickableButton> {
    use battlefield_core::ui;

    let layout = match screen {
        GameScreen::Playing => return Vec::new(),
        GameScreen::MainMenu => ui::main_menu_layout(),
        GameScreen::PlayerDeath => ui::death_layout(),
        GameScreen::GameWon => ui::result_layout(true),
        GameScreen::GameLost => ui::result_layout(false),
    };

    let cx = vw as f64 / 2.0;
    let cy = vh as f64 / 2.0;

    // Overlay tint
    let (or, og, ob, oa) = layout.overlay;
    prim.fill_rect(0.0, 0.0, vw, vh,
        [or as f32 / 255.0, og as f32 / 255.0, ob as f32 / 255.0, oa as f32 / 255.0]);

    // Panel background (9-slice paper or fallback)
    if let Some((pw, ph)) = layout.panel_size {
        let px = (cx - pw / 2.0) as f32;
        let py = (cy - ph / 2.0) as f32;
        if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
            draw_helpers::draw_panel(sprites, tex_id, aw, ah,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                px, py, pw as f32, ph as f32);
        } else {
            prim.fill_rect(px, py, pw as f32, ph as f32, [0.16, 0.12, 0.06, 0.92]);
        }
    }

    // Title text
    if let Some(ref title) = layout.title {
        let tx = (cx + title.offset_x) as f32;
        let ty = (cy + title.offset_y) as f32;
        let size = (title.size * dpi_scale) as f32;
        assets.text.draw_text_centered(sprites, gpu, &title.text, tx, ty, size, title.r, title.g, title.b, title.a);
    }

    // Subtitle
    if let Some(ref sub) = layout.subtitle {
        let sx = (cx + sub.offset_x) as f32;
        let sy = (cy + sub.offset_y) as f32;
        let size = (sub.size * dpi_scale) as f32;
        assets.text.draw_text_centered(sprites, gpu, &sub.text, sx, sy, size, sub.r, sub.g, sub.b, sub.a);
    }

    // Buttons
    let mut buttons = Vec::new();
    for (i, btn) in layout.buttons.iter().enumerate() {
        let bx = (cx + btn.offset_x) as f32;
        let by = (cy + btn.offset_y) as f32;
        let btn_x = bx - btn.w as f32 / 2.0;
        let btn_y = by - btn.h as f32 / 2.0;

        let is_focused = gamepad_connected && i == focused_button;
        let mouse_hover = mouse_x as f32 >= btn_x && mouse_x as f32 <= btn_x + btn.w as f32
            && mouse_y as f32 >= btn_y && mouse_y as f32 <= btn_y + btn.h as f32;
        let hovering = mouse_hover || is_focused;

        let btn_atlas = match btn.style {
            ui::ButtonStyle::Blue => assets.ui_blue_btn,
            ui::ButtonStyle::Red => assets.ui_red_btn,
        };
        if let Some((tex_id, aw, ah)) = btn_atlas {
            draw_helpers::draw_panel(sprites, tex_id, aw, ah,
                &render_util::NINE_SLICE_BUTTON,
                btn_x, btn_y, btn.w as f32, btn.h as f32);
        } else {
            let btn_color = match btn.style {
                ui::ButtonStyle::Blue => [0.15, 0.35, 0.7, 0.9],
                ui::ButtonStyle::Red => [0.7, 0.15, 0.15, 0.9],
            };
            prim.fill_rect(btn_x, btn_y, btn.w as f32, btn.h as f32, btn_color);
        }

        if hovering {
            prim.fill_rect(btn_x, btn_y, btn.w as f32, btn.h as f32, [1.0, 1.0, 1.0, 0.15]);
        }

        let text_size = 24.0 * dpi_scale as f32;
        assets.text.draw_text_centered(sprites, gpu, btn.label, bx, by, text_size, 255, 255, 255, 255);

        buttons.push(ClickableButton {
            x: btn_x as f64, y: btn_y as f64,
            w: btn.w, h: btn.h,
            action: btn.action,
        });
    }

    // Hints
    for hint in &layout.hints {
        let hx = (cx + hint.offset_x) as f32;
        let hy = (cy + hint.offset_y) as f32;
        let size = (hint.size.max(24.0) * dpi_scale) as f32;
        assets.text.draw_text_centered(sprites, gpu, &hint.text, hx, hy, size, hint.r, hint.g, hint.b, hint.a);
    }

    buttons
}

// ─────────────────────────────────────────────────────────────────────────────
// Types
// ─────────────────────────────────────────────────────────────────────────────

pub struct ClickableButton {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub action: battlefield_core::ui::ButtonAction,
}

impl ClickableButton {
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let px = px as f64;
        let py = py as f64;
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}
