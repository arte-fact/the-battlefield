use crate::grid::{self, TILE_SIZE};
use crate::unit::{Faction, Unit};

/// Seconds for a single unit to fully capture a neutral zone.
pub const BASE_CAPTURE_TIME: f32 = 8.0;

/// Maximum capture rate multiplier (diminishing returns).
const MAX_CAPTURE_MULTIPLIER: f32 = 3.0;

/// Interval between reinforcement wave checks (seconds).
pub const REINFORCEMENT_INTERVAL: f32 = 15.0;

/// Maximum reinforcement units per wave per faction.
pub const MAX_REINFORCEMENTS_PER_WAVE: u32 = 2;

/// Hard cap on total units per faction to avoid performance issues.
pub const MAX_UNITS_PER_FACTION: usize = 25;

/// Capture zone radius in tiles (Chebyshev distance).
pub const ZONE_RADIUS: u32 = 4;

/// Capture zone states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneState {
    Neutral,
    Contested,
    Capturing(Faction),
    Controlled(Faction),
}

/// A capture zone on the battlefield.
#[derive(Clone, Debug)]
pub struct CaptureZone {
    pub id: u8,
    pub name: &'static str,
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
}

impl CaptureZone {
    pub fn new(id: u8, name: &'static str, gx: u32, gy: u32, radius: u32) -> Self {
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
        }
    }

    /// Returns true if a world-space position is within this zone (Chebyshev distance).
    pub fn contains_world(&self, wx: f32, wy: f32) -> bool {
        let dx = (wx - self.center_wx).abs();
        let dy = (wy - self.center_wy).abs();
        dx <= self.radius_world && dy <= self.radius_world
    }

    /// Returns true if a grid cell is within this zone.
    pub fn contains_grid(&self, gx: u32, gy: u32) -> bool {
        let dx = (gx as i32 - self.center_gx as i32).unsigned_abs();
        let dy = (gy as i32 - self.center_gy as i32).unsigned_abs();
        dx <= self.radius && dy <= self.radius
    }
}

/// Manages all capture zones on the battlefield.
pub struct ZoneManager {
    pub zones: Vec<CaptureZone>,
    pub reinforcement_timer: f32,
}

impl ZoneManager {
    pub fn empty() -> Self {
        Self {
            zones: Vec::new(),
            reinforcement_timer: 0.0,
        }
    }

    pub fn create_default_zones() -> Self {
        let zones = vec![
            CaptureZone::new(0, "Forward Blue", 16, 32, ZONE_RADIUS),
            CaptureZone::new(1, "North Flank", 25, 18, ZONE_RADIUS),
            CaptureZone::new(2, "Center", 32, 32, ZONE_RADIUS),
            CaptureZone::new(3, "South Flank", 39, 46, ZONE_RADIUS),
            CaptureZone::new(4, "Forward Red", 48, 32, ZONE_RADIUS),
        ];
        Self {
            zones,
            reinforcement_timer: 0.0,
        }
    }

    /// Update unit counts for all zones.
    pub fn count_units(&mut self, units: &[Unit]) {
        for zone in &mut self.zones {
            zone.blue_count = 0;
            zone.red_count = 0;
        }
        for unit in units {
            if !unit.alive {
                continue;
            }
            for zone in &mut self.zones {
                if zone.contains_world(unit.x, unit.y) {
                    match unit.faction {
                        Faction::Blue => zone.blue_count += 1,
                        Faction::Red => zone.red_count += 1,
                        _ => {}
                    }
                }
            }
        }
    }

    /// Tick capture progress for all zones.
    pub fn tick_capture(&mut self, dt: f32) {
        let rate_per_unit = 1.0 / BASE_CAPTURE_TIME;
        let max_rate = MAX_CAPTURE_MULTIPLIER * rate_per_unit;

        for zone in &mut self.zones {
            let (blue, red) = (zone.blue_count, zone.red_count);

            if blue > 0 && red > 0 {
                zone.state = ZoneState::Contested;
                continue;
            }

            if blue == 0 && red == 0 {
                // No units present — state persists, no progress change
                continue;
            }

            let (count, direction) = if blue > 0 {
                (blue, 1.0_f32)
            } else {
                (red, -1.0_f32)
            };

            let rate = ((count as f32).sqrt() * rate_per_unit).min(max_rate);
            zone.progress = (zone.progress + direction * rate * dt).clamp(-1.0, 1.0);

            if zone.progress >= 1.0 {
                zone.state = ZoneState::Controlled(Faction::Blue);
            } else if zone.progress <= -1.0 {
                zone.state = ZoneState::Controlled(Faction::Red);
            } else {
                let faction = if direction > 0.0 {
                    Faction::Blue
                } else {
                    Faction::Red
                };
                zone.state = ZoneState::Capturing(faction);
            }
        }
    }

