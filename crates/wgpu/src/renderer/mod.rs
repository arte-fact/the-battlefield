#![allow(clippy::too_many_arguments)]
//! Main render entry point and coordinate helpers.

pub mod assets;
pub mod draw_helpers;
pub mod effects_batch;
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
use effects_batch::EffectsBatch;
use primitive_batch::PrimitiveBatch;
use sprite_batch::SpriteBatch;

use crate::gpu::{CameraUniform, GpuContext, SpriteVertex};
use wgpu::util::DeviceExt;


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
        let s = tex.scale;
        let src = [
            frame as f32 * fw as f32 * s,
            0.0,
            fw as f32 * s,
            fh as f32 * s,
        ];
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
        let ts = tex.scale;
        let src = [0.0, 0.0, fw as f32 * ts, fh as f32 * ts];
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
    order_pulse: f32,
    order_pulse_radius: f32,
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

    // ── World-space base (terrain layer) ───────────────────────────────

    let mut base_sprites = SpriteBatch::new();
    base_sprites.begin();

    draw_water(
        &mut base_sprites,
        game,
        assets,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
    );
    draw_foam(
        &mut base_sprites,
        game,
        assets,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        elapsed,
    );
    draw_terrain(
        &mut base_sprites,
        game,
        assets,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
    );
    draw_bushes(
        &mut base_sprites,
        game,
        assets,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        elapsed,
    );
    draw_rocks(
        &mut base_sprites,
        game,
        assets,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
    );

    base_sprites.finish(gpu);

    // ── Effects layer (zone circles + player aim — rendered under units) ─

    let mut effects = EffectsBatch::new();
    effects.begin();
    draw_zone_effects(&mut effects, game, elapsed);
    draw_player_effect(&mut effects, game, elapsed, order_pulse, order_pulse_radius);
    effects.finish(gpu);

    // ── World-space foreground (units, labels, fog) ─────────────────────

    let mut sprite_batch = SpriteBatch::new();
    let mut prim_batch = PrimitiveBatch::new();
    sprite_batch.begin();
    prim_batch.begin();

    // Zone labels and progress bars (no circles — those are in effects now)
    draw_zones(&mut prim_batch, &mut sprite_batch, gpu, game, assets, cam);

    // Y-sorted foreground (units, trees, buildings, particles)
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

    // Fog of war — upload visibility data; rendered with fog pipeline in the render pass
    upload_fog_visibility(gpu, game, assets);

    // HP bars + unit markers + order labels (on top of fog)
    draw_unit_overlays(&mut prim_batch, &mut sprite_batch, gpu, game, assets, cam);

    // Floating authority numbers (on top of fog)
    draw_floating_texts(&mut sprite_batch, gpu, game, assets, cam);

    sprite_batch.finish(gpu);
    prim_batch.finish(gpu);

    // ── Screen-space HUD ───────────────────────────────────────────────

    let vw = gpu.surface_config.width as f32;
    let vh = gpu.surface_config.height as f32;

    assets.text.maybe_flush();

    // Three HUD layers: bg (panel backgrounds) → prim (fills/shapes) → fg (text/labels)
    let mut hud_bg = SpriteBatch::new();
    let mut hud_prim = PrimitiveBatch::new();
    let mut hud_sprites = SpriteBatch::new();
    hud_bg.begin();
    hud_prim.begin();
    hud_sprites.begin();

    draw_hud(
        &mut hud_bg,
        &mut hud_prim,
        &mut hud_sprites,
        gpu,
        game,
        assets,
        vw,
        vh,
    );
    draw_minimap(&mut hud_bg, &mut hud_prim, game, assets, vw, vh);
    if screen == GameScreen::Playing {
        draw_touch_controls(
            &mut hud_prim,
            &mut hud_sprites,
            gpu,
            input_state,
            assets,
            vw,
            vh,
            dpi_scale,
        );
    }

    let buttons = draw_screen_overlay(
        &mut hud_bg,
        &mut hud_prim,
        &mut hud_sprites,
        gpu,
        assets,
        screen,
        vw,
        vh,
        mouse_x,
        mouse_y,
        focused_button,
        gamepad_connected,
        dpi_scale,
    );

    hud_bg.finish(gpu);
    hud_prim.finish(gpu);
    hud_sprites.finish(gpu);

    // Upload BOTH camera matrices before the render pass
    gpu.set_camera(&CameraUniform::world_camera(cam));
    gpu.set_hud_camera(&CameraUniform::screen_ortho(vw, vh));

    // Fog quad: a single world-sized quad for the fog shader
    let world_size = game.grid.width as f32 * TILE_SIZE;
    let fog_verts = [
        SpriteVertex { position: [0.0, 0.0],             uv: [0.0, 0.0],       color_mod: [1.0; 4] },
        SpriteVertex { position: [world_size, 0.0],       uv: [1.0, 0.0],       color_mod: [1.0; 4] },
        SpriteVertex { position: [world_size, world_size], uv: [1.0, 1.0],      color_mod: [1.0; 4] },
        SpriteVertex { position: [0.0, world_size],        uv: [0.0, 1.0],      color_mod: [1.0; 4] },
    ];
    let fog_indices: [u32; 6] = [0, 1, 2, 0, 2, 3];
    let fog_vb = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fog_vb"),
        contents: bytemuck::cast_slice(&fog_verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let fog_ib = gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("fog_ib"),
        contents: bytemuck::cast_slice(&fog_indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    // ── Render pass ──────────────────────────────────────────────────────

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("frame"),
        });

    let text_textures = assets.text.textures();

    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.102,
                        g: 0.102,
                        b: 0.149,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // World-space draws: base terrain → effects → foreground → fog → prims
        base_sprites.render(&mut pass, gpu, &assets.textures, text_textures);
        effects.render(&mut pass, gpu);
        sprite_batch.render(&mut pass, gpu, &assets.textures, text_textures);
        render_fog(&mut pass, gpu, assets, &fog_vb, &fog_ib);
        prim_batch.render(&mut pass, gpu);

        // HUD draws: bg panels → primitive fills → foreground text/labels
        pass.set_bind_group(0, &gpu.hud_camera_bind_group, &[]);
        hud_bg.render_without_camera(&mut pass, gpu, &assets.textures, text_textures);
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
    batch: &mut SpriteBatch,
    game: &Game,
    assets: &Assets,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let Some(tex_id) = assets.water_texture else {
        return;
    };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Water
                && !game
                    .water_adjacency
                    .get((gy * game.grid.width + gx) as usize)
                    .copied()
                    .unwrap_or(false)
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
    batch: &mut SpriteBatch,
    game: &Game,
    assets: &Assets,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let Some(tex_id) = assets.foam_texture else {
        return;
    };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    let fs = 192.0_f32;
    let sfs = fs * tex.scale; // scaled frame size in texels
    let fc = (tex.width as f32 / sfs) as u32;
    if fc == 0 {
        return;
    }
    let draw_size = fs; // world pixels (covers 3 tiles)

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if !game
                .water_adjacency
                .get((gy * game.grid.width + gx) as usize)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            if let Some(frame) = render_util::foam_frame(elapsed, gx, gy) {
                let frame = frame % fc;
                let cx = gx as f32 * ts + ts * 0.5;
                let cy = gy as f32 * ts + ts * 0.5;
                batch.draw_sprite(
                    tex_id,
                    [frame as f32 * sfs, 0.0, sfs, sfs],
                    [
                        cx - draw_size * 0.5,
                        cy - draw_size * 0.5,
                        draw_size,
                        draw_size,
                    ],
                    (tex.width, tex.height),
                    false,
                    [1.0, 1.0, 1.0, 1.0],
                );
            }
        }
    }
}

