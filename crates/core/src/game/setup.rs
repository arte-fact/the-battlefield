use super::*;

impl Game {
    /// Wave composition — 21 units interleaved so all buildings produce continuously.
    /// 7 Warriors, 4 Lancers, 8 Archers, 2 Monks.
    const WAVE: &'static [UnitKind] = &[
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Monk,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Monk,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Warrior,
        UnitKind::Archer,
        UnitKind::Archer,
    ];

    pub fn spawn_unit(
        &mut self,
        kind: UnitKind,
        faction: Faction,
        x: u32,
        y: u32,
        is_player: bool,
    ) -> UnitId {
        // Find nearest passable tile via spiral search if requested tile is blocked
        let (sx, sy) = if self.grid.in_bounds(x as i32, y as i32) && self.grid.is_passable(x, y) {
            (x, y)
        } else {
            self.find_nearest_passable(x, y).unwrap_or((x, y))
        };

        let id = self.next_unit_id;
        self.next_unit_id += 1;
        let mut unit = Unit::new(id, kind, faction, sx, sy, is_player);
        // Apply runtime combat stats from config
        unit.stats = kind.stats_from_config(&self.config);
        unit.hp = unit.stats.max_hp;
        // Stagger AI initial attack cooldowns to prevent all acting on the same frame
        if !is_player {
            unit.attack_cooldown = (id as f32 * 0.05) % 0.3;
        }
        self.units.push(unit);
        id
    }

    /// Spiral search for the nearest passable tile around (cx, cy).
    pub(super) fn find_nearest_passable(&self, cx: u32, cy: u32) -> Option<(u32, u32)> {
        for radius in 1..16i32 {
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    if dx.abs() != radius && dy.abs() != radius {
                        continue; // only check the ring perimeter
                    }
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if self.grid.in_bounds(nx, ny) && self.grid.is_passable(nx as u32, ny as u32) {
                        return Some((nx as u32, ny as u32));
                    }
                }
            }
        }
        None
    }

    /// Tick capture zone progress based on unit positions.
    pub fn tick_zones(&mut self, dt: f32) {
        // Snapshot per-zone state before tick
        let states_before: Vec<ZoneState> =
            self.zone_manager.zones.iter().map(|z| z.state).collect();

        self.zone_manager.count_units(&self.units);
        self.zone_manager.tick_capture(
            dt,
            self.config.base_capture_time,
            self.config.max_capture_multiplier,
        );

        // Collect zone state changes, then apply reputation
        let zone_changes: Vec<_> = self
            .zone_manager
            .zones
            .iter()
            .enumerate()
            .filter_map(|(i, zone)| {
                let before = states_before[i];
                let after = zone.state;
                if before == after {
                    return None;
                }
                let in_fov = self.is_tile_in_fov(zone.center_gx, zone.center_gy);
                Some((before, after, in_fov, zone.center_wx, zone.center_wy))
            })
            .collect();

        for (before, after, in_fov, zx, zy) in zone_changes {
            if before != ZoneState::Controlled(Faction::Blue)
                && after == ZoneState::Controlled(Faction::Blue)
            {
                self.on_zone_captured(in_fov, zx, zy);
            }
            if before == ZoneState::Controlled(Faction::Blue)
                && after != ZoneState::Controlled(Faction::Blue)
            {
                self.on_zone_lost(in_fov, zx, zy);
            }
            if before == ZoneState::Controlled(Faction::Red)
                && after != ZoneState::Controlled(Faction::Red)
            {
                self.on_zone_decaptured(in_fov, zx, zy);
            }
        }

        if self.winner.is_none() {
            let hold = self.config.victory_hold_time;
            // A pool below 1.0 cannot field a unit — effectively exhausted
            let exhausted = self.manpower[0] < 1.0 && self.manpower[1] < 1.0;
            let won = if exhausted {
                self.zone_manager.tick_victory_majority(dt, hold)
            } else {
                self.zone_manager.tick_victory(dt, hold)
            };
            if let Some(faction) = won {
                self.winner = Some(faction);
            }
        }

        self.tick_manpower_bleed(dt);
        self.check_annihilation();
    }

    /// Conquest bleed: controlling a majority of zones drains the enemy pool,
    /// scaling with each zone at or above the threshold.
    fn tick_manpower_bleed(&mut self, dt: f32) {
        let threshold = self.config.bleed_zone_threshold;
        if threshold == 0 {
            return;
        }
        for (fi, faction) in [(0usize, Faction::Blue), (1, Faction::Red)] {
            let controlled = self
                .zone_manager
                .zones
                .iter()
                .filter(|z| z.state == ZoneState::Controlled(faction))
                .count();
            if controlled >= threshold {
                let extra_zones = (controlled - threshold + 1) as f32;
                let drain = extra_zones * self.config.bleed_per_extra_zone * dt;
                let enemy = 1 - fi;
                self.manpower[enemy] = (self.manpower[enemy] - drain).max(0.0);
            }
        }
    }

    /// True while `faction`'s pool is actively draining from enemy zone
    /// majority (for HUD warning tint).
    pub fn manpower_bleeding(&self, faction: Faction) -> bool {
        let threshold = self.config.bleed_zone_threshold;
        if threshold == 0 {
            return false;
        }
        let fi = if faction == Faction::Blue { 0 } else { 1 };
        if self.manpower[fi] <= 0.0 {
            return false;
        }
        self.zone_manager
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(faction.enemy()))
            .count()
            >= threshold
    }

    /// Annihilation defeat: a faction with no manpower left and no living
    /// units loses the battle.
    fn check_annihilation(&mut self) {
        if self.winner.is_some() {
            return;
        }
        for (fi, faction) in [(0usize, Faction::Blue), (1, Faction::Red)] {
            if self.manpower[fi] < 1.0
                && !self.units.iter().any(|u| u.alive && u.faction == faction)
            {
                self.winner = Some(faction.enemy());
                return;
            }
        }
    }

    /// Target composition shares (from WAVE: 7 Warriors, 4 Lancers, 8 Archers, 2 Monks).
    const WAVE_SHARES: [(UnitKind, f32); 4] = [
        (UnitKind::Warrior, 7.0),
        (UnitKind::Lancer, 4.0),
        (UnitKind::Archer, 8.0),
        (UnitKind::Monk, 2.0),
    ];

    /// Build a reinforcement wave that restores the target composition:
    /// each slot goes to the kind furthest below its share of the army.
    /// Prevents survivor drift (e.g. fleeing monks accumulating while
    /// fighters die and get replaced).
    fn build_wave(&self, faction: Faction, wave_size: usize) -> Vec<UnitKind> {
        let share_total: f32 = Self::WAVE_SHARES.iter().map(|&(_, s)| s).sum();
        let mut counts = [0.0f32; 4];
        let mut total = 0.0f32;
        for u in &self.units {
            if u.alive && u.faction == faction {
                let i = Self::WAVE_SHARES
                    .iter()
                    .position(|&(k, _)| k == u.kind)
                    .unwrap();
                counts[i] += 1.0;
                total += 1.0;
            }
        }

        let mut queue = Vec::with_capacity(wave_size);
        for _ in 0..wave_size {
            let mut best = 0;
            let mut best_deficit = f32::MIN;
            for (i, &(_, share)) in Self::WAVE_SHARES.iter().enumerate() {
                let deficit = share / share_total * (total + 1.0) - counts[i];
                if deficit > best_deficit {
                    best_deficit = deficit;
                    best = i;
                }
            }
            queue.push(Self::WAVE_SHARES[best].0);
            counts[best] += 1.0;
            total += 1.0;
        }
        queue
    }

    /// Spawn units one-by-one from the queue at the rally point (base center).
    /// When a wave is complete, release all rallying units to march.
    pub fn tick_production(&mut self, dt: f32) {
        let factions = [(0usize, Faction::Blue), (1, Faction::Red)];
        for &(fi, faction) in &factions {
            // Refill queue if empty and under unit cap.
            // Larger wave when holding 0 zones (desperate comeback).
            if self.spawn_queue[fi].is_empty() {
                let alive_count = self
                    .units
                    .iter()
                    .filter(|u| u.alive && u.faction == faction)
                    .count();
                if alive_count < self.config.max_units_per_faction {
                    let controlled: usize = self
                        .zone_manager
                        .zones
                        .iter()
                        .filter(|z| z.state == crate::zone::ZoneState::Controlled(faction))
                        .count();
                    let zone_count = self.zone_manager.zones.len();
                    let slots = self
                        .config
                        .max_units_per_faction
                        .saturating_sub(alive_count);
                    // Double wave size when holding no zones (fill army faster)
                    let max_wave = if controlled > 0 {
                        Self::WAVE.len()
                    } else {
                        Self::WAVE.len() * 2
                    };
                    let wave_size = slots.min(max_wave).min(self.manpower[fi] as usize);
                    self.spawn_queue[fi] = self.build_wave(faction, wave_size);
                    self.spawn_timer[fi] = 0.0;
                    // Skip rally when dominating — reinforcements march out immediately
                    self.skip_rally[fi] = zone_count > 0 && controlled == zone_count;
                }
            }

            if self.spawn_queue[fi].is_empty() {
                continue;
            }

            self.spawn_timer[fi] += dt;
            let interval = self.config.spawn_interval;
            if self.spawn_timer[fi] >= interval {
                self.spawn_timer[fi] -= interval;

                // Bleed can drain the pool mid-wave — cut the wave short and
                // release any rallying units so a partial wave still marches.
                if self.manpower[fi] < 1.0 {
                    self.spawn_queue[fi].clear();
                    for u in &mut self.units {
                        if u.alive && u.faction == faction && u.rally_hold {
                            u.rally_hold = false;
                        }
                    }
                    continue;
                }

                let kind = self.spawn_queue[fi].remove(0);
                self.manpower[fi] -= 1.0;
                // Spawn at the production building that trains this unit kind
                let (sx, sy) = self
                    .buildings
                    .iter()
                    .find(|b| b.faction == faction && b.produces == Some(kind))
                    .map(|b| (b.grid_x, b.grid_y))
                    .unwrap_or(match faction {
                        Faction::Blue => self.blue_gather,
                        _ => self.red_gather,
                    });
                let id = self.spawn_unit(kind, faction, sx, sy, false);
                // Rally hold — unit waits at base until wave is complete.
                // Skip when dominating (all zones held) — just reinforce.
                if !self.skip_rally[fi] {
                    if let Some(u) = self.units.iter_mut().find(|u| u.id == id) {
                        u.rally_hold = true;
                    }
                }

                // Wave complete — release all rallying units
                if self.spawn_queue[fi].is_empty() {
                    for u in &mut self.units {
                        if u.alive && u.faction == faction && u.rally_hold {
                            u.rally_hold = false;
                        }
                    }
                }
            }
        }
    }

    pub fn setup_demo_battle_with_seed(&mut self, seed: u32) {
        let (gen_grid, layout) = mapgen::generate_battlefield(seed, self.config.playable_size);
        self.grid = gen_grid;
        let tiles = (self.grid.width * self.grid.height) as usize;
        self.visible = vec![false; tiles];
        self.revealed = vec![true; tiles];

        // Fresh manpower pools (config may have been live-tuned since Game::new)
        self.manpower = [self.config.manpower_start; 2];

        let (blue_cx, blue_cy) = layout.blue_base;
        let (red_cx, red_cy) = layout.red_base;

        // Each faction's objective is the other faction's base
        self.blue_objective = grid::grid_to_world(red_cx, red_cy);
        self.red_objective = grid::grid_to_world(blue_cx, blue_cy);

        // Create capture zones from BSP layout
        self.zone_manager = ZoneManager::create_from_layout(&layout, self.config.zone_radius);

        // Store gather points for unit rallying
        self.blue_gather = layout.blue_gather;
        self.red_gather = layout.red_gather;

        // Bases face each other; every band rotates with this vector.
        let dxf = red_cx as f32 - blue_cx as f32;
        let dyf = red_cy as f32 - blue_cy as f32;
        let len = (dxf * dxf + dyf * dyf).sqrt().max(1.0);
        let blue_facing = (dxf / len, dyf / len);
        let red_facing = (-blue_facing.0, -blue_facing.1);

        // Procedurally generate all base buildings (castle, towers, production, houses)
        let mut buildings =
            building::generate_base_buildings(Faction::Blue, blue_cx, blue_cy, seed, blue_facing);
        buildings.extend(building::generate_base_buildings(
            Faction::Red,
            red_cx,
            red_cy,
            seed.wrapping_add(0xBEEF),
            red_facing,
        ));
        // Place defense towers at capture zone centers
        for zone in &self.zone_manager.zones {
            buildings.push(building::BaseBuilding {
                kind: building::BuildingKind::DefenseTower,
                faction: Faction::Blue, // updated dynamically based on zone state
                grid_x: zone.center_gx,
                grid_y: zone.center_gy,
                attack_cooldown: 0.0,
                zone_id: Some(zone.id),
                produces: None,
                house_variant: 0,
            });
        }
        for b in &buildings {
            for &(dx, dy) in b.kind.footprint_offsets() {
                let fx = b.grid_x as i32 + dx;
                let fy = b.grid_y as i32 + dy;
                if self.grid.in_bounds(fx, fy) {
                    self.grid.mark_building(fx as u32, fy as u32);
                }
            }
        }
        // Paint road/dirt 1 tile around all building footprints
        Self::paint_road_around_buildings(&mut self.grid, &buildings);
        self.buildings = buildings;

        // Spawn ambient sheep in rear pasture of each base
        self.spawn_base_sheep(blue_cx, blue_cy, blue_facing, seed);
        self.spawn_base_sheep(red_cx, red_cy, red_facing, seed.wrapping_add(7919));

        // Spawn one pawn worker per house
        let mut pawn_seed = seed.wrapping_add(0xCAFE);
        for b in &self.buildings {
            if b.kind == building::BuildingKind::House {
                let (wx, wy) = grid::grid_to_world(b.grid_x, b.grid_y);
                pawn_seed = pawn_seed.wrapping_mul(1103515245).wrapping_add(12345);
                self.pawns.push(Pawn::new(wx, wy, b.faction, pawn_seed));
            }
        }

        // Spawn player at base center
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_cx, blue_cy, true);

        // Spawn starting armies around each base — same composition as a wave, no rally hold
        for (faction, base_cx, base_cy) in [
            (Faction::Blue, blue_cx, blue_cy),
            (Faction::Red, red_cx, red_cy),
        ] {
            for (i, &kind) in Self::WAVE.iter().enumerate() {
                // Spread units in a grid around the base center
                let col = (i % 5) as u32;
                let row = (i / 5) as u32;
                let sx = base_cx.saturating_sub(2) + col;
                let sy = base_cy.saturating_sub(1) + row;
                self.spawn_unit(kind, faction, sx, sy, false);
            }
        }

        // Camera starts centered on player (base center)
        let (cx, cy) = grid::grid_to_world(blue_cx, blue_cy);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = self.camera.ideal_zoom();

        // Pre-compute caches
        self.grid.recompute_caches();
        self.compute_water_adjacency();
        self.compute_fov();
    }

    /// Spawn 10 sheep at random positions in the rear pasture (behind the houses).
    /// `fs` = front sign: 1 for Blue (front=+Y), -1 for Red (front=-Y).
    /// Paint a 1-tile road border around the bounding box of each building footprint.
    fn paint_road_around_buildings(grid: &mut Grid, buildings: &[building::BaseBuilding]) {
        for b in buildings {
            // Compute bounding box of all footprint cells (absolute coords)
            let offsets = b.kind.footprint_offsets();
            if offsets.is_empty() {
                continue;
            }
            let mut min_x = i32::MAX;
            let mut max_x = i32::MIN;
            let mut min_y = i32::MAX;
            let mut max_y = i32::MIN;
            for &(dx, dy) in offsets {
                let ax = b.grid_x as i32 + dx;
                let ay = b.grid_y as i32 + dy;
                min_x = min_x.min(ax);
                max_x = max_x.max(ax);
                min_y = min_y.min(ay);
                max_y = max_y.max(ay);
            }
            // Expand by 2 tiles in all directions
            let border = 2;
            for ry in (min_y - border)..=(max_y + border) {
                for rx in (min_x - border)..=(max_x + border) {
                    if !grid.in_bounds(rx, ry) {
                        continue;
                    }
                    let ux = rx as u32;
                    let uy = ry as u32;
                    let tile = grid.get(ux, uy);
                    if (tile == TileKind::Grass || tile == TileKind::Forest)
                        && grid.elevation(ux, uy) == 0
                    {
                        grid.set(ux, uy, TileKind::Road);
                        grid.set_decoration(ux, uy, None);
                    }
                }
            }
        }
    }

    fn spawn_base_sheep(&mut self, cx: u32, cy: u32, facing: (f32, f32), base_seed: u32) {
        let mut seed = if base_seed == 0 { 1 } else { base_seed };
        let mut next = || -> u32 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let (fx, fy) = facing;
        let (px, py) = (-fy, fx);
        for _ in 0..10 {
            // Rear pasture: lateral ∈ [-4,4], 11-14 tiles behind the center.
            let lat = (next() % 9) as f32 - 4.0;
            let rear = (next() % 4) as f32 + 11.0;
            let gx = (cx as f32 - fx * rear + px * lat).round().max(0.0) as u32;
            let gy = (cy as f32 - fy * rear + py * lat).round().max(0.0) as u32;
            if !self.grid.is_passable(gx, gy) {
                continue;
            }
            let (wx, wy) = grid::grid_to_world(gx, gy);
            self.sheep.push(Sheep::new(wx, wy, next()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_zones_updates_capture_progress() {
        use crate::mapgen::MapLayout;
        let mut game = Game::new(960.0, 640.0);
        let layout = MapLayout {
            blue_base: (21, 21),
            red_base: (138, 138),
            zone_centers: vec![(50, 50), (80, 80), (110, 110)],
            blue_gather: (21, 21),
            red_gather: (138, 138),
            blue_home_zones: vec![0],
            red_home_zones: vec![2],
            connections: vec![vec![1], vec![0, 2], vec![1]],
            villages: Vec::new(),
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
        let z0gx = game.zone_manager.zones[0].center_gx;
        let z0gy = game.zone_manager.zones[0].center_gy;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, z0gx, z0gy, false);
        game.tick_zones(4.0);
        assert!(
            game.zone_manager.zones[0].progress > 0.0,
            "Zone progress should advance with a Blue unit inside"
        );
    }

    #[test]
    fn production_spawns_units_when_under_cap() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);

        // Tick production for ~25s to trigger a reinforcement wave (interval=20s)
        for _ in 0..250 {
            game.tick_production(0.1);
        }

        let blue_units = game
            .units
            .iter()
            .filter(|u| u.alive && u.faction == Faction::Blue)
            .count();
        assert!(
            blue_units > 1,
            "Should have spawned reinforcement wave, got {blue_units}"
        );
    }

    fn count_alive(game: &Game, faction: Faction) -> usize {
        game.units
            .iter()
            .filter(|u| u.alive && u.faction == faction)
            .count()
    }

    #[test]
    fn reinforcement_spawns_cost_manpower() {
        let mut game = Game::new(960.0, 640.0);
        let start = game.manpower[0];
        for _ in 0..250 {
            game.tick_production(0.1);
        }
        let spawned = count_alive(&game, Faction::Blue);
        assert!(spawned > 0, "Expected reinforcements to spawn");
        assert_eq!(
            game.manpower[0],
            start - spawned as f32,
            "Each reinforcement should cost 1 manpower"
        );
    }

    #[test]
    fn production_stops_when_manpower_exhausted() {
        let mut game = Game::new(960.0, 640.0);
        game.manpower[0] = 0.0;
        for _ in 0..250 {
            game.tick_production(0.1);
        }
        assert_eq!(
            count_alive(&game, Faction::Blue),
            0,
            "No reinforcements should spawn with an empty pool"
        );
    }

    #[test]
    fn wave_capped_by_remaining_manpower() {
        let mut game = Game::new(960.0, 640.0);
        game.manpower[0] = 3.0;
        for _ in 0..1000 {
            game.tick_production(0.1);
        }
        assert_eq!(
            count_alive(&game, Faction::Blue),
            3,
            "Only 3 reinforcements should spawn with 3 manpower"
        );
        assert_eq!(game.manpower[0], 0.0);
    }

    #[test]
    fn partial_wave_releases_rally_hold() {
        let mut game = Game::new(960.0, 640.0);
        game.manpower[0] = 3.0;
        for _ in 0..1000 {
            game.tick_production(0.1);
        }
        assert!(
            game.units
                .iter()
                .filter(|u| u.alive && u.faction == Faction::Blue)
                .all(|u| !u.rally_hold),
            "A wave cut short by manpower must still release its rally hold"
        );
    }

    /// Game with a 3-zone layout and a bleed threshold of 2 (majority of 3).
    fn game_with_three_zones() -> Game {
        let mut game = Game::new(960.0, 640.0);
        game.config.bleed_zone_threshold = 2;
        game.config.bleed_per_extra_zone = 1.0;
        let layout = crate::mapgen::MapLayout {
            blue_base: (20, 20),
            red_base: (139, 139),
            zone_centers: vec![(50, 50), (80, 80), (110, 110)],
            blue_gather: (21, 21),
            red_gather: (138, 138),
            blue_home_zones: vec![0],
            red_home_zones: vec![2],
            connections: vec![vec![1], vec![0, 2], vec![1]],
            villages: Vec::new(),
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
        game
    }

    #[test]
    fn zone_majority_bleeds_enemy_manpower() {
        let mut game = game_with_three_zones();
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);
        game.manpower = [50.0, 50.0];
        game.tick_zones(2.0);
        // 2 zones at threshold 2 → 1 "extra" zone → 1.0/s drain on Red for 2s
        assert_eq!(game.manpower[1], 48.0, "Red pool should bleed");
        assert_eq!(game.manpower[0], 50.0, "Blue pool should be untouched");
    }

    #[test]
    fn bleed_scales_with_zones_above_threshold() {
        let mut game = game_with_three_zones();
        for z in &mut game.zone_manager.zones {
            z.state = ZoneState::Controlled(Faction::Red);
        }
        game.manpower = [50.0, 50.0];
        game.tick_zones(1.0);
        // 3 zones at threshold 2 → 2 extra zones → 2.0/s drain on Blue
        assert_eq!(game.manpower[0], 48.0);
    }

    #[test]
    fn no_bleed_below_majority() {
        let mut game = game_with_three_zones();
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.manpower = [50.0, 50.0];
        game.tick_zones(2.0);
        assert_eq!(game.manpower, [50.0, 50.0]);
    }

    #[test]
    fn bleed_clamps_at_zero() {
        let mut game = game_with_three_zones();
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);
        game.manpower = [50.0, 0.5];
        game.tick_zones(2.0);
        assert_eq!(game.manpower[1], 0.0);
    }

    #[test]
    fn annihilation_defeats_faction_with_no_pool_and_no_army() {
        let mut game = game_with_three_zones();
        game.manpower = [50.0, 0.0];
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // No Red units at all
        game.tick_zones(0.1);
        assert_eq!(game.winner, Some(Faction::Blue));
    }

    #[test]
    fn no_annihilation_while_army_lives_or_pool_remains() {
        let mut game = game_with_three_zones();
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);

        // Pool empty but army alive → no winner
        game.manpower = [50.0, 0.0];
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.tick_zones(0.1);
        assert_eq!(game.winner, None);

        // Army dead but pool remains → no winner
        game.manpower = [50.0, 10.0];
        for u in &mut game.units {
            if u.faction == Faction::Red {
                u.alive = false;
            }
        }
        game.tick_zones(0.1);
        assert_eq!(game.winner, None);
    }

    #[test]
    fn production_restores_composition_instead_of_cycling() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        for i in 0..12 {
            game.spawn_unit(UnitKind::Monk, Faction::Blue, 6 + i % 3, 5 + i / 3, false);
        }
        let monks_before = game
            .units
            .iter()
            .filter(|u| u.alive && u.kind == UnitKind::Monk && u.faction == Faction::Blue)
            .count();

        for _ in 0..600 {
            game.tick_production(0.1);
        }

        let monks_after = game
            .units
            .iter()
            .filter(|u| u.alive && u.kind == UnitKind::Monk && u.faction == Faction::Blue)
            .count();
        assert_eq!(
            monks_after, monks_before,
            "monk-saturated army must not produce more monks"
        );
        let warriors = game
            .units
            .iter()
            .filter(|u| u.alive && u.kind == UnitKind::Warrior && u.faction == Faction::Blue)
            .count();
        assert!(warriors > 1, "deficit kinds should be produced");
    }

    #[test]
    fn sudden_death_majority_wins_when_both_pools_empty() {
        let mut game = game_with_three_zones();
        game.manpower = [0.0, 0.0];
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[2].state = ZoneState::Controlled(Faction::Red);

        game.tick_zones(game.config.victory_hold_time * 0.5);
        assert_eq!(game.winner, None);
        game.tick_zones(game.config.victory_hold_time * 0.6);
        assert_eq!(game.winner, Some(Faction::Blue));
    }

    #[test]
    fn sudden_death_timer_pauses_on_flicker_not_resets() {
        let mut game = game_with_three_zones();
        game.manpower = [0.0, 0.0];
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[2].state = ZoneState::Controlled(Faction::Red);

        game.tick_zones(game.config.victory_hold_time * 0.9);
        // Frontline flicker: one Blue zone dips to Capturing — tie
        game.zone_manager.zones[1].state = ZoneState::Capturing(Faction::Red);
        game.tick_zones(1.0);
        assert_eq!(game.winner, None);
        // Majority restored — accumulated time must survive the flicker
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);
        game.tick_zones(game.config.victory_hold_time * 0.2);
        assert_eq!(game.winner, Some(Faction::Blue));
    }

    #[test]
    fn no_sudden_death_while_a_pool_remains() {
        let mut game = game_with_three_zones();
        game.manpower = [10.0, 0.0];
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 100, 100, false);
        game.zone_manager.zones[0].state = ZoneState::Controlled(Faction::Blue);
        game.zone_manager.zones[1].state = ZoneState::Controlled(Faction::Blue);

        game.tick_zones(game.config.victory_hold_time * 2.0);
        assert_eq!(
            game.winner, None,
            "majority is not enough outside sudden death"
        );
    }

    #[test]
    fn battle_setup_resets_manpower_from_config() {
        let mut game = Game::new(960.0, 640.0);
        game.manpower = [1.0, 2.0];
        game.config.manpower_start = 77.0;
        game.setup_demo_battle_with_seed(42);
        assert_eq!(game.manpower, [77.0, 77.0]);
    }
}
