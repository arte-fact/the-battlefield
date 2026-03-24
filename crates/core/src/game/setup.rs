use super::*;

impl Game {
    /// Reinforcement wave composition — spawned all at once per wave.
    const REINFORCE_WAVE: &'static [UnitKind] = &[
        UnitKind::Warrior,
        UnitKind::Warrior,
        UnitKind::Lancer,
        UnitKind::Lancer,
        UnitKind::Archer,
        UnitKind::Archer,
        UnitKind::Monk,
    ];

    /// Seconds between reinforcement waves.
    const REINFORCE_INTERVAL: f32 = 20.0;

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

    /// Spawn a full wave of reinforcements at base when under unit cap.
    pub fn tick_production(&mut self, dt: f32) {
        let factions = [(0usize, Faction::Blue), (1, Faction::Red)];
        for &(fi, faction) in &factions {
            let alive_count = self
                .units
                .iter()
                .filter(|u| u.alive && u.faction == faction)
                .count();
            if alive_count >= MAX_UNITS_PER_FACTION {
                continue;
            }

            self.reinforce_timer[fi] += dt;
            if self.reinforce_timer[fi] >= Self::REINFORCE_INTERVAL {
                self.reinforce_timer[fi] -= Self::REINFORCE_INTERVAL;

                let slots = MAX_UNITS_PER_FACTION.saturating_sub(alive_count);
                let wave_size = slots.min(Self::REINFORCE_WAVE.len());

                for i in 0..wave_size {
                    let kind = Self::REINFORCE_WAVE[i];
                    let target_bk = building::building_for_unit(kind);
                    let (sx, sy) = self
                        .buildings
                        .iter()
                        .find(|b| b.faction == faction && b.kind == target_bk)
                        .map(|b| {
                            // Spawn 3 tiles toward the battlefield from the building
                            let candidate = match faction {
                                Faction::Blue => (b.grid_x, b.grid_y + 3),
                                _ => (b.grid_x, b.grid_y.saturating_sub(3)),
                            };
                            if self.grid.is_passable(candidate.0, candidate.1) {
                                candidate
                            } else if self.grid.is_passable(candidate.0 + 1, candidate.1) {
                                (candidate.0 + 1, candidate.1)
                            } else if self
                                .grid
                                .is_passable(candidate.0.saturating_sub(1), candidate.1)
                            {
                                (candidate.0.saturating_sub(1), candidate.1)
                            } else {
                                match faction {
                                    Faction::Blue => self.zone_manager.blue_base,
                                    _ => self.zone_manager.red_base,
                                }
                            }
                        })
                        .unwrap_or_else(|| match faction {
                            Faction::Blue => self.zone_manager.blue_base,
                            _ => self.zone_manager.red_base,
                        });
                    self.spawn_unit(kind, faction, sx, sy, false);
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

        // Blue army — spawn in front of base (toward center)
        let bx = blue_cx;
        let by = blue_cy + 5;
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, bx + 1, by, true);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, bx + 1, by + 1, false);
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Blue,
            bx + 1,
            by.saturating_sub(1),
            false,
        );
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, bx + 1, by + 2, false);
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Blue,
            bx + 1,
            by.saturating_sub(2),
            false,
        );
        for i in 0..3u32 {
            self.spawn_unit(UnitKind::Lancer, Faction::Blue, bx, by + i, false);
            self.spawn_unit(
                UnitKind::Lancer,
                Faction::Blue,
                bx,
                by.saturating_sub(1 + i),
                false,
            );
        }
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(1),
            by + 1,
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(1),
            by.saturating_sub(1),
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(1),
            by,
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(2),
            by + 2,
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(2),
            by.saturating_sub(2),
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            bx.saturating_sub(2),
            by,
            false,
        );
        self.spawn_unit(
            UnitKind::Monk,
            Faction::Blue,
            bx.saturating_sub(2),
            by + 1,
            false,
        );
        self.spawn_unit(
            UnitKind::Monk,
            Faction::Blue,
            bx.saturating_sub(2),
            by.saturating_sub(1),
            false,
        );

        // Red army — spawn in front of base (toward center)
        let rx = red_cx;
        let ry = red_cy.saturating_sub(5);
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            rx.saturating_sub(1),
            ry,
            false,
        );
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            rx.saturating_sub(1),
            ry + 1,
            false,
        );
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            rx.saturating_sub(1),
            ry.saturating_sub(1),
            false,
        );
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            rx.saturating_sub(1),
            ry + 2,
            false,
        );
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            rx.saturating_sub(1),
            ry.saturating_sub(2),
            false,
        );
        for i in 0..3u32 {
            self.spawn_unit(UnitKind::Lancer, Faction::Red, rx, ry + i, false);
            self.spawn_unit(
                UnitKind::Lancer,
                Faction::Red,
                rx,
                ry.saturating_sub(1 + i),
                false,
            );
        }
        self.spawn_unit(UnitKind::Archer, Faction::Red, rx + 1, ry + 1, false);
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Red,
            rx + 1,
            ry.saturating_sub(1),
            false,
        );
        self.spawn_unit(UnitKind::Archer, Faction::Red, rx + 1, ry, false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, rx + 2, ry + 2, false);
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Red,
            rx + 2,
            ry.saturating_sub(2),
            false,
        );
        self.spawn_unit(UnitKind::Archer, Faction::Red, rx + 2, ry, false);
        self.spawn_unit(UnitKind::Monk, Faction::Red, rx + 2, ry + 1, false);
        self.spawn_unit(
            UnitKind::Monk,
            Faction::Red,
            rx + 2,
            ry.saturating_sub(1),
            false,
        );

        // Camera starts centered on player
        let (cx, cy) = grid::grid_to_world(bx, by);
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
