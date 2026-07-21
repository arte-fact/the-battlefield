use super::*;

impl Game {
    pub fn tick_ai(&mut self, dt: f32) {
        // Reset per-tick A* budget
        self.astar_budget = self.config.astar_budget_per_tick;

        // Rebuild spatial hash for O(1) neighbour queries (separation, enemy search)
        self.rebuild_spatial();

        self.release_retinue_if_player_dead();
        self.recruit_timer += dt;
        if self.recruit_timer >= self.config.recruit_interval {
            self.recruit_timer = 0.0;
            self.recruitment_pass();
        }

        // Recompute macro objectives periodically, staggering the active
        // factions evenly across the interval so score_all_zones() calls
        // never share a frame.
        let active: Vec<Faction> = self.active_factions().to_vec();
        let interval = self.config.objective_interval;
        let mut refresh_objectives = false;
        if active
            .iter()
            .all(|f| self.macro_objectives[f.idx()].is_empty())
        {
            // First time: compute everyone
            self.objective_timer = 0.0;
            for &f in &active {
                self.macro_objectives[f.idx()] = self.zone_manager.score_all_zones(f, &self.config);
            }
            refresh_objectives = true;
        } else {
            let prev = self.objective_timer;
            self.objective_timer += dt;
            let slot = interval / active.len() as f32;
            for (k, &f) in active.iter().enumerate() {
                let phase = k as f32 * slot;
                let crossed = prev < phase && self.objective_timer >= phase;
                let catch_up =
                    self.objective_timer >= phase && self.macro_objectives[f.idx()].is_empty();
                if crossed || catch_up {
                    self.macro_objectives[f.idx()] =
                        self.zone_manager.score_all_zones(f, &self.config);
                    refresh_objectives = true;
                }
            }
            if self.objective_timer >= interval {
                self.objective_timer = 0.0;
                // Phase 0 recomputes on wrap (prev < 0 never true otherwise).
                self.macro_objectives[active[0].idx()] =
                    self.zone_manager.score_all_zones(active[0], &self.config);
                refresh_objectives = true;
            }
        }

        // Stagger flow field updates: one faction per frame round-robin.
        let turn = self.flow_field_rotation as usize % active.len();
        self.update_flow_fields(active[turn]);
        self.flow_field_rotation = self.flow_field_rotation.wrapping_add(1);

        // Assign per-unit zone objectives when macro objectives are refreshed
        if refresh_objectives {
            self.assign_unit_objectives();
        }

        let mut ai_indices: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && !u.is_player)
            .map(|(i, _)| i)
            .collect();

        // Rotate iteration order per tick: the A* budget is consumed in
        // order, and a fixed order starves late units of pathfinding when
        // demand exceeds the per-tick budget.
        if !ai_indices.is_empty() {
            let start = self.ai_rotation as usize % ai_indices.len();
            ai_indices.rotate_left(start);
            self.ai_rotation = self.ai_rotation.wrapping_add(1);
        }

