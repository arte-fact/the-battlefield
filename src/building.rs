use crate::grid;
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

/// A faction's base with production buildings and unit staging area.
#[derive(Clone, Debug)]
pub struct FactionBase {
    pub faction: Faction,
    pub buildings: Vec<ProductionBuilding>,
    pub rally_gx: u32,
    pub rally_gy: u32,
    pub rally_wx: f32,
    pub rally_wy: f32,
    staged_warriors: u32,
    staged_lancers: u32,
    staged_archers: u32,
    staged_monks: u32,
    staging_timer: f32,
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
            staged_warriors: 0,
            staged_lancers: 0,
            staged_archers: 0,
            staged_monks: 0,
            staging_timer: 0.0,
        }
    }

    /// Create the Blue faction base in the top-left corner.
    pub fn create_blue_base() -> Self {
        let f = Faction::Blue;
        let buildings = vec![
            ProductionBuilding::new(BuildingKind::Barracks, f, 3, 4),
            ProductionBuilding::new(BuildingKind::Archery, f, 7, 4),
            ProductionBuilding::new(BuildingKind::Monastery, f, 5, 7),
        ];
        Self::new(f, buildings, 5, 10)
    }

    /// Create the Red faction base in the bottom-right corner.
    pub fn create_red_base() -> Self {
        let f = Faction::Red;
        let buildings = vec![
            ProductionBuilding::new(BuildingKind::Barracks, f, 60, 59),
            ProductionBuilding::new(BuildingKind::Archery, f, 56, 59),
            ProductionBuilding::new(BuildingKind::Monastery, f, 58, 56),
        ];
        Self::new(f, buildings, 58, 53)
    }

    /// Accept a produced unit into the staging area.
    pub fn receive_unit(&mut self, kind: UnitKind) {
        if self.total_staged() == 0 {
            self.staging_timer = 0.0;
        }
        match kind {
            UnitKind::Warrior => self.staged_warriors += 1,
            UnitKind::Lancer => self.staged_lancers += 1,
            UnitKind::Archer => self.staged_archers += 1,
            UnitKind::Monk => self.staged_monks += 1,
        }
    }

    /// Total units currently staged.
    pub fn total_staged(&self) -> u32 {
        self.staged_warriors + self.staged_lancers + self.staged_archers + self.staged_monks
    }

    /// Whether a full group composition is ready.
    pub fn group_ready(&self) -> bool {
        self.staged_warriors >= GROUP_WARRIORS
            && self.staged_lancers >= GROUP_LANCERS
            && self.staged_archers >= GROUP_ARCHERS
            && self.staged_monks >= GROUP_MONKS
    }

    /// Whether the staging timer has exceeded the partial dispatch timeout.
    pub fn should_force_dispatch(&self) -> bool {
        self.total_staged() > 0 && self.staging_timer > PARTIAL_DISPATCH_TIMEOUT
    }

    /// Consume staged units and return the composition to spawn.
    /// Returns (warriors, lancers, archers, monks).
    pub fn dispatch_group(&mut self) -> (u32, u32, u32, u32) {
        let result = (
            self.staged_warriors,
            self.staged_lancers,
            self.staged_archers,
            self.staged_monks,
        );
        self.staged_warriors = 0;
        self.staged_lancers = 0;
        self.staged_archers = 0;
        self.staged_monks = 0;
        self.staging_timer = 0.0;
        result
    }

    /// Advance the staging timer.
    pub fn tick_staging(&mut self, dt: f32) {
        if self.total_staged() > 0 {
            self.staging_timer += dt;
        }
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
    vec![
        (5, 10),  // Blue rally
        (58, 53), // Red rally
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
    fn group_ready_when_composition_met() {
        let mut base = FactionBase::create_blue_base();
        assert!(!base.group_ready());

        for _ in 0..GROUP_WARRIORS {
            base.receive_unit(UnitKind::Warrior);
        }
        for _ in 0..GROUP_LANCERS {
            base.receive_unit(UnitKind::Lancer);
        }
        for _ in 0..GROUP_ARCHERS {
            base.receive_unit(UnitKind::Archer);
        }
        assert!(!base.group_ready()); // Still missing monks

        for _ in 0..GROUP_MONKS {
            base.receive_unit(UnitKind::Monk);
        }
        assert!(base.group_ready());
    }

    #[test]
    fn partial_dispatch_after_timeout() {
        let mut base = FactionBase::create_blue_base();
        base.receive_unit(UnitKind::Warrior);
        base.receive_unit(UnitKind::Archer);

        assert!(!base.should_force_dispatch());

        // Simulate time passing
        base.tick_staging(PARTIAL_DISPATCH_TIMEOUT + 1.0);
        assert!(base.should_force_dispatch());

        let (w, l, a, m) = base.dispatch_group();
        assert_eq!(w, 1);
        assert_eq!(l, 0);
        assert_eq!(a, 1);
        assert_eq!(m, 0);
        assert_eq!(base.total_staged(), 0);
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

    #[test]
    fn dispatch_resets_staging() {
        let mut base = FactionBase::create_blue_base();
        for _ in 0..GROUP_WARRIORS {
            base.receive_unit(UnitKind::Warrior);
        }
        for _ in 0..GROUP_LANCERS {
            base.receive_unit(UnitKind::Lancer);
        }
        for _ in 0..GROUP_ARCHERS {
            base.receive_unit(UnitKind::Archer);
        }
        for _ in 0..GROUP_MONKS {
            base.receive_unit(UnitKind::Monk);
        }
        assert!(base.group_ready());

        let (w, l, a, m) = base.dispatch_group();
        assert_eq!(w, GROUP_WARRIORS);
        assert_eq!(l, GROUP_LANCERS);
        assert_eq!(a, GROUP_ARCHERS);
        assert_eq!(m, GROUP_MONKS);
        assert_eq!(base.total_staged(), 0);
        assert!(!base.group_ready());
    }
}
