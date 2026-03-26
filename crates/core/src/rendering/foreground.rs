//! Shared Y-sorted foreground rendering.
//!
//! Collects all world-space drawables (units, trees, buildings, particles, sheep),
//! sorts by Y foot position, and draws via the [`DrawBackend`] trait.

use crate::asset_manifest;
use crate::game::Game;
use crate::grid::{Decoration, TileKind, TILE_SIZE};
use crate::pawn::PAWN_FRAME_SIZE;
use crate::render_util;
use crate::sheep::SHEEP_FRAME_SIZE;
use crate::sprite::SpriteSheet;
use crate::unit::{Facing, Faction, UnitAnim, UnitKind};
use crate::zone::ZoneState;

use super::{DrawBackend, Drawable, SpriteKey};

/// Draw all Y-sorted foreground entities + projectiles.
///
/// `unit_filter` controls which units are collected (lets callers inject
/// animator-awareness or other visibility logic).
pub fn draw_foreground(
    backend: &mut impl DrawBackend,
    game: &Game,
    viewport: (u32, u32, u32, u32),
    elapsed: f64,
    unit_filter: impl Fn(&crate::unit::Unit) -> bool,
) {
    let ts = TILE_SIZE as f64;
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));
    let (min_gx, min_gy, max_gx, max_gy) = viewport;

    let mut drawables: Vec<(f64, Drawable)> = Vec::new();

    // Units
    for (i, u) in game.units.iter().enumerate() {
        if !unit_filter(u) {
            continue;
        }
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }
        drawables.push((u.y as f64 + ts * 0.5, Drawable::Unit(i)));
    }

    // Trees and water rocks (extend scan by 4 rows for tall tree canopies)
    let tree_max_gy = (max_gy + 4).min(game.grid.height);
    for gy in min_gy..tree_max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts;
            if tile == TileKind::Forest && backend.sprite_info(SpriteKey::Tree(0)).is_some() {
                drawables.push((foot_y, Drawable::Tree(gx, gy)));
            }
            if game.grid.decoration(gx, gy) == Some(Decoration::WaterRock)
                && backend.sprite_info(SpriteKey::WaterRock(0)).is_some()
            {
                drawables.push((foot_y, Drawable::WaterRock(gx, gy)));
            }
        }
    }

    // Buildings
    for (i, b) in game.buildings.iter().enumerate() {
        drawables.push((b.grid_y as f64 * ts, Drawable::BaseBuilding(i)));
    }

    // Sheep
    for (i, s) in game.sheep.iter().enumerate() {
        drawables.push((s.y as f64 + ts * 0.5, Drawable::Sheep(i)));
    }

    // Pawns
    for (i, p) in game.pawns.iter().enumerate() {
        drawables.push((p.y as f64 + ts * 0.5, Drawable::Pawn(i)));
    }

    // Particles
    for (i, p) in game.particles.iter().enumerate() {
        if !p.finished {
            drawables.push((p.world_y as f64 + ts * 0.5, Drawable::Particle(i)));
        }
    }

    // Sort by Y foot position
    drawables.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    // Draw in sorted order
    for (_, drawable) in &drawables {
        match *drawable {
            Drawable::Unit(idx) => draw_unit(backend, game, idx, elapsed),
            Drawable::Tree(gx, gy) => draw_tree(backend, gx, gy, elapsed, player_pos),
            Drawable::WaterRock(gx, gy) => draw_water_rock(backend, gx, gy, elapsed),
            Drawable::BaseBuilding(idx) => draw_building(backend, game, idx, player_pos),
            Drawable::Particle(idx) => draw_particle(backend, game, idx),
            Drawable::Sheep(idx) => draw_sheep(backend, game, idx),
            Drawable::Pawn(idx) => draw_pawn(backend, game, idx),
        }
    }

    // Projectiles fly above everything
    draw_projectiles(backend, game);
}

// ---------------------------------------------------------------------------
// Individual draw helpers
// ---------------------------------------------------------------------------

