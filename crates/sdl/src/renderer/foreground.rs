#![allow(clippy::too_many_arguments)]

use battlefield_core::asset_manifest;
use battlefield_core::camera::Camera;
use battlefield_core::game::{Game, ORDER_FLASH_DURATION};
use battlefield_core::grid::{Decoration, TileKind, TILE_SIZE};
use battlefield_core::render_util;
use battlefield_core::sheep::SHEEP_FRAME_SIZE;
use battlefield_core::sprite::SpriteSheet;
use battlefield_core::unit::{Facing, Faction, UnitAnim, UnitKind};
use battlefield_core::zone::{ZoneState, VICTORY_HOLD_TIME};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::assets::Assets;
use super::draw_helpers::{draw_bar_3slice, draw_small_ribbon, fill_circle, stroke_circle};
use super::{world_to_screen, Drawable, UnitTexKey};

pub(super) fn draw_zones(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &mut Assets,
    game: &Game,
    cam: &Camera,
    ts: f32,
    dpi_scale: f64,
) {
    canvas.set_blend_mode(BlendMode::Blend);
    for zone in &game.zone_manager.zones {
        let (sx, sy) = world_to_screen(zone.center_wx, zone.center_wy, cam);
        let radius = (zone.radius as f32 * ts) as i32;

        let (fr, fg, fb, fa) = render_util::zone_fill_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(fr, fg, fb, fa));
        fill_circle(canvas, sx, sy, radius);

        let (br, bg, bb, ba) = render_util::zone_border_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(br, bg, bb, ba / 3));
        stroke_circle(canvas, sx, sy, radius - 2);
        canvas.set_draw_color(Color::RGBA(br, bg, bb, ba));
        stroke_circle(canvas, sx, sy, radius);
        stroke_circle(canvas, sx, sy, radius + 1);

        let zoom = cam.zoom as f64;
        let zone_font = (24.0 * dpi_scale as f32) * cam.zoom;
        let ribbon_h = 54.0 * zoom;

        let bar_w = 160.0 * zoom;
        let bar_h = 46.0 * zoom;
        let total_h = ribbon_h + 2.0 * zoom + bar_h;
        let top_y = sy as f64 - radius as f64 - total_h - 2.0 * zoom;
        let name_y = (top_y + ribbon_h / 2.0) as i32;
        let bar_x = sx as f64 - bar_w / 2.0;
        let bar_y = top_y + ribbon_h + 2.0 * zoom;

        let ribbon_row = match zone.state {
            ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
            ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 3,
            _ => 9,
        };

        if let Some(ref tex) = assets.ui_small_ribbons {
            let (tw, _th) = assets.text.measure_text(zone.name, zone_font);
            let center_w = tw as f64 + 4.0 * zoom;
            draw_small_ribbon(
                canvas,
                tex,
                ribbon_row,
                sx as f64,
                name_y as f64,
                center_w,
                zoom,
            );
        }

        assets.text.draw_text_centered(
            canvas,
            tc,
            zone.name,
            sx,
            name_y,
            zone_font,
            Color::RGBA(255, 255, 255, 220),
        );

        if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
            draw_bar_3slice(
                canvas,
                tex,
                bw as f64,
                bh as f64,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                24.0 * zoom,
            );
        } else {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 100));
            let _ = canvas.fill_rect(Rect::new(
                bar_x as i32,
                bar_y as i32,
                bar_w as u32,
                bar_h as u32,
            ));
        }

        let progress = zone.progress as f64;
        let fill_inset_x = 10.0 * zoom;
        let fill_inset_y = 12.0 * zoom;
        let inner_w = bar_w - fill_inset_x * 2.0;
        let fill_h = (bar_h - fill_inset_y * 2.0).max(1.0);
        if progress.abs() > 0.01 {
            let (fr, fg, fb) = if progress > 0.0 {
                (60u8, 120u8, 255u8)
            } else {
                (255u8, 60u8, 60u8)
            };
            let fill_w = (inner_w * 0.5 * progress.abs()).max(0.0);
            if fill_w > 0.0 {
                let fill_x = if progress > 0.0 {
                    bar_x + fill_inset_x + inner_w * 0.5
                } else {
                    bar_x + fill_inset_x + inner_w * 0.5 - fill_w
                };
                if let Some(ref mut fill_tex) = assets.ui_bar_fill {
                    super::safe_set_color_mod(fill_tex, fr, fg, fb);
                    let _ = canvas.copy(
                        fill_tex,
                        Rect::new(0, 20, 64, 24),
                        Rect::new(
                            fill_x as i32,
                            (bar_y + fill_inset_y) as i32,
                            fill_w as u32,
                            fill_h as u32,
                        ),
                    );
                    super::safe_set_color_mod(fill_tex, 255, 255, 255);
                } else {
                    canvas.set_draw_color(Color::RGBA(fr, fg, fb, 200));
                    let _ = canvas.fill_rect(Rect::new(
                        fill_x as i32,
                        (bar_y + fill_inset_y) as i32,
                        fill_w as u32,
                        fill_h as u32,
                    ));
                }
            }
        }
    }
}

