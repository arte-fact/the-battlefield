//! Shared asset specifications — filenames, frame sizes, and frame counts.
//!
//! Both WASM and SDL clients reference these instead of hardcoding sprite metadata.
//! This is the single source of truth for all asset paths and dimensions.

use crate::building::BuildingKind;
use crate::particle::ParticleKind;
use crate::unit::{Faction, UnitAnim, UnitKind};

pub const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// Sprite specification for a single animation or texture.
pub struct SpriteSpec {
    pub filename: &'static str,
    pub frame_w: u32,
    pub frame_h: u32,
    pub frame_count: u32,
}

// ---------------------------------------------------------------------------
// Units
// ---------------------------------------------------------------------------

/// Asset folder name for a unit kind (used in path construction).
pub fn unit_kind_folder(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Warrior => "Warrior",
        UnitKind::Archer => "Archer",
        UnitKind::Lancer => "Lancer",
        UnitKind::Monk => "Monk",
    }
}

/// Sprite spec for a (kind, anim) pair. Returns `None` for unsupported combos.
pub fn unit_sprite(kind: UnitKind, anim: UnitAnim) -> Option<SpriteSpec> {
    let fs = kind.frame_size();
    let (filename, frame_count) = match (kind, anim) {
        (UnitKind::Warrior, UnitAnim::Idle) => ("Warrior_Idle.png", 8u32),
        (UnitKind::Warrior, UnitAnim::Run) => ("Warrior_Run.png", 6),
        (UnitKind::Warrior, UnitAnim::Attack) => ("Warrior_Attack1.png", 4),
        (UnitKind::Warrior, UnitAnim::Attack2) => ("Warrior_Attack2.png", 4),
        (UnitKind::Archer, UnitAnim::Idle) => ("Archer_Idle.png", 6),
        (UnitKind::Archer, UnitAnim::Run) => ("Archer_Run.png", 4),
        (UnitKind::Archer, UnitAnim::Attack) => ("Archer_Shoot.png", 8),
        (UnitKind::Lancer, UnitAnim::Idle) => ("Lancer_Idle.png", 12),
        (UnitKind::Lancer, UnitAnim::Run) => ("Lancer_Run.png", 6),
        (UnitKind::Lancer, UnitAnim::Attack) => ("Lancer_Right_Attack.png", 3),
        (UnitKind::Monk, UnitAnim::Idle) => ("Idle.png", 6),
        (UnitKind::Monk, UnitAnim::Run) => ("Run.png", 4),
        (UnitKind::Monk, UnitAnim::Attack) => ("Heal.png", 11),
        _ => return None,
    };
    Some(SpriteSpec {
        filename,
        frame_w: fs,
        frame_h: fs,
        frame_count,
    })
}

// ---------------------------------------------------------------------------
// Buildings
// ---------------------------------------------------------------------------

/// Building sprite specs: (width, height, filename).
/// Index 0–4 = unique kinds, 5–7 = House variants.
/// Textures are loaded as `index * 2 + faction_index` (Blue=0, Red=1).
pub const BUILDING_SPECS: &[(u32, u32, &str)] = &[
    (192, 256, "Barracks.png"),  // 0
    (192, 256, "Archery.png"),   // 1
    (192, 320, "Monastery.png"), // 2
    (320, 256, "Castle.png"),    // 3
    (128, 256, "Tower.png"),     // 4
    (128, 192, "House1.png"),    // 5
    (128, 192, "House2.png"),    // 6
    (128, 192, "House3.png"),    // 7
];

/// Compute the texture array index for a building.
pub fn building_tex_index(kind: BuildingKind, house_variant: u8, faction: Faction) -> usize {
    let kind_index = match kind {
        BuildingKind::Barracks => 0,
        BuildingKind::Archery => 1,
        BuildingKind::Monastery => 2,
        BuildingKind::Castle => 3,
        BuildingKind::DefenseTower => 4,
        BuildingKind::House => 5 + house_variant as usize,
    };
    let faction_index = match faction {
        Faction::Blue => 0,
        Faction::Red => 1,
    };
    kind_index * 2 + faction_index
}

pub const BUILDING_FACTION_FOLDERS: &[&str] = &["Blue Buildings", "Red Buildings"];

/// Tower color folders: 0=neutral, 1=Blue, 2=Red.
pub const TOWER_COLOR_FOLDERS: &[&str] = &["Black Buildings", "Blue Buildings", "Red Buildings"];

// ---------------------------------------------------------------------------
// Trees
// ---------------------------------------------------------------------------

