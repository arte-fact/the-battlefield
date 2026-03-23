use super::*;

/// Radius around the player within which units receive orders (~7 tiles).
const ORDER_RADIUS: f32 = TILE_SIZE * 7.0;

/// Duration of the order flash indicator in seconds.
pub const ORDER_FLASH_DURATION: f32 = 1.0;

impl Game {
    /// Issue a player order to nearby friendly units.
    /// Returns the number of units that acknowledged the order.
    pub fn issue_order(&mut self, order_type: &str) -> usize {
        let (player_x, player_y, player_faction) = match self.player_unit() {
            Some(p) => (p.x, p.y, p.faction),
            None => return 0,
        };

        // Collect eligible unit indices
        let eligible: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.alive
                    && !u.is_player
                    && u.faction == player_faction
                    && u.distance_to_pos(player_x, player_y) <= ORDER_RADIUS
            })
            .map(|(i, _)| i)
            .collect();

        let mut acknowledged = 0usize;
        for idx in eligible {
            // Deterministic ~85% follow chance based on unit id
            let follows = {
                let hash = self.units[idx].id.wrapping_mul(2654435761);
                (hash % 100) < 85
            };

            if !follows {
                continue;
            }

            let unit_x = self.units[idx].x;
            let unit_y = self.units[idx].y;
            let faction = self.units[idx].faction;

            let order = match order_type {
                "hold" => OrderKind::Hold {
                    target_x: unit_x,
                    target_y: unit_y,
                },
                "go" => {
                    let (tx, ty) = self
                        .zone_manager
                        .best_target_zone(faction)
                        .map(|z| (z.center_wx, z.center_wy))
                        .unwrap_or_else(|| self.faction_objective(faction));
                    OrderKind::Go {
                        target_x: tx,
                        target_y: ty,
                    }
                }
                "retreat" => {
                    let (tx, ty) = self
                        .zone_manager
                        .retreat_zone(faction, unit_x, unit_y)
                        .map(|z| (z.center_wx, z.center_wy))
                        .unwrap_or_else(|| {
                            // Fallback: base spawn point
                            let (sx, sy) = match faction {
                                Faction::Blue => self.zone_manager.blue_base,
                                _ => self.zone_manager.red_base,
                            };
                            grid::grid_to_world(sx, sy)
                        });
                    OrderKind::Retreat {
                        target_x: tx,
                        target_y: ty,
                    }
                }
                "follow" => OrderKind::Follow,
                _ => continue,
            };

            self.units[idx].order = Some(order);
            self.units[idx].order_flash = ORDER_FLASH_DURATION;
            // Clear pathfinding state so the unit re-paths toward the order target
            self.units[idx].ai_waypoints.clear();
            self.units[idx].ai_waypoint_idx = 0;
            self.units[idx].ai_path_cooldown = 0.0;
            acknowledged += 1;
        }
        acknowledged
    }

    /// Hold order AI: defend current position, fight enemies within leash.
    pub(super) fn ai_order_hold_tick(&mut self, ai_idx: usize, hold_x: f32, hold_y: f32, dt: f32) {
        let ai_id = self.units[ai_idx].id;
        let leash = TILE_SIZE * 5.0;

        if let Some((ex, ey, enemy_id, dist)) = self.find_nearest_enemy(ai_idx) {
            let melee_reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
            let melee_reach = melee_reach.max(MELEE_RANGE);

            // Always fight enemies within attack range (self-defense)
            if self.units[ai_idx].can_act() && dist <= melee_reach {
                self.execute_attack(ai_id, enemy_id, None);
                return;
            }

            // Chase enemies within leash of the hold point
            let enemy_hold_dx = ex - hold_x;
            let enemy_hold_dy = ey - hold_y;
            let enemy_hold_dist =
                (enemy_hold_dx * enemy_hold_dx + enemy_hold_dy * enemy_hold_dy).sqrt();

            if enemy_hold_dist <= leash {
                self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
                return;
            }
        }

        // Walk to hold point, idle when close
        let dist = self.units[ai_idx].distance_to_pos(hold_x, hold_y);
        if dist < TILE_SIZE * 1.5 {
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            self.ai_move_toward_continuous(ai_idx, hold_x, hold_y, dt);
        }
    }

    /// Go order AI: advance to target zone, fight enemies on the way.
    pub(super) fn ai_order_go_tick(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        let ai_id = self.units[ai_idx].id;

        // Fight enemies encountered along the way
        if let Some((ex, ey, enemy_id, dist)) = self.find_nearest_enemy(ai_idx) {
            let melee_reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
            let melee_reach = melee_reach.max(MELEE_RANGE);
            if self.units[ai_idx].can_act() && dist <= melee_reach {
                self.execute_attack(ai_id, enemy_id, None);
                return;
            }
            if dist < TILE_SIZE * 4.0 {
                self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
                return;
            }
        }

        // Move toward target zone
        let dist = self.units[ai_idx].distance_to_pos(target_x, target_y);
        if dist < TILE_SIZE * 2.0 {
            // Arrived — clear order, resume normal AI
            self.units[ai_idx].order = None;
        } else {
            self.ai_move_toward_continuous(ai_idx, target_x, target_y, dt);
        }
    }

    /// Retreat order AI: fall back to target, only fight in melee self-defense.
    pub(super) fn ai_order_retreat_tick(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        let ai_id = self.units[ai_idx].id;

        // Only fight if enemy is in melee reach (self-defense)
        if let Some((_ex, _ey, enemy_id, dist)) = self.find_nearest_enemy(ai_idx) {
            let melee_reach = MELEE_RANGE;
            if self.units[ai_idx].can_act() && dist <= melee_reach {
                self.execute_attack(ai_id, enemy_id, None);
            }
        }

        // Always move toward retreat target
        let dist = self.units[ai_idx].distance_to_pos(target_x, target_y);
        if dist < TILE_SIZE * 2.0 {
            // Arrived — switch to Hold
            self.units[ai_idx].order = Some(OrderKind::Hold { target_x, target_y });
        } else {
            self.ai_move_toward_continuous(ai_idx, target_x, target_y, dt);
        }
    }

    /// Follow order AI: stay near the player, fight enemies encountered nearby.
    pub(super) fn ai_order_follow_tick(&mut self, ai_idx: usize, dt: f32) {
        let ai_id = self.units[ai_idx].id;

        // Look up player position
        let (player_x, player_y) = match self.player_unit() {
            Some(p) => (p.x, p.y),
            None => {
                // Player dead — clear order, resume normal AI
                self.units[ai_idx].order = None;
                return;
            }
        };

        // Fight enemies within range
        if let Some((ex, ey, enemy_id, dist)) = self.find_nearest_enemy(ai_idx) {
            let melee_reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
            let melee_reach = melee_reach.max(MELEE_RANGE);
            if self.units[ai_idx].can_act() && dist <= melee_reach {
                self.execute_attack(ai_id, enemy_id, None);
                return;
            }
            if dist < TILE_SIZE * 4.0 {
                self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
                return;
            }
        }

        // Follow the player — move toward them, idle when close
        let dist = self.units[ai_idx].distance_to_pos(player_x, player_y);
        if dist < TILE_SIZE * 2.0 {
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            self.ai_move_toward_continuous(ai_idx, player_x, player_y, dt);
        }
    }
}