pub(super) fn draw_player_overlay(canvas: &mut Canvas<Window>, game: &Game, cam: &Camera) {
    let player = match game.player_unit() {
        Some(p) => p,
        None => return,
    };

    let (px, py) = world_to_screen(player.x, player.y, cam);

    let radius = (24.0 * cam.zoom) as i32;
    canvas.set_draw_color(Color::RGBA(255, 255, 51, 50));
    draw_filled_circle(canvas, px, py, radius);
}

fn draw_filled_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
    for dy in -radius..=radius {
        let dx = ((radius * radius - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.draw_line((cx - dx, cy + dy), (cx + dx, cy + dy));
    }
}

pub(super) fn draw_foreground(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let ts_f64 = TILE_SIZE as f64;
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));

    // Reuse persistent buffer to avoid per-frame allocation
    assets.drawable_buf.clear();

    // Units
    for (i, u) in game.units.iter().enumerate() {
        if !u.alive && u.death_fade <= 0.0 {
            continue;
        }
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }
        assets
            .drawable_buf
            .push((u.y as f64 + ts_f64 * 0.5, Drawable::Unit(i)));
    }

    // Trees, water rocks, and elevated tiles
    let tree_max_gy = (max_gy + 4).min(game.grid.height);
    for gy in min_gy..tree_max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts_f64;
            if tile == TileKind::Forest && !assets.tree_textures.is_empty() {
                assets.drawable_buf.push((foot_y, Drawable::Tree(gx, gy)));
            }
            if game.grid.decoration(gx, gy) == Some(Decoration::WaterRock)
                && !assets.water_rock_textures.is_empty()
            {
                assets
                    .drawable_buf
                    .push((foot_y, Drawable::WaterRock(gx, gy)));
            }
            if game.grid.elevation(gx, gy) >= 2 {
                assets
                    .drawable_buf
                    .push((foot_y, Drawable::ElevatedTile(gx, gy)));
            }
        }
    }

    // Production buildings
    for (i, b) in game.buildings.iter().enumerate() {
        let foot_y = b.grid_y as f64 * ts_f64;
        assets
            .drawable_buf
            .push((foot_y, Drawable::BaseBuilding(i)));
    }

    // Sheep
    for (i, s) in game.sheep.iter().enumerate() {
        assets
            .drawable_buf
            .push((s.y as f64 + ts_f64 * 0.5, Drawable::Sheep(i)));
    }

    // Pawns
    for (i, p) in game.pawns.iter().enumerate() {
        assets
            .drawable_buf
            .push((p.y as f64 + ts_f64 * 0.5, Drawable::Pawn(i)));
    }

    // Particles
    for (i, p) in game.particles.iter().enumerate() {
        if !p.finished {
            assets
                .drawable_buf
                .push((p.world_y as f64 + ts_f64 * 0.5, Drawable::Particle(i)));
        }
    }

    assets
        .drawable_buf
        .sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Take the buffer out to avoid borrow conflict (draw functions need &mut assets)
    let drawables = std::mem::take(&mut assets.drawable_buf);

    for (_, drawable) in &drawables {
        match drawable {
            Drawable::Unit(idx) => {
                draw_unit(canvas, game, assets, cam, ts, *idx, elapsed);
            }
            Drawable::Tree(gx, gy) => {
                draw_tree(canvas, assets, cam, ts, *gx, *gy, elapsed, player_pos);
            }
            Drawable::WaterRock(gx, gy) => {
                draw_water_rock(canvas, assets, cam, ts, *gx, *gy, elapsed);
            }
            Drawable::BaseBuilding(idx) => {
                draw_base_building(canvas, game, assets, cam, ts, *idx, player_pos);
            }
            Drawable::Particle(idx) => {
                draw_particle(canvas, game, assets, cam, ts, *idx);
            }
            Drawable::Sheep(idx) => {
                draw_sheep(canvas, game, assets, cam, ts, *idx);
            }
            Drawable::Pawn(idx) => {
                draw_pawn(canvas, game, assets, cam, ts, *idx);
            }
            Drawable::ElevatedTile(gx, gy) => {
                super::terrain::draw_elevated_tile(canvas, game, assets, cam, ts, *gx, *gy);
            }
        }
    }

    // Return the buffer for reuse next frame
    assets.drawable_buf = drawables;
}

