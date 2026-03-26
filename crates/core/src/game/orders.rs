use super::*;

/// Duration of the order flash indicator in seconds.
pub const ORDER_FLASH_DURATION: f32 = 1.0;

/// How far ahead of the player charge targets are placed (in world units).
const CHARGE_DISTANCE: f32 = TILE_SIZE * 8.0;
/// How close a unit must be to the charge target to consider it arrived.
const CHARGE_ARRIVAL: f32 = TILE_SIZE * 1.5;
/// Follow distance — how close followers stay to the player.
const FOLLOW_DISTANCE: f32 = TILE_SIZE * 1.5;
/// Follow/charge leash — max distance from anchor to engage enemies.
const ORDER_LEASH: f32 = TILE_SIZE * 4.0;

// Defend formation: line distances behind the anchor point.
const DEFEND_LINE_WARRIOR: f32 = TILE_SIZE * 2.0;
const DEFEND_LINE_LANCER: f32 = TILE_SIZE * 3.5;
const DEFEND_LINE_ARCHER: f32 = TILE_SIZE * 5.0;
const DEFEND_LINE_MONK: f32 = TILE_SIZE * 6.5;
/// Perpendicular spacing between units in the same line.
const DEFEND_SPACING: f32 = TILE_SIZE;
/// Defend units engage enemies within this distance of their post.
/// Melee defend leash — warriors/lancers stay close to their post (2 tiles).
const DEFEND_LEASH_MELEE: f32 = TILE_SIZE * 2.0;
/// Ranged defend leash — archers can fire at enemies approaching the formation.
const DEFEND_LEASH_RANGED: f32 = TILE_SIZE * 8.0;

/// Monks flee when enemies are closer than this (mirrors MONK_SAFE_DIST in ai.rs).
const MONK_SAFE_DIST: f32 = TILE_SIZE * 3.0;

impl Game {
    // ── Shared combat helper ─────────────────────────────────────────────

    /// Type-aware combat for ordered units. Returns true if the unit is busy
    /// with combat and the caller should skip positioning.
    ///
    /// Mirrors the per-type logic from `ai_melee_tick`, `ai_archer_tick`, and
    /// `ai_monk_tick` but with a leash: enemies beyond `leash` distance from
    /// `(leash_x, leash_y)` are ignored.
    fn ai_order_combat(
        &mut self,
        ai_idx: usize,
        leash_x: f32,
        leash_y: f32,
        leash: f32,
        dt: f32,
    ) -> bool {
        let kind = self.units[ai_idx].kind;

        // Monks don't fight — they flee from nearby enemies instead.
        // (Healing is already handled by try_monk_heal in ai_unit_tick.)
        if kind == UnitKind::Monk {
            if let Some((ex, ey, _, dist)) = self.find_nearest_enemy(ai_idx) {
                if dist < MONK_SAFE_DIST {
                    let ax = self.units[ai_idx].x;
                    let ay = self.units[ai_idx].y;
                    let flee_x = ax + (ax - ex);
                    let flee_y = ay + (ay - ey);
                    self.ai_move_toward_continuous(ai_idx, flee_x, flee_y, dt);
                    return true;
                }
            }
            return false;
        }

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => return false,
        };
        let (ex, ey, enemy_id, dist) = enemy;

        // Check leash — ignore enemies too far from the anchor point
        let enemy_leash_dx = ex - leash_x;
        let enemy_leash_dy = ey - leash_y;
        let enemy_leash_dist =
            (enemy_leash_dx * enemy_leash_dx + enemy_leash_dy * enemy_leash_dy).sqrt();
        if enemy_leash_dist > leash {
            // Exception: always allow melee self-defense
            let melee_reach = MELEE_RANGE;
            if dist <= melee_reach && self.units[ai_idx].can_act() {
                let ai_id = self.units[ai_idx].id;
                self.execute_attack(ai_id, enemy_id, None);
                return true;
            }
            return false;
        }

        let attack_range = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
        let attack_range = attack_range.max(MELEE_RANGE);
        let ai_id = self.units[ai_idx].id;

        if self.units[ai_idx].can_act() && dist <= attack_range {
            // In range and ready — attack
            self.execute_attack(ai_id, enemy_id, None);
            return true;
        }

