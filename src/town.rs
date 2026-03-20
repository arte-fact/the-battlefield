use crate::building::{BuildingKind, ProductionBuilding};
use crate::grid::{self, BORDER_SIZE, PLAYABLE_SIZE, TILE_SIZE};
use crate::unit::{Faction, Unit};
use crate::zone::ZoneState;

/// Seconds for a single unit to fully capture a neutral town.
pub const BASE_CAPTURE_TIME: f32 = 8.0;

/// Maximum capture rate multiplier (diminishing returns).
const MAX_CAPTURE_MULTIPLIER: f32 = 3.0;

/// Hard cap on total units per faction.
pub const MAX_UNITS_PER_FACTION: usize = 40;

/// Town capture radius in tiles (Euclidean distance).
pub const TOWN_RADIUS: u32 = 5;

/// Duration (seconds) a faction must hold all towns to win.
pub const VICTORY_HOLD_TIME: f32 = 120.0;

/// A building placed within a town (house or tower).
#[derive(Clone, Debug)]
pub struct TownBuilding {
    pub kind: TownBuildingKind,
    pub grid_x: u32,
    pub grid_y: u32,
}

/// Types of decorative/structural buildings in a town.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TownBuildingKind {
    Tower,
    House1,
    House2,
    House3,
}

impl TownBuildingKind {
    /// Sprite dimensions (width, height) in pixels.
    pub fn sprite_size(self) -> (u32, u32) {
        match self {
            TownBuildingKind::Tower => (128, 256),
            TownBuildingKind::House1 | TownBuildingKind::House2 | TownBuildingKind::House3 => {
                (128, 192)
            }
        }
    }

    /// Grid footprint offsets relative to the building's anchor tile.
    pub fn footprint_offsets(self) -> &'static [(i32, i32)] {
        match self {
            TownBuildingKind::Tower => &[
                (-1, -1), (0, -1), (1, -1),
                (-1,  0), (0,  0), (1,  0),
            ],
            TownBuildingKind::House1 | TownBuildingKind::House2 | TownBuildingKind::House3 => &[
                (0, -1), (1, -1),
                (0,  0), (1,  0),
            ],
        }
    }

    /// Asset filename for this building kind.
    pub fn asset_filename(self) -> &'static str {
        match self {
            TownBuildingKind::Tower => "Tower.png",
            TownBuildingKind::House1 => "House1.png",
            TownBuildingKind::House2 => "House2.png",
            TownBuildingKind::House3 => "House3.png",
        }
    }
}

/// A capturable town on the battlefield.
#[derive(Clone, Debug)]
pub struct Town {
    pub id: u8,
    pub name: String,
    pub center_gx: u32,
    pub center_gy: u32,
    pub radius: u32,
    pub center_wx: f32,
    pub center_wy: f32,
    pub radius_world: f32,
    pub state: ZoneState,
    /// -1.0 = fully Red, 0.0 = neutral, +1.0 = fully Blue.
    pub progress: f32,
    pub blue_count: u32,
    pub red_count: u32,
    /// Decorative/structural buildings (tower, houses).
    pub buildings: Vec<TownBuilding>,
    /// Production building (one per town, produces units when captured).
    pub production: ProductionBuilding,
}

impl Town {
    pub fn new(
        id: u8,
        name: String,
        gx: u32,
        gy: u32,
        radius: u32,
        buildings: Vec<TownBuilding>,
        production: ProductionBuilding,
    ) -> Self {
        let (wx, wy) = grid::grid_to_world(gx, gy);
        Self {
            id,
            name,
            center_gx: gx,
            center_gy: gy,
            radius,
            center_wx: wx,
            center_wy: wy,
            radius_world: radius as f32 * TILE_SIZE,
            state: ZoneState::Neutral,
            progress: 0.0,
            blue_count: 0,
            red_count: 0,
            buildings,
            production,
        }
    }

    /// Returns true if a world-space position is within this town's capture zone.
    pub fn contains_world(&self, wx: f32, wy: f32) -> bool {
        let dx = wx - self.center_wx;
        let dy = wy - self.center_wy;
        dx * dx + dy * dy <= self.radius_world * self.radius_world
    }

    /// Returns true if a grid cell is within this town's capture zone.
    pub fn contains_grid(&self, gx: u32, gy: u32) -> bool {
        let dx = gx as i32 - self.center_gx as i32;
        let dy = gy as i32 - self.center_gy as i32;
        (dx * dx + dy * dy) as u32 <= self.radius * self.radius
    }
}