fn draw_terrain(
    batch: &mut SpriteBatch,
    game: &Game,
    assets: &Assets,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let Some(tex_id) = assets.tilemap_texture else {
        return;
    };
    let tex = &assets.textures[tex_id];
    let ts = TILE_SIZE;
    let w = game.grid.width;
    let h = game.grid.height;

    // Pre-tinted road tileset (sand-blended at load time), falls back to normal
    let road_tex_id = assets.tilemap_road.unwrap_or(tex_id);
    let road_tex = &assets.textures[road_tex_id];

    // Sub-pass: Road surface using pre-tinted sand tileset.
    // Road neighbors use the normal tileset (grass pass covers them).
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let is_road = tile == TileKind::Road;
            let is_road_neighbor = !is_road
                && tile.is_land()
                && ((gx > 0 && game.grid.get(gx - 1, gy) == TileKind::Road)
                    || (gx + 1 < w && game.grid.get(gx + 1, gy) == TileKind::Road)
                    || (gy > 0 && game.grid.get(gx, gy - 1) == TileKind::Road)
                    || (gy + 1 < h && game.grid.get(gx, gy + 1) == TileKind::Road));
            if !is_road && !is_road_neighbor {
                continue;
            }
            let mask = autotile::cardinal_land_mask(&game.grid, gx, gy);
            let (col, row) = autotile::flat_ground_entry(mask);
            let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
            let flip = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
            // Both road tiles and neighbors use sand tileset so road color
            // shows through the grass autotile fringe at road/grass borders.
            let tid = road_tex_id;
            let tsz = (road_tex.width, road_tex.height);
            batch.draw_sprite(
                tid,
                [sx as f32, sy as f32, sw as f32, sh as f32],
                [gx as f32 * ts, gy as f32 * ts, ts, ts],
                tsz,
                flip,
                [1.0, 1.0, 1.0, 1.0],
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
                        [
                            cx - shadow_size * 0.5,
                            cy - shadow_size * 0.5,
                            shadow_size,
                            shadow_size,
                        ],
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
    let tex_id = if level == 2 {
        assets.tilemap_texture2
    } else {
        assets.tilemap_texture
    };
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
    batch: &mut SpriteBatch,
    game: &Game,
    assets: &Assets,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
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
            let Some(&(tex_id, fw, fh, fc)) = assets.bush_textures_ref().get(
                render_util::variant_index(gx, gy, assets.bush_count(), 41, 23),
            ) else {
                continue;
            };
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
    batch: &mut SpriteBatch,
    game: &Game,
    assets: &Assets,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let ts = TILE_SIZE;
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let Some(&tex_id) = assets.rock_textures_ref().get(render_util::variant_index(
                gx,
                gy,
                assets.rock_count(),
                13,
                29,
            )) else {
                continue;
            };
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
    prim: &mut PrimitiveBatch,
    sprites: &mut SpriteBatch,
    gpu: &GpuContext,
    game: &Game,
    assets: &mut Assets,
    _cam: &battlefield_core::camera::Camera,
) {
    let ts = TILE_SIZE;

    for zone in &game.zone_manager.zones {
        let r = zone.radius as f32 * ts;

        // Zone circles are rendered by the effects shader (under units).

        // Zone name label (above circle) — shadowed text, no ribbon
        let name_font = 21.0;
        let (_tw, th) = assets.text.measure_text(zone.name, name_font);
        let label_h = th as f32 + 4.0;
        let label_cy = zone.center_wy - r - label_h * 0.5 - 8.0;
        let off = 1.5_f32;

        // Shadow
        assets.text.draw_text_centered(
            sprites, gpu, zone.name,
            zone.center_wx + off, label_cy + off, name_font,
            0, 0, 0, 180,
        );
        // Foreground
        assets.text.draw_text_centered(
            sprites, gpu, zone.name,
            zone.center_wx, label_cy, name_font,
            255, 255, 255, 220,
        );

        // Capture progress bar below name
        let bar_w = 80.0;
        let bar_h = 6.0;
        let bar_x = zone.center_wx - bar_w * 0.5;
        let bar_y = label_cy + label_h * 0.5 + 2.0;

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
            zone.center_wx - 0.5,
            bar_y,
            1.0,
            bar_h,
            [1.0, 1.0, 1.0, 0.3],
        );
    }
}

