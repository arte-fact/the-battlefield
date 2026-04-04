use super::*;

impl Game {
    /// Move AI unit continuously toward target using waypoint-following with A*.
    /// Pathfinding is rate-limited by ai_path_cooldown (one repath per 0.5s per unit).
    pub(super) fn ai_move_toward_continuous(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        // Tick path cooldown
        self.units[ai_idx].ai_path_cooldown = (self.units[ai_idx].ai_path_cooldown - dt).max(0.0);

        // Re-path if cooldown expired or path exhausted
        let needs_repath = self.units[ai_idx].ai_path_cooldown <= 0.0
            || self.units[ai_idx].ai_waypoint_idx >= self.units[ai_idx].ai_waypoints.len();

        if needs_repath {
            // Cap A* calls per frame to prevent spike frames
            if self.astar_budget == 0 {
                // Defer to next frame — keep following current waypoints
                self.units[ai_idx].ai_path_cooldown = self.config.deferred_repath_delay;
            } else {
                self.astar_budget -= 1;

                let (ax, ay) = self.units[ai_idx].grid_cell();
                let (gx, gy) = grid::world_to_grid(target_x, target_y);
                let gx = gx.max(0) as u32;
                let gy = gy.max(0) as u32;

                let path = self.grid.find_path(ax, ay, gx, gy, self.config.astar_search_limit, |_, _| false);

                if let Some(steps) = path {
                    self.units[ai_idx].ai_waypoints = steps
                        .iter()
                        .map(|&(x, y)| grid::grid_to_world(x, y))
                        .collect();
                    self.units[ai_idx].ai_waypoint_idx = 0;
                    // Jitter cooldown using golden ratio to spread units evenly
                    let golden = 0.618034;
                    let jitter = ((self.units[ai_idx].id as f32 * golden) % 1.0) * self.config.repath_cooldown_mod;
                    self.units[ai_idx].ai_path_cooldown = self.config.repath_cooldown_base + jitter;
                } else {
                    self.units[ai_idx].ai_waypoints.clear();
                    self.units[ai_idx].ai_waypoint_idx = 0;
                    self.units[ai_idx].ai_path_cooldown = self.config.failed_path_cooldown;
                }
            }
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

            if dist < TILE_SIZE * self.config.waypoint_arrival_frac {
                self.units[ai_idx].ai_waypoint_idx += 1;
            } else if dist > 0.01 {
                let dir_x = ddx / dist;
                let dir_y = ddy / dist;
                self.move_unit(ai_idx, dir_x, dir_y, dt);
            }
        }
    }

    /// Return the strategic objective for a faction (world-space coordinates).
    /// Used as fallback when macro objectives are empty.
    pub(super) fn faction_objective(&self, faction: Faction) -> (f32, f32) {
        if let Some(zone) = self.zone_manager.best_target_zone(faction) {
            return (zone.center_wx, zone.center_wy);
        }
        if let Some(zone) = self.zone_manager.most_advanced_zone(faction) {
            return (zone.center_wx, zone.center_wy);
        }
        match faction {
            Faction::Blue => self.blue_objective,
            _ => self.red_objective,
        }
    }

