use crate::grid::{BORDER_SIZE, TILE_SIZE};
use crate::unit::{Faction, UnitKind};
use std::collections::HashSet;

/// The type of building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Barracks,     // Produces Warriors + Lancers
    Archery,      // Produces Archers
    Monastery,    // Produces Monks
    Castle,       // Defensive — fires like 4 archers
    DefenseTower, // Defensive — fires like 2 archers
    House,        // Decorative only
}

impl BuildingKind {
    /// Sprite dimensions (width, height) in pixels.
    pub fn sprite_size(self) -> (u32, u32) {
        match self {
            BuildingKind::Barracks | BuildingKind::Archery => (192, 256),
            BuildingKind::Monastery => (192, 320),
            BuildingKind::Castle => (320, 256),
            BuildingKind::DefenseTower => (128, 256),
            BuildingKind::House => (128, 192),
        }
    }

    /// Grid footprint offsets (dx, dy) relative to the building's anchor tile.
    /// Sprite is drawn bottom-center at (grid_x + 0.5, grid_y + 1) in tile coords.
    /// Footprint covers the visible building walls, not transparent padding or roof.
    pub fn footprint_offsets(self) -> &'static [(i32, i32)] {
        match self {
            BuildingKind::Barracks | BuildingKind::Archery => {
                // 192x256 sprite = 3×4 tiles; base walls in middle 2 rows
                &[(-1, -1), (0, -1), (1, -1)]
            }
            BuildingKind::Monastery => {
                // 192x320 sprite = 3×5 tiles; walls span 3 rows
                &[(-1, -1), (0, -1), (1, -1)]
            }
            // Castle: 320x256 sprite; ground-level walls = 5×2
            BuildingKind::Castle => &[(-2, -1), (-1, -1), (0, -1), (1, -1), (2, -1)],
            // DefenseTower: 128x256 sprite; circular base = 2×2
            BuildingKind::DefenseTower => &[(0, -1)],
            // House: 128x192 sprite; walls = 2×2
            BuildingKind::House => &[(0, -1)],
        }
    }

    /// Asset filename for this building kind.
    pub fn asset_filename(self) -> &'static str {
        match self {
            BuildingKind::Barracks => "Barracks.png",
            BuildingKind::Archery => "Archery.png",
            BuildingKind::Monastery => "Monastery.png",
            BuildingKind::Castle => "Castle.png",
            BuildingKind::DefenseTower => "Tower.png",
            BuildingKind::House => "House1.png",
        }
    }

    /// Attack range in world units. Returns 0 for non-combat buildings.
    pub fn attack_range(self) -> f32 {
        match self {
            BuildingKind::Castle | BuildingKind::DefenseTower => 7.0 * TILE_SIZE,
            _ => 0.0,
        }
    }

    /// Attack damage per hit (same as archer: 2).
    pub fn attack_damage(self) -> i32 {
        match self {
            BuildingKind::Castle | BuildingKind::DefenseTower => 2,
            _ => 0,
        }
    }

    /// Base cooldown between attacks in seconds.
    /// Tower = 2 archers (half archer cooldown), Castle = 4 archers (quarter).
    pub fn base_cooldown(self) -> f32 {
        const ARCHER_COOLDOWN: f32 = 0.55;
        match self {
            BuildingKind::DefenseTower => ARCHER_COOLDOWN / 2.0,
            BuildingKind::Castle => ARCHER_COOLDOWN / 4.0,
            _ => 0.0,
        }
    }

    /// Whether this building type has combat capability.
    pub fn is_combat(self) -> bool {
        matches!(self, BuildingKind::Castle | BuildingKind::DefenseTower)
    }
}