fn draw_unit(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
    elapsed: f64,
) {
    let unit = &game.units[idx];
    let key = UnitTexKey {
        faction: unit.faction,
        kind: unit.kind,
        anim: unit.current_anim,
    };

    if let Some((tex, fw, _fh, _frames)) = assets.unit_textures.get_mut(&key) {
        let fw_val = *fw;
        let frame_count = unit.animation.frame_count;
        let sheet = SpriteSheet {
            frame_width: fw_val,
            frame_height: fw_val,
            frame_count,
        };

        let anim_frame = if unit.kind == UnitKind::Archer && unit.current_anim == UnitAnim::Idle {
            let (gx, gy) = unit.grid_cell();
            render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15)
        } else {
            unit.animation.current_frame
        };

        let (sx, sy, sw, sh) = sheet.frame_src_rect(anim_frame);
        let draw_size = ts * (fw_val as f32 / TILE_SIZE);
        let (screen_x, screen_y) = world_to_screen(unit.x, unit.y, cam);
        let half = (draw_size / 2.0) as i32;

        let dst = Rect::new(
            screen_x - half,
            screen_y - half,
            draw_size as u32,
            draw_size as u32,
        );
        let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);

        let opacity = render_util::unit_opacity(unit.alive, unit.death_fade, unit.hit_flash);
        let alpha = (opacity * 255.0) as u8;
        super::safe_set_alpha(tex, alpha);

        let flip = unit.facing == Facing::Left;
        let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip, false);

        super::safe_set_alpha(tex, 255);
    } else {
        let (screen_x, screen_y) = world_to_screen(unit.x, unit.y, cam);
        let color = match unit.faction {
            Faction::Blue => Color::RGB(60, 120, 255),
            Faction::Red => Color::RGB(255, 60, 60),
        };
        canvas.set_draw_color(color);
        let size = (ts * 0.6) as u32;
        let half = size as i32 / 2;
        let _ = canvas.fill_rect(Rect::new(screen_x - half, screen_y - half, size, size));
    }
}

