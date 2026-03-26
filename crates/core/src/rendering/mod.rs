//! Shared rendering logic — trait, types, and draw functions.
//!
//! Platform clients (WASM, SDL) implement [`DrawBackend`] to bridge their
//! graphics API, then call the shared draw functions in this module.

pub mod foreground;

use crate::unit::{Faction, UnitAnim, UnitKind};

// ---------------------------------------------------------------------------
// Sprite addressing
// ---------------------------------------------------------------------------

/// Identifies a loaded sprite / texture for backend lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpriteKey {
    /// Unit animation sheet.
    Unit {
        faction: Faction,
        kind: UnitKind,
        anim: UnitAnim,
    },
    /// Base building texture (index from `asset_manifest::building_tex_index`).
    Building(usize),
    /// Tower color variant (0=neutral, 1=Blue, 2=Red).
    Tower(usize),
    /// Tree variant index.
    Tree(usize),
    /// Rock variant index.
    Rock(usize),
    /// Bush variant index.
    Bush(usize),
    /// Water rock variant index.
    WaterRock(usize),
    /// Particle effect (index from `asset_manifest::particle_sprite_index`).
    Particle(usize),
    /// Arrow projectile.
    Arrow,
    /// Sheep animation (0=Idle, 1=Move, 2=Grass).
    Sheep(usize),
    /// Pawn worker: faction_index * PAWN_SPECS.len() + sprite_index.
    Pawn(usize),
    /// Unit avatar portrait (index from `asset_manifest::avatar_index`).
    Avatar(usize),
}

/// Metadata returned by `DrawBackend::sprite_info`.
#[derive(Clone, Copy, Debug)]
pub struct SpriteInfo {
    pub frame_w: u32,
    pub frame_h: u32,
    pub frame_count: u32,
}

// ---------------------------------------------------------------------------
// Draw backend trait
// ---------------------------------------------------------------------------

/// Abstraction over platform-specific rendering.
///
/// All coordinates are in **world pixels**. The backend is responsible for
/// applying camera transform (zoom + offset) if needed.
#[allow(clippy::too_many_arguments)]
pub trait DrawBackend {
    /// Draw one frame of a sprite sheet.
    ///
    /// `(x, y)` = top-left in world pixels, `(w, h)` = draw size in world pixels.
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
    );

    /// Draw a sprite rotated around its center (used for arrow projectiles).
    fn draw_rotated(
        &mut self,
        key: SpriteKey,
        center_x: f64,
        center_y: f64,
        size: f64,
        angle: f64,
    );

    /// Query sprite metadata. Returns `None` if the texture isn't loaded.
    fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo>;
}

// ---------------------------------------------------------------------------
// Drawable enum (Y-sort entity types)
// ---------------------------------------------------------------------------

/// A drawable entity tag for Y-sorted rendering.
#[derive(Clone, Copy)]
pub enum Drawable {
    Unit(usize),
    Tree(u32, u32),
    WaterRock(u32, u32),
    BaseBuilding(usize),
    Particle(usize),
    Sheep(usize),
    Pawn(usize),
}
