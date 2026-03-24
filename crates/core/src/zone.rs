use crate::grid::{self, BORDER_SIZE, PLAYABLE_SIZE, TILE_SIZE};
use crate::mapgen::MapLayout;
use crate::unit::{Faction, Unit};

/// Seconds for a single unit to fully capture a neutral zone.
pub const BASE_CAPTURE_TIME: f32 = 8.0;

/// Maximum capture rate multiplier (diminishing returns).
const MAX_CAPTURE_MULTIPLIER: f32 = 3.0;

/// Hard cap on total units per faction to avoid performance issues.
pub const MAX_UNITS_PER_FACTION: usize = 35;

/// Capture zone radius in tiles (Euclidean distance).
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
    /// Attack cooldown for the zone tower (fires when controlled).
    pub tower_cooldown: f32,
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
            tower_cooldown: 0.0,
        }
    }

    /// Returns true if a world-space position is within this zone (Euclidean distance).
    pub fn contains_world(&self, wx: f32, wy: f32) -> bool {
        let dx = wx - self.center_wx;
        let dy = wy - self.center_wy;
        dx * dx + dy * dy <= self.radius_world * self.radius_world
    }

    /// Returns true if a grid cell is within this zone (Euclidean distance).
    pub fn contains_grid(&self, gx: u32, gy: u32) -> bool {
        let dx = gx as i32 - self.center_gx as i32;
        let dy = gy as i32 - self.center_gy as i32;
        (dx * dx + dy * dy) as u32 <= self.radius * self.radius
    }
}

/// Duration (seconds) a faction must hold all zones to win.
pub const VICTORY_HOLD_TIME: f32 = 120.0;

/// Manages all capture zones on the battlefield.
pub struct ZoneManager {
    pub zones: Vec<CaptureZone>,
    pub reinforcement_timer: f32,
    /// Tracks how long a faction has held all zones. Resets when control is lost.
    pub victory_timer: f32,
    /// The faction currently holding all zones (if any).
    pub victory_candidate: Option<Faction>,
    /// Blue base grid position (from layout).
    pub blue_base: (u32, u32),
    /// Red base grid position (from layout).
    pub red_base: (u32, u32),
}

impl ZoneManager {
    pub fn empty() -> Self {
        Self {
            zones: Vec::new(),
            reinforcement_timer: 0.0,
            victory_timer: 0.0,
            victory_candidate: None,
            blue_base: (BORDER_SIZE + 5, BORDER_SIZE + 5),
            red_base: (
                BORDER_SIZE + PLAYABLE_SIZE - 6,
                BORDER_SIZE + PLAYABLE_SIZE - 6,
            ),
        }
    }