fn draw_tree(
    canvas: &mut Canvas<Window>,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    gx: u32,
    gy: u32,
    elapsed: f64,
    player_pos: Option<(f64, f64)>,
) {
    let variant_idx = render_util::variant_index(gx, gy, assets.tree_textures.len(), 31, 17);
    let (ref mut tex, fw, fh, frame_count) = assets.tree_textures[variant_idx];
    let ts_f64 = TILE_SIZE as f64;

    let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15);
    let sx = frame * fw;

    let draw_w = cam.zoom * TILE_SIZE * 3.0;
    let draw_h = draw_w * (fh as f32 / fw as f32);
    let wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    let wy = gy as f32 * TILE_SIZE + TILE_SIZE;
    let (screen_cx, screen_by) = world_to_screen(wx, wy, cam);
    let dst_x = screen_cx - (draw_w * 0.5) as i32;
    let dst_y = screen_by - draw_h as i32;
    let dst = Rect::new(dst_x, dst_y, draw_w as u32, draw_h as u32);
    let src = Rect::new(sx as i32, 0, fw, fh);

    let tree_cx = gx as f64 * ts_f64 + ts_f64 * 0.5;
    let tree_cy = gy as f64 * ts_f64 - ts_f64 * 1.0;
    let alpha_f = render_util::tree_alpha(tree_cx, tree_cy, player_pos, ts_f64);
    let alpha = (alpha_f * 255.0) as u8;
    super::safe_set_alpha(tex, alpha);

    let flip_h = render_util::tile_flip(gx, gy);
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);

    super::safe_set_alpha(tex, 255);
}

fn draw_water_rock(
    canvas: &mut Canvas<Window>,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    gx: u32,
    gy: u32,
    elapsed: f64,
) {
    let variant_idx = render_util::variant_index(gx, gy, assets.water_rock_textures.len(), 37, 19);
    let (ref tex, fw, fh, frame_count) = assets.water_rock_textures[variant_idx];

    let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.2);
    let sx = frame * fw;

    let tsi = ts.ceil() as u32;
    let wx = gx as f32 * TILE_SIZE;
    let wy = gy as f32 * TILE_SIZE;
    let (screen_x, screen_y) = world_to_screen(wx, wy, cam);

    let src = Rect::new(sx as i32, 0, fw, fh);
    let dst = Rect::new(screen_x, screen_y, tsi, tsi);
    let flip_h = render_util::tile_flip(gx, gy);
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
}

fn draw_base_building(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    idx: usize,
    player_pos: Option<(f64, f64)>,
) {
    let building = &game.buildings[idx];
    let (sw, sh) = building.kind.sprite_size();
    let wx = building.grid_x as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    let wy = building.grid_y as f32 * TILE_SIZE + TILE_SIZE;
    let (scx, sby) = world_to_screen(wx, wy, cam);
    let draw_w = sw as f32 * cam.zoom;
    let draw_h = sh as f32 * cam.zoom;
    let dst = Rect::new(
        scx - (draw_w * 0.5) as i32,
        sby - draw_h as i32,
        draw_w as u32,
        draw_h as u32,
    );

    // Fade when player is behind the building (sprite covers the player)
    let ts_f64 = TILE_SIZE as f64;
    let bldg_cx = wx as f64;
    let bldg_cy = wy as f64 - sh as f64 * 0.5;
    let proximity_alpha = render_util::tree_alpha(bldg_cx, bldg_cy, player_pos, ts_f64);

    // Zone-linked towers use tower_textures (neutral/blue/red) with alpha modulation
    if let Some(zid) = building.zone_id {
        if assets.tower_textures.is_empty() {
            return;
        }
        let zone = &game.zone_manager.zones[zid as usize];
        let color_idx = match zone.state {
            ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
            ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 2,
            _ => 0,
        };
        if color_idx >= assets.tower_textures.len() {
            return;
        }
        let zone_alpha = match zone.state {
            ZoneState::Capturing(_) => (zone.progress.abs() as f64 * 0.5 + 0.5).clamp(0.5, 1.0),
            _ => 1.0,
        };
        let alpha = (proximity_alpha * zone_alpha * 255.0) as u8;
        let tex = &mut assets.tower_textures[color_idx];
        super::safe_set_alpha(tex, alpha);
        let _ = canvas.copy(tex, Rect::new(0, 0, sw, sh), dst);
        super::safe_set_alpha(tex, 255);
        return;
    }

    let tex_idx =
        asset_manifest::building_tex_index(building.kind, building.house_variant, building.faction);
    if let Some(Some((ref mut tex, _sw, _sh))) = assets.building_textures.get_mut(tex_idx) {
        let alpha = (proximity_alpha * 255.0) as u8;
        super::safe_set_alpha(tex, alpha);
        let _ = canvas.copy(tex, None, dst);
        super::safe_set_alpha(tex, 255);
    }
}