    /// Returns Some(faction) if one faction controls all zones.
    pub fn all_zones_controlled_by(&self) -> Option<Faction> {
        if self.zones.is_empty() {
            return None;
        }
        if self
            .zones
            .iter()
            .all(|z| z.state == ZoneState::Controlled(Faction::Blue))
        {
            Some(Faction::Blue)
        } else if self
            .zones
            .iter()
            .all(|z| z.state == ZoneState::Controlled(Faction::Red))
        {
            Some(Faction::Red)
        } else {
            None
        }
    }

    /// Count zones controlled by a given faction.
    pub fn zones_controlled_by(&self, faction: Faction) -> u32 {
        self.zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .count() as u32
    }

    /// Return the best zone target for AI of the given faction.
    /// Priority: contested zones first, then nearest uncontrolled zone to own spawn.
    /// Returns None if all zones are controlled by this faction.
    pub fn best_target_zone(&self, faction: Faction) -> Option<&CaptureZone> {
        let own_x: f32 = match faction {
            Faction::Blue => 5.0,
            _ => 59.0,
        };

        // Priority 1: Contested zones (reinforce — nearest to own spawn)
        let contested = self
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Contested)
            .min_by_key(|z| ((z.center_gx as f32 - own_x).abs() * 10.0) as i32);
        if contested.is_some() {
            return contested;
        }