        if dist <= attack_range {
            // In range but on cooldown — hold position (don't move)
            return true;
        }

        // Out of range — melee units chase, ranged units only approach to their attack range
        if kind == UnitKind::Archer {
            // Archers approach to firing range, not into melee
            self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
        } else {
            // Warriors and Lancers close the distance
            self.ai_move_toward_continuous(ai_idx, ex, ey, dt);
        }
        true
    }

    // ── Recruitment ────────────────────────────────────────────────────

    /// Recruit nearby friendly units into the persistent follower list.
    /// Returns the number of newly recruited units.
    pub fn recruit_units(&mut self) -> usize {
        let (player_x, player_y, player_faction) = match self.player_unit() {
            Some(p) => (p.x, p.y, p.faction),
            None => return 0,
        };

        let recruit_radius = self.authority_command_radius();
        let follow_chance = self.authority_follow_chance();

        // Collect eligible unit indices (not already recruited)
        let eligible: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.alive
                    && !u.is_player
                    && u.faction == player_faction
                    && !self.recruited.contains(&u.id)
                    && u.distance_to_pos(player_x, player_y) <= recruit_radius
            })
            .map(|(i, _)| i)
            .collect();

        let mut count = 0usize;
        for idx in eligible {
            if self.follower_count() >= self.authority_max_followers() {
                break;
            }

            // Probabilistic acceptance based on authority
            let accepts = {
                let ux = (self.units[idx].x * 100.0) as u32;
                let uy = (self.units[idx].y * 100.0) as u32;
                let mut h =
                    self.units[idx].id.wrapping_mul(2654435761) ^ ux ^ uy.wrapping_mul(40503);
                h ^= h >> 13;
                h = h.wrapping_mul(0x5bd1e995);
                (h % 100) < (follow_chance * 100.0) as u32
            };

            if !accepts {
                continue;
            }

            self.recruited.insert(self.units[idx].id);
            // Newly recruited units default to Follow
            self.units[idx].order = Some(OrderKind::Follow);
            self.units[idx].order_flash = ORDER_FLASH_DURATION;
            self.units[idx].zone_lock_timer = 0.0;
            self.units[idx].ai_waypoints.clear();
            self.units[idx].ai_waypoint_idx = 0;
            self.units[idx].ai_path_cooldown = 0.0;
            count += 1;
        }
        count
    }

    // ── Order issuance ───────────────────────────────────────────────────

    /// Issue an order to all recruited units.
    /// Returns the number of units that received the order.
    pub fn issue_order(&mut self, order_type: &str) -> usize {
        let player_x = match self.player_unit() {
            Some(p) => p.x,
            None => return 0,
        };
        let player_y = self.player_unit().map(|p| p.y).unwrap_or(0.0);

        // Collect indices of alive recruited units
        let recruited_indices: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && self.recruited.contains(&u.id))
            .map(|(i, _)| i)
            .collect();

        let mut acknowledged = 0usize;
        for idx in recruited_indices {
            let order = match order_type {
                "follow" => OrderKind::Follow,
                "charge" => {
                    let aim = self.player_aim_dir;
                    OrderKind::Charge {
                        target_x: player_x + aim.cos() * CHARGE_DISTANCE,
                        target_y: player_y + aim.sin() * CHARGE_DISTANCE,
                    }
                }
                "defend" => OrderKind::Defend {
                    anchor_x: player_x,
                    anchor_y: player_y,
                    facing_dir: self.player_aim_dir,
                },
                _ => continue,
            };

            self.units[idx].order = Some(order);
            self.units[idx].order_flash = ORDER_FLASH_DURATION;
            self.units[idx].zone_lock_timer = 0.0;
            self.units[idx].ai_waypoints.clear();
            self.units[idx].ai_waypoint_idx = 0;
            self.units[idx].ai_path_cooldown = 0.0;
            acknowledged += 1;
        }
        acknowledged
    }

    // ── Order tick functions ─────────────────────────────────────────────

    /// Follow order AI: stay near the player, fight enemies encountered nearby.
    pub(super) fn ai_order_follow_tick(&mut self, ai_idx: usize, dt: f32) {
        let (player_x, player_y) = match self.player_unit() {
            Some(p) => (p.x, p.y),
            None => {
                self.units[ai_idx].order = None;
                return;
            }
        };

        // Combat (leashed to player position)
        if self.ai_order_combat(ai_idx, player_x, player_y, ORDER_LEASH, dt) {
            return;
        }

        // Follow the player — spread out in a ring
        let dist = self.units[ai_idx].distance_to_pos(player_x, player_y);
        if dist < FOLLOW_DISTANCE {
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            let slot = self.units[ai_idx].id as f32;
            let angle = slot * 2.39996; // golden angle for even spacing
            let offset_x = angle.cos() * FOLLOW_DISTANCE * 0.7;
            let offset_y = angle.sin() * FOLLOW_DISTANCE * 0.7;
            self.ai_move_toward_continuous(ai_idx, player_x + offset_x, player_y + offset_y, dt);
        }
    }

    /// Charge order AI: rush to target, fight enemies on the way, then switch to Follow.
    pub(super) fn ai_order_charge_tick(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        // Combat (leashed to charge target)
        if self.ai_order_combat(ai_idx, target_x, target_y, ORDER_LEASH, dt) {
            return;
        }

        // Move toward charge target
        let dist = self.units[ai_idx].distance_to_pos(target_x, target_y);
        if dist < CHARGE_ARRIVAL {
            // Arrived — transition to Follow
            self.units[ai_idx].order = Some(OrderKind::Follow);
            self.units[ai_idx].ai_waypoints.clear();
            self.units[ai_idx].ai_waypoint_idx = 0;
        } else {
            self.ai_move_toward_continuous(ai_idx, target_x, target_y, dt);
        }
    }

    /// Defend order AI: hold formation behind the anchor point.
    pub(super) fn ai_order_defend_tick(
        &mut self,
        ai_idx: usize,
        anchor_x: f32,
        anchor_y: f32,
        facing_dir: f32,
        dt: f32,
    ) {
        let kind = self.units[ai_idx].kind;

        // Determine which line this unit belongs to
        let row_dist = match kind {
            UnitKind::Warrior => DEFEND_LINE_WARRIOR,
            UnitKind::Lancer => DEFEND_LINE_LANCER,
            UnitKind::Archer => DEFEND_LINE_ARCHER,
            UnitKind::Monk => DEFEND_LINE_MONK,
        };

        // Assign a slot within the line based on unit ID among same-kind defenders
        let unit_id = self.units[ai_idx].id;
        let mut same_kind_ids: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| {
                u.alive && u.kind == kind && matches!(u.order, Some(OrderKind::Defend { .. }))
            })
            .map(|u| u.id)
            .collect();
        same_kind_ids.sort_unstable();
        let slot = same_kind_ids
            .iter()
            .position(|&id| id == unit_id)
            .unwrap_or(0) as f32;
        let count = same_kind_ids.len() as f32;

        // Behind direction (opposite of facing)
        let behind_dir = facing_dir + std::f32::consts::PI;
        // Perpendicular axis (90° from facing direction)
        let perp_x = -facing_dir.sin();
        let perp_y = facing_dir.cos();

        // Position: anchor + behind offset + perpendicular spread
        let behind_x = behind_dir.cos() * row_dist;
        let behind_y = behind_dir.sin() * row_dist;
        let lateral_offset = (slot - (count - 1.0) / 2.0) * DEFEND_SPACING;
        let post_x = anchor_x + behind_x + perp_x * lateral_offset;
        let post_y = anchor_y + behind_y + perp_y * lateral_offset;

        // Combat (leashed to formation post — melee stays close, ranged has longer reach)
        let leash = match kind {
            UnitKind::Warrior | UnitKind::Lancer => DEFEND_LEASH_MELEE,
            UnitKind::Archer => DEFEND_LEASH_RANGED,
            UnitKind::Monk => DEFEND_LEASH_MELEE, // monks stay near post, only heal
        };
        if self.ai_order_combat(ai_idx, post_x, post_y, leash, dt) {
            return;
        }

        // Move to formation post, idle when close
        let dist = self.units[ai_idx].distance_to_pos(post_x, post_y);
        if dist < TILE_SIZE * 0.5 {
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            self.ai_move_toward_continuous(ai_idx, post_x, post_y, dt);
        }
    }
}