fn draw_particle(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
) {
    let p = &game.particles[idx];
    if p.finished {
        return;
    }
    if let Some(tex) = assets.particle_textures.get_mut(&p.kind) {
        let is_heal = p.kind == battlefield_core::particle::ParticleKind::HealEffect;
        if is_heal {
            super::safe_set_alpha(tex, 153); // ~60% opacity
        }
        let fs = p.kind.frame_size();
        let sheet = SpriteSheet {
            frame_width: fs,
            frame_height: fs,
            frame_count: p.kind.frame_count(),
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(p.animation.current_frame);
        let draw_size = ts * (fs as f32 / TILE_SIZE);
        let (screen_x, screen_y) = world_to_screen(p.world_x, p.world_y, cam);
        let half = (draw_size / 2.0) as i32;

        let dst = Rect::new(
            screen_x - half,
            screen_y - half,
            draw_size as u32,
            draw_size as u32,
        );
        let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);
        let _ = canvas.copy(tex, src, dst);
        if is_heal {
            super::safe_set_alpha(tex, 255);
        }
    }
}

fn draw_sheep(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
) {
    let sheep = &game.sheep[idx];
    let sprite_idx = sheep.sprite_index();
    if sprite_idx >= assets.sheep_textures.len() {
        return;
    }
    let (ref mut tex, _frame_count) = assets.sheep_textures[sprite_idx];
    let sheet = SpriteSheet {
        frame_width: SHEEP_FRAME_SIZE,
        frame_height: SHEEP_FRAME_SIZE,
        frame_count: sheep.anim_frame_count(),
    };
    let (sx, sy, sw, sh) = sheet.frame_src_rect(sheep.animation.current_frame);
    let draw_size = ts * (SHEEP_FRAME_SIZE as f32 / TILE_SIZE);
    let (screen_x, screen_y) = world_to_screen(sheep.x, sheep.y, cam);
    let half = (draw_size / 2.0) as i32;

    let dst = Rect::new(
        screen_x - half,
        screen_y - half,
        draw_size as u32,
        draw_size as u32,
    );
    let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);
    let flip = sheep.facing == Facing::Left;
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip, false);
}

fn draw_pawn(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
) {
    let pawn = &game.pawns[idx];
    let faction_offset = match pawn.faction {
        Faction::Blue => 0,
        Faction::Red => asset_manifest::PAWN_SPECS.len(),
    };
    let tex_idx = faction_offset + pawn.sprite_index();
    if tex_idx >= assets.pawn_textures.len() {
        return;
    }
    let (ref tex, fw, _fh, _fc) = assets.pawn_textures[tex_idx];
    let sheet = SpriteSheet {
        frame_width: fw,
        frame_height: fw,
        frame_count: pawn.anim_frame_count(),
    };
    let (sx, sy, sw, sh) = sheet.frame_src_rect(pawn.animation.current_frame);
    let draw_size = ts * (fw as f32 / TILE_SIZE);
    let (screen_x, screen_y) = world_to_screen(pawn.x, pawn.y, cam);
    let half = (draw_size / 2.0) as i32;
    let dst = Rect::new(
        screen_x - half,
        screen_y - half,
        draw_size as u32,
        draw_size as u32,
    );
    let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);
    let flip = pawn.facing == Facing::Left;
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip, false);
}

pub(super) fn draw_projectiles(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
) {
    let zoom = cam.zoom;
    for proj in &game.projectiles {
        if proj.finished {
            continue;
        }
        let (sx, sy) = world_to_screen(proj.current_x, proj.current_y, cam);

        if let Some(ref tex) = assets.arrow_texture {
            let arrow_size = (64.0 * zoom) as u32;
            let half = arrow_size as i32 / 2;
            let dst = Rect::new(sx - half, sy - half, arrow_size, arrow_size);
            let angle_deg = proj.angle.to_degrees() as f64;
            let _ = canvas.copy_ex(tex, None, dst, angle_deg, None, false, false);
        } else {
            let w = (8.0 * zoom) as u32;
            let h = (4.0 * zoom) as u32;
            canvas.set_draw_color(Color::RGB(200, 180, 120));
            let _ = canvas.fill_rect(Rect::new(sx - w as i32 / 2, sy - h as i32 / 2, w, h));
        }
    }
}