fn draw_zone_effects(effects: &mut EffectsBatch, game: &Game, elapsed: f64) {
    use battlefield_core::zone::ZoneState;
    let ts = TILE_SIZE;
    for zone in &game.zone_manager.zones {
        let r = zone.radius as f32 * ts;
        // Full-strength colors — the procedural shader controls alpha itself.
        // (zone_fill_rgba targets SDL solid fills and is far too faint here.)
        let color = match zone.state {
            ZoneState::Neutral => [0.78, 0.78, 0.78, 1.0],
            ZoneState::Contested => [1.0, 0.78, 0.0, 1.0],
            ZoneState::Capturing(Faction::Blue) => [0.24, 0.47, 1.0, 0.85],
            ZoneState::Capturing(Faction::Red) => [1.0, 0.24, 0.24, 0.85],
            ZoneState::Controlled(Faction::Blue) => [0.24, 0.47, 1.0, 1.0],
            ZoneState::Controlled(Faction::Red) => [1.0, 0.24, 0.24, 1.0],
        };
        let capturing = matches!(zone.state, ZoneState::Capturing(_) | ZoneState::Contested);
        // Shift up half a tile so the ring aligns with the tower base
        // (zone center is tile-center, but tower anchor is tile-bottom).
        effects.draw_circle(
            zone.center_wx,
            zone.center_wy - ts * 0.5,
            r,
            color,
            elapsed as f32,
            0.0,
            if capturing { 1.0 } else { 0.0 },
        );
    }
}

