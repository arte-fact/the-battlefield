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
    /// Tower color variant (0=neutral, 1=Blue, 2=Red, 3=Yellow, 4=Purple).
    Tower(usize),
    /// Neutral (Black) building texture (index from
    /// `asset_manifest::neutral_building_tex_index`).
    NeutralBuilding(usize),
    /// Tree variant index.
    Tree(usize),
    /// Rock variant index.
    Rock(usize),
    /// Bush variant index.
    Bush(usize),
    /// Water rock variant index.
    WaterRock(usize),
    /// Gold stone variant index.
    GoldStone(usize),
    /// Particle effect (index from `asset_manifest::particle_sprite_index`).
    Particle(usize),
    /// Arrow projectile.
    Arrow,
    /// Sheep animation (0=Idle, 1=Move, 2=Grass).
    Sheep(usize),
    /// Pawn worker: color_index * PAWN_SPECS.len() + sprite_index
    /// (colors from `asset_manifest::PAWN_COLOR_FOLDERS`).
    Pawn(usize),
    /// Unit avatar portrait (index from `asset_manifest::avatar_index`).
    Avatar(usize),
    /// Resource icon (index from `ResourceKind::idx()`: meat, gold, wood).
    ResourceIcon(usize),
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
    fn draw_rotated(&mut self, key: SpriteKey, center_x: f64, center_y: f64, size: f64, angle: f64);

    /// Query sprite metadata. Returns `None` if the texture isn't loaded.
    fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo>;

    /// Draw an elevated terrain tile (surface + cliff face). Platform-specific.
    fn draw_elevated_tile(&mut self, _game: &crate::game::Game, _gx: u32, _gy: u32) {}
}

// ---------------------------------------------------------------------------
// Drawable enum (Y-sort entity types)
// ---------------------------------------------------------------------------

/// A drawable entity tag for Y-sorted rendering.
/// Sprite color for a pawn: village pawns follow their zone's owner
/// (2 = neutral Black), base pawns their faction.
pub fn pawn_color_index(pawn: &crate::pawn::Pawn, zones: &crate::zone::ZoneManager) -> usize {
    use crate::unit::Faction;
    use crate::zone::ZoneState;
    let faction = match pawn.zone_id {
        None => Some(pawn.faction),
        Some(zid) => match zones.zones.get(zid as usize).map(|z| z.state) {
            Some(ZoneState::Controlled(f)) | Some(ZoneState::Capturing(f)) => Some(f),
            _ => None,
        },
    };
    match faction {
        Some(Faction::Blue) => 0,
        Some(Faction::Red) => 1,
        Some(Faction::Yellow) => 3,
        Some(Faction::Purple) => 4,
        Some(Faction::Villager) | None => 2,
    }
}

#[derive(Clone, Copy)]
pub enum Drawable {
    Unit(usize),
    Tree(u32, u32),
    WaterRock(u32, u32),
    GoldStone(u32, u32),
    BaseBuilding(usize),
    Particle(usize),
    Sheep(usize),
    Pawn(usize),
    ElevatedTile(u32, u32),
}
