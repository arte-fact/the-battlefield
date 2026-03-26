use crate::grid::{BORDER_SIZE, PLAYABLE_SIZE, TILE_SIZE};
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
    occupied: HashSet<(i32, i32)>,
    out: Vec<BaseBuilding>,
}

impl BasePlacer {
    fn new(faction: Faction, cx: i32, cy: i32) -> Self {
        Self {
            faction,
            cx,
            cy,
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
        let (hw, vlo, vhi): (i32, i32, i32) = match kind {
            BuildingKind::Castle => (3, -2, 1),
            BuildingKind::Barracks | BuildingKind::Archery | BuildingKind::Monastery => (2, -1, 1),
            BuildingKind::DefenseTower => (1, -1, 1),
            BuildingKind::House => (2, -1, 0),
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
        // Must stay within base footprint (±11 sides, ±13 rear for village)
        if bx < self.cx - 11 || bx > self.cx + 11 || by < self.cy - 13 || by > self.cy + 13 {
            return false;
        }
        // Must stay within the playable area
        let b = BORDER_SIZE as i32;
        let p = PLAYABLE_SIZE as i32;
        if bx < b || bx >= b + p || by < b || by >= b + p {
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
        });
        true
    }

    fn finish(self) -> Vec<BaseBuilding> {
        self.out
    }
}

/// Procedurally generate all buildings for a faction base.
///
/// Layout (front = direction toward battlefield):
///   Center — Open gather zone (army rally point, no buildings)
///   Front of gather zone — 1 Castle (guards the rally area)
///   Front line — 4 DefenseTowers spread across width
///   Flanking gather zone — 2 Barracks + 2 Archery + 1 Monastery (production)
///   Periphery — up to 10 Houses (cycling 3 sprite variants)
///
/// `seed` drives the RNG; different seeds yield organically varied layouts.
/// Blue front is +Y (toward map center); Red front is −Y.
pub fn generate_base_buildings(faction: Faction, cx: u32, cy: u32, seed: u32) -> Vec<BaseBuilding> {
    let fs: i32 = match faction {
        Faction::Blue => 1,
        Faction::Red => -1,
    };
    let icx = cx as i32;
    let icy = cy as i32;
    let mut rng = Rng::new(seed);
    let mut p = BasePlacer::new(faction, icx, icy);

    // --- Castle at front of gather zone (between rally area and battlefield) ---
    p.try_place(BuildingKind::Castle, icx, icy + fs * 4);

    // --- Production buildings flanking the castle / gather zone ---
    // Placed BEFORE towers so they get priority on exclusion zones.

    // Barracks (Warriors) — left flank near castle
    if p.try_place(BuildingKind::Barracks, icx - 6, icy + fs * 2) {
        if let Some(b) = p.out.last_mut() {
            b.produces = Some(UnitKind::Warrior);
        }
    }
    // Barracks (Lancers) — right flank near castle
    if p.try_place(BuildingKind::Barracks, icx + 6, icy + fs * 2) {
        if let Some(b) = p.out.last_mut() {
            b.produces = Some(UnitKind::Lancer);
        }
    }
    // Archery — left-rear of gather zone
    if p.try_place(BuildingKind::Archery, icx - 6, icy - fs * 2) {
        if let Some(b) = p.out.last_mut() {
            b.produces = Some(UnitKind::Archer);
        }
    }
    // Archery — right-rear of gather zone
    if p.try_place(BuildingKind::Archery, icx + 6, icy - fs * 2) {
        if let Some(b) = p.out.last_mut() {
            b.produces = Some(UnitKind::Archer);
        }
    }
    // Monastery — center rear
    if p.try_place(BuildingKind::Monastery, icx, icy - fs * 5) {
        if let Some(b) = p.out.last_mut() {
            b.produces = Some(UnitKind::Monk);
        }
    }

    // --- 2 front-flank towers + 2 mid-flank towers ---
    // Placed after production buildings to avoid exclusion conflicts.
    let tower_positions: [(i32, i32); 4] = [
        (-7, fs * 7),  // front-left
        (7, fs * 7),   // front-right
        (-10, 0),      // mid-left flank
        (10, 0),       // mid-right flank
    ];
    for &(dx, dy) in &tower_positions {
        let tx = icx + dx;
        let ty = icy + dy;
        if !p.try_place(BuildingKind::DefenseTower, tx, ty) {
            for &delta in &[1i32, -1, 2, -2] {
                if p.try_place(BuildingKind::DefenseTower, tx + delta, ty) {
                    break;
                }
            }
        }
    }

    // --- Rear village: houses only behind production zone ---
    // Build candidates row by row from the back, with X shuffled per row for organic layout.
    let mut house_cands: Vec<(i32, i32)> = Vec::new();
    for rear_dist in (3..=12).rev() {
        let mut row: Vec<i32> = (-9..=9).collect();
        rng.shuffle(&mut row);
        for dx in row {
            house_cands.push((icx + dx, icy - fs * rear_dist));
        }
    }

    let mut placed = 0u8;
    for &(hx, hy) in &house_cands {
        if placed >= 10 {
            break;
        }
        if p.try_place(BuildingKind::House, hx, hy) {
            if let Some(b) = p.out.last_mut() {
                b.house_variant = placed % 3;
            }
            placed += 1;
        }
    }

    p.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn building_for_unit_mapping() {
        assert_eq!(building_for_unit(UnitKind::Warrior), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Lancer), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Archer), BuildingKind::Archery);
        assert_eq!(building_for_unit(UnitKind::Monk), BuildingKind::Monastery);
    }

    #[test]
    fn generate_base_buildings_in_playable_area() {
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let cx_blue = b + 10;
        let cy_blue = b + 10;
        let cx_red = b + p - 11;
        let cy_red = b + p - 11;
        for building in generate_base_buildings(Faction::Blue, cx_blue, cy_blue, 42)
            .iter()
            .chain(generate_base_buildings(Faction::Red, cx_red, cy_red, 42).iter())
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
        let b = BORDER_SIZE;
        let buildings = generate_base_buildings(Faction::Blue, b + 10, b + 10, 42);
        let count = |k: BuildingKind| buildings.iter().filter(|b| b.kind == k).count();
        assert_eq!(count(BuildingKind::Castle), 1, "must have 1 castle");
        assert_eq!(count(BuildingKind::DefenseTower), 4, "must have 4 towers");
        assert_eq!(count(BuildingKind::Barracks), 2, "must have 2 barracks");
        assert_eq!(count(BuildingKind::Archery), 2, "must have 2 archery");
        assert_eq!(count(BuildingKind::Monastery), 1, "must have 1 monastery");
        assert_eq!(count(BuildingKind::House), 10, "must have 10 houses");
    }

    #[test]
    fn generate_base_buildings_is_deterministic() {
        let b = BORDER_SIZE;
        let a = generate_base_buildings(Faction::Blue, b + 10, b + 10, 1337);
        let c = generate_base_buildings(Faction::Blue, b + 10, b + 10, 1337);
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
        let buildings = generate_base_buildings(Faction::Blue, b + 10, b + 10, 42);
        let houses: Vec<u8> = buildings
            .iter()
            .filter(|b| b.kind == BuildingKind::House)
            .map(|b| b.house_variant)
            .collect();
        assert_eq!(houses.len(), 10);
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
        let buildings = generate_base_buildings(Faction::Blue, b + 10, b + 10, 42);
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