    /// Create zones from BSP-generated layout.
    pub fn create_from_layout(layout: &MapLayout) -> Self {
        // Zone names cycle through letters
        const NAMES: &[&str] = &[
            "Zone A", "Zone B", "Zone C", "Zone D", "Zone E", "Zone F", "Zone G", "Zone H",
            "Zone I", "Zone J", "Zone K", "Zone L", "Zone M", "Zone N",
        ];
        let zones = layout
            .zone_centers
            .iter()
            .enumerate()
            .map(|(i, &(gx, gy))| {
                CaptureZone::new(i as u8, NAMES[i % NAMES.len()], gx, gy, ZONE_RADIUS)
            })
            .collect();
        Self {
            zones,
            reinforcement_timer: 0.0,
            victory_timer: 0.0,
            victory_candidate: None,
            blue_base: layout.blue_base,
            red_base: layout.red_base,
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

    /// Return the best zone target for AI of the given faction.
    /// Priority: contested zones first, then nearest uncontrolled zone to own spawn.
    /// Returns None if all zones are controlled by this faction.
    pub fn best_target_zone(&self, faction: Faction) -> Option<&CaptureZone> {
        let (own_x, own_y): (f32, f32) = match faction {
            Faction::Blue => (self.blue_base.0 as f32, self.blue_base.1 as f32),
            _ => (self.red_base.0 as f32, self.red_base.1 as f32),
        };

        let dist_sq = |z: &CaptureZone| -> i32 {
            let dx = z.center_gx as f32 - own_x;
            let dy = z.center_gy as f32 - own_y;
            ((dx * dx + dy * dy) * 10.0) as i32
        };

        // Priority 1: Contested zones (reinforce — nearest to own base)
        let contested = self
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Contested)
            .min_by_key(|z| dist_sq(z));
        if contested.is_some() {
            return contested;
        }

        // Priority 2: Nearest uncontrolled zone (neutral, capturing by either, or enemy-controlled)
        self.zones
            .iter()
            .filter(|z| z.state != ZoneState::Controlled(faction))
            .min_by_key(|z| dist_sq(z))
    }

    /// Return the most advanced controlled zone (farthest from own base).
    /// Used when all zones are held — army defends the front line instead of rushing the enemy base.
    pub fn most_advanced_zone(&self, faction: Faction) -> Option<&CaptureZone> {
        let (own_x, own_y) = match faction {
            Faction::Blue => (self.blue_base.0 as f32, self.blue_base.1 as f32),
            _ => (self.red_base.0 as f32, self.red_base.1 as f32),
        };
        self.zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .max_by_key(|z| {
                let dx = z.center_gx as f32 - own_x;
                let dy = z.center_gy as f32 - own_y;
                ((dx * dx + dy * dy) * 10.0) as i32
            })
    }

    /// Score all zones for a faction, return top 3 as (world_x, world_y, score).
    /// Fewer than 3 entries if fewer valid targets exist.
    pub fn score_top3_zones(&self, faction: Faction) -> Vec<(f32, f32, f32)> {
        if self.zones.is_empty() {
            return Vec::new();
        }

        let (own_x, own_y) = match faction {
            Faction::Blue => (self.blue_base.0 as f32, self.blue_base.1 as f32),
            _ => (self.red_base.0 as f32, self.red_base.1 as f32),
        };

        // Max distance for normalization
        let max_dist_sq = self
            .zones
            .iter()
            .map(|z| {
                let dx = z.center_gx as f32 - own_x;
                let dy = z.center_gy as f32 - own_y;
                dx * dx + dy * dy
            })
            .fold(1.0_f32, f32::max);

        let mut scored: Vec<(f32, f32, f32)> = self
            .zones
            .iter()
            .map(|z| {
                let mut score = 0.0_f32;

                let controlled_by_us = z.state == ZoneState::Controlled(faction);
                let progress_for_us = match faction {
                    Faction::Blue => z.progress,  // +1 = fully Blue
                    Faction::Red => -z.progress,  // flip so +1 = fully Red
                };

                // Expansion target: zones we don't control
                if !controlled_by_us {
                    score += 40.0;
                }

                // Urgency: zone we own but progress is eroding
                if controlled_by_us && progress_for_us < 0.99 {
                    score += 50.0 * (1.0 - progress_for_us);
                }

                // Momentum: zone we're actively capturing — finish the job
                if !controlled_by_us && progress_for_us > 0.0 {
                    score += 30.0 * progress_for_us;
                }

                // Contested bonus
                if z.state == ZoneState::Contested {
                    score += 20.0;
                }

                // Enemy pressure
                let enemy_count = match faction {
                    Faction::Blue => z.red_count,
                    Faction::Red => z.blue_count,
                };
                score += 5.0 * enemy_count as f32;

                // Distance penalty (normalized 0-1)
                let dx = z.center_gx as f32 - own_x;
                let dy = z.center_gy as f32 - own_y;
                let norm_dist = (dx * dx + dy * dy) / max_dist_sq;
                score -= 15.0 * norm_dist;

                // All zones controlled — defend the most advanced one
                if controlled_by_us && progress_for_us >= 0.99 {
                    // Only distance-based score (farthest from base = most advanced)
                    score = norm_dist * 10.0;
                }

                (z.center_wx, z.center_wy, score)
            })
            .collect();

        // Sort by score descending, take top 3
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(3);
        scored
    }

    /// Update victory timer. Returns Some(faction) if a faction has won.
    pub fn tick_victory(&mut self, dt: f32) -> Option<Faction> {
        match self.all_zones_controlled_by() {
            Some(faction) => {
                if self.victory_candidate == Some(faction) {
                    self.victory_timer += dt;
                } else {
                    // New faction took total control — reset timer
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
                // No total control — reset
                self.victory_timer = 0.0;
                self.victory_candidate = None;
                None
            }
        }
    }

    /// Progress toward victory (0.0 to 1.0) for the faction currently holding all zones.
    pub fn victory_progress(&self) -> f32 {
        if self.victory_candidate.is_some() {
            (self.victory_timer / VICTORY_HOLD_TIME).min(1.0)
        } else {
            0.0
        }
    }

    /// Return the best retreat zone for a faction: a controlled zone that is
    /// closer to own base than the unit ("behind" it), picking the most advanced
    /// (farthest from base) among those. Falls back to nearest controlled zone.
    pub fn retreat_zone(
        &self,
        faction: Faction,
        unit_wx: f32,
        unit_wy: f32,
    ) -> Option<&CaptureZone> {
        let (base_x, base_y): (f32, f32) = match faction {
            Faction::Blue => (
                self.blue_base.0 as f32 * TILE_SIZE,
                self.blue_base.1 as f32 * TILE_SIZE,
            ),
            _ => (
                self.red_base.0 as f32 * TILE_SIZE,
                self.red_base.1 as f32 * TILE_SIZE,
            ),
        };

        let unit_dist_sq =
            (unit_wx - base_x) * (unit_wx - base_x) + (unit_wy - base_y) * (unit_wy - base_y);

        let controlled: Vec<&CaptureZone> = self
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .collect();

        if controlled.is_empty() {
            return None;
        }

        // Zones closer to base than the unit (behind the unit)
        let behind: Vec<&CaptureZone> = controlled
            .iter()
            .copied()
            .filter(|z| {
                let d = (z.center_wx - base_x) * (z.center_wx - base_x)
                    + (z.center_wy - base_y) * (z.center_wy - base_y);
                d < unit_dist_sq
            })
            .collect();

        if !behind.is_empty() {
            // Pick the most advanced rear zone (farthest from base among behind zones)
            return behind.into_iter().max_by(|a, b| {
                let da = (a.center_wx - base_x) * (a.center_wx - base_x)
                    + (a.center_wy - base_y) * (a.center_wy - base_y);
                let db = (b.center_wx - base_x) * (b.center_wx - base_x)
                    + (b.center_wy - base_y) * (b.center_wy - base_y);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Fallback: nearest controlled zone
        controlled.into_iter().min_by(|a, b| {
            let da = (a.center_wx - unit_wx) * (a.center_wx - unit_wx)
                + (a.center_wy - unit_wy) * (a.center_wy - unit_wy);
            let db = (b.center_wx - unit_wx) * (b.center_wx - unit_wx)
                + (b.center_wy - unit_wy) * (b.center_wy - unit_wy);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unit::UnitKind;

    /// Create a test layout with known zone positions for unit tests.
    fn test_layout() -> MapLayout {
        let b = BORDER_SIZE;
        MapLayout {
            blue_base: (b + 5, b + 5),
            red_base: (b + PLAYABLE_SIZE - 6, b + PLAYABLE_SIZE - 6),
            zone_centers: vec![
                (b + 35, b + 35), // diagonal 25%
                (b + 64, b + 64), // diagonal 50% (center)
                (b + 93, b + 93), // diagonal 75%
                (b + 35, b + 93), // flank
                (b + 93, b + 35), // flank
            ],
        }
    }

    fn test_zones() -> ZoneManager {
        ZoneManager::create_from_layout(&test_layout())
    }

    #[test]
    fn zone_contains_center() {
        let zone = CaptureZone::new(0, "Test", 64, 64, 4);
        assert!(zone.contains_grid(64, 64)); // center
        assert!(zone.contains_grid(60, 64)); // 4 tiles left (on boundary)
        assert!(zone.contains_grid(64, 60)); // 4 tiles up (on boundary)
        assert!(zone.contains_grid(62, 62)); // sqrt(8) ≈ 2.83, inside
    }

    #[test]
    fn zone_excludes_outside() {
        let zone = CaptureZone::new(0, "Test", 64, 64, 4);
        assert!(!zone.contains_grid(59, 64)); // 5 tiles away
        assert!(!zone.contains_grid(64, 69)); // 5 tiles away
        assert!(!zone.contains_grid(60, 60)); // diagonal corner, sqrt(32) > 4
    }

    #[test]
    fn zone_contains_world_check() {
        let zone = CaptureZone::new(0, "Test", 64, 64, 4);
        // Center should be inside
        assert!(zone.contains_world(zone.center_wx, zone.center_wy));
        // Far outside
        assert!(!zone.contains_world(0.0, 0.0));
    }

    #[test]
    fn neutral_zone_captures_with_blue() {
        let mut mgr = test_zones();
        mgr.zones[1].blue_count = 1;
        mgr.zones[1].red_count = 0;
        mgr.tick_capture(4.0); // half of BASE_CAPTURE_TIME
        assert!(mgr.zones[1].progress > 0.4 && mgr.zones[1].progress < 0.6);
        assert!(matches!(
            mgr.zones[1].state,
            ZoneState::Capturing(Faction::Blue)
        ));
    }

    #[test]
    fn neutral_zone_captures_with_red() {
        let mut mgr = test_zones();
        mgr.zones[1].blue_count = 0;
        mgr.zones[1].red_count = 1;
        mgr.tick_capture(4.0);
        assert!(mgr.zones[1].progress < -0.4 && mgr.zones[1].progress > -0.6);
        assert!(matches!(
            mgr.zones[1].state,
            ZoneState::Capturing(Faction::Red)
        ));
    }

    #[test]
    fn contested_zone_freezes_progress() {
        let mut mgr = test_zones();
        mgr.zones[1].progress = 0.5;
        mgr.zones[1].blue_count = 3;
        mgr.zones[1].red_count = 1;
        mgr.tick_capture(1.0);
        assert!((mgr.zones[1].progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(mgr.zones[1].state, ZoneState::Contested);
    }

    #[test]
    fn more_units_capture_faster() {
        let mut mgr1 = test_zones();
        let mut mgr2 = test_zones();
        mgr1.zones[0].blue_count = 1;
        mgr2.zones[0].blue_count = 4;
        mgr1.tick_capture(1.0);
        mgr2.tick_capture(1.0);
        assert!(mgr2.zones[0].progress > mgr1.zones[0].progress);
    }

    #[test]
    fn fully_captured_becomes_controlled() {
        let mut mgr = test_zones();
        mgr.zones[0].blue_count = 4;
        for _ in 0..50 {
            mgr.tick_capture(0.1);
        }
        assert_eq!(mgr.zones[0].state, ZoneState::Controlled(Faction::Blue));
        assert!((mgr.zones[0].progress - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn all_zones_controlled_victory() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
            zone.progress = 1.0;
        }
        assert_eq!(mgr.all_zones_controlled_by(), Some(Faction::Blue));
    }

    #[test]
    fn mixed_control_no_victory() {
        let mut mgr = test_zones();
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
    fn best_target_prefers_contested() {
        let mut mgr = test_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        mgr.zones[1].state = ZoneState::Neutral;
        mgr.zones[2].state = ZoneState::Contested;
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert_eq!(target.id, 2); // contested gets priority
    }

    #[test]
    fn best_target_nearest_uncontrolled() {
        let mut mgr = test_zones();
        mgr.zones[0].state = ZoneState::Controlled(Faction::Blue);
        // Zone 1 is nearest to Blue base among remaining neutral zones
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert_eq!(target.id, 1);
    }

    #[test]
    fn best_target_none_when_all_controlled() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
        }
        assert!(mgr.best_target_zone(Faction::Blue).is_none());
    }

    #[test]
    fn count_units_tallies_correctly() {
        let mut mgr = test_zones();
        let z0x = mgr.zones[0].center_gx;
        let z0y = mgr.zones[0].center_gy;
        let units = vec![
            Unit::new(1, UnitKind::Warrior, Faction::Blue, z0x, z0y, false),
            Unit::new(2, UnitKind::Warrior, Faction::Blue, z0x, z0y + 1, false),
            Unit::new(3, UnitKind::Warrior, Faction::Red, z0x, z0y - 1, false),
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
        let mut mgr = test_zones();
        mgr.zones[0].progress = 0.3;
        mgr.zones[0].state = ZoneState::Capturing(Faction::Blue);
        mgr.tick_capture(5.0);
        assert!((mgr.zones[0].progress - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn progress_clamped_to_range() {
        let mut mgr = test_zones();
        mgr.zones[0].blue_count = 9;
        mgr.zones[0].progress = 0.95;
        mgr.tick_capture(10.0);
        assert!((mgr.zones[0].progress - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn victory_timer_accumulates() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
        }
        assert!(mgr.tick_victory(60.0).is_none());
        assert!((mgr.victory_timer - 60.0).abs() < f32::EPSILON);
        assert_eq!(mgr.victory_candidate, Some(Faction::Blue));
    }

    #[test]
    fn victory_triggers_after_hold_time() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
        }
        assert!(mgr.tick_victory(VICTORY_HOLD_TIME + 1.0).is_some());
    }

    #[test]
    fn victory_timer_resets_on_loss() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Blue);
        }
        mgr.tick_victory(60.0);
        assert!(mgr.victory_timer > 0.0);

        mgr.zones[0].state = ZoneState::Neutral;
        mgr.tick_victory(1.0);
        assert!((mgr.victory_timer).abs() < f32::EPSILON);
        assert_eq!(mgr.victory_candidate, None);
    }

    #[test]
    fn victory_progress_fraction() {
        let mut mgr = test_zones();
        for zone in &mut mgr.zones {
            zone.state = ZoneState::Controlled(Faction::Red);
        }
        mgr.tick_victory(60.0);
        let progress = mgr.victory_progress();
        assert!(
            (progress - 0.5).abs() < 0.01,
            "Expected ~0.5, got {progress}"
        );
    }

    #[test]
    fn create_from_layout_creates_correct_count() {
        let layout = test_layout();
        let mgr = ZoneManager::create_from_layout(&layout);
        assert_eq!(mgr.zones.len(), layout.zone_centers.len());
    }
}