fn draw_unit(backend: &mut impl DrawBackend, game: &Game, idx: usize, elapsed: f64) {
    let unit = &game.units[idx];
    let key = SpriteKey::Unit {
        faction: unit.faction,
        kind: unit.kind,
        anim: unit.current_anim,
    };

    let info = backend.sprite_info(key).or_else(|| {
        backend.sprite_info(SpriteKey::Unit {
            faction: unit.faction,
            kind: unit.kind,
            anim: UnitAnim::Idle,
        })
    });
    let Some(info) = info else { return };

    let frame_count = unit.animation.frame_count;
    let anim_frame = if unit.kind == UnitKind::Archer && unit.current_anim == UnitAnim::Idle {
        let (gx, gy) = unit.grid_cell();
        render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15)
    } else {
        unit.animation.current_frame
    };

    let sprite_size = info.frame_w as f64;
    let alpha = render_util::unit_opacity(unit.alive, unit.death_fade, unit.hit_flash);
    let flip = unit.facing == Facing::Left;
    let dx = unit.x as f64 - sprite_size / 2.0;
    let dy = unit.y as f64 - sprite_size / 2.0;

    backend.draw_sprite(
        key,
        anim_frame,
        dx,
        dy,
        sprite_size,
        sprite_size,
        flip,
        alpha,
    );
}

fn draw_tree(
    backend: &mut impl DrawBackend,
    gx: u32,
    gy: u32,
    elapsed: f64,
    player_pos: Option<(f64, f64)>,
) {
    let tree_count = asset_manifest::TREE_SPECS.len();
    let variant_idx = render_util::variant_index(gx, gy, tree_count, 31, 17);
    let key = SpriteKey::Tree(variant_idx);
    let Some(info) = backend.sprite_info(key) else {
        return;
    };

    let ts = TILE_SIZE as f64;
    let frame = render_util::compute_wave_frame(elapsed, gx, gy, info.frame_count, 0.15);

    let draw_w = ts * 3.0;
    let draw_h = draw_w * (info.frame_h as f64 / info.frame_w as f64);
    let anchor_x = gx as f64 * ts + ts * 0.5;
    let anchor_y = (gy + 1) as f64 * ts;

    let tree_cx = anchor_x;
    let tree_cy = gy as f64 * ts - ts;
    let alpha = render_util::tree_alpha(tree_cx, tree_cy, player_pos, ts);
    let flip = render_util::tile_flip(gx, gy);

    backend.draw_sprite(
        key,
        frame,
        anchor_x - draw_w / 2.0,
        anchor_y - draw_h,
        draw_w,
        draw_h,
        flip,
        alpha,
    );
}

fn draw_water_rock(backend: &mut impl DrawBackend, gx: u32, gy: u32, elapsed: f64) {
    let count = asset_manifest::WATER_ROCK_VARIANTS;
    let variant_idx = render_util::variant_index(gx, gy, count, 37, 19);
    let key = SpriteKey::WaterRock(variant_idx);
    let Some(info) = backend.sprite_info(key) else {
        return;
    };

    let ts = TILE_SIZE as f64;
    let frame = render_util::compute_wave_frame(elapsed, gx, gy, info.frame_count, 0.2);
    let flip = render_util::tile_flip(gx, gy);

    backend.draw_sprite(
        key,
        frame,
        gx as f64 * ts,
        gy as f64 * ts,
        ts,
        ts,
        flip,
        1.0,
    );
}