/// Tree specs: (frame_w, frame_h, frame_count, filename).
pub const TREE_SPECS: &[(u32, u32, u32, &str)] = &[
    (192, 256, 8, "Tree1.png"),
    (192, 256, 8, "Tree2.png"),
    (192, 192, 8, "Tree3.png"),
    (192, 192, 8, "Tree4.png"),
];

// ---------------------------------------------------------------------------
// Decorations
// ---------------------------------------------------------------------------

pub const ROCK_VARIANTS: usize = 4;
pub const BUSH_VARIANTS: usize = 4;
pub const BUSH_FRAME_SIZE: u32 = 128;
pub const BUSH_FRAME_COUNT: u32 = 8;
pub const WATER_ROCK_VARIANTS: usize = 4;
pub const WATER_ROCK_FRAME_SIZE: u32 = 64;
pub const WATER_ROCK_FRAME_COUNT: u32 = 16;

// ---------------------------------------------------------------------------
// Particles
// ---------------------------------------------------------------------------

/// Particle specs loaded at startup: (frame_size, frame_count, filename).
/// Particle specs from Particle FX folder: (frame_size, frame_count, filename).
pub const PARTICLE_SPECS: &[(u32, u32, &str)] = &[
    (64, 8, "Dust_01.png"),       // 0
    (192, 8, "Explosion_01.png"), // 1
    (192, 10, "Explosion_02.png"), // 2
];

/// Heal effect particle (from Units/Blue Units/Monk/).
pub const HEAL_EFFECT_SPEC: (u32, u32, &str) = (192, 11, "Heal_Effect.png");
/// Index of heal effect in the particle texture array (after PARTICLE_SPECS).
pub const HEAL_EFFECT_INDEX: usize = 3;

/// Map a ParticleKind to its index in PARTICLE_SPECS / the particle texture array.
pub fn particle_sprite_index(kind: ParticleKind) -> usize {
    match kind {
        ParticleKind::Dust => 0,
        ParticleKind::ExplosionLarge => 2,
        ParticleKind::HealEffect => HEAL_EFFECT_INDEX,
    }
}

// ---------------------------------------------------------------------------
// Sheep
// ---------------------------------------------------------------------------

/// Sheep animation specs: (filename, frame_count).
pub const SHEEP_SPECS: &[(&str, u32)] = &[
    ("Sheep_Idle.png", 6),
    ("Sheep_Move.png", 4),
    ("Sheep_Grass.png", 12),
];

// ---------------------------------------------------------------------------
// Pawns
// ---------------------------------------------------------------------------

/// Pawn animation specs: (filename, frame_count). Frame size = 192×192.
/// Index: sprite_index * 2 + faction_index (Blue=0, Red=1).
pub const PAWN_SPECS: &[(&str, u32)] = &[
    ("Pawn_Idle.png", 8),         // 0: idle empty-handed
    ("Pawn_Run.png", 6),          // 1: walking empty
    ("Pawn_Interact Axe.png", 6), // 2: chopping
    ("Pawn_Idle Wood.png", 8),    // 3: idle carrying wood
    ("Pawn_Run Wood.png", 6),     // 4: walking with wood
];

// ---------------------------------------------------------------------------
// Unit avatars (portraits for HUD)
// ---------------------------------------------------------------------------

/// Avatar filenames for each unit kind. 256×256 each.
/// Index: 0=Warrior, 1=Lancer, 2=Archer, 3=Monk.
pub const AVATAR_FILES: &[&str] = &[
    "Avatars_01.png", // Warrior (knight helmet with plume)
    "Avatars_02.png", // Lancer (round helmet)
    "Avatars_03.png", // Archer (pointy hat)
    "Avatars_04.png", // Monk (curly hair)
];

/// Map UnitKind to avatar index.
pub fn avatar_index(kind: UnitKind) -> usize {
    match kind {
        UnitKind::Warrior => 0,
        UnitKind::Lancer => 1,
        UnitKind::Archer => 2,
        UnitKind::Monk => 3,
    }
}

// ---------------------------------------------------------------------------
// Variant index helpers (spatial hashing)
// ---------------------------------------------------------------------------

/// Magic numbers for deterministic variant selection per entity type.
pub const TREE_VARIANT_PRIMES: (u32, u32) = (31, 17);
pub const WATER_ROCK_VARIANT_PRIMES: (u32, u32) = (37, 19);
pub const BUSH_VARIANT_PRIMES: (u32, u32) = (41, 23);
pub const ROCK_VARIANT_PRIMES: (u32, u32) = (13, 29);