/// A building placed on the battlefield (at a faction base, or at a capture zone).
#[derive(Clone, Debug)]
pub struct BaseBuilding {
    pub kind: BuildingKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    /// Current attack cooldown timer (only used by combat buildings).
    pub attack_cooldown: f32,
    /// If set, this building is linked to a capture zone and changes faction dynamically.
    pub zone_id: Option<u8>,
    /// Which unit kind this production building trains (None for non-production buildings).
    pub produces: Option<UnitKind>,
    /// House sprite variant: 0=House1, 1=House2, 2=House3 (ignored for non-House kinds).
    pub house_variant: u8,
    /// Seconds until this production building may train again.
    pub train_cooldown: f32,
}

/// Which building produces a given unit type.
pub fn building_for_unit(kind: UnitKind) -> BuildingKind {
    match kind {
        UnitKind::Warrior | UnitKind::Lancer => BuildingKind::Barracks,
        UnitKind::Archer => BuildingKind::Archery,
        UnitKind::Monk => BuildingKind::Monastery,
    }
}

/// Simple xorshift32 PRNG for deterministic base generation.
struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }
    fn next(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = (self.next() as usize) % (i + 1);
            v.swap(i, j);
        }
    }
}

/// Tracks occupied exclusion cells during base placement.
struct BasePlacer {
    faction: Faction,
    cx: i32,
    cy: i32,
    /// Playable-area bounds in tiles (min inclusive, max exclusive) —
    /// from the live grid, never the compile-time default size.
    bounds: (i32, i32),
    occupied: HashSet<(i32, i32)>,
    out: Vec<BaseBuilding>,
}

impl BasePlacer {
    fn new(faction: Faction, cx: i32, cy: i32, playable_size: u32) -> Self {
        let b = BORDER_SIZE as i32;
        Self {
            faction,
            cx,
            cy,
            bounds: (b, b + playable_size as i32),
            occupied: HashSet::new(),
            out: Vec::new(),
        }
    }

    /// Exclusion footprint + buffer for a building at (bx, by).
    /// hw = half-width, vlo/vhi = vertical range (anchor-relative).
    /// Towers and houses use a narrow exclusion (hw=1) so the 4-tower defensive
    /// line fits across the ~20-tile base width without self-blocking.
    fn exclusion(kind: BuildingKind, bx: i32, by: i32) -> Vec<(i32, i32)> {
        // hw = horizontal half-width, vlo/vhi = vertical range relative to anchor.
        // Castle gets a wide exclusion (5-tile footprint + 1 buffer).
        // Production buildings use a narrow exclusion (footprint row + 1 buffer row)
        // so that 5 buildings fit across the ~20-tile base.
        // Towers/Houses also narrow so the defensive line packs naturally.
        // Two clear tiles between structures: units are 56px wide in
        // 64px tiles, and single-tile corridors wedge circle movement —
        // the same rule village layouts follow.
        let (hw, vlo, vhi): (i32, i32, i32) = match kind {
            BuildingKind::Castle => (4, -3, 2),
            BuildingKind::Barracks | BuildingKind::Archery | BuildingKind::Monastery => (3, -2, 2),
            BuildingKind::DefenseTower => (2, -2, 2),
            BuildingKind::House => (2, -2, 1),
        };
        let mut cells = Vec::new();
        for dy in vlo..=vhi {
            for dx in -hw..=hw {
                cells.push((bx + dx, by + dy));
            }
        }
        cells
    }

    /// Try to place `kind` at grid position (bx, by).
    /// Returns false (no-op) if out of bounds or exclusion zone conflicts.
    fn try_place(&mut self, kind: BuildingKind, bx: i32, by: i32) -> bool {
        // Must stay within the cleared base ground (facing-agnostic circle).
        let (dx, dy) = (bx - self.cx, by - self.cy);
        if dx * dx + dy * dy > BASE_BAND_RADIUS * BASE_BAND_RADIUS {
            return false;
        }
        // Must stay within the playable area
        let (lo, hi) = self.bounds;
        if bx < lo || bx >= hi || by < lo || by >= hi {
            return false;
        }
        // Collision check
        for &cell in &Self::exclusion(kind, bx, by) {
            if self.occupied.contains(&cell) {
                return false;
            }
        }
        // Commit
        for cell in Self::exclusion(kind, bx, by) {
            self.occupied.insert(cell);
        }
        self.out.push(BaseBuilding {
            kind,
            faction: self.faction,
            grid_x: bx as u32,
            grid_y: by as u32,
            attack_cooldown: 0.0,
            zone_id: None,
            produces: None,
            house_variant: 0,
            train_cooldown: 0.0,
        });
        true
    }