fn draw_player_effect(
    effects: &mut EffectsBatch,
    game: &Game,
    elapsed: f64,
    order_pulse: f32,
    order_pulse_radius: f32,
) {
    let Some(player) = game.player_unit() else {
        return;
    };
    // Center slightly above feet (unit.y is sprite center)
    let ring_y = player.y + TILE_SIZE * 0.25;

    // Order range pulse (expanding ring when an order is given)
    if order_pulse > 0.0 {
        let alpha = order_pulse / 0.6; // 1.0 → 0.0 over the pulse duration
        let progress = 1.0 - alpha;
        effects.draw_circle(
            player.x,
            ring_y,
            order_pulse_radius,
            [0.9, 0.85, 0.4, alpha],
            progress,
            2.0,
            0.0,
        );
    }

    // Base aim circle (always visible) — larger for better visibility
    effects.draw_circle(
        player.x,
        ring_y,
        32.0,
        [1.0, 1.0, 0.2, 0.8],
        elapsed as f32,
        1.0,
        0.0,
    );
}

fn draw_unit_overlays(
    prim: &mut PrimitiveBatch,
    sprites: &mut SpriteBatch,
    gpu: &GpuContext,
    game: &Game,
    assets: &mut Assets,
    _cam: &battlefield_core::camera::Camera,
) {
    let ts = TILE_SIZE;

    for u in &game.units {
        if !u.alive {
            continue;
        }
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }

        // HP bar — fixed world-space size, camera matrix scales with zoom
        let ratio = u.hp as f64 / u.stats.max_hp as f64;
        let bar_w = 36.0_f32;
        let bar_h = 4.0_f32;
        let bar_x = u.x - bar_w * 0.5;
        let bar_y = u.y - ts * 0.7;

        prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.16, 0.16, 0.16, 0.78]);
        let (hr, hg, hb) = render_util::hp_bar_color(ratio);
        let fill_w = bar_w * ratio as f32;
        prim.fill_rect(
            bar_x,
            bar_y,
            fill_w,
            bar_h,
            [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 1.0],
        );

        // Order acknowledgement "!" (shadowed text, no ribbon)
        if u.order_flash > 0.0 && u.order.is_some() {
            let alpha = (u.order_flash / game.config.order_flash_duration).min(1.0);
            let font_size = 33.0_f32;
            let cx = u.x;
            let cy = u.y - ts;
            let a = (alpha * 255.0) as u8;
            // Shadow
            let off = 1.5_f32;
            assets.text.draw_text_centered(
                sprites,
                gpu,
                "!",
                cx + off,
                cy + off,
                font_size,
                0,
                0,
                0,
                a,
            );
            // Foreground (white)
            assets
                .text
                .draw_text_centered(sprites, gpu, "!", cx, cy, font_size, 255, 255, 255, a);
        }

        // Rejection "?" (shadowed text, reddish)
        if u.reject_flash > 0.0 {
            let alpha = (u.reject_flash / game.config.order_flash_duration).min(1.0);
            let font_size = 33.0_f32;
            let cx = u.x;
            let cy = u.y - ts;
            let a = (alpha * 255.0) as u8;
            let off = 1.5_f32;
            assets.text.draw_text_centered(
                sprites,
                gpu,
                "?",
                cx + off,
                cy + off,
                font_size,
                0,
                0,
                0,
                a,
            );
            assets
                .text
                .draw_text_centered(sprites, gpu, "?", cx, cy, font_size, 255, 255, 255, a);
        }
    }
}

/// Upload visibility data to the fog texture. The fog shader computes smoothing on the GPU.
fn upload_fog_visibility(gpu: &GpuContext, game: &Game, assets: &Assets) {
    if assets.fog_texture.is_none() {
        return;
    }
    let size = game.grid.width;
    // Convert bool visibility to u8: true → 255 (1.0 in R8Unorm), false → 0
    let pixels: Vec<u8> = game.visible.iter().map(|&v| if v { 255 } else { 0 }).collect();
    assets.update_fog(gpu, &pixels, size);
}

