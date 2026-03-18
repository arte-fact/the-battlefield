use crate::grid::{self, BORDER_SIZE, PLAYABLE_SIZE};
use crate::unit::{Faction, UnitKind};

/// Group composition targets.
pub const GROUP_WARRIORS: u32 = 5;
pub const GROUP_LANCERS: u32 = 6;
pub const GROUP_ARCHERS: u32 = 3;
pub const GROUP_MONKS: u32 = 2;
pub const GROUP_TOTAL: u32 = GROUP_WARRIORS + GROUP_LANCERS + GROUP_ARCHERS + GROUP_MONKS;

/// Target time for a full group to form (seconds).
pub const GROUP_FORMATION_TIME: f32 = 40.0;

/// Force-dispatch partial groups after this many seconds of staging.
pub const PARTIAL_DISPATCH_TIMEOUT: f32 = 60.0;

/// The type of production building.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Barracks,  // Produces Warriors + Lancers
    Archery,   // Produces Archers
    Monastery, // Produces Monks
}

impl BuildingKind {
    /// Sprite dimensions (width, height) in pixels.
    pub fn sprite_size(self) -> (u32, u32) {
        match self {
            BuildingKind::Barracks | BuildingKind::Archery => (192, 256),
            BuildingKind::Monastery => (192, 320),
        }
    }

    /// Grid footprint offsets (dx, dy) relative to the building's anchor tile.
    /// The sprite is 3 tiles wide (centered) and extends upward; these offsets
    /// cover the solid base of the building that should block movement.
    pub fn footprint_offsets(self) -> &'static [(i32, i32)] {
        match self {
            // 3 wide × 2 tall base
            BuildingKind::Barracks | BuildingKind::Archery => &[
                (-1, -1), (0, -1), (1, -1),
                (-1,  0), (0,  0), (1,  0),
            ],
            // 3 wide × 3 tall base (taller building)
            BuildingKind::Monastery => &[
                (-1, -2), (0, -2), (1, -2),
                (-1, -1), (0, -1), (1, -1),
                (-1,  0), (0,  0), (1,  0),
            ],
        }
    }

    /// Asset filename for this building kind.
    pub fn asset_filename(self) -> &'static str {
        match self {
            BuildingKind::Barracks => "Barracks.png",
            BuildingKind::Archery => "Archery.png",
            BuildingKind::Monastery => "Monastery.png",
        }
    }
}

/// Static production queue for each building kind.
const BARRACKS_QUEUE: &[UnitKind] = &[
    UnitKind::Warrior, UnitKind::Lancer, UnitKind::Lancer,
    UnitKind::Warrior, UnitKind::Lancer, UnitKind::Warrior,
    UnitKind::Lancer, UnitKind::Lancer, UnitKind::Warrior,
    UnitKind::Lancer, UnitKind::Warrior,
];

const ARCHERY_QUEUE: &[UnitKind] = &[
    UnitKind::Archer, UnitKind::Archer, UnitKind::Archer,
];

const MONASTERY_QUEUE: &[UnitKind] = &[
    UnitKind::Monk, UnitKind::Monk,
];

/// A single production building that continuously produces units.
#[derive(Clone, Debug)]
pub struct ProductionBuilding {
    pub kind: BuildingKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    pub world_x: f32,
    pub world_y: f32,
    production_timer: f32,
    production_interval: f32,
    production_queue: &'static [UnitKind],
    queue_index: usize,
}

impl ProductionBuilding {
    pub fn new(kind: BuildingKind, faction: Faction, gx: u32, gy: u32) -> Self {
        let (wx, wy) = grid::grid_to_world(gx, gy);
        let (interval, queue) = match kind {
            BuildingKind::Barracks => (
                GROUP_FORMATION_TIME / (GROUP_WARRIORS + GROUP_LANCERS) as f32,
                BARRACKS_QUEUE,
            ),
            BuildingKind::Archery => (
                GROUP_FORMATION_TIME / GROUP_ARCHERS as f32,
                ARCHERY_QUEUE,
            ),
            BuildingKind::Monastery => (
                GROUP_FORMATION_TIME / GROUP_MONKS as f32,
                MONASTERY_QUEUE,
            ),
        };
        Self {
            kind,
            faction,
            grid_x: gx,
            grid_y: gy,
            world_x: wx,
            world_y: wy,
            production_timer: 0.0,
            production_interval: interval,
            production_queue: queue,
            queue_index: 0,
        }
    }

    /// Advance production timer. Returns Some(UnitKind) when a unit is produced.
    pub fn tick(&mut self, dt: f32) -> Option<UnitKind> {
        self.production_timer += dt;
        if self.production_timer >= self.production_interval {
            self.production_timer -= self.production_interval;
            let kind = self.production_queue[self.queue_index];
            self.queue_index = (self.queue_index + 1) % self.production_queue.len();
            Some(kind)
        } else {
            None
        }
    }
}

