use crate::config::GameConfig;
use crate::grid::{self, BORDER_SIZE, PLAYABLE_SIZE, TILE_SIZE};
use crate::mapgen::MapLayout;
use crate::unit::{Faction, Unit};

/// Settlement size tier: drives capture radius, garrison size and
/// (in mapgen) building counts. Cities are faction capitals.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SettlementTier {
    Hamlet,
    Village,
    Town,
    City,
}

impl SettlementTier {
    pub fn capture_radius(self) -> u32 {
        match self {
            SettlementTier::Hamlet => 5,
            SettlementTier::Village => 6,
            SettlementTier::Town => 7,
            SettlementTier::City => 8,
        }
    }

    /// War-score weight: what holding a settlement of this tier is worth.
    pub fn war_weight(self) -> u32 {
        match self {
            SettlementTier::Hamlet => 1,
            SettlementTier::Village => 2,
            SettlementTier::Town => 3,
            SettlementTier::City => 4,
        }
    }

    pub fn garrison_cap(self) -> u8 {
        match self {
            SettlementTier::Hamlet => 2,
            SettlementTier::Village => 4,
            SettlementTier::Town => 6,
            SettlementTier::City => 8,
        }
    }

    pub fn house_count(self, roll: u32) -> usize {
        match self {
            SettlementTier::Hamlet => 2,
            SettlementTier::Village => 3 + (roll % 2) as usize,
            SettlementTier::Town => 5 + (roll % 2) as usize,
            // City housing comes from the band generator at setup.
            SettlementTier::City => 0,
        }
    }

