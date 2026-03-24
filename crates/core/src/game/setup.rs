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

    /// Seconds between individual unit spawns within a wave.
    const SPAWN_INTERVAL: f32 = 1.5;

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
        let blue_before = self
            .zone_manager
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(Faction::Blue))
            .count();

        self.zone_manager.count_units(&self.units);
        self.zone_manager.tick_capture(dt);

        let blue_after = self
            .zone_manager
            .zones
            .iter()
            .filter(|z| z.state == ZoneState::Controlled(Faction::Blue))
            .count();

        if blue_after > blue_before {
            self.on_zone_captured();
        }
        if blue_after < blue_before {
            self.on_zone_lost();
        }

        if self.winner.is_none() {
            if let Some(faction) = self.zone_manager.tick_victory(dt) {
                self.winner = Some(faction);
            }
        }
    }

    /// Spawn units one-by-one from the queue at the rally point (base center).
    /// When a wave is complete, release all rallying units to march.
    pub fn tick_production(&mut self, dt: f32) {
        let factions = [(0usize, Faction::Blue), (1, Faction::Red)];
        for &(fi, faction) in &factions {
            // Refill queue if empty and under unit cap
            if self.spawn_queue[fi].is_empty() {
                let alive_count = self
                    .units
                    .iter()
                    .filter(|u| u.alive && u.faction == faction)
                    .count();
                if alive_count < MAX_UNITS_PER_FACTION {
                    let slots = MAX_UNITS_PER_FACTION.saturating_sub(alive_count);
                    let wave_size = slots.min(Self::WAVE.len());
                    self.spawn_queue[fi] = Self::WAVE[..wave_size].to_vec();
                    self.spawn_timer[fi] = 0.0;
                }
            }

            if self.spawn_queue[fi].is_empty() {
                continue;
            }

            self.spawn_timer[fi] += dt;
            if self.spawn_timer[fi] >= Self::SPAWN_INTERVAL {
                self.spawn_timer[fi] -= Self::SPAWN_INTERVAL;

                let kind = self.spawn_queue[fi].remove(0);
                // Spawn at the production building for this unit type
                let target_bk = building::building_for_unit(kind);
                let (sx, sy) = self
                    .buildings
                    .iter()
                    .find(|b| b.faction == faction && b.kind == target_bk)
                    .map(|b| {
                        // Spawn 1 tile in front of the building (toward battlefield)
                        match faction {
                            Faction::Blue => (b.grid_x, b.grid_y + 1),
                            _ => (b.grid_x, b.grid_y.saturating_sub(1)),
                        }
                    })
                    .unwrap_or_else(|| match faction {
                        Faction::Blue => self.zone_manager.blue_base,
                        _ => self.zone_manager.red_base,
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
        self.zone_manager = ZoneManager::create_from_layout(&layout);

        // Mark tower center tile as impassable.
        // Tower sprite (2x4 tiles) has bottom-center at (center_gx+0.5, center_gy+1).
        // Only block the single center tile to keep the navigation footprint tight
        // (is_wide_passable expands this by 1 tile in each cardinal direction).
        for zone in &self.zone_manager.zones {
            self.grid.mark_building(zone.center_gx, zone.center_gy);
        }

        // Place production buildings at both bases
        let mut buildings = building::base_buildings(Faction::Blue, blue_cx, blue_cy);
        buildings.extend(building::base_buildings(Faction::Red, red_cx, red_cy));
        // Place defensive buildings (castle, towers, houses) at both bases
        buildings.extend(building::base_defense_buildings(
            Faction::Blue,
            blue_cx,
            blue_cy,
        ));
        buildings.extend(building::base_defense_buildings(
            Faction::Red,
            red_cx,
            red_cy,
        ));
        for b in &buildings {
            for &(dx, dy) in b.kind.footprint_offsets() {
                let fx = b.grid_x as i32 + dx;
                let fy = b.grid_y as i32 + dy;
                if self.grid.in_bounds(fx, fy) {
                    self.grid.mark_building(fx as u32, fy as u32);
                }
            }
        }
        self.buildings = buildings;

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
        self.compute_water_adjacency();
        self.compute_fov();
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
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout);
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