/// Merged pass for HP bars, unit markers, and order labels.
/// Single iteration over units instead of 3 separate passes.
pub(super) fn draw_unit_overlays(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    game: &Game,
    cam: &Camera,
    dpi_scale: f64,
) {
    canvas.set_blend_mode(BlendMode::Blend);
    let zoom = game.camera.zoom;
    let dot_r = (4.0 * zoom).max(2.0) as i32;

    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = unit.grid_cell();
        if !render_util::is_visible_to_player(unit.faction, gx, gy, &game.visible, game.grid.width)
        {
            continue;
        }

        let (sx, sy) = world_to_screen(unit.x, unit.y, cam);

        // HP bar
        let bar_w = (36.0 * zoom) as i32;
        let bar_h = (4.0 * zoom).max(2.0) as i32;
        let bar_y = sy - (TILE_SIZE * zoom * 0.7) as i32;
        let bar_x = sx - bar_w / 2;

        canvas.set_draw_color(Color::RGBA(40, 40, 40, 200));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        let ratio = unit.hp as f32 / unit.stats.max_hp as f32;
        let fill_w = (bar_w as f32 * ratio) as u32;
        let (hr, hg, hb) = render_util::hp_bar_color(ratio as f64);
        canvas.set_draw_color(Color::RGB(hr, hg, hb));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, fill_w, bar_h as u32));

        // Unit marker (player = green, recruited = yellow)
        let marker_color = if unit.is_player {
            Some(Color::RGBA(50, 220, 50, 220))
        } else if game.recruited.contains(&unit.id) {
            Some(Color::RGBA(255, 220, 50, 220))
        } else {
            None
        };
        if let Some(color) = marker_color {
            let marker_y = sy - (TILE_SIZE * zoom * 0.95) as i32;
            canvas.set_draw_color(color);
            for dy in -dot_r..=dot_r {
                let dx = ((dot_r * dot_r - dy * dy) as f32).sqrt() as i32;
                let _ = canvas.draw_line((sx - dx, marker_y + dy), (sx + dx, marker_y + dy));
            }
        }

        // Order label (flashing text above unit)
        if unit.order_flash > 0.0 {
            if let Some(label) = render_util::order_label(unit.order.as_ref()) {
                let alpha = ((unit.order_flash / ORDER_FLASH_DURATION) * 255.0) as u8;
                let label_y = sy - (TILE_SIZE * zoom) as i32;
                let font_size = (24.0 * dpi_scale as f32) * zoom;
                let ribbon_h = 54.0 * zoom as f64;
                let label_cy = label_y - (ribbon_h / 2.0) as i32;

                if let Some(ref tex) = assets.ui_small_ribbons {
                    let (tw, _th) = assets.text.measure_text(label, font_size);
                    let center_w = tw as f64 + 4.0 * zoom as f64;
                    draw_small_ribbon(
                        canvas,
                        tex,
                        5, // Yellow row
                        sx as f64,
                        label_cy as f64,
                        center_w,
                        zoom as f64,
                    );
                }

                assets.text.draw_text_centered(
                    canvas,
                    tc,
                    label,
                    sx,
                    label_cy,
                    font_size,
                    Color::RGBA(255, 215, 0, alpha),
                );
            }
        }
    }
}

