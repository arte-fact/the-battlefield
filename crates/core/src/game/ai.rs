use super::*;

/// Monks try to stay at least this far from enemies (3 tiles).
const MONK_SAFE_DIST: f32 = TILE_SIZE * 3.0;

/// Monks stop approaching allies once within this distance (2 tiles).
const MONK_FOLLOW_DIST: f32 = TILE_SIZE * 2.0;

impl Game {
    /// Real-time AI tick: all units get decisions every frame, pathfinding is rate-limited.
    pub fn tick_ai(&mut self, dt: f32) {
        // Build occupied set once per frame (shared across all pathfinding calls)
        self.ai_occupied_cache = self
            .units
            .iter()
            .filter(|u| u.alive)
            .map(|u| u.grid_cell())
            .collect();

        // Update flow fields for both factions before unit loop
        self.update_flow_field_if_needed(Faction::Blue);
        self.update_flow_field_if_needed(Faction::Red);

        let ai_indices: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && !u.is_player)
            .map(|(i, _)| i)
            .collect();

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

        // Player orders take priority (orders cancel rally state)
        if let Some(order) = self.units[ai_idx].order {
            match order {
                OrderKind::Hold { target_x, target_y } => {
                    self.ai_order_hold_tick(ai_idx, target_x, target_y, dt);
                }
                OrderKind::Go { target_x, target_y } => {
                    self.ai_order_go_tick(ai_idx, target_x, target_y, dt);
                }
                OrderKind::Retreat { target_x, target_y } => {
                    self.ai_order_retreat_tick(ai_idx, target_x, target_y, dt);
                }
                OrderKind::Follow => {
                    self.ai_order_follow_tick(ai_idx, dt);
                }
            }
            return;
        }

        let kind = self.units[ai_idx].kind;
        match kind {
            UnitKind::Monk => self.ai_monk_tick(ai_idx, dt),
            UnitKind::Archer => self.ai_archer_tick(ai_idx, dt),
            UnitKind::Warrior | UnitKind::Lancer => self.ai_melee_tick(ai_idx, dt),
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

    /// Real-time melee AI: attack if in melee range and can_act, else move toward.
    fn ai_melee_tick(&mut self, ai_idx: usize, dt: f32) {
        let ai_id = self.units[ai_idx].id;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => {
                self.ai_move_via_flowfield(ai_idx, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        let reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
        let reach = reach.max(MELEE_RANGE);

        if self.units[ai_idx].can_act() && dist <= reach {
            self.execute_attack(ai_id, enemy_id, None);
        } else if dist <= reach {
            // In range but on cooldown — hold position (lancers keep distance)
        } else {
            self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
        }
    }

    /// Real-time archer AI: ranged if in range, melee if adjacent, hold if on cooldown, approach otherwise.
    fn ai_archer_tick(&mut self, ai_idx: usize, dt: f32) {
        let ai_id = self.units[ai_idx].id;
        let range_world = self.units[ai_idx].stats.range as f32 * TILE_SIZE;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => {
                self.ai_move_via_flowfield(ai_idx, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if self.units[ai_idx].can_act() && dist <= range_world {
            self.execute_attack(ai_id, enemy_id, None);
        } else if dist <= range_world {
            // In range but on cooldown — hold position
        } else {
            self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
        }
    }

    /// Compute a standoff point for a monk: a position MONK_FOLLOW_DIST away from
    /// the ally, in the direction from ally back toward the monk. If the monk is
    /// already within MONK_FOLLOW_DIST, returns the monk's own position (hold).
    fn monk_standoff_point(monk_x: f32, monk_y: f32, ally_x: f32, ally_y: f32) -> (f32, f32) {
        let dx = monk_x - ally_x;
        let dy = monk_y - ally_y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < MONK_FOLLOW_DIST {
            return (monk_x, monk_y);
        }
        let ratio = MONK_FOLLOW_DIST / dist;
        (ally_x + dx * ratio, ally_y + dy * ratio)
    }

    /// Real-time monk AI: heal nearby wounded ally if can_act, flee from enemies,
    /// approach wounded allies to heal them, or follow friendlies at standoff distance.
    fn ai_monk_tick(&mut self, ai_idx: usize, dt: f32) {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let ai_id = self.units[ai_idx].id;

        // Healing is handled by try_monk_heal in ai_unit_tick (before orders)

        // Flee if an enemy is too close
        let enemy_dist = self.nearest_enemy_dist(ax, ay, faction);
        if enemy_dist < MONK_SAFE_DIST {
            if let Some(enemy) = self.find_nearest_enemy(ai_idx) {
                let (ex, ey, _, _) = enemy;
                let flee_x = ax + (ax - ex);
                let flee_y = ay + (ay - ey);
                self.ai_move_toward_continuous(ai_idx, flee_x, flee_y, dt);
                return;
            }
        }

        // Move directly toward wounded ally to get in heal range
        let vision_range = Self::AI_VISION_RADIUS as f32 * TILE_SIZE;
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

        // Follow nearest friendly combatant at standoff distance
        let follow_target = self
            .units
            .iter()
            .filter(|u| {
                u.alive && u.faction == faction && u.id != ai_id && u.kind != UnitKind::Monk
            })
            .min_by(|a, b| {
                let da = a.distance_to_pos(ax, ay);
                let db = b.distance_to_pos(ax, ay);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|u| (u.x, u.y));
        if let Some((tx, ty)) = follow_target {
            let (sx, sy) = Self::monk_standoff_point(ax, ay, tx, ty);
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
        // Archer at distance 3 tiles, within range 7 tiles
        game.spawn_unit(UnitKind::Archer, Faction::Red, 8, 5, false);
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
            "Archer should hold position when in range"
        );
    }
}
