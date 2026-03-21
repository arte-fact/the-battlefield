use super::*;

impl Game {
    /// Move AI unit continuously toward target using waypoint-following with A*.
    /// Pathfinding is rate-limited by ai_path_cooldown (one repath per 0.5s per unit).
    pub(super) fn ai_move_toward_continuous(&mut self, ai_idx: usize, target_x: f32, target_y: f32, dt: f32) {
        // Tick path cooldown
        self.units[ai_idx].ai_path_cooldown = (self.units[ai_idx].ai_path_cooldown - dt).max(0.0);

        // Re-path if cooldown expired or path exhausted
        let needs_repath = self.units[ai_idx].ai_path_cooldown <= 0.0
            || self.units[ai_idx].ai_waypoint_idx >= self.units[ai_idx].ai_waypoints.len();

        if needs_repath {
            let (ax, ay) = self.units[ai_idx].grid_cell();
            let (gx, gy) = grid::world_to_grid(target_x, target_y);
            let gx = gx.max(0) as u32;
            let gy = gy.max(0) as u32;

            // First try pathing around occupied tiles
            let path = self.grid.find_path(ax, ay, gx, gy, 40, |x, y| {
                self.ai_occupied_cache.contains(&(x, y))
            });

            // If that fails (blocked by friendlies), path ignoring them
            let path = path.or_else(|| self.grid.find_path(ax, ay, gx, gy, 40, |_, _| false));

            if let Some(steps) = path {
                self.units[ai_idx].ai_waypoints = steps
                    .iter()
                    .map(|&(x, y)| grid::grid_to_world(x, y))
                    .collect();
                self.units[ai_idx].ai_waypoint_idx = 0;
            } else {
                self.units[ai_idx].ai_waypoints.clear();
                self.units[ai_idx].ai_waypoint_idx = 0;
            }
            self.units[ai_idx].ai_path_cooldown = 0.5;
        }

        // Follow current waypoint
        let wp_idx = self.units[ai_idx].ai_waypoint_idx;
        if wp_idx < self.units[ai_idx].ai_waypoints.len() {
            let (wx, wy) = self.units[ai_idx].ai_waypoints[wp_idx];
            let ux = self.units[ai_idx].x;
            let uy = self.units[ai_idx].y;
            let ddx = wx - ux;
            let ddy = wy - uy;
            let dist = (ddx * ddx + ddy * ddy).sqrt();

            if dist < TILE_SIZE / 3.0 {
                self.units[ai_idx].ai_waypoint_idx += 1;
            } else if dist > 0.01 {
                let dir_x = ddx / dist;
                let dir_y = ddy / dist;
                self.move_unit(ai_idx, dir_x, dir_y, dt);
            }
        }
    }

    /// Return the strategic objective for a faction (world-space coordinates).
    /// Prioritizes capture zones; falls back to enemy base if all zones are controlled.
    pub(super) fn faction_objective(&self, faction: Faction) -> (f32, f32) {
        if let Some(zone) = self.zone_manager.best_target_zone(faction) {
            return (zone.center_wx, zone.center_wy);
        }
        match faction {
            Faction::Blue => self.blue_objective,
            _ => self.red_objective,
        }
    }

    /// Update a faction's flow field if the objective has moved significantly.
    pub(super) fn update_flow_field_if_needed(&mut self, faction: Faction) {
        let objective = self.faction_objective(faction);
        let flow_state = match faction {
            Faction::Blue => &self.blue_flow,
            _ => &self.red_flow,
        };
        let dx = objective.0 - flow_state.cached_goal.0;
        let dy = objective.1 - flow_state.cached_goal.1;
        let dist_sq = dx * dx + dy * dy;
        let half_tile = TILE_SIZE * 0.5;
        if flow_state.field.is_some() && dist_sq < half_tile * half_tile {
            return; // goal hasn't moved enough
        }
        let (gx, gy) = grid::world_to_grid(objective.0, objective.1);
        let gx = gx.max(0) as u32;
        let gy = gy.max(0) as u32;
        let field = crate::flowfield::FlowField::generate(&self.grid, gx, gy);
        let state = match faction {
            Faction::Blue => &mut self.blue_flow,
            _ => &mut self.red_flow,
        };
        state.field = Some(field);
        state.cached_goal = objective;
    }

    /// Move AI unit via flow field toward faction objective.
    /// Blends 80% flow direction + 20% separation steering.
    /// Falls back to A* if flow field is absent or cell is unreachable.
    pub(super) fn ai_move_via_flowfield(&mut self, ai_idx: usize, dt: f32) {
        let faction = self.units[ai_idx].faction;
        let flow_state = match faction {
            Faction::Blue => &self.blue_flow,
            _ => &self.red_flow,
        };

        if let Some(ref field) = flow_state.field {
            let (gx, gy) = self.units[ai_idx].grid_cell();
            let dir = field.direction_at(gx, gy);
            if dir != (0, 0) {
                let (sep_x, sep_y) = self.compute_separation(ai_idx);
                let flow_x = dir.0 as f32;
                let flow_y = dir.1 as f32;
                let bx = flow_x * 0.8 + sep_x * 0.2;
                let by = flow_y * 0.8 + sep_y * 0.2;
                let len = (bx * bx + by * by).sqrt();
                if len > 0.01 {
                    self.move_unit(ai_idx, bx / len, by / len, dt);
                }
                return;
            }
        }

        // Fallback: use A* toward objective
        let objective = self.faction_objective(faction);
        self.ai_move_toward_continuous(ai_idx, objective.0, objective.1, dt);
    }

    /// Compute separation steering: repulsion from nearby same-faction units.
    pub(super) fn compute_separation(&self, ai_idx: usize) -> (f32, f32) {
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let faction = self.units[ai_idx].faction;
        let sep_radius = UNIT_RADIUS * 3.0;
        let sep_radius_sq = sep_radius * sep_radius;

        let mut rx = 0.0f32;
        let mut ry = 0.0f32;

        for (i, u) in self.units.iter().enumerate() {
            if i == ai_idx || !u.alive || u.faction != faction {
                continue;
            }
            let dx = ax - u.x;
            let dy = ay - u.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < sep_radius_sq && dist_sq > 0.01 {
                let dist = dist_sq.sqrt();
                let weight = 1.0 - dist / sep_radius;
                rx += (dx / dist) * weight;
                ry += (dy / dist) * weight;
            }
        }

        let len = (rx * rx + ry * ry).sqrt();
        if len > 0.01 {
            (rx / len, ry / len)
        } else {
            (0.0, 0.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_melee_marches_to_objective() {
        let mut game = Game::new(960.0, 640.0);
        // Set up objective to the right
        game.blue_objective = grid::grid_to_world(50, 5);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, false);
        // No enemies at all — AI should march toward objective
        let start_x = game.units[0].x;
        for _ in 0..60 {
            game.tick_ai(0.016);
        }
        assert!(
            game.units[0].x > start_x,
            "AI should march toward objective when no enemy in sight"
        );
    }

    #[test]
    fn ai_targets_zone_not_spawn() {
        use crate::mapgen::MapLayout;
        let mut game = Game::new(960.0, 640.0);
        let layout = MapLayout {
            blue_base: (21, 21),
            red_base: (138, 138),
            zone_centers: vec![(50, 50), (80, 80), (110, 110)],
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout);
        game.blue_objective = grid::grid_to_world(138, 138);
        let obj = game.faction_objective(Faction::Blue);
        let (base_wx, _) = grid::grid_to_world(138, 138);
        assert!(
            obj.0 < base_wx,
            "Blue should target a zone (x < {base_wx}), got x={}",
            obj.0
        );
    }
}
