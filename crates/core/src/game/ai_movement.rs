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
            if state.zone_congestion.len() != n {
                state.zone_congestion.resize(n, 0);
            }
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
        let fi = match faction {
            Faction::Blue => 0,
            Faction::Red => 1,
        };

        // Build goal tuples — separate block to avoid borrow conflict with flow state below
        let goals: Vec<(u32, u32, u32)> = {
            let objectives = &self.macro_objectives[fi];
            if objectives.is_empty() {
                return;
            }
            let mut v = Vec::with_capacity(objectives.len());
            for &(wx, wy, score) in objectives {
                let (gx, gy) = grid::world_to_grid(wx, wy);
                let gx = gx.max(0) as u32;
                let gy = gy.max(0) as u32;
                let (gx, gy) = if self.grid.is_passable(gx, gy) {
                    (gx, gy)
                } else {
                    self.find_nearest_passable(gx, gy).unwrap_or((gx, gy))
                };
                let initial_cost = self.config.flow_initial_cost_base.saturating_sub((score * self.config.flow_score_multiplier).round() as u32);
                v.push((gx, gy, initial_cost));
            }
            v
        };

        // Skip regeneration if goals are identical to the cached set
        let cached = match faction {
            Faction::Blue => &self.blue_flow.cached_goals,
            _ => &self.red_flow.cached_goals,
        };
        let needs_regen = {
            let field_missing = match faction {
                Faction::Blue => self.blue_flow.field.is_none(),
                _ => self.red_flow.field.is_none(),
            };
            field_missing
                || cached.len() != goals.len()
                || cached.iter().zip(goals.iter()).any(|(a, b)| a != b)
        };

        if !needs_regen {
            return;
        }

        let field = crate::flowfield::FlowField::generate_multi_source(&self.grid, &goals);
        let state = match faction {
            Faction::Blue => &mut self.blue_flow,
            _ => &mut self.red_flow,
        };
        state.field = Some(field);
        state.cached_goals = goals;
    }

    /// Assign each AI unit to its best-scoring zone based on flow cost,
    /// congestion, influence, role, health, and hysteresis.
    pub(super) fn assign_unit_objectives(&mut self) {
        let zone_count = self.zone_manager.zones.len();
        if zone_count == 0 {
            return;
        }

        // === Phase 1: Gather read-only data to avoid borrow conflicts ===
        let player_pos = self.player_unit().map(|p| (p.x, p.y));
        let authority = self.authority;

        let zone_info: Vec<(f32, f32, ZoneState, u32, u32, f32)> = self
            .zone_manager
            .zones
            .iter()
            .map(|z| {
                (
                    z.center_wx,
                    z.center_wy,
                    z.state,
                    z.blue_count,
                    z.red_count,
                    z.progress,
                )
            })
            .collect();
        let zone_radius_sq = (self.config.zone_radius as f32 * TILE_SIZE).powi(2);

        let macro_obj = [
            self.macro_objectives[0].clone(),
            self.macro_objectives[1].clone(),
        ];

        // Previous congestion (from last assignment cycle)
        let prev_blue_cong = self.blue_flow.zone_congestion.clone();
        let prev_red_cong = self.red_flow.zone_congestion.clone();

        // Collect per-unit data + flow costs (avoids repeated flow state borrows)
        #[allow(clippy::type_complexity)]
        let unit_data: Vec<(
            usize,
            Faction,
            Option<u8>,
            f32,
            UnitKind,
            f32,
            f32,
            f32,
            Vec<u32>,
        )> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && !u.is_player)
            .map(|(ui, u)| {
                let (gx, gy) = u.grid_cell();
                let flow_state = match u.faction {
                    Faction::Blue => &self.blue_flow,
                    _ => &self.red_flow,
                };
                let costs: Vec<u32> = (0..zone_count)
                    .map(|zi| {
                        flow_state
                            .zone_fields
                            .get(zi)
                            .and_then(|f| f.as_ref())
                            .map(|f| f.cost_at(gx, gy))
                            .unwrap_or(u32::MAX)
                    })
                    .collect();
                (
                    ui,
                    u.faction,
                    u.assigned_zone,
                    u.hp as f32 / u.stats.max_hp as f32,
                    u.kind,
                    u.x,
                    u.y,
                    u.zone_lock_timer,
                    costs,
                )
            })
            .collect();

        // === Phase 2: Score zones per-unit and assign ===
        let mut assignments: Vec<(usize, Option<u8>)> = Vec::with_capacity(unit_data.len());
        let mut new_blue_cong = vec![0u32; zone_count];
        let mut new_red_cong = vec![0u32; zone_count];

        for (ui, faction, current_zone, hp_ratio, kind, ux, uy, lock_timer, flow_costs) in
            &unit_data
        {
            // If unit is locked to its current zone, skip scoring unless zone is
            // fully controlled by our faction (no point guarding a secured zone)
            if *lock_timer > 0.0 {
                if let Some(zi) = current_zone {
                    let zi_usize = *zi as usize;
                    if zi_usize < zone_info.len() {
                        let (_, _, state, _, _, _) = zone_info[zi_usize];
                        if state != ZoneState::Controlled(*faction) {
                            // Still locked — keep current assignment, count in congestion
                            assignments.push((*ui, *current_zone));
                            match faction {
                                Faction::Blue => new_blue_cong[zi_usize] += 1,
                                Faction::Red => new_red_cong[zi_usize] += 1,
                            }
                            continue;
                        }
                    }
                }
            }

            let fi = match faction {
                Faction::Blue => 0,
                Faction::Red => 1,
            };
            let prev_cong = match faction {
                Faction::Blue => &prev_blue_cong,
                Faction::Red => &prev_red_cong,
            };

            let mut best_score = f32::NEG_INFINITY;
            let mut best_zone: Option<u8> = None;

            for (zi, &(zwx, zwy, state, blue_count, red_count, progress)) in
                zone_info.iter().enumerate()
            {
                let cost = flow_costs[zi];
                if cost == u32::MAX {
                    continue;
                }

                // Base strategic score from macro objectives
                let base_score = macro_obj[fi]
                    .iter()
                    .find(|(wx, wy, _)| (wx - zwx).abs() < 1.0 && (wy - zwy).abs() < 1.0)
                    .map(|(_, _, s)| *s)
                    .unwrap_or(0.0);

                let cfg = &self.config;

                // Terrain-aware distance penalty (normalized)
                let cost_norm = cost as f32 / cfg.zone_cost_norm_divisor;

                // Congestion: fewer allies heading there = better
                let cong = prev_cong.get(zi).copied().unwrap_or(0) as f32;

                // Hysteresis: bonus for staying on current assignment
                let hysteresis = if *current_zone == Some(zi as u8) {
                    cfg.zone_hysteresis
                } else {
                    0.0
                };

                // Influence: player authority pulls Blue units toward zones near player
                let influence = if *faction == Faction::Blue {
                    if let Some((px, py)) = player_pos {
                        let d = ((px - zwx).powi(2) + (py - zwy).powi(2)).sqrt();
                        let r = TILE_SIZE * cfg.zone_authority_radius_tiles;
                        if d < r {
                            cfg.zone_authority_influence * (authority / 100.0) * (1.0 - d / r)
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                // Wounded units prefer closer objectives
                let health_pen = if *hp_ratio < 0.5 {
                    cost_norm * (1.0 - hp_ratio) * cfg.zone_health_penalty
                } else {
                    0.0
                };

                // Contested bonus: units can make a difference
                let contested = if state == ZoneState::Contested {
                    cfg.zone_contested_bonus
                } else {
                    0.0
                };

                // Role bonus: archers prefer zones with allies (safety in numbers)
                let role_bonus = match kind {
                    UnitKind::Archer => {
                        let allies = match faction {
                            Faction::Blue => blue_count,
                            Faction::Red => red_count,
                        };
                        allies as f32 * cfg.zone_archer_ally_bonus
                    }
                    _ => 0.0,
                };

                // Capture commitment: strong bonus when inside a zone still being captured.
                let capture_commit = {
                    let dx = ux - zwx;
                    let dy = uy - zwy;
                    let inside = dx * dx + dy * dy <= zone_radius_sq;
                    if inside && state != ZoneState::Controlled(*faction) {
                        let progress_for_us = match faction {
                            Faction::Blue => progress,
                            Faction::Red => -progress,
                        };
                        if progress_for_us > 0.0 {
                            cfg.zone_capture_commit_extra_base
                                + cfg.zone_capture_commit_progress_mult * progress_for_us
                        } else {
                            cfg.zone_capture_commit_base
                        }
                    } else {
                        0.0
                    }
                };

                let score = base_score
                    + cfg.zone_distance_weight * cost_norm
                    + cfg.zone_congestion_weight * cong
                    + hysteresis
                    + influence
                    - health_pen
                    + contested
                    + role_bonus
                    + capture_commit;

                if score > best_score {
                    best_score = score;
                    best_zone = Some(zi as u8);
                }
            }

            assignments.push((*ui, best_zone));
            if let Some(zi) = best_zone {
                match faction {
                    Faction::Blue => new_blue_cong[zi as usize] += 1,
                    Faction::Red => new_red_cong[zi as usize] += 1,
                }
            }
        }

        // === Phase 3: Apply assignments and congestion ===
        for (ui, zone) in assignments {
            if zone != self.units[ui].assigned_zone {
                self.units[ui].zone_lock_timer = self.config.zone_lock_duration;
            }
            self.units[ui].assigned_zone = zone;
        }
        self.blue_flow.zone_congestion = new_blue_cong;
        self.red_flow.zone_congestion = new_red_cong;
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

        // If already inside the assigned zone, stop
        if let Some(zi) = assigned_zone {
            if (zi as usize) < self.zone_manager.zones.len()
                && self.zone_manager.zones[zi as usize].contains_world(ux, uy)
            {
                return;
            }
        }

        // Read direction from assigned zone's per-zone field, else unified field
        let (gx, gy) = self.units[ai_idx].grid_cell();
        let dir = {
            let flow_state = match faction {
                Faction::Blue => &self.blue_flow,
                _ => &self.red_flow,
            };
            // Try per-zone field first
            let zone_dir = assigned_zone.and_then(|zi| {
                flow_state
                    .zone_fields
                    .get(zi as usize)
                    .and_then(|f| f.as_ref())
                    .map(|f| f.direction_at(gx, gy))
            });
            // Fallback to unified field
            zone_dir.or_else(|| flow_state.field.as_ref().map(|f| f.direction_at(gx, gy)))
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
