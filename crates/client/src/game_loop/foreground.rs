use super::assets::{LoadedTextures, UnitTextureKey};
use crate::renderer::{Canvas2dRenderer, Renderer};
use battlefield_core::animation::TurnAnimator;
use battlefield_core::building::BuildingKind;
use battlefield_core::game::Game;
use battlefield_core::grid::{Decoration, TileKind, TILE_SIZE};
use battlefield_core::render_util;
use battlefield_core::sprite::SpriteSheet;
use battlefield_core::unit::{Facing, Faction, UnitAnim, UnitKind};
use battlefield_core::zone::ZoneState;
use wasm_bindgen::prelude::*;

/// A drawable entity for Y-sorted rendering.
enum Drawable {
    Unit(usize),         // index into game.units
    Tree(u32, u32),      // (gx, gy)
    WaterRock(u32, u32), // (gx, gy)
    Building(u8),        // zone index (tower)
    BaseBuilding(usize), // index into game.buildings
    Particle(usize),     // index into game.particles
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_foreground(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    animator: &TurnAnimator,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    // Player position for tree transparency
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));

    // Collect all drawable entities with their Y-sort key (foot position)
    let mut drawables: Vec<(f64, Drawable)> = Vec::new();

    // Units (hide enemies outside friendly line of sight)
    for (i, u) in game.units.iter().enumerate() {
        let alive_or_fading = if animator.is_playing() {
            animator.is_visually_alive(u.id) || u.death_fade > 0.0
        } else {
            u.alive || u.death_fade > 0.0
        };
        if !alive_or_fading {
            continue;
        }
        // Hide enemy units on non-visible tiles
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }
        drawables.push((u.y as f64 + ts * 0.5, Drawable::Unit(i)));
    }

    // Trees are up to 4 tiles tall (bottom-anchored), so roots below the viewport
    // can have visible canopies. Extend scan range downward.
    let tree_max_gy = (max_gy + 4).min(game.grid.height);
    for gy in min_gy..tree_max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts;
            if tile == TileKind::Forest && !loaded.tree_textures.is_empty() {
                drawables.push((foot_y, Drawable::Tree(gx, gy)));
            }
            if game.grid.decoration(gx, gy) == Some(Decoration::WaterRock)
                && !loaded.water_rock_textures.is_empty()
            {
                drawables.push((foot_y, Drawable::WaterRock(gx, gy)));
            }
        }
    }

    // Tower buildings at zone centers
    for (i, zone) in game.zone_manager.zones.iter().enumerate() {
        let foot_y = (zone.center_gy as f64 + 1.0) * ts;
        drawables.push((foot_y, Drawable::Building(i as u8)));
    }

    // Production buildings at faction bases
    for (i, b) in game.buildings.iter().enumerate() {
        let foot_y = (b.grid_y as f64 + 1.0) * ts;
        drawables.push((foot_y, Drawable::BaseBuilding(i)));
    }

    // Particles
    for (i, _) in game.particles.iter().enumerate() {
        drawables.push((
            game.particles[i].world_y as f64 + ts * 0.5,
            Drawable::Particle(i),
        ));
    }

    // Sort by Y (foot position)
    drawables.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Draw in Y order
    for (_, drawable) in &drawables {
        match drawable {
            Drawable::Unit(idx) => {
                draw_unit(r, game, loaded, animator, *idx, elapsed)?;
            }
            Drawable::Tree(gx, gy) => {
                draw_tree(r, loaded, *gx, *gy, elapsed, ts, player_pos)?;
            }
            Drawable::WaterRock(gx, gy) => {
                draw_water_rock(r, loaded, *gx, *gy, elapsed, ts)?;
            }
            Drawable::Building(zone_idx) => {
                draw_tower(r, game, loaded, *zone_idx, ts)?;
            }
            Drawable::BaseBuilding(idx) => {
                draw_base_building(r, game, loaded, *idx, ts)?;
            }
            Drawable::Particle(idx) => {
                draw_particle(r, game, loaded, *idx)?;
            }
        }
    }

    // Arrow projectiles (drawn last -- they fly above everything)
    if let Some(&arrow_tex_id) = loaded.arrow_texture.as_ref() {
        for proj in &game.projectiles {
            let flip = proj.angle.abs() > std::f32::consts::FRAC_PI_2;
            let draw_angle = if flip {
                (proj.angle as f64) + std::f64::consts::PI
            } else {
                proj.angle as f64
            };

            r.save();
            r.translate(proj.current_x as f64, proj.current_y as f64)?;
            r.rotate(draw_angle)?;
            r.draw_texture(arrow_tex_id, 0.0, 0.0, 64.0, 64.0, -32.0, -32.0, 64.0, 64.0)?;
            r.restore();
        }
    }

    Ok(())
}