/// A faction's base with production buildings.
#[derive(Clone, Debug)]
pub struct FactionBase {
    pub faction: Faction,
    pub buildings: Vec<ProductionBuilding>,
    pub rally_gx: u32,
    pub rally_gy: u32,
    pub rally_wx: f32,
    pub rally_wy: f32,
    /// Timer tracking how long rallying units have existed (for force-dispatch).
    pub staging_timer: f32,
}

impl FactionBase {
    fn new(faction: Faction, buildings: Vec<ProductionBuilding>, rally_gx: u32, rally_gy: u32) -> Self {
        let (rally_wx, rally_wy) = grid::grid_to_world(rally_gx, rally_gy);
        Self {
            faction,
            buildings,
            rally_gx,
            rally_gy,
            rally_wx,
            rally_wy,
            staging_timer: 0.0,
        }
    }

    /// Create the Blue faction base in the top-left of the playable area.
    pub fn create_blue_base() -> Self {
        let f = Faction::Blue;
        let b = BORDER_SIZE;
        let buildings = vec![
            ProductionBuilding::new(BuildingKind::Barracks, f, b + 3, b + 4),
            ProductionBuilding::new(BuildingKind::Archery, f, b + 7, b + 4),
            ProductionBuilding::new(BuildingKind::Monastery, f, b + 5, b + 7),
        ];
        Self::new(f, buildings, b + 5, b + 10)
    }

    /// Create the Red faction base in the bottom-right of the playable area.
    pub fn create_red_base() -> Self {
        let f = Faction::Red;
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let buildings = vec![
            ProductionBuilding::new(BuildingKind::Barracks, f, b + p - 4, b + p - 5),
            ProductionBuilding::new(BuildingKind::Archery, f, b + p - 8, b + p - 5),
            ProductionBuilding::new(BuildingKind::Monastery, f, b + p - 6, b + p - 8),
        ];
        Self::new(f, buildings, b + p - 6, b + p - 11)
    }

    /// Return building grid positions for mapgen clearing.
    pub fn building_positions(&self) -> Vec<(u32, u32)> {
        self.buildings.iter().map(|b| (b.grid_x, b.grid_y)).collect()
    }
}

/// All building grid positions for both bases (for mapgen clearing).
pub fn all_building_positions() -> Vec<(u32, u32)> {
    let blue = FactionBase::create_blue_base();
    let red = FactionBase::create_red_base();
    let mut positions = blue.building_positions();
    positions.extend(red.building_positions());
    positions
}

/// Rally point positions for both bases (for mapgen clearing).
pub fn all_rally_positions() -> Vec<(u32, u32)> {
    let b = BORDER_SIZE;
    let p = PLAYABLE_SIZE;
    vec![
        (b + 5, b + 10),      // Blue rally
        (b + p - 6, b + p - 11), // Red rally
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn barracks_produces_warrior_after_interval() {
        let mut b = ProductionBuilding::new(BuildingKind::Barracks, Faction::Blue, 3, 4);
        // First in queue is Warrior
        let result = b.tick(b.production_interval + 0.001);
        assert_eq!(result, Some(UnitKind::Warrior));
    }

    #[test]
    fn building_cycles_production_queue() {
        let mut b = ProductionBuilding::new(BuildingKind::Archery, Faction::Blue, 7, 4);
        let interval = b.production_interval;
        // Archery queue: [Archer, Archer, Archer]
        for _ in 0..3 {
            assert_eq!(b.tick(interval + 0.001), Some(UnitKind::Archer));
        }
        // Should cycle back to start
        assert_eq!(b.tick(interval + 0.001), Some(UnitKind::Archer));
    }

    #[test]
    fn production_queue_lengths_match_group() {
        // Barracks should produce exactly GROUP_WARRIORS + GROUP_LANCERS per cycle
        assert_eq!(BARRACKS_QUEUE.len() as u32, GROUP_WARRIORS + GROUP_LANCERS);
        let warriors = BARRACKS_QUEUE.iter().filter(|&&k| k == UnitKind::Warrior).count() as u32;
        let lancers = BARRACKS_QUEUE.iter().filter(|&&k| k == UnitKind::Lancer).count() as u32;
        assert_eq!(warriors, GROUP_WARRIORS);
        assert_eq!(lancers, GROUP_LANCERS);

        // Archery should produce exactly GROUP_ARCHERS per cycle
        assert_eq!(ARCHERY_QUEUE.len() as u32, GROUP_ARCHERS);

        // Monastery should produce exactly GROUP_MONKS per cycle
        assert_eq!(MONASTERY_QUEUE.len() as u32, GROUP_MONKS);
    }

}