    /// Return the world position of the objective nearest to a unit (Euclidean).
    /// Used for the zone-stop check and A* fallback.
    fn nearest_objective_pos(&self, ai_idx: usize) -> (f32, f32) {
        let faction = self.units[ai_idx].faction;
        let fi = match faction {
            Faction::Blue => 0,
            Faction::Red => 1,
        };
        let objectives = &self.macro_objectives[fi];
        if objectives.is_empty() {
            return self.faction_objective(faction);
        }
        let ux = self.units[ai_idx].x;
        let uy = self.units[ai_idx].y;
        objectives
            .iter()
            .min_by(|&&(ax, ay, _), &&(bx, by, _)| {
                let da = (ux - ax) * (ux - ax) + (uy - ay) * (uy - ay);
                let db = (ux - bx) * (ux - bx) + (uy - by) * (uy - by);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|&(wx, wy, _)| (wx, wy))
            .unwrap_or_else(|| self.faction_objective(faction))
    }

    /// Update per-zone flow fields for a faction.
    /// Each zone gets its own Dijkstra field (cached until zone position changes).
    fn update_per_zone_fields(&mut self, faction: Faction) {
        let zone_count = self.zone_manager.zones.len();
        if zone_count == 0 {
            return;
        }

        // Collect zone grid positions (releases borrows before mutating flow state)
        let mut zone_goals = Vec::with_capacity(zone_count);
        for z in &self.zone_manager.zones {
            let (gx, gy) = (z.center_gx, z.center_gy);
            let (gx, gy) = if self.grid.is_passable(gx, gy) {
                (gx, gy)
            } else {
                self.find_nearest_passable(gx, gy).unwrap_or((gx, gy))
            };
            zone_goals.push((gx, gy));
        }

        // Ensure vectors are properly sized
        let ensure_size = |state: &mut crate::flowfield::FactionFlowState, n: usize| {
            state.zone_fields.resize_with(n, || None);
            state.cached_zone_goals.resize_with(n, || None);
        };
        match faction {
            Faction::Blue => ensure_size(&mut self.blue_flow, zone_count),
            _ => ensure_size(&mut self.red_flow, zone_count),
        }

        // Generate/update per-zone fields (only when zone position changes)
        for (zi, &(gx, gy)) in zone_goals.iter().enumerate() {
            let needs_regen = match faction {
                Faction::Blue => {
                    self.blue_flow.cached_zone_goals[zi] != Some((gx, gy))
                        || self.blue_flow.zone_fields[zi].is_none()
                }
                _ => {
                    self.red_flow.cached_zone_goals[zi] != Some((gx, gy))
                        || self.red_flow.zone_fields[zi].is_none()
                }
            };
            if !needs_regen {
                continue;
            }

            let field = crate::flowfield::FlowField::generate(&self.grid, gx, gy);
            match faction {
                Faction::Blue => {
                    self.blue_flow.zone_fields[zi] = Some(field);
                    self.blue_flow.cached_zone_goals[zi] = Some((gx, gy));
                }
                _ => {
                    self.red_flow.zone_fields[zi] = Some(field);
                    self.red_flow.cached_zone_goals[zi] = Some((gx, gy));
                }
            }
        }
    }

    /// Update the unified multi-source flow field for a faction.
    /// Seeds Dijkstra from every scored zone; higher-score zones get lower initial cost.
    pub(super) fn update_flow_fields(&mut self, faction: Faction) {
        self.update_per_zone_fields(faction);
    }

    /// Faction-level objective planner: picks 1-2 target zones and assigns
    /// all units in bulk. Concentrates force instead of spreading thin.
    pub(super) fn assign_unit_objectives(&mut self) {
        let zone_count = self.zone_manager.zones.len();
        if zone_count == 0 {
            return;
        }

        for &faction in &[Faction::Blue, Faction::Red] {
            let fi = match faction {
                Faction::Blue => 0,
                Faction::Red => 1,
            };
            let objectives = &self.macro_objectives[fi];
            if objectives.is_empty() {
                continue;
            }

            // Find defend target (Tier 1: score >= 200) and attack target (Tier 2: score >= 85)
            let mut defend_zone: Option<u8> = None;
            let mut attack_zone: Option<u8> = None;

            for &(wx, wy, score) in objectives {
                let zi = self
                    .zone_manager
                    .zones
                    .iter()
                    .position(|z| (z.center_wx - wx).abs() < 1.0 && (z.center_wy - wy).abs() < 1.0);
                let Some(zi) = zi else { continue };

                if score >= 200.0 && defend_zone.is_none() {
                    defend_zone = Some(zi as u8);
                } else if score >= 85.0 && attack_zone.is_none() {
                    attack_zone = Some(zi as u8);
                }
                if defend_zone.is_some() && attack_zone.is_some() {
                    break;
                }
            }

            // Collect available AI units for this faction, sorted by index
            let mut available: Vec<usize> = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| u.alive && !u.is_player && u.faction == faction)
                .map(|(i, _)| i)
                .collect();

            if available.is_empty() {
                continue;
            }

            match (defend_zone, attack_zone) {
                (Some(def_zi), Some(atk_zi)) => {
                    // Split: 40% defend, 60% attack. Sort by flow cost to each target.
                    let n_defend = ((available.len() as f32 * 0.4).ceil() as usize).min(available.len());

                    // Sort by flow cost to defend zone (nearest first)
                    let flow_state = match faction {
                        Faction::Blue => &self.blue_flow,
                        _ => &self.red_flow,
                    };
                    available.sort_by_key(|&ui| {
                        let (gx, gy) = self.units[ui].grid_cell();
                        flow_state
                            .zone_fields
                            .get(def_zi as usize)
                            .and_then(|f| f.as_ref())
                            .map(|f| f.cost_at(gx, gy))
                            .unwrap_or(u32::MAX)
                    });

                    // Nearest n_defend → defend, rest → attack
                    for (i, &ui) in available.iter().enumerate() {
                        let target = if i < n_defend { def_zi } else { atk_zi };
                        self.units[ui].assigned_zone = Some(target);
                        self.units[ui].rally_hold = false;
                    }
                }
                (None, Some(atk_zi)) => {
                    // All-in attack
                    for &ui in &available {
                        self.units[ui].assigned_zone = Some(atk_zi);
                        self.units[ui].rally_hold = false;
                    }
                }
                (Some(def_zi), None) => {
                    // Only defending — all to defend target
                    for &ui in &available {
                        self.units[ui].assigned_zone = Some(def_zi);
                        self.units[ui].rally_hold = false;
                    }
                }
                (None, None) => {
                    // All secure — spread evenly across owned zones (Tier 3)
                    let owned: Vec<u8> = self
                        .zone_manager
                        .zones
                        .iter()
                        .enumerate()
                        .filter(|(_, z)| z.state == ZoneState::Controlled(faction))
                        .map(|(i, _)| i as u8)
                        .collect();

                    if owned.is_empty() {
                        // Fallback: send all to first objective
                        if let Some(zi) = self.zone_manager.zones.iter().position(|z| {
                            let (wx, wy) = (z.center_wx, z.center_wy);
                            objectives
                                .iter()
                                .any(|(ox, oy, _)| (ox - wx).abs() < 1.0 && (oy - wy).abs() < 1.0)
                        }) {
                            for &ui in &available {
                                self.units[ui].assigned_zone = Some(zi as u8);
                                self.units[ui].rally_hold = false;
                            }
                        }
                    } else {
                        // Distribute evenly by round-robin
                        for (i, &ui) in available.iter().enumerate() {
                            self.units[ui].assigned_zone = Some(owned[i % owned.len()]);
                            self.units[ui].rally_hold = false;
                        }
                    }
                }
            }
        }
    }

