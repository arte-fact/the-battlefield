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
                // Defer to next frame — keep following current waypoints.
                // Without any, steer straight at the target: under sustained
                // demand a unit may never win a budget slot, and standing
                // still until it does freezes entire battles.
                self.units[ai_idx].ai_path_cooldown = self.config.deferred_repath_delay;
                self.last_path_result = None;
                if self.units[ai_idx].ai_waypoint_idx >= self.units[ai_idx].ai_waypoints.len() {
                    self.steer_toward(ai_idx, target_x, target_y, dt);
                    return;
                }
            } else {
                self.astar_budget -= 1;

                let (ax, ay) = self.units[ai_idx].grid_cell();
                let (gx, gy) = grid::world_to_grid(target_x, target_y);
                let gx = gx.max(0) as u32;
                let gy = gy.max(0) as u32;

                let path =
                    self.grid
                        .find_path(ax, ay, gx, gy, self.config.astar_search_limit, |_, _| false);

                self.last_path_result = Some(path.is_some());
                if let Some(steps) = path {
                    self.units[ai_idx].ai_waypoints = steps
                        .iter()
                        .map(|&(x, y)| grid::grid_to_world(x, y))
                        .collect();
                    self.units[ai_idx].ai_waypoint_idx = 0;
                    // Jitter cooldown using golden ratio to spread units evenly
                    let golden = 0.618034;
                    let jitter = ((self.units[ai_idx].id as f32 * golden) % 1.0)
                        * self.config.repath_cooldown_mod;
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
            } else {
                self.steer_toward(ai_idx, wx, wy, dt);
            }
        }
    }

    /// Separation-blended steering toward a world position (collision-checked).
    pub(super) fn steer_toward(&mut self, ai_idx: usize, wx: f32, wy: f32, dt: f32) {
        let ux = self.units[ai_idx].x;
        let uy = self.units[ai_idx].y;
        let ddx = wx - ux;
        let ddy = wy - uy;
        let dist = (ddx * ddx + ddy * ddy).sqrt();
        if dist <= 0.01 {
            return;
        }
        let dir_x = ddx / dist;
        let dir_y = ddy / dist;
        let (raw_sep_x, raw_sep_y) = self.compute_separation(ai_idx);
        let alpha = self.config.separation_smoothing;
        self.units[ai_idx].sep_smooth_x =
            self.units[ai_idx].sep_smooth_x * (1.0 - alpha) + raw_sep_x * alpha;
        self.units[ai_idx].sep_smooth_y =
            self.units[ai_idx].sep_smooth_y * (1.0 - alpha) + raw_sep_y * alpha;
        let sep_x = self.units[ai_idx].sep_smooth_x;
        let sep_y = self.units[ai_idx].sep_smooth_y;
        let bx = dir_x * self.config.flow_weight + sep_x * self.config.separation_weight;
        let by = dir_y * self.config.flow_weight + sep_y * self.config.separation_weight;
        let len = (bx * bx + by * by).sqrt();
        if len > 0.01 {
            self.move_unit(ai_idx, bx / len, by / len, dt);
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
        self.faction_objectives[faction.idx()]
    }

    /// Return the world position of the objective nearest to a unit (Euclidean).
    /// Used for the zone-stop check and A* fallback.
    fn nearest_objective_pos(&self, ai_idx: usize) -> (f32, f32) {
        let faction = self.units[ai_idx].faction;
        if faction == Faction::Villager {
            // Militia has no strategic objectives; it stays where it is.
            return (self.units[ai_idx].x, self.units[ai_idx].y);
        }
        let fi = faction.idx();
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

            // Seed all passable cells inside the zone radius so units spread across the area
            let zone = &self.zone_manager.zones[zi];
            let r = zone.radius as i32;
            let mut goals = Vec::new();
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let nx = gx as i32 + dx;
                    let ny = gy as i32 + dy;
                    if self.grid.in_bounds(nx, ny) && self.grid.is_passable(nx as u32, ny as u32) {
                        goals.push((nx as u32, ny as u32, 0));
                    }
                }
            }
            if goals.is_empty() {
                goals.push((gx, gy, 0));
            }
            let field = crate::flowfield::FlowField::generate_multi_source(&self.grid, &goals);
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

    /// Assign a unit to a zone, resetting arrival state when the target changes.
    fn assign_zone(&mut self, ui: usize, target: u8, lock_dur: f32) {
        if self.units[ui].assigned_zone != Some(target) {
            self.units[ui].zone_arrived = false;
        }
        self.units[ui].assigned_zone = Some(target);
        self.units[ui].zone_lock_timer = lock_dur;
    }

    /// Faction-level objective planner: picks 1-2 target zones and assigns
    /// all units in bulk. Concentrates force instead of spreading thin.
    pub(super) fn assign_unit_objectives(&mut self) {
        let zone_count = self.zone_manager.zones.len();
        if zone_count == 0 {
            return;
        }

        for &faction in self.active_factions() {
            let fi = faction.idx();
            let objectives = &self.macro_objectives[fi];
            if objectives.is_empty() {
                continue;
            }

            // Find defend target (Tier 1: score >= 200) and attack target (Tier 2: score >= 85)
            let mut defend_zone: Option<u8> = None;
            let mut attack_zone: Option<u8> = None;
            let mut zone_scores: Vec<(u8, f32)> = Vec::new();

            for &(wx, wy, score) in objectives {
                let zi =
                    self.zone_manager.zones.iter().position(|z| {
                        (z.center_wx - wx).abs() < 1.0 && (z.center_wy - wy).abs() < 1.0
                    });
                let Some(zi) = zi else { continue };
                zone_scores.push((zi as u8, score));

                if score >= 200.0 && defend_zone.is_none() {
                    defend_zone = Some(zi as u8);
                } else if score >= 85.0 && attack_zone.is_none() {
                    attack_zone = Some(zi as u8);
                }
            }

            // Target stickiness: capture completion collapses a zone's score
            // and boundary flicker resurrects it seconds later, so a raw
            // argmax teleports the whole army back and forth. Keep the
            // current target while it still qualifies unless the challenger
            // clearly beats it.
            let score_of = |zi: Option<u8>| -> f32 {
                zi.and_then(|z| zone_scores.iter().find(|(i, _)| *i == z))
                    .map(|&(_, s)| s)
                    .unwrap_or(0.0)
            };
            let (prev_defend, prev_attack) = self.planner_targets[fi];
            if prev_defend != defend_zone {
                let prev_score = score_of(prev_defend);
                if prev_score >= 200.0 && score_of(defend_zone) < prev_score * 1.3 {
                    defend_zone = prev_defend;
                }
            }
            if prev_attack != attack_zone {
                let prev_score = score_of(prev_attack);
                if prev_score >= 85.0
                    && prev_attack != defend_zone
                    && score_of(attack_zone) < prev_score * 1.3
                {
                    attack_zone = prev_attack;
                }
            }
            self.planner_targets[fi] = (defend_zone, attack_zone);

            // Collect available AI units for this faction, sorted by index.
            // Skip rally_hold units (still assembling) and zone-locked units (mid-travel).
            let mut available: Vec<usize> = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| {
                    u.alive
                        && !u.is_player
                        && u.faction == faction
                        && !u.rally_hold
                        && u.zone_lock_timer <= 0.0
                })
                .map(|(i, _)| i)
                .collect();

            if available.is_empty() {
                continue;
            }

            let lock_dur = self.config.zone_lock_duration;

            match (defend_zone, attack_zone) {
                (Some(def_zi), Some(atk_zi)) => {
                    // Split: 40% defend, 60% attack. Sort by flow cost to each target.
                    let n_defend =
                        ((available.len() as f32 * 0.4).ceil() as usize).min(available.len());

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
                    for (i, ui) in available.clone().into_iter().enumerate() {
                        let target = if i < n_defend { def_zi } else { atk_zi };
                        self.assign_zone(ui, target, lock_dur);
                    }
                }
                (None, Some(atk_zi)) => {
                    // All-in attack
                    for ui in available.clone() {
                        self.assign_zone(ui, atk_zi, lock_dur);
                    }
                }
                (Some(def_zi), None) => {
                    // Only defending — all to defend target
                    for ui in available.clone() {
                        self.assign_zone(ui, def_zi, lock_dur);
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
                        let fallback_zi = self
                            .zone_manager
                            .zones
                            .iter()
                            .position(|z| {
                                let (wx, wy) = (z.center_wx, z.center_wy);
                                objectives.iter().any(|(ox, oy, _)| {
                                    (ox - wx).abs() < 1.0 && (oy - wy).abs() < 1.0
                                })
                            })
                            .or_else(|| {
                                // No objective match — pick nearest non-owned zone to push toward
                                let sample_idx = available[0];
                                let ux = self.units[sample_idx].x;
                                let uy = self.units[sample_idx].y;
                                self.zone_manager
                                    .zones
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, z)| z.state != ZoneState::Controlled(faction))
                                    .min_by(|(_, a), (_, b)| {
                                        let da =
                                            (ux - a.center_wx).powi(2) + (uy - a.center_wy).powi(2);
                                        let db =
                                            (ux - b.center_wx).powi(2) + (uy - b.center_wy).powi(2);
                                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                                    })
                                    .map(|(i, _)| i)
                            });
                        if let Some(zi) = fallback_zi {
                            for ui in available.clone() {
                                self.assign_zone(ui, zi as u8, lock_dur);
                            }
                        }
                    } else {
                        // Distribute evenly by round-robin
                        for (i, ui) in available.clone().into_iter().enumerate() {
                            self.assign_zone(ui, owned[i % owned.len()], lock_dur);
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

        // Hold ground at the assigned zone on a per-unit scattered point
        // (golden-angle disc keyed by unit id). Settling everyone at the
        // first ring they crossed crusted the army at the circle edge,
        // where inbound traffic shoved the idle clump around — the visible
        // boundary jitter. Distributed hold points fill the zone interior
        // instead. The outer radius+margin boundary remains only as exit
        // hysteresis against collision jitter.
        if let Some(zi) = assigned_zone {
            let zi_usize = zi as usize;
            if zi_usize < self.zone_manager.zones.len() {
                let zone = &self.zone_manager.zones[zi_usize];
                let dx = ux - zone.center_wx;
                let dy = uy - zone.center_wy;
                let dist_sq = dx * dx + dy * dy;
                let r = zone.radius as f32 * TILE_SIZE;
                let exit = r + self.config.zone_idle_margin_tiles * TILE_SIZE;

                let id = self.units[ai_idx].id;
                let (hx, hy) = match zone_hold_point(&self.grid, zone, id) {
                    Some(p) => p,
                    None => {
                        // No standable point found: hold in place once
                        // inside, else aim just inside the circle edge.
                        if dist_sq <= (r * 0.85) * (r * 0.85) {
                            (ux, uy)
                        } else {
                            let d = dist_sq.sqrt().max(1.0);
                            (
                                zone.center_wx + dx / d * r * 0.7,
                                zone.center_wy + dy / d * r * 0.7,
                            )
                        }
                    }
                };

                let hd_sq = (ux - hx) * (ux - hx) + (uy - hy) * (uy - hy);
                if hd_sq <= TILE_SIZE * TILE_SIZE {
                    self.units[ai_idx].zone_arrived = true;
                } else if dist_sq > exit * exit {
                    self.units[ai_idx].zone_arrived = false;
                }
                if self.units[ai_idx].zone_arrived {
                    self.units[ai_idx].set_anim(UnitAnim::Idle);
                    return;
                }
                // Inside the circle but short of the hold point: walk to it
                // directly instead of following the center-seeking flow.
                if dist_sq <= r * r {
                    self.steer_toward(ai_idx, hx, hy, dt);
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
                let (raw_sep_x, raw_sep_y) = self.compute_separation(ai_idx);
                let alpha = self.config.separation_smoothing;
                self.units[ai_idx].sep_smooth_x =
                    self.units[ai_idx].sep_smooth_x * (1.0 - alpha) + raw_sep_x * alpha;
                self.units[ai_idx].sep_smooth_y =
                    self.units[ai_idx].sep_smooth_y * (1.0 - alpha) + raw_sep_y * alpha;
                let sep_x = self.units[ai_idx].sep_smooth_x;
                let sep_y = self.units[ai_idx].sep_smooth_y;
                let bx =
                    dir.0 as f32 * self.config.flow_weight + sep_x * self.config.separation_weight;
                let by =
                    dir.1 as f32 * self.config.flow_weight + sep_y * self.config.separation_weight;
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

impl Game {
    /// Diagnostic snapshot of unit movement state (bench/debug tooling).
    pub fn flow_diagnostics(&self) -> String {
        let mut out = String::new();
        for (fi, faction) in [(0usize, Faction::Blue), (1, Faction::Red)] {
            let flow = if fi == 0 {
                &self.blue_flow
            } else {
                &self.red_flow
            };
            let mut unassigned = 0;
            let mut idle_in_zone = 0;
            let mut flowing = 0;
            let mut unreachable = 0;
            let mut zone_hist = std::collections::BTreeMap::new();
            for u in self
                .units
                .iter()
                .filter(|u| u.alive && !u.is_player && u.faction == faction)
            {
                let Some(zi) = u.assigned_zone else {
                    unassigned += 1;
                    continue;
                };
                *zone_hist.entry(zi).or_insert(0) += 1;
                let z = &self.zone_manager.zones[zi as usize];
                let dx = u.x - z.center_wx;
                let dy = u.y - z.center_wy;
                let stop_r =
                    z.radius as f32 * TILE_SIZE + self.config.zone_idle_margin_tiles * TILE_SIZE;
                if dx * dx + dy * dy <= stop_r * stop_r {
                    idle_in_zone += 1;
                    continue;
                }
                let (gx, gy) = u.grid_cell();
                let cost = flow
                    .zone_fields
                    .get(zi as usize)
                    .and_then(|f| f.as_ref())
                    .map(|f| f.cost_at(gx, gy))
                    .unwrap_or(u32::MAX);
                if cost == u32::MAX {
                    unreachable += 1;
                } else {
                    flowing += 1;
                }
            }
            let mut dist_sum = 0.0f32;
            let mut dist_n = 0;
            for u in self
                .units
                .iter()
                .filter(|u| u.alive && !u.is_player && u.faction == faction)
            {
                if let Some(zi) = u.assigned_zone {
                    let z = &self.zone_manager.zones[zi as usize];
                    dist_sum += ((u.x - z.center_wx).powi(2) + (u.y - z.center_wy).powi(2)).sqrt();
                    dist_n += 1;
                }
            }
            let avg_dist = if dist_n > 0 {
                dist_sum / dist_n as f32 / TILE_SIZE
            } else {
                0.0
            };
            out.push_str(&format!(
                "{faction:?}: unassigned={unassigned} idle_in_zone={idle_in_zone} flowing={flowing} unreachable={unreachable} avg_dist_tiles={avg_dist:.1} zones={zone_hist:?}\n"
            ));
            let mut front: Vec<usize> = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| {
                    u.alive && !u.is_player && u.faction == faction && u.assigned_zone.is_some()
                })
                .map(|(i, _)| i)
                .collect();
            front.sort_by(|&a, &b| {
                let d = |i: usize| {
                    let u = &self.units[i];
                    let z = &self.zone_manager.zones[u.assigned_zone.unwrap() as usize];
                    (u.x - z.center_wx).powi(2) + (u.y - z.center_wy).powi(2)
                };
                d(a).partial_cmp(&d(b)).unwrap()
            });
            for &i in front.iter().take(8) {
                let u = &self.units[i];
                let zi = u.assigned_zone.unwrap();
                let (gx, gy) = u.grid_cell();
                let dir = flow
                    .zone_fields
                    .get(zi as usize)
                    .and_then(|f| f.as_ref())
                    .map(|f| f.direction_at(gx, gy))
                    .unwrap_or((0, 0));
                let enemy = self
                    .units
                    .iter()
                    .filter(|e| e.alive && e.faction != faction)
                    .map(|e| {
                        let d = ((e.x - u.x).powi(2) + (e.y - u.y).powi(2)).sqrt();
                        (d, e.x, e.y)
                    })
                    .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                let (edist, elos) = match enemy {
                    Some((d, ex, ey)) => (d / TILE_SIZE, self.has_line_of_sight(u.x, u.y, ex, ey)),
                    None => (-1.0, false),
                };
                out.push_str(&format!(
                    "  front id={} kind={:?} hp={} pos=({:.0},{:.0}) dir={dir:?} enemy_dist={edist:.1} los={elos} cd={:.2} target={:?} wp={}\n",
                    u.id, u.kind, u.hp, u.x / TILE_SIZE, u.y / TILE_SIZE,
                    u.attack_cooldown, u.combat_target, u.ai_waypoints.len(),
                ));
            }
            out.push_str(&format!(
                "  objectives={:?}\n",
                self.macro_objectives[fi]
                    .iter()
                    .map(|&(x, y, s)| (x as u32 / 64, y as u32 / 64, s as i32))
                    .collect::<Vec<_>>()
            ));
        }
        out
    }
}

/// Personal hold position inside a capture zone, keyed by unit id
/// (golden-angle disc). A unit is a wide circle, so candidates must be
/// circle-passable — a tile-passable spot beside the centre tower still
/// grinds the unit against it. Falls back through further golden angles
/// before giving up.
pub(super) fn zone_hold_point(
    grid: &crate::grid::Grid,
    zone: &crate::zone::CaptureZone,
    id: crate::unit::UnitId,
) -> Option<(f32, f32)> {
    let r = zone.radius as f32 * TILE_SIZE;
    let idf = id as f32;
    let rad_frac = ((idf * 0.618034) % 1.0).sqrt();
    let hold_r = r * (0.35 + 0.45 * rad_frac);
    for k in 0..8u32 {
        let angle = (idf + k as f32) * 2.39996;
        let hx = zone.center_wx + angle.cos() * hold_r;
        let hy = zone.center_wy + angle.sin() * hold_r;
        if grid.is_circle_passable(hx, hy, crate::unit::UNIT_RADIUS) {
            return Some((hx, hy));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn zone_hold_points_are_standable() {
        for seed in [42, 777, 9999] {
            let mut game = Game::new(960.0, 640.0);
            game.setup_demo_battle_with_seed(seed);
            for zone in &game.zone_manager.zones {
                let r = zone.radius as f32 * TILE_SIZE;
                for id in 1..300u32 {
                    let p = zone_hold_point(&game.grid, zone, id);
                    let (hx, hy) = p
                        .unwrap_or_else(|| panic!("seed {seed} zone {} id {id}: no hold", zone.id));
                    assert!(
                        game.grid
                            .is_circle_passable(hx, hy, crate::unit::UNIT_RADIUS),
                        "seed {seed} zone {} id {id}: hold not circle-passable",
                        zone.id
                    );
                    let d = ((hx - zone.center_wx).powi(2) + (hy - zone.center_wy).powi(2)).sqrt();
                    assert!(
                        d < r * 0.85,
                        "seed {seed} zone {} id {id}: hold outside settle band ({:.0}px)",
                        zone.id,
                        d
                    );
                }
            }
        }
    }

    use super::*;

    #[test]
    fn ai_melee_marches_to_objective() {
        let mut game = Game::new(960.0, 640.0);
        // Set up objective to the right
        game.faction_objectives[0] = grid::grid_to_world(50, 5);
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
            settlements: Vec::new(),
            extra_bases: Vec::new(),
        };
        game.zone_manager = ZoneManager::create_from_layout(&layout, game.config.zone_radius);
        game.faction_objectives[0] = grid::grid_to_world(138, 138);
        let obj = game.faction_objective(Faction::Blue);
        let (base_wx, _) = grid::grid_to_world(138, 138);
        assert!(
            obj.0 < base_wx,
            "Blue should target a zone (x < {base_wx}), got x={}",
            obj.0
        );
    }
}