        // Priority 2: Nearest uncontrolled zone (neutral, capturing by either, or enemy-controlled)
        self.zones
            .iter()
            .filter(|z| z.state != ZoneState::Controlled(faction))
            .min_by_key(|z| ((z.center_gx as f32 - own_x).abs() * 10.0) as i32)
    }

    /// Return the zone positions (grid coordinates) for mapgen clearing.
    pub fn default_zone_centers() -> Vec<(u32, u32)> {
        vec![(16, 32), (25, 18), (32, 32), (39, 46), (48, 32)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitKind;

    #[test]
    fn zone_contains_center() {
        let zone = CaptureZone::new(0, "Test", 32, 32, 4);
        assert!(zone.contains_grid(32, 32));
        assert!(zone.contains_grid(28, 28));
        assert!(zone.contains_grid(36, 36));
    }

    #[test]
    fn zone_excludes_outside() {
        let zone = CaptureZone::new(0, "Test", 32, 32, 4);
        assert!(!zone.contains_grid(27, 32));
        assert!(!zone.contains_grid(32, 37));
    }

    #[test]
    fn zone_contains_world_check() {
        let zone = CaptureZone::new(0, "Test", 32, 32, 4);
        // Center should be inside
        assert!(zone.contains_world(zone.center_wx, zone.center_wy));
        // Far outside
        assert!(!zone.contains_world(0.0, 0.0));
    }

    #[test]
    fn neutral_zone_captures_with_blue() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[2].blue_count = 1;
        mgr.zones[2].red_count = 0;
        mgr.tick_capture(4.0); // half of BASE_CAPTURE_TIME
        assert!(mgr.zones[2].progress > 0.4 && mgr.zones[2].progress < 0.6);
        assert!(matches!(
            mgr.zones[2].state,
            ZoneState::Capturing(Faction::Blue)
        ));
    }

    #[test]
    fn neutral_zone_captures_with_red() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[2].blue_count = 0;
        mgr.zones[2].red_count = 1;
        mgr.tick_capture(4.0);
        assert!(mgr.zones[2].progress < -0.4 && mgr.zones[2].progress > -0.6);
        assert!(matches!(
            mgr.zones[2].state,
            ZoneState::Capturing(Faction::Red)
        ));
    }

    #[test]
    fn contested_zone_freezes_progress() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[2].progress = 0.5;
        mgr.zones[2].blue_count = 3;
        mgr.zones[2].red_count = 1;
        mgr.tick_capture(1.0);
        assert!((mgr.zones[2].progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(mgr.zones[2].state, ZoneState::Contested);
    }

    #[test]
    fn more_units_capture_faster() {
        let mut mgr1 = ZoneManager::create_default_zones();
        let mut mgr2 = ZoneManager::create_default_zones();
        mgr1.zones[0].blue_count = 1;
        mgr2.zones[0].blue_count = 4;
        mgr1.tick_capture(1.0);
        mgr2.tick_capture(1.0);
        assert!(mgr2.zones[0].progress > mgr1.zones[0].progress);
    }

    #[test]
    fn fully_captured_becomes_controlled() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].blue_count = 4;
        // Rate = sqrt(4)/8 = 0.25/s, need 4 seconds
        for _ in 0..50 {
            mgr.tick_capture(0.1);
        }
        assert_eq!(mgr.zones[0].state, ZoneState::Controlled(Faction::Blue));
        assert!((mgr.zones[0].progress - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn all_zones_controlled_victory() {
        let mut mgr = ZoneManager::create_default_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
            zone.progress = 1.0;
        }
        assert_eq!(mgr.all_zones_controlled_by(), Some(Faction::Blue));
    }

    #[test]
    fn mixed_control_no_victory() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        mgr.zones[1].state = ZoneState::Controlled(Faction::Red);
        assert_eq!(mgr.all_zones_controlled_by(), None);
    }

    #[test]
    fn empty_zones_no_victory() {
        let mgr = ZoneManager::empty();
        assert_eq!(mgr.all_zones_controlled_by(), None);
    }

    #[test]
    fn zones_controlled_count() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        mgr.zones[1].state = ZoneState::Controlled(Faction::Blue);
        mgr.zones[2].state = ZoneState::Controlled(Faction::Red);
        assert_eq!(mgr.zones_controlled_by(Faction::Blue), 2);
        assert_eq!(mgr.zones_controlled_by(Faction::Red), 1);
    }

    #[test]
    fn best_target_prefers_contested() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        mgr.zones[1].state = ZoneState::Neutral;
        mgr.zones[2].state = ZoneState::Contested;
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert_eq!(target.id, 2); // contested gets priority
    }

    #[test]
    fn best_target_nearest_uncontrolled() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        // Zones 1-4 are neutral, zone 1 (x=25) is nearest to Blue spawn (x=5)
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert_eq!(target.id, 1);
    }

    #[test]
    fn best_target_none_when_all_controlled() {
        let mut mgr = ZoneManager::create_default_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
        }
        assert!(mgr.best_target_zone(Faction::Blue).is_none());
    }

    #[test]
    fn count_units_tallies_correctly() {
        let mut mgr = ZoneManager::create_default_zones();
        let units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, 16, 32, false),
            Unit::new(2, UnitKind::Warrior, Faction::Blue, 16, 33, false),
            Unit::new(3, UnitKind::Warrior, Faction::Red, 16, 31, false),
            Unit::new(4, UnitKind::Warrior, Faction::Red, 0, 0, false), // outside all zones
        ];
        mgr.count_units(&units);
        assert_eq!(mgr.zones[0].blue_count, 2);
        assert_eq!(mgr.zones[0].red_count, 1);
        // Unit 4 should not be in any zone
        for zone in &mgr.zones[1..] {
            assert_eq!(zone.blue_count, 0);
        }
    }

    #[test]
    fn no_progress_when_empty() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].progress = 0.3;
        mgr.zones[0].state = ZoneState::Capturing(Faction::Blue);
        // No units counted
        mgr.tick_capture(5.0);
        // Progress should not change
        assert!((mgr.zones[0].progress - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn progress_clamped_to_range() {
        let mut mgr = ZoneManager::create_default_zones();
        mgr.zones[0].blue_count = 9;
        mgr.zones[0].progress = 0.95;
        mgr.tick_capture(10.0); // way more than needed
        assert!((mgr.zones[0].progress - 1.0).abs() < f32::EPSILON);
    }
}