    pub fn production_count(self) -> usize {
        match self {
            SettlementTier::Hamlet | SettlementTier::Village => 1,
            SettlementTier::Town => 2,
            SettlementTier::City => 0, // band generator provides the full set
        }
    }
}

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
    /// Settlement size of this zone.
    pub tier: SettlementTier,
    /// Who holds the zone (Controlled).
    pub owner: Option<Faction>,
    /// Who is currently converting it (draining the owner or building
    /// ownership of a neutral zone).
    pub capturing: Option<Faction>,
    /// 0..1 — the owner's hold strength while owned, otherwise the
    /// capturer's progress. Draining a hold and building your own take
    /// the same time, preserving the old signed-scalar pacing.
    pub progress: f32,
    /// Units inside per army faction (ARMY_FACTIONS order).
    pub counts: [u32; 4],
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
            tier: SettlementTier::Village,
            owner: None,
            capturing: None,
            progress: 0.0,
            counts: [0; 4],
        }
    }

    pub fn count(&self, faction: Faction) -> u32 {
        faction.army_idx().map(|i| self.counts[i]).unwrap_or(0)
    }

    pub fn total_inside(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// Mark the zone fully held by `faction` (tests and scripted setups).
    pub fn set_controlled(&mut self, faction: Faction) {
        self.owner = Some(faction);
        self.capturing = None;
        self.progress = 1.0;
        self.state = ZoneState::Controlled(faction);
    }

    /// The faction a zone's tower fights for: the owner, else whoever
    /// has meaningful capture progress.
    pub fn effective_faction(&self) -> Option<Faction> {
        self.owner.or(if self.progress > 0.01 {
            self.capturing
        } else {
            None
        })
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

/// Manages all capture zones on the battlefield.
pub struct ZoneManager {
    pub zones: Vec<CaptureZone>,
    pub reinforcement_timer: f32,
    /// Tracks how long a faction has held all zones. Resets when control is lost.
    /// Blue base grid position (from layout).
    pub blue_base: (u32, u32),
    /// Red base grid position (from layout).
    pub red_base: (u32, u32),
    /// Zone indices that are "home" zones for Blue (always capturable).
    pub blue_home_zones: Vec<u8>,
    /// Zone indices that are "home" zones for Red (always capturable).
    pub red_home_zones: Vec<u8>,
    /// Adjacency list: connections[i] = zone indices connected to zone i.
    pub connections: Vec<Vec<u8>>,
    /// Zone each unit currently counts for (membership hysteresis: a unit
    /// keeps counting until it moves half a tile beyond the radius, so a
    /// soldier dancing on the rim cannot strobe the zone state).
    membership: std::collections::HashMap<u32, u8>,
}

impl ZoneManager {
    pub fn empty() -> Self {
        Self {
            zones: Vec::new(),
            reinforcement_timer: 0.0,
            blue_base: (BORDER_SIZE + 5, BORDER_SIZE + 5),
            red_base: (
                BORDER_SIZE + PLAYABLE_SIZE - 6,
                BORDER_SIZE + PLAYABLE_SIZE - 6,
            ),
            blue_home_zones: Vec::new(),
            red_home_zones: Vec::new(),
            connections: Vec::new(),
            membership: std::collections::HashMap::new(),
        }
    }

    /// Create zones from BSP-generated layout.
    pub fn create_from_layout(layout: &MapLayout, _zone_radius: u32) -> Self {
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
                let tier = layout
                    .settlements
                    .get(i)
                    .map(|sp| sp.tier)
                    .unwrap_or(SettlementTier::Village);
                let name = match tier {
                    SettlementTier::City => "Capital",
                    SettlementTier::Town => "Town",
                    _ => NAMES[i % NAMES.len()],
                };
                let mut z = CaptureZone::new(i as u8, name, gx, gy, tier.capture_radius());
                z.tier = tier;
                z
            })
            .collect();
        Self {
            zones,
            reinforcement_timer: 0.0,
            blue_base: layout.blue_base,
            red_base: layout.red_base,
            blue_home_zones: layout.blue_home_zones.clone(),
            red_home_zones: layout.red_home_zones.clone(),
            connections: layout.connections.clone(),
            membership: std::collections::HashMap::new(),
        }
    }

    /// Update unit counts for all zones.
    pub fn count_units(&mut self, units: &[Unit]) {
        for zone in &mut self.zones {
            zone.counts = [0; 4];
        }
        const EXIT_MARGIN: f32 = crate::grid::TILE_SIZE * 0.5;
        for unit in units {
            if !unit.alive {
                self.membership.remove(&unit.id);
                continue;
            }
            // Sticky membership: keep counting for the current zone until
            // clearly outside it, then re-evaluate.
            let sticky = self.membership.get(&unit.id).copied().and_then(|zi| {
                let z = self.zones.get(zi as usize)?;
                let dx = unit.x - z.center_wx;
                let dy = unit.y - z.center_wy;
                let keep = z.radius as f32 * crate::grid::TILE_SIZE + EXIT_MARGIN;
                (dx * dx + dy * dy <= keep * keep).then_some(zi)
            });
            let zone_idx = sticky.or_else(|| {
                self.zones
                    .iter()
                    .position(|z| z.contains_world(unit.x, unit.y))
                    .map(|i| i as u8)
            });
            match zone_idx {
                Some(zi) => {
                    self.membership.insert(unit.id, zi);
                    // Militia contests with swords, not with the circle.
                    if let Some(fi) = unit.faction.army_idx() {
                        self.zones[zi as usize].counts[fi] += 1;
                    }
                }
                None => {
                    self.membership.remove(&unit.id);
                }
            }
        }
    }

    /// Tick capture progress for all zones.
    pub fn tick_capture(&mut self, dt: f32, base_capture_time: f32, max_capture_multiplier: f32) {
        let rate_per_unit = 1.0 / base_capture_time;
        let max_rate = max_capture_multiplier * rate_per_unit;

        for zone in &mut self.zones {
            let total = zone.total_inside();
            if total == 0 {
                // No units present — state persists, no progress change
                continue;
            }

            // Majority capture: the strongest faction present converts the
            // zone at the rate of its advantage over EVERYONE else inside.
            // A tie for strongest freezes the zone.
            let (best_i, best_c) = zone
                .counts
                .iter()
                .copied()
                .enumerate()
                .max_by_key(|&(_, c)| c)
                .unwrap();
            let tie = zone
                .counts
                .iter()
                .enumerate()
                .any(|(i, &c)| i != best_i && c == best_c);
            let adv = best_c as i64 - (total - best_c) as i64;
            if tie || adv <= 0 {
                zone.state = ZoneState::Contested;
                continue;
            }
            let best_f = crate::unit::ARMY_FACTIONS[best_i];
            let rate = ((adv as f32).sqrt() * rate_per_unit).min(max_rate);
            let step = rate * dt;
            let crowd = zone.counts.iter().filter(|&&c| c > 0).count() > 1;

            match zone.owner {
                // The owner reinforces its hold back toward full.
                Some(o) if o == best_f => {
                    zone.progress = (zone.progress + step).min(1.0);
                    zone.capturing = None;
                    // A full hold reads Controlled even mid-melee.
                    zone.state = if zone.progress >= 1.0 {
                        ZoneState::Controlled(o)
                    } else if crowd {
                        ZoneState::Contested
                    } else {
                        ZoneState::Capturing(o)
                    };
                }
                // An attacker drains the owner's hold to zero first.
                Some(_) => {
                    zone.progress -= step;
                    zone.capturing = Some(best_f);
                    if zone.progress <= 0.0 {
                        zone.owner = None;
                        zone.progress = 0.0;
                    }
                    zone.state = if crowd {
                        ZoneState::Contested
                    } else {
                        ZoneState::Capturing(best_f)
                    };
                }
                // Neutral: drain a rival's partial claim, then build your own.
                None => {
                    if zone.capturing != Some(best_f) && zone.progress > 0.0 {
                        zone.progress -= step;
                        if zone.progress <= 0.0 {
                            zone.progress = 0.0;
                            zone.capturing = Some(best_f);
                        }
                    } else {
                        zone.capturing = Some(best_f);
                        zone.progress = (zone.progress + step).min(1.0);
                    }
                    // Completion beats the crowd: an overwhelming majority
                    // takes the zone with defenders still alive inside.
                    if zone.progress >= 1.0 {
                        zone.owner = Some(best_f);
                        zone.capturing = None;
                        zone.state = ZoneState::Controlled(best_f);
                    } else {
                        zone.state = if crowd {
                            ZoneState::Contested
                        } else {
                            ZoneState::Capturing(best_f)
                        };
                    }
                }
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

    /// Score all zones for a faction, return all as (world_x, world_y, score) sorted desc.
    pub fn score_all_zones(&self, faction: Faction, cfg: &GameConfig) -> Vec<(f32, f32, f32)> {
        if self.zones.is_empty() {
            return Vec::new();
        }

        // Settlement leader (the RIVAL with the highest war score): its
        // holdings score higher — the pack turns on whoever is winning.
        let leader: Option<Faction> = crate::unit::ARMY_FACTIONS
            .iter()
            .copied()
            .filter(|&f| f != faction)
            .map(|f| (f, self.war_score(f)))
            .filter(|&(_, c)| c > 0)
            .max_by_key(|&(_, c)| c)
            .map(|(f, _)| f);

        let (own_x, own_y) = match faction {
            Faction::Blue => (self.blue_base.0 as f32, self.blue_base.1 as f32),
            _ => (self.red_base.0 as f32, self.red_base.1 as f32),
        };

        let max_dist_sq = self
            .zones
            .iter()
            .map(|z| {
                let dx = z.center_gx as f32 - own_x;
                let dy = z.center_gy as f32 - own_y;
                dx * dx + dy * dy
            })
            .fold(1.0_f32, f32::max);

        let mut any_controlled = false;

        let mut scored: Vec<(f32, f32, f32)> = self
            .zones
            .iter()
            .map(|z| {
                let controlled_by_us = z.state == ZoneState::Controlled(faction);
                let progress_for_us = match (z.owner, z.capturing) {
                    (Some(o), _) if o == faction => z.progress,
                    (Some(_), _) => -z.progress,
                    (None, Some(c)) if c == faction => z.progress,
                    (None, Some(_)) => -z.progress,
                    (None, None) => 0.0,
                };
                let enemy_count = z.total_inside() - z.count(faction);

                // Distance tiebreaker (0..1, closer = lower)
                let dx = z.center_gx as f32 - own_x;
                let dy = z.center_gy as f32 - own_y;
                let norm_dist = (dx * dx + dy * dy) / max_dist_sq;

                // 3-tier scoring: defend > attack > hold
                let score = if controlled_by_us {
                    any_controlled = true;
                    if enemy_count > 0 || progress_for_us < 0.9 {
                        // Tier 1: Under attack — defend urgently
                        200.0 + (1.0 - progress_for_us) * 50.0 - norm_dist * 15.0
                    } else {
                        // Tier 3: Secure — low priority hold
                        10.0 - norm_dist * 5.0
                    }
                } else {
                    // Tier 2: Not ours — attack, prefer momentum + nearness
                    let mut sc = 100.0 + progress_for_us.max(0.0) * 30.0 - norm_dist * 15.0;
                    if z.owner.is_some() && z.owner == leader {
                        sc += cfg.ai_w_leader;
                    }
                    if self.connections.get(z.id as usize).is_some_and(|adj| {
                        adj.iter().any(|&n| {
                            self.zones
                                .get(n as usize)
                                .is_some_and(|nz| nz.state == ZoneState::Controlled(faction))
                        })
                    }) {
                        sc += cfg.ai_w_neighbor;
                    }
                    sc
                };

                (z.center_wx, z.center_wy, score)
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Focus mechanic: when holding 0 zones, boost the top target so units
        // concentrate force instead of spreading thin across all zones.
        if !any_controlled {
            if let Some(top) = scored.first_mut() {
                top.2 += 50.0;
            }
        }

        scored
    }

    pub fn controlled_count(&self, faction: Faction) -> usize {
        self.zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .count()
    }

    /// Tier-weighted settlements held — the war score shown on the HUD
    /// and read by the planner's leader weighting.
    pub fn war_score(&self, faction: Faction) -> u32 {
        self.zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction))
            .map(|z| z.tier.war_weight())
            .sum()
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
                (b + 35, b + 35), // B1
                (b + 35, b + 93), // B2
                (b + 64, b + 35), // C1
                (b + 64, b + 64), // C2
                (b + 64, b + 93), // C3
                (b + 93, b + 35), // R1
                (b + 93, b + 93), // R2
            ],
            blue_gather: (b + 5, b + 5),
            red_gather: (b + PLAYABLE_SIZE - 6, b + PLAYABLE_SIZE - 6),
            blue_home_zones: vec![0, 1],
            red_home_zones: vec![5, 6],
            connections: vec![
                vec![1, 2, 3],
                vec![0, 3, 4],
                vec![0, 3, 5],
                vec![0, 1, 2, 4, 5, 6],
                vec![1, 3, 6],
                vec![2, 3, 6],
                vec![3, 4, 5],
            ],
            settlements: Vec::new(),
            extra_bases: Vec::new(),
        }
    }

    fn test_zones() -> ZoneManager {
        ZoneManager::create_from_layout(&test_layout(), 4)
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
        mgr.zones[1].counts[0] = 1;
        mgr.zones[1].counts[1] = 0;
        mgr.tick_capture(4.0, 8.0, 3.0);
        assert!(mgr.zones[1].progress > 0.4 && mgr.zones[1].progress < 0.6);
        assert!(matches!(
            mgr.zones[1].state,
            ZoneState::Capturing(Faction::Blue)
        ));
    }

    #[test]
    fn neutral_zone_captures_with_red() {
        let mut mgr = test_zones();
        mgr.zones[1].counts[0] = 0;
        mgr.zones[1].counts[1] = 1;
        mgr.tick_capture(4.0, 8.0, 3.0);
        assert!(mgr.zones[1].progress > 0.4 && mgr.zones[1].progress < 0.6);
        assert_eq!(mgr.zones[1].capturing, Some(Faction::Red));
        assert!(matches!(
            mgr.zones[1].state,
            ZoneState::Capturing(Faction::Red)
        ));
    }

    #[test]
    fn third_faction_captures_and_drains() {
        let mut mgr = test_zones();
        // Yellow takes a neutral zone
        mgr.zones[0].counts[2] = 4;
        for _ in 0..200 {
            mgr.tick_capture(0.1, 8.0, 3.0);
        }
        assert_eq!(mgr.zones[0].owner, Some(Faction::Yellow));
        assert_eq!(mgr.zones[0].state, ZoneState::Controlled(Faction::Yellow));

        // Purple must drain Yellow's hold before building its own
        mgr.zones[0].counts = [0, 0, 0, 3];
        mgr.tick_capture(1.0, 8.0, 3.0);
        assert_eq!(mgr.zones[0].owner, Some(Faction::Yellow));
        assert!(mgr.zones[0].progress < 1.0);
        assert_eq!(mgr.zones[0].capturing, Some(Faction::Purple));
        for _ in 0..200 {
            mgr.tick_capture(0.1, 8.0, 3.0);
        }
        assert_eq!(mgr.zones[0].owner, Some(Faction::Purple));
    }

    #[test]
    fn strongest_of_three_converts_at_advantage_rate() {
        let mut mgr = test_zones();
        // Blue 5, Red 2, Yellow 1 — Blue advantage is 5 - 3 = 2
        mgr.zones[0].counts = [5, 2, 1, 0];
        mgr.tick_capture(1.0, 8.0, 3.0);
        assert_eq!(mgr.zones[0].capturing, Some(Faction::Blue));
        assert_eq!(mgr.zones[0].state, ZoneState::Contested);
        let expected = (2.0f32).sqrt() / 8.0;
        assert!((mgr.zones[0].progress - expected).abs() < 1e-4);
    }

    #[test]
    fn equal_contest_freezes_progress() {
        let mut mgr = test_zones();
        mgr.zones[1].progress = 0.5;
        mgr.zones[1].counts[0] = 3;
        mgr.zones[1].counts[1] = 3;
        mgr.tick_capture(1.0, 8.0, 3.0);
        assert!((mgr.zones[1].progress - 0.5).abs() < f32::EPSILON);
        assert_eq!(mgr.zones[1].state, ZoneState::Contested);
    }

    #[test]
    fn majority_progresses_contested_capture() {
        let mut mgr = test_zones();
        mgr.zones[1].progress = 0.0;
        mgr.zones[1].counts[0] = 5;
        mgr.zones[1].counts[1] = 2;
        mgr.tick_capture(1.0, 8.0, 3.0);
        assert!(
            mgr.zones[1].progress > 0.0,
            "attacker majority must advance capture"
        );
        assert_eq!(mgr.zones[1].state, ZoneState::Contested);

        // Same margin, same rate: 5v2 progresses like 3v0
        let mut solo = test_zones();
        solo.zones[1].counts[0] = 3;
        solo.tick_capture(1.0, 8.0, 3.0);
        assert!((mgr.zones[1].progress - solo.zones[1].progress).abs() < f32::EPSILON);
    }

    #[test]
    fn overwhelm_completes_capture_despite_defenders() {
        let mut mgr = test_zones();
        mgr.zones[1].capturing = Some(Faction::Blue);
        mgr.zones[1].progress = 0.95;
        mgr.zones[1].counts[0] = 8;
        mgr.zones[1].counts[1] = 2;
        mgr.tick_capture(1.0, 8.0, 3.0);
        assert_eq!(mgr.zones[1].state, ZoneState::Controlled(Faction::Blue));
        assert_eq!(mgr.zones[1].owner, Some(Faction::Blue));
    }

    #[test]
    fn minority_defenders_cannot_hold_forever() {
        let mut mgr = test_zones();
        mgr.zones[1].set_controlled(Faction::Red);
        mgr.zones[1].counts[0] = 6;
        mgr.zones[1].counts[1] = 1;
        for _ in 0..600 {
            mgr.tick_capture(0.1, 8.0, 3.0);
        }
        assert_eq!(mgr.zones[1].state, ZoneState::Controlled(Faction::Blue));
    }

    #[test]
    fn more_units_capture_faster() {
        let mut mgr1 = test_zones();
        let mut mgr2 = test_zones();
        mgr1.zones[0].counts[0] = 1;
        mgr2.zones[0].counts[0] = 4;
        mgr1.tick_capture(1.0, 8.0, 3.0);
        mgr2.tick_capture(1.0, 8.0, 3.0);
        assert!(mgr2.zones[0].progress > mgr1.zones[0].progress);
    }

    #[test]
    fn fully_captured_becomes_controlled() {
        let mut mgr = test_zones();
        mgr.zones[0].counts[0] = 4;
        for _ in 0..50 {
            mgr.tick_capture(0.1, 8.0, 3.0);
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
        // Nearest uncontrolled zone to Blue base among remaining neutral zones
        let target = mgr.best_target_zone(Faction::Blue).unwrap();
        assert!(
            target.id != 0,
            "Should not select the already-controlled zone"
        );
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
        assert_eq!(mgr.zones[0].counts[0], 2);
        assert_eq!(mgr.zones[0].counts[1], 1);
        // Unit 4 should not be in any zone
        for zone in &mgr.zones[1..] {
            assert_eq!(zone.counts[0], 0);
        }
    }

    #[test]
    fn no_progress_when_empty() {
        let mut mgr = test_zones();
        mgr.zones[0].progress = 0.3;
        mgr.zones[0].state = ZoneState::Capturing(Faction::Blue);
        mgr.tick_capture(5.0, 8.0, 3.0);
        assert!((mgr.zones[0].progress - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn progress_clamped_to_range() {
        let mut mgr = test_zones();
        mgr.zones[0].counts[0] = 9;
        mgr.zones[0].capturing = Some(Faction::Blue);
        mgr.zones[0].progress = 0.95;
        mgr.tick_capture(10.0, 8.0, 3.0);
        assert!((mgr.zones[0].progress - 1.0).abs() < f32::EPSILON);
        assert_eq!(mgr.zones[0].owner, Some(Faction::Blue));
    }

    #[test]
    fn create_from_layout_creates_correct_count() {
        let layout = test_layout();
        let mgr = ZoneManager::create_from_layout(&layout, 4);
        assert_eq!(mgr.zones.len(), layout.zone_centers.len());
    }

    #[test]
    fn war_score_weights_tiers() {
        let mut mgr = ZoneManager::empty();
        for (i, tier) in [
            SettlementTier::City,
            SettlementTier::Town,
            SettlementTier::Village,
            SettlementTier::Hamlet,
        ]
        .iter()
        .enumerate()
        {
            let mut z = CaptureZone::new(i as u8, "Z", 10 + i as u32 * 20, 10, 6);
            z.tier = *tier;
            z.set_controlled(Faction::Blue);
            mgr.zones.push(z);
        }
        assert_eq!(mgr.war_score(Faction::Blue), 4 + 3 + 2 + 1);
        assert_eq!(mgr.war_score(Faction::Red), 0);
    }
}
