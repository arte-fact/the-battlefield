use crate::unit::{Faction, UnitKind};

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

/// A production building placed at a faction base.
#[derive(Clone, Copy, Debug)]
pub struct BaseBuilding {
    pub kind: BuildingKind,
    pub faction: Faction,
    pub grid_x: u32,
    pub grid_y: u32,
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
            },
            BaseBuilding {
                kind: BuildingKind::Archery,
                faction,
                grid_x: cx + 3,
                grid_y: cy.saturating_sub(4),
            },
            BaseBuilding {
                kind: BuildingKind::Monastery,
                faction,
                grid_x: cx,
                grid_y: cy.saturating_sub(6),
            },
        ],
        _ => vec![
            BaseBuilding {
                kind: BuildingKind::Barracks,
                faction,
                grid_x: cx + 3,
                grid_y: cy + 4,
            },
            BaseBuilding {
                kind: BuildingKind::Archery,
                faction,
                grid_x: cx.saturating_sub(3),
                grid_y: cy + 4,
            },
            BaseBuilding {
                kind: BuildingKind::Monastery,
                faction,
                grid_x: cx,
                grid_y: cy + 6,
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
}