    /// Move AI unit via its assigned zone's per-zone flow field.
    /// Blends 80% flow direction + 20% separation steering.
    /// Falls back to unified field, then A* toward the nearest objective.
    pub(super) fn ai_move_via_flowfield(&mut self, ai_idx: usize, dt: f32) {
        let faction = self.units[ai_idx].faction;
        let ux = self.units[ai_idx].x;
        let uy = self.units[ai_idx].y;
        let assigned_zone = self.units[ai_idx].assigned_zone;

        // Determine target position (assigned zone center, or nearest objective fallback)
        let (obj_wx, obj_wy) = if let Some(zi) = assigned_zone {
            if (zi as usize) < self.zone_manager.zones.len() {
                let z = &self.zone_manager.zones[zi as usize];
                (z.center_wx, z.center_wy)
            } else {
                self.nearest_objective_pos(ai_idx)
            }
        } else {
            self.nearest_objective_pos(ai_idx)
        };

        // If inside our assigned zone and it's fully captured, hold position.
        // Don't stop if the zone is contested or has enemies — keep fighting.
        if let Some(zi) = assigned_zone {
            let zi_usize = zi as usize;
            if zi_usize < self.zone_manager.zones.len() {
                let zone = &self.zone_manager.zones[zi_usize];
                if zone.contains_world(ux, uy)
                    && zone.state == crate::zone::ZoneState::Controlled(faction)
                {
                    return;
                }
            }
        }

        // Read direction from assigned zone's per-zone flow field
        let (gx, gy) = self.units[ai_idx].grid_cell();
        let dir = {
            let flow_state = match faction {
                Faction::Blue => &self.blue_flow,
                _ => &self.red_flow,
            };
            assigned_zone.and_then(|zi| {
                flow_state
                    .zone_fields
                    .get(zi as usize)
                    .and_then(|f| f.as_ref())
                    .map(|f| f.direction_at(gx, gy))
            })
        };

        if let Some(dir) = dir {
            if dir != (0, 0) {
                let (sep_x, sep_y) = self.compute_separation(ai_idx);
                let bx = dir.0 as f32 * self.config.flow_weight + sep_x * self.config.separation_weight;
                let by = dir.1 as f32 * self.config.flow_weight + sep_y * self.config.separation_weight;
                let len = (bx * bx + by * by).sqrt();
                if len > 0.01 {
                    self.move_unit(ai_idx, bx / len, by / len, dt);
                }
                return;
            }
        }

        // Fallback: A* toward target objective
        self.ai_move_toward_continuous(ai_idx, obj_wx, obj_wy, dt);
    }

    /// Compute separation steering: repulsion from nearby same-faction units.
    /// Uses the per-frame spatial hash to avoid O(n) full scan.
    pub(super) fn compute_separation(&self, ai_idx: usize) -> (f32, f32) {
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let faction = self.units[ai_idx].faction;
        let sep_radius = UNIT_RADIUS * self.config.separation_radius_mult;
        let sep_radius_sq = sep_radius * sep_radius;

        let mut rx = 0.0f32;
        let mut ry = 0.0f32;

        for i in self.spatial.query(ax, ay, sep_radius) {
            if i == ai_idx {
                continue;
            }
            let u = &self.units[i];
            if u.faction != faction {
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
            blue_gather: (21, 21),
            red_gather: (138, 138),
            blue_home_zones: vec![0],
            red_home_zones: vec![2],
            connections: vec![vec![1], vec![0, 2], vec![1]],
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
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