pub(super) fn draw_fog(
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
    let w = game.grid.width;

    let pixels = render_util::build_fog_pixels(&game.visible, w, game.grid.height);

    if let Some(ref mut tex) = assets.fog_texture {
        let pitch = (w * 4) as usize;
        let _ = tex.update(None, &pixels, pitch);

        let src_w = (max_gx - min_gx).max(1);
        let src_h = (max_gy - min_gy).max(1);
        let src = Rect::new(min_gx as i32, min_gy as i32, src_w, src_h);

        let (sx, sy) = world_to_screen(min_gx as f32 * TILE_SIZE, min_gy as f32 * TILE_SIZE, cam);
        let dst_w = (src_w as f32 * ts) as u32;
        let dst_h = (src_h as f32 * ts) as u32;
        let dst = Rect::new(sx, sy, dst_w, dst_h);

        let _ = canvas.copy(tex, src, dst);
    }
}

pub(super) fn draw_victory_progress(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &mut Assets,
    game: &Game,
    dpi_scale: f64,
) {
    let progress = game.zone_manager.victory_progress();
    if progress < f32::EPSILON || game.winner.is_some() {
        return;
    }
    let faction = match game.zone_manager.victory_candidate {
        Some(f) => f,
        None => return,
    };

    let (w, _h) = canvas.output_size().unwrap_or((960, 640));
    let cx = w as f64 / 2.0;
    let bar_w = 300.0_f64;
    let bar_h = 46.0_f64;
    let bar_x = cx - bar_w / 2.0;
    let bar_y = 46.0_f64;

    canvas.set_blend_mode(BlendMode::Blend);

    if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
        draw_bar_3slice(
            canvas, tex, bw as f64, bh as f64, bar_x, bar_y, bar_w, bar_h, 24.0,
        );
    }

    let fill_left = 10.0_f64;
    let fill_right = 10.0_f64;
    let fill_top = 12.0_f64;
    let fill_bottom = 12.0_f64;
    let inner_w = bar_w - fill_left - fill_right;
    let fill_w = (inner_w * progress as f64).max(0.0);
    let fill_h = (bar_h - fill_top - fill_bottom).max(1.0);
    if fill_w > 0.0 {
        let (fr, fg, fb) = match faction {
            Faction::Blue => (70u8, 130u8, 230u8),
            Faction::Red => (220, 60, 60),
        };
        if let Some(ref mut fill_tex) = assets.ui_bar_fill {
            super::safe_set_color_mod(fill_tex, fr, fg, fb);
            let _ = canvas.copy(
                fill_tex,
                Rect::new(0, 20, 64, 24),
                Rect::new(
                    (bar_x + fill_left) as i32,
                    (bar_y + fill_top) as i32,
                    fill_w as u32,
                    fill_h as u32,
                ),
            );
            super::safe_set_color_mod(fill_tex, 255, 255, 255);
        } else {
            canvas.set_draw_color(Color::RGB(fr, fg, fb));
            let _ = canvas.fill_rect(Rect::new(
                (bar_x + fill_left) as i32,
                (bar_y + fill_top) as i32,
                fill_w as u32,
                fill_h as u32,
            ));
        }
    }

    if assets.ui_bar_base.is_none() {
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 100));
        let _ = canvas.draw_rect(Rect::new(
            bar_x as i32,
            bar_y as i32,
            bar_w as u32,
            bar_h as u32,
        ));
    }

    let remaining = ((1.0 - progress) * VICTORY_HOLD_TIME) as u32;
    let faction_name = if faction == Faction::Blue {
        "Blue"
    } else {
        "Red"
    };
    let msg = format!(
        "{} holds all zones. Victory in {}s",
        faction_name, remaining
    );
    let victory_font = 24.0_f32 * dpi_scale as f32;
    let ribbon_h = 54.0;
    let victory_cy = (bar_y - ribbon_h / 2.0 - 2.0) as i32;

    if let Some(ref tex) = assets.ui_small_ribbons {
        let (tw, _th) = assets.text.measure_text(&msg, victory_font);
        let center_w = tw as f64 + 8.0;
        let color_row = render_util::small_ribbon_row(faction);
        draw_small_ribbon(canvas, tex, color_row, cx, victory_cy as f64, center_w, 1.0);
    }

    assets.text.draw_text_centered(
        canvas,
        tc,
        &msg,
        cx as i32,
        victory_cy,
        victory_font,
        Color::RGBA(255, 255, 255, 220),
    );
}