/// Render fog quad using the dedicated fog pipeline.
fn render_fog<'a>(
    pass: &mut wgpu::RenderPass<'a>,
    gpu: &'a GpuContext,
    assets: &'a Assets,
    fog_vb: &'a wgpu::Buffer,
    fog_ib: &'a wgpu::Buffer,
) {
    let Some(tex_id) = assets.fog_texture else {
        return;
    };
    let tex = &assets.textures[tex_id];
    pass.set_pipeline(&gpu.fog_pipeline);
    pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
    pass.set_bind_group(1, &tex.bind_group, &[]);
    pass.set_vertex_buffer(0, fog_vb.slice(..));
    pass.set_index_buffer(fog_ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..6, 0, 0..1);
}

fn draw_floating_texts(
    sprites: &mut SpriteBatch,
    gpu: &GpuContext,
    game: &Game,
    assets: &mut Assets,
    _cam: &battlefield_core::camera::Camera,
) {
    use battlefield_core::game::FLOATING_TEXT_DURATION;
    for ft in &game.floating_texts {
        let alpha = (ft.remaining / FLOATING_TEXT_DURATION).clamp(0.0, 1.0);
        let a = (alpha * 255.0) as u8;
        let font_size = 27.0_f32;

        let sign = if ft.value >= 0.0 { "+" } else { "" };
        // Show integer if whole, one decimal otherwise
        let text = if (ft.value - ft.value.round()).abs() < 0.01 {
            format!("{sign}{}", ft.value as i32)
        } else {
            format!("{sign}{:.1}", ft.value)
        };

        let (r, g, b) = (255u8, 255, 255);

        let off = 1.5_f32;
        // Shadow
        assets.text.draw_text_centered(
            sprites,
            gpu,
            &text,
            ft.x + off,
            ft.y + off,
            font_size,
            0,
            0,
            0,
            a,
        );
        // Foreground
        assets
            .text
            .draw_text_centered(sprites, gpu, &text, ft.x, ft.y, font_size, r, g, b, a);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Screen-space HUD
// ─────────────────────────────────────────────────────────────────────────────

fn draw_hud(
    bg: &mut SpriteBatch,
    prim: &mut PrimitiveBatch,
    sprites: &mut SpriteBatch,
    _gpu: &GpuContext,
    game: &Game,
    assets: &mut Assets,
    vw: f32,
    _vh: f32,
) {
    use battlefield_core::zone::ZoneState;

    // Player HP bar at top-left (fixed pixel sizes — canvas handles DPI)
    if let Some(player) = game.player_unit() {
        let bar_x = 10.0_f32;
        let bar_y = 6.0_f32;
        let bar_w = 200.0_f32;
        let bar_h = 46.0_f32;

        // Bar base (3-slice texture or fallback)
        if let Some((tex_id, bw, bh)) = assets.ui_bar_base {
            draw_helpers::draw_bar_3slice(
                sprites, tex_id, bw, bh, bar_x, bar_y, bar_w, bar_h, 24.0,
            );
        } else {
            prim.fill_rect(bar_x, bar_y, bar_w, bar_h, [0.0, 0.0, 0.0, 0.7]);
        }

        // HP fill
        let ratio = player.hp as f64 / player.stats.max_hp as f64;
        let fill_left = 10.0;
        let fill_top = 14.0;
        let inner_w = bar_w - 20.0;
        let fill_h = (bar_h - 24.0).max(1.0);
        let fill_w = (inner_w * ratio as f32).max(0.0);
        if fill_w > 0.0 {
            let (hr, hg, hb) = render_util::hp_bar_color(ratio);
            if let Some(fill_id) = assets.ui_bar_fill {
                let tex = &assets.textures[fill_id];
                sprites.draw_sprite(
                    fill_id,
                    [0.0, 20.0, 64.0, 24.0],
                    [bar_x + fill_left, bar_y + fill_top, fill_w, fill_h],
                    (tex.width, tex.height),
                    false,
                    [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 1.0],
                );
            } else {
                prim.fill_rect(
                    bar_x + fill_left,
                    bar_y + fill_top,
                    fill_w,
                    fill_h,
                    [hr as f32 / 255.0, hg as f32 / 255.0, hb as f32 / 255.0, 0.9],
                );
            }
        }

        // Authority bar below HP
        let auth_y = bar_y + bar_h + 6.0;
        if let Some((tex_id, bw, bh)) = assets.ui_bar_base {
            draw_helpers::draw_bar_3slice(
                sprites, tex_id, bw, bh, bar_x, auth_y, bar_w, bar_h, 24.0,
            );
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
                sprites.draw_sprite(
                    fill_id,
                    [0.0, 20.0, 64.0, 24.0],
                    [bar_x + fill_left, auth_y + fill_top, auth_fill, fill_h],
                    (tex.width, tex.height),
                    false,
                    [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 1.0],
                );
            } else {
                prim.fill_rect(
                    bar_x + fill_left,
                    auth_y + fill_top,
                    auth_fill,
                    fill_h,
                    [ar as f32 / 255.0, ag as f32 / 255.0, ab as f32 / 255.0, 0.9],
                );
            }
        }
    }

    // Zone control pips at top-center
    let zone_count = game.zone_manager.zones.len();
    if zone_count > 0 {
        let pip_r = 10.0;
        let pip_gap = 28.0;
        let total_w = zone_count as f32 * pip_gap - pip_gap + pip_r * 2.0;
        // 9-slice SpecialPaper scaled down so borders stay compact
        let scale = 0.2;
        let border_h = (44.0 + 43.0) * scale;
        let border_w = 54.0 * 2.0 * scale;
        let content_pad = 4.0;
        let panel_w = total_w + content_pad * 2.0 + border_w;
        let panel_h = pip_r * 2.0 + content_pad * 2.0 + border_h;
        let panel_x = vw * 0.5 - panel_w * 0.5;
        let panel_y_pos = 4.0;
        let pip_y = panel_y_pos + panel_h * 0.5;
        let start_x = vw * 0.5 - total_w * 0.5 + pip_r;
        if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
            draw_helpers::draw_panel_scaled(
                bg, tex_id, aw, ah,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                panel_x, panel_y_pos, panel_w, panel_h,
                scale,
            );
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

fn draw_minimap(
    bg: &mut SpriteBatch,
    prim: &mut PrimitiveBatch,
    game: &Game,
    assets: &Assets,
    _vw: f32,
    vh: f32,
) {
    let mm_size = 200.0; // Match SDL
    let pad = 10.0;
    let mm_x = pad;
    let mm_y = vh - pad - mm_size;

    let gw = game.grid.width as f32;
    let gh = game.grid.height as f32;
    let sx = mm_size / gw;
    let sy = mm_size / gh;

    // Background (9-slice paper or solid fallback)
    let panel_pad = 6.0;
    if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
        draw_helpers::draw_panel(
            bg,
            tex_id,
            aw,
            ah,
            &render_util::NINE_SLICE_SPECIAL_PAPER,
            mm_x - panel_pad,
            mm_y - panel_pad,
            mm_size + panel_pad * 2.0,
            mm_size + panel_pad * 2.0,
        );
    } else {
        prim.fill_rect(
            mm_x - 2.0,
            mm_y - 2.0,
            mm_size + 4.0,
            mm_size + 4.0,
            [0.16, 0.12, 0.06, 0.86],
        );
    }
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
                    } else if game.grid.decoration(gx, gy) == Some(Decoration::Bush) {
                        (0.22, 0.39, 0.18)
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
        prim.fill_circle(
            zx,
            zy,
            zr,
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 0.78],
        );
    }

    // Unit dots
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (ugx, ugy) = grid::world_to_grid(unit.x, unit.y);
        if unit.faction != Faction::Blue {
            let idx = (ugy as u32 * game.grid.width + ugx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] {
                continue;
            }
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
    prim.draw_line(
        cvx + cvw,
        cvy,
        cvx + cvw,
        cvy + cvh,
        1.0,
        [1.0, 1.0, 1.0, 0.78],
    );
    prim.draw_line(
        cvx + cvw,
        cvy + cvh,
        cvx,
        cvy + cvh,
        1.0,
        [1.0, 1.0, 1.0, 0.78],
    );
    prim.draw_line(cvx, cvy + cvh, cvx, cvy, 1.0, [1.0, 1.0, 1.0, 0.78]);
}

fn draw_touch_controls(
    prim: &mut PrimitiveBatch,
    sprites: &mut SpriteBatch,
    gpu: &GpuContext,
    input: &crate::input::InputState,
    assets: &mut Assets,
    _vw: f32,
    vh: f32,
    dpi_scale: f64,
) {
    if !input.is_touch_device {
        return;
    }

    let dpr = dpi_scale as f32;

    // Ghost joystick hint (before first use)
    if !input.has_used_joystick && !input.joystick.active {
        let ghost_x = 100.0 * dpr;
        let ghost_y = vh - 120.0 * dpr;
        let radius = input.joystick.max_radius;
        prim.fill_circle(ghost_x, ghost_y, radius, [1.0, 1.0, 1.0, 0.15]);
        prim.stroke_circle(ghost_x, ghost_y, radius, 1.0, [1.0, 1.0, 1.0, 0.2]);
        let font_size = 18.0 * dpr;
        assets.text.draw_text_centered(
            sprites, gpu, "MOVE", ghost_x, ghost_y, font_size, 255, 255, 255, 77,
        );
    }

    // Virtual joystick
    if input.joystick.active {
        let cx = input.joystick.center_x;
        let cy = input.joystick.center_y;
        let base_r = input.joystick.max_radius;
        prim.fill_circle(cx, cy, base_r, [1.0, 1.0, 1.0, 0.25]);
        let knob_r = 22.0;
        prim.fill_circle(
            input.joystick.stick_x,
            input.joystick.stick_y,
            knob_r,
            [1.0, 1.0, 1.0, 0.6],
        );
        prim.stroke_circle(
            input.joystick.stick_x,
            input.joystick.stick_y,
            knob_r,
            1.5,
            [1.0, 1.0, 1.0, 0.5],
        );
    }

    // Attack button
    let atk = &input.attack_button;
    let atk_alpha = if atk.pressed { 0.6 } else { 0.35 };
    prim.fill_circle(
        atk.center_x,
        atk.center_y,
        atk.radius + 2.0,
        [0.0, 0.0, 0.0, atk_alpha * 0.6],
    );
    prim.fill_circle(
        atk.center_x,
        atk.center_y,
        atk.radius,
        [0.86, 0.2, 0.2, atk_alpha],
    );
    prim.stroke_circle(
        atk.center_x,
        atk.center_y,
        atk.radius,
        1.5,
        [1.0, 1.0, 1.0, atk_alpha * 0.7],
    );
    let btn_font = (atk.radius * 0.75).max(15.0 * dpr);
    assets.text.draw_text_centered(
        sprites,
        gpu,
        "ATK",
        atk.center_x,
        atk.center_y,
        btn_font,
        255,
        255,
        255,
        242,
    );

    // Order buttons
    let order_btns: [(&crate::input::ActionButton, [f32; 3], &str); 3] = [
        (&input.order_follow_btn, [0.63, 0.31, 0.78], "F"),
        (&input.order_charge_btn, [0.86, 0.2, 0.2], "C"),
        (&input.order_defend_btn, [0.2, 0.47, 0.78], "V"),
    ];
    for (btn, rgb, label) in &order_btns {
        let a = if btn.pressed { 0.6 } else { 0.35 };
        prim.fill_circle(
            btn.center_x,
            btn.center_y,
            btn.radius + 2.0,
            [0.0, 0.0, 0.0, a * 0.5],
        );
        prim.fill_circle(
            btn.center_x,
            btn.center_y,
            btn.radius,
            [rgb[0], rgb[1], rgb[2], a],
        );
        prim.stroke_circle(
            btn.center_x,
            btn.center_y,
            btn.radius,
            1.0,
            [1.0, 1.0, 1.0, a * 0.6],
        );
        let btn_font = (btn.radius * 0.75).max(15.0 * dpr);
        assets.text.draw_text_centered(
            sprites,
            gpu,
            label,
            btn.center_x,
            btn.center_y,
            btn_font,
            255,
            255,
            255,
            242,
        );
    }
}

fn draw_screen_overlay(
    _bg: &mut SpriteBatch,
    prim: &mut PrimitiveBatch,
    sprites: &mut SpriteBatch,
    gpu: &GpuContext,
    assets: &mut Assets,
    screen: GameScreen,
    vw: f32,
    vh: f32,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    _dpi_scale: f64,
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
    prim.fill_rect(
        0.0,
        0.0,
        vw,
        vh,
        [
            or as f32 / 255.0,
            og as f32 / 255.0,
            ob as f32 / 255.0,
            oa as f32 / 255.0,
        ],
    );

    // Panel background (9-slice paper or fallback)
    // Drawn to sprites (not bg) so it layers above the prim overlay tint.
    if let Some((pw, ph)) = layout.panel_size {
        let px = (cx - pw / 2.0) as f32;
        let py = (cy - ph / 2.0) as f32;
        if let Some((tex_id, aw, ah)) = assets.ui_special_paper {
            draw_helpers::draw_panel(
                sprites,
                tex_id,
                aw,
                ah,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                px,
                py,
                pw as f32,
                ph as f32,
            );
        } else {
            prim.fill_rect(px, py, pw as f32, ph as f32, [0.16, 0.12, 0.06, 0.92]);
        }
    }

    // Title ribbon (big ribbon behind title)
    if let Some((color_row, ribbon_offset_y, ribbon_w, ribbon_h)) = layout.title_ribbon {
        if let Some(tex_id) = assets.ui_big_ribbons {
            let tex = &assets.textures[tex_id];
            let panel_y = layout.panel_size.map(|(_, ph)| cy - ph / 2.0).unwrap_or(cy);
            let rx = (cx - ribbon_w / 2.0) as f32;
            let ry = (panel_y + ribbon_offset_y) as f32;
            draw_helpers::draw_ribbon(
                sprites,
                tex_id,
                tex.width,
                tex.height,
                color_row,
                rx,
                ry,
                ribbon_w as f32,
                ribbon_h as f32,
                ribbon_h as f32,
            );
        }
    }

    // Title text
    if let Some(ref title) = layout.title {
        let tx = (cx + title.offset_x) as f32;
        let ty = (cy + title.offset_y) as f32;
        let size = title.size as f32;
        assets.text.draw_text_centered(
            sprites,
            gpu,
            &title.text,
            tx,
            ty,
            size,
            title.r,
            title.g,
            title.b,
            title.a,
        );
    }

    // Subtitle
    if let Some(ref sub) = layout.subtitle {
        let sx = (cx + sub.offset_x) as f32;
        let sy = (cy + sub.offset_y) as f32;
        let size = sub.size as f32;
        assets.text.draw_text_centered(
            sprites, gpu, &sub.text, sx, sy, size, sub.r, sub.g, sub.b, sub.a,
        );
    }

    // Buttons
    let mut buttons = Vec::new();
    for (i, btn) in layout.buttons.iter().enumerate() {
        let bx = (cx + btn.offset_x) as f32;
        let by = (cy + btn.offset_y) as f32;
        let btn_x = bx - btn.w as f32 / 2.0;
        let btn_y = by - btn.h as f32 / 2.0;

        let is_focused = gamepad_connected && i == focused_button;
        let mouse_hover = mouse_x as f32 >= btn_x
            && mouse_x as f32 <= btn_x + btn.w as f32
            && mouse_y as f32 >= btn_y
            && mouse_y as f32 <= btn_y + btn.h as f32;
        let hovering = mouse_hover || is_focused;

        let btn_atlas = match btn.style {
            ui::ButtonStyle::Blue => assets.ui_blue_btn,
            ui::ButtonStyle::Red => assets.ui_red_btn,
        };
        if let Some((tex_id, aw, ah)) = btn_atlas {
            draw_helpers::draw_panel(
                sprites,
                tex_id,
                aw,
                ah,
                &render_util::NINE_SLICE_BUTTON,
                btn_x,
                btn_y,
                btn.w as f32,
                btn.h as f32,
            );
        } else {
            let btn_color = match btn.style {
                ui::ButtonStyle::Blue => [0.15, 0.35, 0.7, 0.9],
                ui::ButtonStyle::Red => [0.7, 0.15, 0.15, 0.9],
            };
            prim.fill_rect(btn_x, btn_y, btn.w as f32, btn.h as f32, btn_color);
        }

        if hovering {
            prim.fill_rect(
                btn_x,
                btn_y,
                btn.w as f32,
                btn.h as f32,
                [1.0, 1.0, 1.0, 0.15],
            );
        }

        let text_size = (btn.h as f32 * 0.42).max(16.0);
        assets.text.draw_text_centered(
            sprites, gpu, btn.label, bx, by + 2.0, text_size, 255, 255, 255, 255,
        );

        buttons.push(ClickableButton {
            x: btn_x as f64,
            y: btn_y as f64,
            w: btn.w,
            h: btn.h,
            action: btn.action,
        });
    }

    // Hints
    for hint in &layout.hints {
        let hx = (cx + hint.offset_x) as f32;
        let hy = (cy + hint.offset_y) as f32;
        let size = hint.size.max(16.0) as f32;
        assets.text.draw_text_centered(
            sprites, gpu, &hint.text, hx, hy, size, hint.r, hint.g, hint.b, hint.a,
        );
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