    fn finish(self) -> Vec<BaseBuilding> {
        self.out
    }
}

/// Buildings sit within this radius of the base center; the map
/// generator clears one tile more.
pub const BASE_BAND_RADIUS: i32 = 17;

/// Procedurally generate all buildings for a faction base.
///
/// `facing` is the unit vector toward the enemy base. Functional bands,
/// rotated to the facing:
///   Front — Castle guarding the gather zone, 3-5 DefenseTowers on the
///   front arc and flanks
///   Flanks — production buildings, one per wave unit kind
///   Rear — 8-12 Houses, pasture behind (sheep spawned by setup)
///
/// `seed` drives counts and arc positions; layouts vary per battle.
pub fn generate_base_buildings(
    faction: Faction,
    cx: u32,
    cy: u32,
    seed: u32,
    facing: (f32, f32),
    playable_size: u32,
) -> Vec<BaseBuilding> {
    let icx = cx as i32;
    let icy = cy as i32;
    let mut rng = Rng::new(seed);
    let mut p = BasePlacer::new(faction, icx, icy, playable_size);
    let (fx, fy) = facing;
    let (px, py) = (-fy, fx);
    let pos = |forward: f32, lateral: f32| -> (i32, i32) {
        (
            (icx as f32 + fx * forward + px * lateral).round() as i32,
            (icy as f32 + fy * forward + py * lateral).round() as i32,
        )
    };

    // Castle at the front of the gather zone, facing the enemy.
    let (x, y) = pos(5.0, 0.0);
    p.try_place(BuildingKind::Castle, x, y);

    // Production band flanking the gather zone, one building per wave kind.
    let prod: &[(BuildingKind, UnitKind, f32, f32)] = &[
        (BuildingKind::Barracks, UnitKind::Warrior, 3.0, -8.0),
        (BuildingKind::Barracks, UnitKind::Lancer, 3.0, 8.0),
        (BuildingKind::Archery, UnitKind::Archer, -3.0, -8.0),
        (BuildingKind::Archery, UnitKind::Archer, -3.0, 8.0),
        (BuildingKind::Monastery, UnitKind::Monk, -7.0, 0.0),
    ];
    // Wave production depends on every kind having a building: try the
    // jittered spot first, then widen the search until one fits.
    let mut deltas: Vec<(f32, f32)> = Vec::new();
    for df in -2i32..=2 {
        for dl in -4i32..=4 {
            deltas.push((df as f32, dl as f32));
        }
    }
    deltas.sort_by_key(|&(df, dl)| (df.abs() + dl.abs()) as i32);
    for &(kind, unit, fw, lat) in prod {
        let jf = (rng.next() % 3) as f32 - 1.0;
        let placed = deltas.iter().any(|&(df, dl)| {
            let (x, y) = pos(fw + jf + df, lat + dl);
            p.try_place(kind, x, y)
        });
        if placed {
            if let Some(b) = p.out.last_mut() {
                b.produces = Some(unit);
            }
        }
    }

    // Defense towers: front pair first so coverage survives low rolls.
    let tower_count = 3 + (rng.next() % 3) as usize;
    let slots: [(f32, f32); 5] = [
        (9.0, -9.0),
        (9.0, 9.0),
        (0.0, -12.0),
        (0.0, 12.0),
        (11.0, 0.0),
    ];
    let mut towers = 0usize;
    for &(fw, lat) in slots.iter() {
        if towers >= tower_count {
            break;
        }
        let (x, y) = pos(fw, lat);
        let placed = p.try_place(BuildingKind::DefenseTower, x, y)
            || [1.0f32, -1.0, 2.0, -2.0].iter().any(|&d| {
                let (x, y) = pos(fw, lat + d);
                p.try_place(BuildingKind::DefenseTower, x, y)
            });
        if placed {
            towers += 1;
        }
    }

    // Living band: houses row by row from the back, shuffled per row.
    let house_target = 8 + (rng.next() % 5) as u8;
    let mut cands: Vec<(f32, f32)> = Vec::new();
    for rear in (5..=15).rev() {
        let mut row: Vec<i32> = (-11..=11).collect();
        rng.shuffle(&mut row);
        for lat in row {
            cands.push((-(rear as f32), lat as f32));
        }
    }
    let mut houses = 0u8;
    for (fw, lat) in cands {
        if houses >= house_target {
            break;
        }
        let (x, y) = pos(fw, lat);
        if p.try_place(BuildingKind::House, x, y) {
            if let Some(b) = p.out.last_mut() {
                b.house_variant = houses % 3;
            }
            houses += 1;
        }
    }

    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::PLAYABLE_SIZE;

    #[test]
    fn building_for_unit_mapping() {
        assert_eq!(building_for_unit(UnitKind::Warrior), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Lancer), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Archer), BuildingKind::Archery);
        assert_eq!(building_for_unit(UnitKind::Monk), BuildingKind::Monastery);
    }

    const F_DOWN: (f32, f32) = (0.0, 1.0);
    const F_UP: (f32, f32) = (0.0, -1.0);

    #[test]
    fn generate_base_buildings_in_playable_area() {
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let cx_blue = b + 15;
        let cy_blue = b + 15;
        let cx_red = b + p - 16;
        let cy_red = b + p - 16;
        for building in generate_base_buildings(Faction::Blue, cx_blue, cy_blue, 42, F_DOWN, PLAYABLE_SIZE)
            .iter()
            .chain(generate_base_buildings(Faction::Red, cx_red, cy_red, 42, F_UP, PLAYABLE_SIZE).iter())
        {
            assert!(
                building.grid_x >= b && building.grid_x < b + p,
                "grid_x {} out of playable area [{b}, {end})",
                building.grid_x,
                end = b + p,
            );
            assert!(
                building.grid_y >= b && building.grid_y < b + p,
                "grid_y {} out of playable area [{b}, {end})",
                building.grid_y,
                end = b + p,
            );
        }
    }

    #[test]
    fn generate_base_buildings_has_required_types() {
        for seed in [1, 42, 777, 31337] {
            let b = BORDER_SIZE;
            let buildings = generate_base_buildings(Faction::Blue, b + 20, b + 20, seed, F_DOWN, PLAYABLE_SIZE);
            let count = |k: BuildingKind| buildings.iter().filter(|b| b.kind == k).count();
            assert_eq!(count(BuildingKind::Castle), 1, "must have 1 castle");
            let towers = count(BuildingKind::DefenseTower);
            assert!((3..=5).contains(&towers), "3-5 towers, got {towers}");
            assert_eq!(count(BuildingKind::Barracks), 2, "must have 2 barracks");
            assert_eq!(count(BuildingKind::Archery), 2, "must have 2 archery");
            assert_eq!(count(BuildingKind::Monastery), 1, "must have 1 monastery");
            let houses = count(BuildingKind::House);
            assert!((7..=12).contains(&houses), "7-12 houses, got {houses}");
        }
    }

    #[test]
    fn base_defense_covers_the_front() {
        for seed in [1, 42, 777, 31337] {
            let b = BORDER_SIZE;
            let (cx, cy) = (b + 20, b + 20);
            let buildings = generate_base_buildings(Faction::Blue, cx, cy, seed, F_DOWN, PLAYABLE_SIZE);
            let front_towers = buildings
                .iter()
                .filter(|bl| bl.kind == BuildingKind::DefenseTower && bl.grid_y as i32 > cy as i32)
                .count();
            assert!(
                front_towers >= 2,
                "seed {seed}: only {front_towers} towers guard the front approach"
            );
            let castle = buildings
                .iter()
                .find(|bl| bl.kind == BuildingKind::Castle)
                .expect("castle");
            assert!(
                castle.grid_y as i32 > cy as i32,
                "castle must face the enemy"
            );
        }
    }

    #[test]
    fn base_production_covers_every_wave_kind() {
        let b = BORDER_SIZE;
        let buildings = generate_base_buildings(Faction::Blue, b + 20, b + 20, 42, F_DOWN, PLAYABLE_SIZE);
        for kind in [
            UnitKind::Warrior,
            UnitKind::Lancer,
            UnitKind::Archer,
            UnitKind::Monk,
        ] {
            assert!(
                buildings.iter().any(|bl| bl.produces == Some(kind)),
                "no building produces {kind:?}"
            );
        }
    }

    #[test]
    fn generate_base_buildings_is_deterministic() {
        let b = BORDER_SIZE;
        let a = generate_base_buildings(Faction::Blue, b + 20, b + 20, 1337, F_DOWN, PLAYABLE_SIZE);
        let c = generate_base_buildings(Faction::Blue, b + 20, b + 20, 1337, F_DOWN, PLAYABLE_SIZE);
        assert_eq!(a.len(), c.len());
        for (x, y) in a.iter().zip(c.iter()) {
            assert_eq!(x.kind, y.kind);
            assert_eq!(x.grid_x, y.grid_x);
            assert_eq!(x.grid_y, y.grid_y);
        }
    }

    #[test]
    fn house_variants_distributed() {
        let b = BORDER_SIZE;
        let buildings = generate_base_buildings(Faction::Blue, b + 20, b + 20, 42, F_DOWN, PLAYABLE_SIZE);
        let houses: Vec<u8> = buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::House)
            .map(|b| b.house_variant)
            .collect();
        assert!(houses.len() >= 7);
        // All 3 variants should appear
        for v in 0..3u8 {
            assert!(
                houses.contains(&v),
                "house_variant {v} missing from {houses:?}"
            );
        }
    }

    #[test]
    fn production_buildings_have_produces() {
        let b = BORDER_SIZE;
        let buildings = generate_base_buildings(Faction::Blue, b + 20, b + 20, 42, F_DOWN, PLAYABLE_SIZE);
        let warriors: Vec<_> = buildings
            .iter()
            .filter(|b| b.produces == Some(UnitKind::Warrior))
            .collect();
        let lancers: Vec<_> = buildings
            .iter()
            .filter(|b| b.produces == Some(UnitKind::Lancer))
            .collect();
        let archers: Vec<_> = buildings
            .iter()
            .filter(|b| b.produces == Some(UnitKind::Archer))
            .collect();
        let monks: Vec<_> = buildings
            .iter()
            .filter(|b| b.produces == Some(UnitKind::Monk))
            .collect();
        assert_eq!(warriors.len(), 1, "1 barracks produces warriors");
        assert_eq!(lancers.len(), 1, "1 barracks produces lancers");
        assert_eq!(archers.len(), 2, "2 archeries produce archers");
        assert_eq!(monks.len(), 1, "1 monastery produces monks");
    }

    #[test]
    fn combat_stats_correct() {
        assert!(BuildingKind::Castle.is_combat());
        assert!(BuildingKind::DefenseTower.is_combat());
        assert!(!BuildingKind::House.is_combat());
        assert!(!BuildingKind::Barracks.is_combat());
        assert!(BuildingKind::Castle.base_cooldown() < BuildingKind::DefenseTower.base_cooldown());
    }
}