fn draw_unit(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    _animator: &TurnAnimator,
    idx: usize,
    elapsed: f64,
) -> Result<(), JsValue> {
    let unit = &game.units[idx];
    let key = UnitTextureKey {
        faction: unit.faction,
        kind: unit.kind,
        anim: unit.current_anim,
    };

    let tex_id = match loaded.unit_textures.get(&key) {
        Some(&id) => id,
        None => {
            let fallback_key = UnitTextureKey {
                faction: unit.faction,
                kind: unit.kind,
                anim: UnitAnim::Idle,
            };
            match loaded.unit_textures.get(&fallback_key) {
                Some(&id) => id,
                None => return Ok(()),
            }
        }
    };

    if let Some(info) = r.texture_info(tex_id) {
        let sheet = SpriteSheet {
            frame_width: info.frame_width,
            frame_height: info.frame_height,
            frame_count: unit.animation.frame_count,
        };
        // Archer idle uses wind wave pattern to sync with trees/bushes
        let anim_frame = if unit.kind == UnitKind::Archer && unit.current_anim == UnitAnim::Idle {
            let (gx, gy) = unit.grid_cell();
            render_util::compute_wave_frame(elapsed, gx, gy, unit.animation.frame_count, 0.15)
        } else {
            unit.animation.current_frame
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(anim_frame);
        let sprite_size = unit.kind.frame_size() as f64;

        let opacity = render_util::unit_opacity(unit.alive, unit.death_fade, unit.hit_flash);

        let dx = (unit.x as f64) - sprite_size / 2.0;
        let dy = (unit.y as f64) - sprite_size / 2.0;

        r.draw_sprite(
            tex_id,
            sx,
            sy,
            sw,
            sh,
            dx,
            dy,
            sprite_size,
            sprite_size,
            unit.facing == Facing::Left,
            opacity,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw_tree(
    r: &Canvas2dRenderer,
    loaded: &LoadedTextures,
    gx: u32,
    gy: u32,
    elapsed: f64,
    ts: f64,
    player_pos: Option<(f64, f64)>,
) -> Result<(), JsValue> {
    let variant_idx = render_util::variant_index(gx, gy, loaded.tree_textures.len(), 31, 17);
    let (tex_id, frame_w, frame_h) = loaded.tree_textures[variant_idx];

    if let Some(info) = r.texture_info(tex_id) {
        let fw = frame_w as f64;
        let fh = frame_h as f64;

        let frame = render_util::compute_wave_frame(elapsed, gx, gy, info.frame_count, 0.15);
        let sx = frame as f64 * fw;

        let draw_w = ts * 3.0;
        let draw_h = draw_w * (fh / fw);
        let dx = (gx as f64) * ts + ts / 2.0 - draw_w / 2.0;
        let dy = (gy as f64) * ts + ts - draw_h;

        // Tree visual center (canopy), not root tile — trees are ~4 tiles tall
        let tree_cx = (gx as f64) * ts + ts / 2.0;
        let tree_cy = (gy as f64) * ts - ts * 1.0;

        // Semi-transparent when near the player to avoid hiding them
        let alpha = render_util::tree_alpha(tree_cx, tree_cy, player_pos, ts);

        if (alpha - 1.0).abs() > 0.001 {
            r.set_alpha(alpha);
        }

        if render_util::tile_flip(gx, gy) {
            if let Some((flipped, _, _)) = loaded.tree_textures_flipped.get(variant_idx) {
                let sheet_w = info.frame_count as f64 * fw;
                let flipped_sx = sheet_w - sx - fw;
                r.draw_canvas_region(flipped, flipped_sx, 0.0, fw, fh, dx, dy, draw_w, draw_h)?;
            }
        } else {
            r.draw_texture(tex_id, sx, 0.0, fw, fh, dx, dy, draw_w, draw_h)?;
        }

        if (alpha - 1.0).abs() > 0.001 {
            r.set_alpha(1.0);
        }
    }
    Ok(())
}

fn draw_water_rock(
    r: &Canvas2dRenderer,
    loaded: &LoadedTextures,
    gx: u32,
    gy: u32,
    elapsed: f64,
    ts: f64,
) -> Result<(), JsValue> {
    let variant_idx = render_util::variant_index(gx, gy, loaded.water_rock_textures.len(), 37, 19);
    let (tex_id, frame_w, frame_h) = loaded.water_rock_textures[variant_idx];

    if let Some(info) = r.texture_info(tex_id) {
        let fw = frame_w as f64;
        let fh = frame_h as f64;

        let frame = render_util::compute_wave_frame(elapsed, gx, gy, info.frame_count, 0.2);
        let sx = frame as f64 * fw;

        let dx = (gx as f64) * ts;
        let dy = (gy as f64) * ts;

        if render_util::tile_flip(gx, gy) {
            if let Some((flipped, _, _)) = loaded.water_rock_textures_flipped.get(variant_idx) {
                let sheet_w = info.frame_count as f64 * fw;
                let flipped_sx = sheet_w - sx - fw;
                r.draw_canvas_region(flipped, flipped_sx, 0.0, fw, fh, dx, dy, ts, ts)?;
            }
        } else {
            r.draw_texture(tex_id, sx, 0.0, fw, fh, dx, dy, ts, ts)?;
        }
    }
    Ok(())
}

fn draw_tower(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    zone_idx: u8,
    ts: f64,
) -> Result<(), JsValue> {
    if loaded.tower_textures.is_empty() {
        return Ok(());
    }
    let zone = &game.zone_manager.zones[zone_idx as usize];

    // Select tower color based on zone state
    let color_idx = match zone.state {
        ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
        ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 2,
        _ => 0, // Black (neutral / contested)
    };

    let tex_id = loaded.tower_textures[color_idx];
    let draw_w = ts * 2.0;
    let draw_h = ts * 4.0;
    let dx = (zone.center_gx as f64) * ts + ts / 2.0 - draw_w / 2.0;
    let dy = (zone.center_gy as f64) * ts + ts - draw_h;

    // Pulse opacity during capturing to show in-progress
    let alpha = match zone.state {
        ZoneState::Capturing(_) => (zone.progress.abs() as f64 * 0.5 + 0.5).clamp(0.5, 1.0),
        _ => 1.0,
    };

    if (alpha - 1.0).abs() > 0.001 {
        r.set_alpha(alpha);
    }

    r.draw_texture(tex_id, 0.0, 0.0, 128.0, 256.0, dx, dy, draw_w, draw_h)?;

    if (alpha - 1.0).abs() > 0.001 {
        r.set_alpha(1.0);
    }

    Ok(())
}

fn draw_base_building(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    idx: usize,
    ts: f64,
) -> Result<(), JsValue> {
    if loaded.building_textures.is_empty() {
        return Ok(());
    }
    let b = &game.buildings[idx];
    let kind_index = match b.kind {
        BuildingKind::Barracks => 0,
        BuildingKind::Archery => 1,
        BuildingKind::Monastery => 2,
        BuildingKind::Castle => 3,
        BuildingKind::DefenseTower => 4,
        BuildingKind::House => 5,
    };
    let faction_index = match b.faction {
        Faction::Blue => 0,
        _ => 1,
    };
    let tex_idx = kind_index * 2 + faction_index;
    if tex_idx < loaded.building_textures.len() {
        let (tex_id, sprite_w, sprite_h) = loaded.building_textures[tex_idx];
        let sw = sprite_w as f64;
        let sh = sprite_h as f64;
        let draw_w = sw;
        let draw_h = sh;
        let dx = (b.grid_x as f64) * ts + ts / 2.0 - draw_w / 2.0;
        let dy = (b.grid_y as f64) * ts + ts - draw_h;
        r.draw_texture(tex_id, 0.0, 0.0, sw, sh, dx, dy, draw_w, draw_h)?;
    }
    Ok(())
}

fn draw_particle(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    idx: usize,
) -> Result<(), JsValue> {
    let particle = &game.particles[idx];
    let filename = particle.kind.asset_filename();
    let tex_id = match loaded.particle_textures.get(filename) {
        Some(&id) => id,
        None => return Ok(()),
    };

    if let Some(info) = r.texture_info(tex_id) {
        let sheet = SpriteSheet {
            frame_width: info.frame_width,
            frame_height: info.frame_height,
            frame_count: particle.animation.frame_count,
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(particle.animation.current_frame);
        let size = particle.kind.frame_size() as f64;
        let dx = (particle.world_x as f64) - size / 2.0;
        let dy = (particle.world_y as f64) - size / 2.0;

        r.draw_sprite(tex_id, sx, sy, sw, sh, dx, dy, size, size, false, 1.0)?;
    }
    Ok(())
}
