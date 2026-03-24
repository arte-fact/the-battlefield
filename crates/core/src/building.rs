use crate::grid::TILE_SIZE;
use crate::unit::{Faction, UnitKind};

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
    pub fn footprint_offsets(self) -> &'static [(i32, i32)] {
        match self {
            BuildingKind::Barracks | BuildingKind::Archery => {
                &[(-1, -1), (0, -1), (1, -1), (-1, 0), (0, 0), (1, 0)]
            }
            BuildingKind::Monastery => &[
                (-1, -2),
                (0, -2),
                (1, -2),
                (-1, -1),
                (0, -1),
                (1, -1),
                (-1, 0),
                (0, 0),
                (1, 0),
            ],
            // Castle: 320x256 = 5 wide × 4 tall, bottom-center anchored
            BuildingKind::Castle => &[
                (-2, -3),
                (-1, -3),
                (0, -3),
                (1, -3),
                (2, -3),
                (-2, -2),
                (-1, -2),
                (0, -2),
                (1, -2),
                (2, -2),
                (-2, -1),
                (-1, -1),
                (0, -1),
                (1, -1),
                (2, -1),
                (-2, 0),
                (-1, 0),
                (0, 0),
                (1, 0),
                (2, 0),
            ],
            // DefenseTower: 128x256 = 2 wide × 4 tall, bottom-center anchored
            BuildingKind::DefenseTower => &[
                (-1, -3),
                (0, -3),
                (-1, -2),
                (0, -2),
                (-1, -1),
                (0, -1),
                (-1, 0),
                (0, 0),
            ],
            // House: 128x192 = 2 wide × 3 tall, bottom-center anchored
            BuildingKind::House => &[(-1, -2), (0, -2), (-1, -1), (0, -1), (-1, 0), (0, 0)],
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

/// A building placed at a faction base.
#[derive(Clone, Debug)]
pub struct BaseBuilding {
    pub kind: BuildingKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
    /// Current attack cooldown timer (only used by combat buildings).
    pub attack_cooldown: f32,
}

/// Which building produces a given unit type.
pub fn building_for_unit(kind: UnitKind) -> BuildingKind {
    match kind {
        UnitKind::Warrior | UnitKind::Lancer => BuildingKind::Barracks,
        UnitKind::Archer => BuildingKind::Archery,
        UnitKind::Monk => BuildingKind::Monastery,
    }
}

/// Place production buildings around a base center for a given faction.
/// Blue: buildings above center (negative y offset), spawn corridor below.
/// Red: buildings below center (positive y offset), spawn corridor above.
pub fn base_buildings(faction: Faction, cx: u32, cy: u32) -> Vec<BaseBuilding> {
    match faction {
        Faction::Blue => vec![
            BaseBuilding {
                kind: BuildingKind::Barracks,
                faction,
                grid_x: cx.saturating_sub(3),
                grid_y: cy.saturating_sub(4),
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::Archery,
                faction,
                grid_x: cx + 3,
                grid_y: cy.saturating_sub(4),
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::Monastery,
                faction,
                grid_x: cx,
                grid_y: cy.saturating_sub(6),
                attack_cooldown: 0.0,
            },
        ],
        _ => vec![
            BaseBuilding {
                kind: BuildingKind::Barracks,
                faction,
                grid_x: cx + 3,
                grid_y: cy + 4,
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::Archery,
                faction,
                grid_x: cx.saturating_sub(3),
                grid_y: cy + 4,
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::Monastery,
                faction,
                grid_x: cx,
                grid_y: cy + 6,
                attack_cooldown: 0.0,
            },
        ],
    }
}

/// Place defensive buildings (castle, towers, houses) around a base center.
pub fn base_defense_buildings(faction: Faction, cx: u32, cy: u32) -> Vec<BaseBuilding> {
    match faction {
        Faction::Blue => vec![
            // Castle at base center
            BaseBuilding {
                kind: BuildingKind::Castle,
                faction,
                grid_x: cx,
                grid_y: cy + 8,
                attack_cooldown: 0.0,
            },
            // Defense towers flanking the spawn corridor (below base)
            BaseBuilding {
                kind: BuildingKind::DefenseTower,
                faction,
                grid_x: cx.saturating_sub(5),
                grid_y: cy + 2,
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::DefenseTower,
                faction,
                grid_x: cx + 5,
                grid_y: cy + 2,
                attack_cooldown: 0.0,
            },
            // Decorative houses behind the castle
            BaseBuilding {
                kind: BuildingKind::House,
                faction,
                grid_x: cx.saturating_sub(4),
                grid_y: cy.saturating_sub(4),
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::House,
                faction,
                grid_x: cx + 4,
                grid_y: cy.saturating_sub(4),
                attack_cooldown: 0.0,
            },
        ],
        _ => vec![
            // Castle at base center
            BaseBuilding {
                kind: BuildingKind::Castle,
                faction,
                grid_x: cx,
                grid_y: cy.saturating_sub(8),
                attack_cooldown: 0.0,
            },
            // Defense towers flanking the spawn corridor (above base)
            BaseBuilding {
                kind: BuildingKind::DefenseTower,
                faction,
                grid_x: cx + 5,
                grid_y: cy.saturating_sub(2),
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::DefenseTower,
                faction,
                grid_x: cx.saturating_sub(5),
                grid_y: cy.saturating_sub(2),
                attack_cooldown: 0.0,
            },
            // Decorative houses behind the castle
            BaseBuilding {
                kind: BuildingKind::House,
                faction,
                grid_x: cx + 4,
                grid_y: cy + 4,
                attack_cooldown: 0.0,
            },
            BaseBuilding {
                kind: BuildingKind::House,
                faction,
                grid_x: cx.saturating_sub(4),
                grid_y: cy + 4,
                attack_cooldown: 0.0,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{BORDER_SIZE, PLAYABLE_SIZE};

    #[test]
    fn building_for_unit_mapping() {
        assert_eq!(building_for_unit(UnitKind::Warrior), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Lancer), BuildingKind::Barracks);
        assert_eq!(building_for_unit(UnitKind::Archer), BuildingKind::Archery);
        assert_eq!(building_for_unit(UnitKind::Monk), BuildingKind::Monastery);
    }

    #[test]
    fn base_buildings_in_playable_area() {
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let cx_blue = b + 10;
        let cy_blue = b + 10;
        let cx_red = b + p - 11;
        let cy_red = b + p - 11;
        for building in base_buildings(Faction::Blue, cx_blue, cy_blue)
            .iter()
            .chain(base_buildings(Faction::Red, cx_red, cy_red).iter())
        {
            assert!(
                building.grid_x >= b && building.grid_x < b + p,
                "grid_x {} out of playable area [{}, {})",
                building.grid_x,
                b,
                b + p
            );
            assert!(
                building.grid_y >= b && building.grid_y < b + p,
                "grid_y {} out of playable area [{}, {})",
                building.grid_y,
                b,
                b + p
            );
        }
    }

    #[test]
    fn base_buildings_count() {
        assert_eq!(base_buildings(Faction::Blue, 50, 50).len(), 3);
        assert_eq!(base_buildings(Faction::Red, 100, 100).len(), 3);
    }

    #[test]
    fn defense_buildings_count() {
        assert_eq!(base_defense_buildings(Faction::Blue, 50, 50).len(), 5);
        assert_eq!(base_defense_buildings(Faction::Red, 100, 100).len(), 5);
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