/// Manages all towns on the battlefield.
pub struct TownManager {
    pub towns: Vec<Town>,
    pub reinforcement_timer: f32,
    pub victory_timer: f32,
    pub victory_candidate: Option<Faction>,
}

impl TownManager {
    pub fn empty() -> Self {
        Self {
            towns: Vec::new(),
            reinforcement_timer: 0.0,
            victory_timer: 0.0,
            victory_candidate: None,
        }
    }

    pub fn new(towns: Vec<Town>) -> Self {
        Self {
            towns,
            reinforcement_timer: 0.0,
            victory_timer: 0.0,
            victory_candidate: None,
        }
    }

    /// Update unit counts for all towns.
    pub fn count_units(&mut self, units: &[Unit]) {
        for town in &mut self.towns {
            town.blue_count = 0;
            town.red_count = 0;
        }
        for unit in units {
            if !unit.alive || unit.rallying {
                continue;
            }
            for town in &mut self.towns {
                if town.contains_world(unit.x, unit.y) {
                    match unit.faction {
                        Faction::Blue => town.blue_count += 1,
                        Faction::Red => town.red_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }

    /// Tick capture progress for all towns.
    pub fn tick_capture(&mut self, dt: f32) {
        let rate_per_unit = 1.0 / BASE_CAPTURE_TIME;
        let max_rate = MAX_CAPTURE_MULTIPLIER * rate_per_unit;

        for town in &mut self.towns {
            let (blue, red) = (town.blue_count, town.red_count);

            if blue > 0 && red > 0 {
                town.state = ZoneState::Contested;
                continue;
            }

            if blue == 0 && red == 0 {
                continue;
            }

            let (count, direction) = if blue > 0 {
                (blue, 1.0_f32)
            } else {
                (red, -1.0_f32)
            };

            let rate = ((count as f32).sqrt() * rate_per_unit).min(max_rate);
            town.progress = (town.progress + direction * rate * dt).clamp(-1.0, 1.0);

            if town.progress >= 1.0 {
                town.state = ZoneState::Controlled(Faction::Blue);
            } else if town.progress <= -1.0 {
                town.state = ZoneState::Controlled(Faction::Red);
            } else {
                let faction = if direction > 0.0 {
                    Faction::Blue
                } else {
                    Faction::Red
                };
                town.state = ZoneState::Capturing(faction);
            }
        }
    }

    /// Returns Some(faction) if one faction controls all towns.
    pub fn all_towns_controlled_by(&self) -> Option<Faction> {
        if self.towns.is_empty() {
            return None;
        }
        if self
            .towns
            .iter()
            .all(|t| t.state == ZoneState::Controlled(Faction::Blue))
        {
            Some(Faction::Blue)
        } else if self
            .towns
            .iter()
            .all(|t| t.state == ZoneState::Controlled(Faction::Red))
        {
            Some(Faction::Red)
        } else {
            None
        }
    }

    /// Count towns controlled by a given faction.
    pub fn towns_controlled_by(&self, faction: Faction) -> u32 {
        self.towns
            .iter()
            .filter(|t| t.state == ZoneState::Controlled(faction))
            .count() as u32
    }

    /// Return the best town target for AI of the given faction.
    pub fn best_target_zone(&self, faction: Faction) -> Option<&Town> {
        let (own_x, own_y): (f32, f32) = match faction {
            Faction::Blue => (
                (BORDER_SIZE + 5) as f32,
                (BORDER_SIZE + 5) as f32,
            ),
            _ => (
                (BORDER_SIZE + PLAYABLE_SIZE - 6) as f32,
                (BORDER_SIZE + PLAYABLE_SIZE - 6) as f32,
            ),
        };

        let dist_sq = |t: &Town| -> i32 {
            let dx = t.center_gx as f32 - own_x;
            let dy = t.center_gy as f32 - own_y;
            ((dx * dx + dy * dy) * 10.0) as i32
        };

        // Priority 1: Contested towns
        let contested = self
            .towns
            .iter()
            .filter(|t| t.state == ZoneState::Contested)
            .min_by_key(|t| dist_sq(t));
        if contested.is_some() {
            return contested;
        }

        // Priority 2: Nearest uncontrolled town
        self.towns
            .iter()
            .filter(|t| t.state != ZoneState::Controlled(faction))
            .min_by_key(|t| dist_sq(t))
    }

    /// Update victory timer. Returns Some(faction) if a faction has won.
    pub fn tick_victory(&mut self, dt: f32) -> Option<Faction> {
        match self.all_towns_controlled_by() {
            Some(faction) => {
                if self.victory_candidate == Some(faction) {
                    self.victory_timer += dt;
                } else {
                    self.victory_candidate = Some(faction);
                    self.victory_timer = dt;
                }
                if self.victory_timer >= VICTORY_HOLD_TIME {
                    Some(faction)
                } else {
                    None
                }
            }
            None => {
                self.victory_timer = 0.0;
                self.victory_candidate = None;
                None
            }
        }
    }

    /// Progress toward victory (0.0 to 1.0).
    pub fn victory_progress(&self) -> f32 {
        if self.victory_candidate.is_some() {
            (self.victory_timer / VICTORY_HOLD_TIME).min(1.0)
        } else {
            0.0
        }
    }

    /// Return the most advanced (frontline) town controlled by a faction.
    pub fn most_advanced_zone(&self, faction: Faction) -> Option<&Town> {
        let controlled: Vec<&Town> = self
            .towns
            .iter()
            .filter(|t| t.state == ZoneState::Controlled(faction))
            .collect();
        if controlled.is_empty() {
            return None;
        }
        let (own_x, own_y): (f32, f32) = match faction {
            Faction::Blue => (BORDER_SIZE as f32, BORDER_SIZE as f32),
            _ => (
                (BORDER_SIZE + PLAYABLE_SIZE) as f32,
                (BORDER_SIZE + PLAYABLE_SIZE) as f32,
            ),
        };
        controlled
            .into_iter()
            .max_by_key(|t| {
                let dx = t.center_gx as f32 - own_x;
                let dy = t.center_gy as f32 - own_y;
                (dx * dx + dy * dy) as i32
            })
    }

    /// Return the town whose center is closest to the given world position.
    pub fn nearest_zone(&self, wx: f32, wy: f32) -> Option<&Town> {
        self.towns
            .iter()
            .min_by(|a, b| {
                let da = (a.center_wx - wx) * (a.center_wx - wx)
                    + (a.center_wy - wy) * (a.center_wy - wy);
                let db = (b.center_wx - wx) * (b.center_wx - wx)
                    + (b.center_wy - wy) * (b.center_wy - wy);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    }

    /// Return the best retreat town for a faction.
    pub fn retreat_zone(
        &self,
        faction: Faction,
        unit_wx: f32,
        unit_wy: f32,
    ) -> Option<&Town> {
        let (base_x, base_y): (f32, f32) = match faction {
            Faction::Blue => (
                BORDER_SIZE as f32 * TILE_SIZE,
                BORDER_SIZE as f32 * TILE_SIZE,
            ),
            _ => (
                (BORDER_SIZE + PLAYABLE_SIZE) as f32 * TILE_SIZE,
                (BORDER_SIZE + PLAYABLE_SIZE) as f32 * TILE_SIZE,
            ),
        };

        let unit_dist_sq = (unit_wx - base_x) * (unit_wx - base_x)
            + (unit_wy - base_y) * (unit_wy - base_y);

        let controlled: Vec<&Town> = self
            .towns
            .iter()
            .filter(|t| t.state == ZoneState::Controlled(faction))
            .collect();

        if controlled.is_empty() {
            return None;
        }

        let behind: Vec<&Town> = controlled
            .iter()
            .copied()
            .filter(|t| {
                let d = (t.center_wx - base_x) * (t.center_wx - base_x)
                    + (t.center_wy - base_y) * (t.center_wy - base_y);
                d < unit_dist_sq
            })
            .collect();

        if !behind.is_empty() {
            return behind
                .into_iter()
                .max_by(|a, b| {
                    let da = (a.center_wx - base_x) * (a.center_wx - base_x)
                        + (a.center_wy - base_y) * (a.center_wy - base_y);
                    let db = (b.center_wx - base_x) * (b.center_wx - base_x)
                        + (b.center_wy - base_y) * (b.center_wy - base_y);
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
        }

        controlled
            .into_iter()
            .min_by(|a, b| {
                let da = (a.center_wx - unit_wx) * (a.center_wx - unit_wx)
                    + (a.center_wy - unit_wy) * (a.center_wy - unit_wy);
                let db = (b.center_wx - unit_wx) * (b.center_wx - unit_wx)
                    + (b.center_wy - unit_wy) * (b.center_wy - unit_wy);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Name generation: combine prefix + suffix arrays for fantasy town names.
const PREFIXES: &[&str] = &[
    "Iron", "Stone", "Oak", "Ash", "Wolf", "Raven", "Storm", "Dark", "Sun",
    "Moon", "Frost", "Silver", "Gold", "Shadow", "Thorn", "Eagle", "Bear",
    "Fire", "Wind", "Star",
];
const SUFFIXES: &[&str] = &[
    "hold", "vale", "keep", "gate", "ford", "march", "watch", "haven",
    "fall", "crest", "wood", "peak", "bridge", "moor", "wick",
];

/// Generate `count` unique town names using a seeded RNG value.
pub fn generate_town_names(seed: u32, count: usize) -> Vec<String> {
    let mut names = Vec::with_capacity(count);
    let mut state = if seed == 0 { 1u32 } else { seed };
    let mut next = || -> u32 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    let mut attempts = 0;
    while names.len() < count && attempts < 200 {
        let pi = (next() as usize) % PREFIXES.len();
        let si = (next() as usize) % SUFFIXES.len();
        let name = format!("{}{}", PREFIXES[pi], SUFFIXES[si]);
        if !names.contains(&name) {
            names.push(name);
        }
        attempts += 1;
    }

    // Fallback if we somehow couldn't generate enough
    while names.len() < count {
        names.push(format!("Town {}", names.len() + 1));
    }

    names
}

/// Data returned from mapgen for creating towns.
pub struct TownPlacement {
    pub center_gx: u32,
    pub center_gy: u32,
    pub buildings: Vec<TownBuilding>,
    pub production_kind: BuildingKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitKind;

    fn make_test_town(id: u8, gx: u32, gy: u32) -> Town {
        Town::new(
            id,
            format!("Town{}", id),
            gx,
            gy,
            TOWN_RADIUS,
            vec![],
            ProductionBuilding::new(BuildingKind::Barracks, Faction::Blue, gx, gy + 3),
        )
    }

    #[test]
    fn town_contains_center() {
        let town = make_test_town(0, 64, 64);
        assert!(town.contains_grid(64, 64));
        assert!(town.contains_grid(60, 64)); // 4 tiles left
        assert!(town.contains_grid(64, 60)); // 4 tiles up
    }

    #[test]
    fn town_excludes_outside() {
        let town = make_test_town(0, 64, 64);
        assert!(!town.contains_grid(58, 64)); // 6 tiles away
    }

    #[test]
    fn name_generator_produces_unique() {
        let names = generate_town_names(42, 5);
        assert_eq!(names.len(), 5);
        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                assert_ne!(names[i], names[j], "Duplicate name: {}", names[i]);
            }
        }
    }

    #[test]
    fn capture_progress() {
        let mut mgr = TownManager::new(vec![
            make_test_town(0, 64, 64),
            make_test_town(1, 80, 80),
        ]);
        mgr.towns[0].blue_count = 1;
        mgr.tick_capture(4.0);
        assert!(mgr.towns[0].progress > 0.4 && mgr.towns[0].progress < 0.6);
        assert!(matches!(mgr.towns[0].state, ZoneState::Capturing(Faction::Blue)));
    }

    #[test]
    fn all_towns_controlled_victory() {
        let mut mgr = TownManager::new(vec![
            make_test_town(0, 64, 64),
            make_test_town(1, 80, 80),
        ]);
        for town in &mut mgr.towns {
            town.state = ZoneState::Controlled(Faction::Blue);
            town.progress = 1.0;
        }
        assert_eq!(mgr.all_towns_controlled_by(), Some(Faction::Blue));
    }

    #[test]
    fn victory_triggers_after_hold() {
        let mut mgr = TownManager::new(vec![make_test_town(0, 64, 64)]);
        mgr.towns[0].state = ZoneState::Controlled(Faction::Blue);
        assert!(mgr.tick_victory(VICTORY_HOLD_TIME + 1.0).is_some());
    }

    #[test]
    fn best_target_prefers_contested() {
        let mut mgr = TownManager::new(vec![
            make_test_town(0, 50, 50),
            make_test_town(1, 70, 70),
            make_test_town(2, 90, 90),
        ]);
        mgr.towns[0].state = ZoneState::Controlled(Faction::Blue);
        mgr.towns[1].state = ZoneState::Neutral;
        mgr.towns[2].state = ZoneState::Contested;
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert_eq!(target.id, 2);
    }

    #[test]
    fn count_units_tallies_correctly() {
        let mut mgr = TownManager::new(vec![make_test_town(0, 64, 64)]);
        let units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, 64, 64, false),
            Unit::new(2, UnitKind::Warrior, Faction::Blue, 64, 65, false),
            Unit::new(3, UnitKind::Warrior, Faction::Red, 64, 63, false),
            Unit::new(4, UnitKind::Warrior, Faction::Red, 0, 0, false),
        ];
        mgr.count_units(&units);
        assert_eq!(mgr.towns[0].blue_count, 2);
        assert_eq!(mgr.towns[0].red_count, 1);
    }
}
