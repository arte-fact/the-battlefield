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
        self.zone_manager.tick_capture(dt, self.config.base_capture_time, self.config.max_capture_multiplier);

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
            if let Some(faction) = self.zone_manager.tick_victory(dt, self.config.victory_hold_time) {
                self.winner = Some(faction);
            }
        }
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
                    let holds_zone = self
                        .zone_manager
                        .zones
                        .iter()
                        .any(|z| z.state == crate::zone::ZoneState::Controlled(faction));
                    let slots = self.config.max_units_per_faction.saturating_sub(alive_count);
                    // Double wave size when holding no zones (fill army faster)
                    let max_wave = if holds_zone {
                        Self::WAVE.len()
                    } else {
                        Self::WAVE.len() * 2
                    };
                    let wave_size = slots.min(max_wave);
                    // Build wave by cycling through WAVE pattern
                    let mut queue = Vec::with_capacity(wave_size);
                    for i in 0..wave_size {
                        queue.push(Self::WAVE[i % Self::WAVE.len()]);
                    }
                    self.spawn_queue[fi] = queue;
                    self.spawn_timer[fi] = 0.0;
                }
            }

            if self.spawn_queue[fi].is_empty() {
                continue;
            }

            self.spawn_timer[fi] += dt;
            let interval = self.config.spawn_interval;
            if self.spawn_timer[fi] >= interval {
                self.spawn_timer[fi] -= interval;

                let kind = self.spawn_queue[fi].remove(0);
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
                // Set rally hold — unit waits at base until wave is complete
                if let Some(u) = self.units.iter_mut().find(|u| u.id == id) {
                    u.rally_hold = true;
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
        let (gen_grid, layout) = mapgen::generate_battlefield(seed);
        self.grid = gen_grid;

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

        // Procedurally generate all base buildings (castle, towers, production, houses)
        let mut buildings =
            building::generate_base_buildings(Faction::Blue, blue_cx, blue_cy, seed);
        buildings.extend(building::generate_base_buildings(
            Faction::Red,
            red_cx,
            red_cy,
            seed.wrapping_add(0xBEEF),
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
        self.spawn_base_sheep(blue_cx, blue_cy, 1, seed);
        self.spawn_base_sheep(red_cx, red_cy, -1, seed.wrapping_add(7919));

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

    fn spawn_base_sheep(&mut self, cx: u32, cy: u32, fs: i32, base_seed: u32) {
        let mut seed = if base_seed == 0 { 1 } else { base_seed };
        let mut next = || -> u32 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..10 {
            // Random position in rear pasture: dx ∈ [-4,4], distance 11–14 behind center
            let dx = (next() % 9) as i32 - 4;
            let rear_dist = (next() % 4) as i32 + 11; // 11, 12, 13, or 14
            let gx = (cx as i32 + dx).max(0) as u32;
            let gy = (cy as i32 - rear_dist * fs).max(0) as u32;
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
}