fn draw_building(
    backend: &mut impl DrawBackend,
    game: &Game,
    idx: usize,
    player_pos: Option<(f64, f64)>,
) {
    let b = &game.buildings[idx];
    let ts = TILE_SIZE as f64;
    let (sprite_w, sprite_h) = b.kind.sprite_size();
    let sw = sprite_w as f64;
    let sh = sprite_h as f64;
    let anchor_x = b.grid_x as f64 * ts + ts / 2.0;
    let anchor_y = b.grid_y as f64 * ts + ts;

    let proximity_alpha = render_util::building_alpha(anchor_x, anchor_y, sw, sh, player_pos, ts);

    // Zone-linked towers
    if let Some(zid) = b.zone_id {
        let zone = &game.zone_manager.zones[zid as usize];
        let color_idx = match zone.state {
            ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
            ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 2,
            _ => 0,
        };
        let zone_alpha = match zone.state {
            ZoneState::Capturing(_) => (zone.progress.abs() as f64 * 0.5 + 0.5).clamp(0.5, 1.0),
            _ => 1.0,
        };
        let alpha = proximity_alpha * zone_alpha;
        backend.draw_sprite(
            SpriteKey::Tower(color_idx),
            0,
            anchor_x - sw / 2.0,
            anchor_y - sh,
            sw,
            sh,
            false,
            alpha,
        );
        return;
    }

    // Base buildings
    let tex_idx = asset_manifest::building_tex_index(b.kind, b.house_variant, b.faction);
    backend.draw_sprite(
        SpriteKey::Building(tex_idx),
        0,
        anchor_x - sw / 2.0,
        anchor_y - sh,
        sw,
        sh,
        false,
        proximity_alpha,
    );
}

fn draw_particle(backend: &mut impl DrawBackend, game: &Game, idx: usize) {
    let p = &game.particles[idx];
    if p.finished {
        return;
    }
    let sprite_idx = asset_manifest::particle_sprite_index(p.kind);
    let key = SpriteKey::Particle(sprite_idx);
    let Some(info) = backend.sprite_info(key) else {
        return;
    };

    let sheet = SpriteSheet {
        frame_width: info.frame_w,
        frame_height: info.frame_h,
        frame_count: p.animation.frame_count,
    };
    let _ = sheet.frame_src_rect(p.animation.current_frame);
    let size = info.frame_w as f64;
    let dx = p.world_x as f64 - size / 2.0;
    let dy = p.world_y as f64 - size / 2.0;

    let alpha = if p.kind == crate::particle::ParticleKind::HealEffect {
        0.6
    } else {
        1.0
    };
    backend.draw_sprite(
        key,
        p.animation.current_frame,
        dx,
        dy,
        size,
        size,
        false,
        alpha,
    );
}

fn draw_sheep(backend: &mut impl DrawBackend, game: &Game, idx: usize) {
    let sheep = &game.sheep[idx];
    let sprite_idx = sheep.sprite_index();
    let key = SpriteKey::Sheep(sprite_idx);
    if backend.sprite_info(key).is_none() {
        return;
    }

    let size = SHEEP_FRAME_SIZE as f64;
    let flip = sheep.facing == Facing::Left;
    let dx = sheep.x as f64 - size / 2.0;
    let dy = sheep.y as f64 - size / 2.0;

    backend.draw_sprite(
        key,
        sheep.animation.current_frame,
        dx,
        dy,
        size,
        size,
        flip,
        1.0,
    );
}

fn draw_pawn(backend: &mut impl DrawBackend, game: &Game, idx: usize) {
    let pawn = &game.pawns[idx];
    let faction_offset = match pawn.faction {
        Faction::Blue => 0,
        Faction::Red => asset_manifest::PAWN_SPECS.len(),
    };
    let key = SpriteKey::Pawn(faction_offset + pawn.sprite_index());
    if backend.sprite_info(key).is_none() {
        return;
    }

    let size = PAWN_FRAME_SIZE as f64;
    let flip = pawn.facing == Facing::Left;
    let dx = pawn.x as f64 - size / 2.0;
    let dy = pawn.y as f64 - size / 2.0;

    backend.draw_sprite(
        key,
        pawn.animation.current_frame,
        dx,
        dy,
        size,
        size,
        flip,
        1.0,
    );
}

fn draw_projectiles(backend: &mut impl DrawBackend, game: &Game) {
    for proj in &game.projectiles {
        backend.draw_rotated(
            SpriteKey::Arrow,
            proj.current_x as f64,
            proj.current_y as f64,
            64.0,
            proj.angle as f64,
        );
    }
}