        for ai_idx in ai_indices {
            if !self.units[ai_idx].alive {
                continue;
            }
            self.ai_unit_tick(ai_idx, dt);
        }
    }

    /// Dispatch real-time AI action based on unit type.
    fn ai_unit_tick(&mut self, ai_idx: usize, dt: f32) {
        // Monks always try to heal nearby wounded allies, even when under orders
        if self.units[ai_idx].kind == UnitKind::Monk && self.try_monk_heal(ai_idx) {
            return;
        }

        // Rally hold: walk to rally point (base center), then idle. Fight in self-defense.
        if self.units[ai_idx].rally_hold && self.units[ai_idx].order.is_none() {
            // Self-defense
            if let Some((ex, ey, enemy_id, dist)) = self.find_nearest_enemy(ai_idx) {
                if self.units[ai_idx].can_act() && dist <= self.attack_reach(ai_idx) {
                    self.attack_target(ai_idx, ex, ey, enemy_id);
                    return;
                }
            }
            // Walk toward rally point (base center)
            let faction = self.units[ai_idx].faction;
            let (rx, ry) = self
                .rally_zone(faction)
                .map(|z| (z.center_gx, z.center_gy))
                .unwrap_or(self.gathers[faction.idx()]);
            let (rwx, rwy) = grid::grid_to_world(rx, ry);
            let dist = self.units[ai_idx].distance_to_pos(rwx, rwy);
            if dist > TILE_SIZE * 2.0 {
                self.ai_move_toward_continuous(ai_idx, rwx, rwy, dt);
            } else {
                self.units[ai_idx].set_anim(UnitAnim::Idle);
            }
            return;
        }

        // Player orders take priority
        if let Some(order) = self.units[ai_idx].order {
            match order {
                OrderKind::Follow => {
                    self.ai_order_follow_tick(ai_idx, dt);
                }
                OrderKind::Charge { target_x, target_y } => {
                    self.ai_order_charge_tick(ai_idx, target_x, target_y, dt);
                }
                OrderKind::Defend {
                    anchor_x,
                    anchor_y,
                    facing_dir,
                } => {
                    self.ai_order_defend_tick(ai_idx, anchor_x, anchor_y, facing_dir, dt);
                }
                OrderKind::DefendZone { zone } => {
                    self.ai_order_defend_zone_tick(ai_idx, zone, dt);
                }
            }
            return;
        }

        let kind = self.units[ai_idx].kind;
        match kind {
            UnitKind::Monk => self.ai_monk_tick(ai_idx, dt),
            UnitKind::Archer => self.ai_archer_tick(ai_idx, dt),
            UnitKind::Lancer => self.ai_lancer_tick(ai_idx, dt),
            UnitKind::Warrior => self.ai_melee_tick(ai_idx, dt),
        }
    }

    /// Try to heal a nearby wounded ally. Returns true if a heal was performed.
    pub(super) fn try_monk_heal(&mut self, ai_idx: usize) -> bool {
        if !self.units[ai_idx].can_act() {
            return false;
        }
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let ai_id = self.units[ai_idx].id;
        let heal_range = self.units[ai_idx].stats.range as f32 * TILE_SIZE;

        let heal_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
            .filter(|u| {
                let dist = u.distance_to_pos(ax, ay);
                dist <= heal_range && u.hp < u.stats.max_hp
            })
            .min_by_key(|u| u.hp)
            .map(|u| u.id);

        if let Some(target_id) = heal_target {
            self.execute_heal(ai_idx, target_id);
            return true;
        }
        false
    }

    /// Resolve combat target with persistence: stick with the current target
    /// for `combat_target_commit_secs` before re-scanning. Prevents frame-to-frame
    /// target flipping and combat/flowfield mode oscillation.
    fn resolve_combat_target(&mut self, ai_idx: usize) -> Option<(f32, f32, UnitId, f32)> {
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let vision_range = self.config.ai_vision_radius as f32 * TILE_SIZE;
        let disengage_range = vision_range + self.config.combat_disengage_margin_tiles * TILE_SIZE;

        // Try to keep current target
        if let Some(tid) = self.units[ai_idx].combat_target {
            if self.units[ai_idx].combat_target_timer > 0.0 {
                if let Some(ti) = self.units.iter().position(|u| u.id == tid && u.alive) {
                    let dist = self.units[ti].distance_to_pos(ax, ay);
                    if dist <= disengage_range {
                        return Some((self.units[ti].x, self.units[ti].y, tid, dist));
                    }
                }
            }
            // Target dead, too far, or timer expired — clear
            self.units[ai_idx].combat_target = None;
            self.units[ai_idx].combat_target_timer = 0.0;
        }

        // Scan for a new target
        let enemy = self.find_nearest_enemy(ai_idx);
        if let Some((ex, ey, eid, dist)) = enemy {
            self.units[ai_idx].combat_target = Some(eid);
            self.units[ai_idx].combat_target_timer = self.config.combat_target_commit_secs;
            return Some((ex, ey, eid, dist));
        }
        None
    }

    /// Move toward a combat target via A*, tracking failures: a target that
    /// is visible but unreachable (e.g. across water) stalls the unit forever
    /// otherwise. After repeated failures the chase is abandoned and combat
    /// acquisition shrinks to attack reach until `chase_block_timer` expires,
    /// letting flow-field movement route around the obstacle.
    pub(super) fn chase_enemy(&mut self, ai_idx: usize, ex: f32, ey: f32, dt: f32) {
        const MAX_FAIL_STREAK: u8 = 3;
        self.last_path_result = Some(true);
        self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
        match self.last_path_result {
            Some(false) => {
                let u = &mut self.units[ai_idx];
                u.chase_fail_streak += 1;
                if u.chase_fail_streak >= MAX_FAIL_STREAK {
                    u.chase_fail_streak = 0;
                    u.chase_block_timer = self.config.chase_block_secs;
                    u.combat_target = None;
                    u.combat_target_timer = 0.0;
                }
            }
            Some(true) => self.units[ai_idx].chase_fail_streak = 0,
            None => {}
        }
    }

    /// Real-time melee AI: attack if in range, else close distance to enemy.
    /// If no enemy visible, follow flow field toward zone objective.
    pub(super) fn ai_melee_tick(&mut self, ai_idx: usize, dt: f32) {
        let enemy = match self.resolve_combat_target(ai_idx) {
            Some(e) => e,
            None => {
                self.ai_move_via_flowfield(ai_idx, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if self.units[ai_idx].can_act() && dist <= self.attack_reach(ai_idx) {
            self.attack_target(ai_idx, ex, ey, enemy_id);
        } else {
            // Enemy visible — close distance
            self.chase_enemy(ai_idx, ex, ey, dt);
        }
    }

    /// Real-time archer AI: attack if in range, kite if too close, approach if too far.
    /// If no enemy visible, follow flow field toward zone objective.
    pub(super) fn ai_archer_tick(&mut self, ai_idx: usize, dt: f32) {
        let ai_id = self.units[ai_idx].id;
        let range_world = self.units[ai_idx].stats.range as f32 * TILE_SIZE;

        let enemy = match self.resolve_combat_target(ai_idx) {
            Some(e) => e,
            None => {
                self.units[ai_idx].is_kiting = false;
                self.ai_move_via_flowfield(ai_idx, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        // Kite hysteresis: enter at range*0.5, exit at range*0.5 + margin
        let kite_enter = range_world * 0.5;
        let kite_exit = kite_enter + self.config.kite_hysteresis_tiles * TILE_SIZE;
        if dist < kite_enter {
            self.units[ai_idx].is_kiting = true;
        } else if dist > kite_exit {
            self.units[ai_idx].is_kiting = false;
        }

        if self.units[ai_idx].can_act() && dist <= range_world {
            self.execute_attack(ai_id, enemy_id, None);
        } else if self.units[ai_idx].is_kiting {
            // Enemy too close — back away to maintain range
            let ax = self.units[ai_idx].x;
            let ay = self.units[ai_idx].y;
            let flee_x = ax + (ax - ex);
            let flee_y = ay + (ay - ey);
            self.ai_move_toward_continuous(ai_idx, flee_x, flee_y, dt);
        } else if dist <= range_world {
            // In range but on cooldown — hold position
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            // Out of range — approach enemy
            self.chase_enemy(ai_idx, ex, ey, dt);
        }
    }

    /// Real-time lancer AI: attack at reach distance, maintain standoff.
    /// If no enemy visible, follow flow field toward zone objective.
    pub(super) fn ai_lancer_tick(&mut self, ai_idx: usize, dt: f32) {
        let enemy = match self.resolve_combat_target(ai_idx) {
            Some(e) => e,
            None => {
                self.units[ai_idx].is_backing_off = false;
                self.ai_move_via_flowfield(ai_idx, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        let reach = self.attack_reach(ai_idx);

        // Backoff hysteresis: enter at MELEE_RANGE*0.7, exit at that + margin
        let backoff_enter = MELEE_RANGE * 0.7;
        let backoff_exit = backoff_enter + self.config.lancer_backoff_hysteresis_tiles * TILE_SIZE;
        if dist <= backoff_enter {
            self.units[ai_idx].is_backing_off = true;
        } else if dist > backoff_exit {
            self.units[ai_idx].is_backing_off = false;
        }

        if self.units[ai_idx].can_act() && dist <= reach {
            self.attack_target(ai_idx, ex, ey, enemy_id);
        } else if self.units[ai_idx].is_backing_off {
            // Enemy inside melee range — back off to use reach advantage
            let ax = self.units[ai_idx].x;
            let ay = self.units[ai_idx].y;
            let away_x = ax + (ax - ex);
            let away_y = ay + (ay - ey);
            self.ai_move_toward_continuous(ai_idx, away_x, away_y, dt);
        } else {
            // Close distance to reach range
            self.chase_enemy(ai_idx, ex, ey, dt);
        }
    }

    /// Compute a standoff point for a monk: a position monk_follow_dist away from
    /// the ally, in the direction from ally back toward the monk.
    fn monk_standoff_point(
        monk_x: f32,
        monk_y: f32,
        ally_x: f32,
        ally_y: f32,
        follow_dist: f32,
    ) -> (f32, f32) {
        let dx = monk_x - ally_x;
        let dy = monk_y - ally_y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.01 || dist < follow_dist {
            return (monk_x, monk_y);
        }
        let ratio = follow_dist / dist;
        (ally_x + dx * ratio, ally_y + dy * ratio)
    }

    /// Real-time monk AI: heal nearby wounded ally if can_act, flee from enemies,
    /// approach wounded allies to heal them, or follow friendlies at standoff distance.
    pub(super) fn ai_monk_tick(&mut self, ai_idx: usize, dt: f32) {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let ai_id = self.units[ai_idx].id;

        // Healing is handled by try_monk_heal in ai_unit_tick (before orders)

        // Flee if a visible enemy is too close
        let monk_safe = self.config.monk_safe_dist_tiles * TILE_SIZE;
        if let Some((ex, ey, _, dist)) = self.resolve_combat_target(ai_idx) {
            if dist < monk_safe {
                let flee_x = ax + (ax - ex);
                let flee_y = ay + (ay - ey);
                self.ai_move_toward_continuous(ai_idx, flee_x, flee_y, dt);
                return;
            }
        }

        // Move directly toward wounded ally to get in heal range
        let vision_range = self.config.ai_vision_radius as f32 * TILE_SIZE;
        let wounded_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
            .filter(|u| u.hp < u.stats.max_hp)
            .filter(|u| u.distance_to_pos(ax, ay) <= vision_range)
            .min_by_key(|u| u.hp)
            .map(|u| (u.x, u.y));

        if let Some((tx, ty)) = wounded_target {
            self.ai_move_toward_continuous(ai_idx, tx, ty, dt);
            return;
        }

        // Follow nearest ADVANCING combatant (not rally_hold, not player, not monk).
        // This prevents monks from orbiting the base when the player stays home.
        let max_follow = self.config.monk_max_follow_tiles * TILE_SIZE;
        let monk_follow_dist = self.config.monk_follow_dist_tiles * TILE_SIZE;
        let follow_target = self
            .units
            .iter()
            .filter(|u| {
                u.alive
                    && u.faction == faction
                    && u.id != ai_id
                    && u.kind != UnitKind::Monk
                    && !u.is_player
                    && !u.rally_hold
            })
            .filter(|u| u.distance_to_pos(ax, ay) <= max_follow)
            .min_by(|a, b| {
                let da = a.distance_to_pos(ax, ay);
                let db = b.distance_to_pos(ax, ay);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|u| (u.x, u.y));
        if let Some((tx, ty)) = follow_target {
            let (sx, sy) = Self::monk_standoff_point(ax, ay, tx, ty, monk_follow_dist);
            self.ai_move_toward_continuous(ai_idx, sx, sy, dt);
            return;
        }

        // No allies nearby — advance via flowfield toward objective
        self.ai_move_via_flowfield(ai_idx, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assault_captures_undefended_enemy_zone() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.clear();
        for z in &mut game.zone_manager.zones {
            z.set_controlled(Faction::Blue);
        }
        let z = (
            game.zone_manager.zones[3].center_gx,
            game.zone_manager.zones[3].center_gy,
        );
        for i in 0..8 {
            game.spawn_unit(
                UnitKind::Warrior,
                Faction::Red,
                z.0.saturating_sub(10),
                z.1.saturating_sub(2) + i % 4,
                false,
            );
        }
        for _ in 0..7200 {
            game.tick_ai(1.0 / 60.0);
            game.tick_cooldowns(1.0 / 60.0);
            game.tick_zones(1.0 / 60.0);
        }
        // The old radius+margin stop parked assaults outside the capture
        // circle forever; with the ownership-aware settle they must take
        // at least their focused target within a minute.
        let red_zones = game
            .zone_manager
            .zones
            .iter()
            .filter(|z| z.state == crate::zone::ZoneState::Controlled(Faction::Red))
            .count();
        assert!(
            red_zones >= 1,
            "an unopposed 8-warrior assault must capture a zone, states: {:?}",
            game.zone_manager
                .zones
                .iter()
                .map(|z| z.state)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn settled_units_sit_inside_capture_radius() {
        let mut game = Game::new(960.0, 640.0);
        game.setup_demo_battle_with_seed(42);
        game.units.clear();
        let z = (
            game.zone_manager.zones[3].center_gx,
            game.zone_manager.zones[3].center_gy,
        );
        for i in 0..6 {
            game.spawn_unit(
                UnitKind::Warrior,
                Faction::Red,
                z.0.saturating_sub(12),
                z.1.saturating_sub(1) + i % 3,
                false,
            );
        }
        // March until the army holds every zone, then give it a settle grace.
        let mut all_held_at = None;
        for t in 0..36000 {
            game.tick_ai(1.0 / 60.0);
            game.tick_cooldowns(1.0 / 60.0);
            game.tick_zones(1.0 / 60.0);
            if all_held_at.is_none()
                && game.zone_manager.all_zones_controlled_by() == Some(Faction::Red)
            {
                all_held_at = Some(t);
            }
            if let Some(t0) = all_held_at {
                if t >= t0 + 3600 {
                    break;
                }
            }
        }
        assert!(
            all_held_at.is_some(),
            "6-unit army should capture every zone within 10 simulated minutes"
        );
        let inside = game
            .units
            .iter()
            .filter(|u| {
                u.alive
                    && u.assigned_zone
                        .map(|zi| {
                            let z = &game.zone_manager.zones[zi as usize];
                            z.contains_world(u.x, u.y)
                        })
                        .unwrap_or(false)
            })
            .count();
        let alive = game.units.iter().filter(|u| u.alive).count();
        assert!(
            inside * 2 >= alive,
            "settled units must hold ground inside their zone's capture radius ({inside}/{alive})"
        );
    }

    #[test]
    fn unreachable_visible_enemy_triggers_chase_block() {
        let mut game = Game::new(960.0, 640.0);
        // Full-height water wall: enemy is visible across it but unpathable
        for y in 0..GRID_SIZE {
            game.grid.set(30, y, TileKind::Water);
        }
        game.grid.recompute_caches();
        let a = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 28, 20, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 32, 20, false);

        for _ in 0..30 {
            game.tick_ai(0.1);
        }

        let u = game.units.iter().find(|u| u.id == a).unwrap();
        assert!(
            u.chase_block_timer > 0.0,
            "failed chases must trigger the chase block"
        );
        assert_eq!(u.combat_target, None);
    }

    #[test]
    fn starved_astar_budget_still_moves_units() {
        let mut game = Game::new(960.0, 640.0);
        game.config.astar_budget_per_tick = 0;
        let a = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 20, 20, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 26, 20, false);
        let x0 = game.units.iter().find(|u| u.id == a).unwrap().x;

        for _ in 0..30 {
            game.tick_ai(1.0 / 60.0);
        }

        let u = game.units.iter().find(|u| u.id == a).unwrap();
        assert!(
            u.x > x0 + TILE_SIZE,
            "unit must steer toward its target even with no pathfinding budget"
        );
    }

    #[test]
    fn chase_block_still_fights_within_reach() {
        let mut game = Game::new(960.0, 640.0);
        let a = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 20, 20, false);
        let b = game.spawn_unit(UnitKind::Warrior, Faction::Red, 21, 20, false);
        game.units.iter_mut().for_each(|u| {
            u.chase_block_timer = 10.0;
            u.attack_cooldown = 0.0;
        });

        game.tick_ai(0.1);

        let hp = game.units.iter().find(|u| u.id == b).unwrap().hp;
        let max = game.units.iter().find(|u| u.id == a).unwrap().stats.max_hp;
        assert!(hp < max, "blocked units still fight enemies in reach");
    }

    #[test]
    fn successful_chase_resets_fail_streak() {
        let mut game = Game::new(960.0, 640.0);
        let a = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 20, 20, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 26, 20, false);
        game.units
            .iter_mut()
            .find(|u| u.id == a)
            .unwrap()
            .chase_fail_streak = 2;

        for _ in 0..5 {
            game.tick_ai(0.1);
        }

        let u = game.units.iter().find(|u| u.id == a).unwrap();
        assert_eq!(u.chase_fail_streak, 0);
        assert_eq!(u.chase_block_timer, 0.0);
    }

    #[test]
    fn tick_ai_melee_moves_when_ready() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 10, 5, false);
        game.units[1].attack_cooldown = 0.0;
        let start_x = game.units[1].x;
        // Run AI for several frames to let it path and move
        for _ in 0..60 {
            game.tick_ai(0.016);
        }
        let enemy = game.units.iter().find(|u| !u.is_player && u.alive).unwrap();
        assert!(
            enemy.x < start_x,
            "AI melee should have moved toward player"
        );
    }

    #[test]
    fn tick_ai_archer_holds_in_range() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Archer at distance 5 tiles, within range (7) but beyond kite threshold (3.5)
        game.spawn_unit(UnitKind::Archer, Faction::Red, 10, 5, false);
        let start_x = game.units[1].x;
        game.units[1].attack_cooldown = 0.5;
        game.tick_ai(0.016);
        let archer = game
            .units
            .iter()
            .find(|u| u.kind == UnitKind::Archer)
            .unwrap();
        assert!(
            (archer.x - start_x).abs() < 1.0,
            "Archer should hold position when in range but beyond kite distance"
        );
    }

    #[test]
    fn tick_ai_archer_kites_when_too_close() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Archer at distance 2 tiles — inside kite threshold (range*0.5 = 3.5 tiles)
        game.spawn_unit(UnitKind::Archer, Faction::Red, 7, 5, false);
        let start_x = game.units[1].x;
        game.units[1].attack_cooldown = 0.5;
        game.tick_ai(0.016);
        let archer = game
            .units
            .iter()
            .find(|u| u.kind == UnitKind::Archer)
            .unwrap();
        assert!(
            archer.x > start_x,
            "Archer should kite away when enemy is too close, before={start_x} after={}",
            archer.x
        );
    }
}
