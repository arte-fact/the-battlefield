use super::*;

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
                if dist < self.config.monk_safe_dist_tiles * TILE_SIZE {
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
            // Melee self-defense is exempt from the leash
            if dist <= MELEE_RANGE && self.units[ai_idx].can_act() {
                self.attack_target(ai_idx, ex, ey, enemy_id);
                return true;
            }
            return false;
        }

        let attack_range = self.attack_reach(ai_idx);

        if self.units[ai_idx].can_act() && dist <= attack_range {
            self.attack_target(ai_idx, ex, ey, enemy_id);
            return true;
        }

        if dist <= attack_range {
            // In range but on cooldown — hold position (don't move)
            return true;
        }

        // Out of range — approach to attack range, abandoning unreachable targets
        self.chase_enemy(ai_idx, ex, ey, dt);
        true
    }

    // ── Recruitment (auto-follow retinue) ────────────────────────────────

    /// Deterministic per-unit acceptance roll, keyed on unit ID + authority
    /// bracket: same unit, same authority level → same answer.
    pub(super) fn order_acceptance_roll(&self, unit_id: UnitId) -> bool {
        let follow_chance = self.authority_follow_chance();
        let auth_bracket = (self.authority * 10.0) as u32;
        let mut h = unit_id.wrapping_mul(2654435761) ^ auth_bracket.wrapping_mul(40503);
        h ^= h >> 13;
        h = h.wrapping_mul(0x5bd1e995);
        h ^= h >> 15;
        (h % 100) < (follow_chance * 100.0) as u32
    }

    /// Allied units in command radius with no active order roll the
    /// acceptance check and join as sticky followers, up to the cap.
    pub(super) fn recruitment_pass(&mut self) {
        let (px, py, pf) = match self.player_unit() {
            Some(p) => (p.x, p.y, p.faction),
            None => return,
        };
        let radius = self.authority_command_radius();
        let max_followers = self.authority_max_followers();
        let mut retinue = self
            .units
            .iter()
            .filter(|u| u.alive && !u.is_player && u.order.is_some())
            .count();
        if retinue >= max_followers {
            return;
        }

        let candidates: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| {
                u.alive
                    && !u.is_player
                    && u.faction == pf
                    && u.order.is_none()
                    && u.re_recruit_cooldown <= 0.0
                    && u.distance_to_pos(px, py) <= radius
            })
            .map(|(i, _)| i)
            .collect();

        for idx in candidates {
            if retinue >= max_followers {
                break;
            }
            if !self.order_acceptance_roll(self.units[idx].id) {
                continue;
            }
            let u = &mut self.units[idx];
            u.order = Some(OrderKind::Follow);
            u.order_timer = 0.0;
            u.order_flash = self.config.order_flash_duration;
            u.zone_lock_timer = 0.0;
            u.ai_waypoints.clear();
            u.ai_waypoint_idx = 0;
            u.ai_path_cooldown = 0.0;
            u.follow_arrived = false;
            u.defend_slot = None;
            u.lost_contact_timer = 0.0;
            retinue += 1;
        }
    }

    /// Release a unit from the retinue back to the faction AI.
    pub(super) fn release_unit(&mut self, idx: usize) {
        let u = &mut self.units[idx];
        u.order = None;
        u.order_timer = 0.0;
        u.follow_arrived = false;
        u.defend_slot = None;
        u.defend_in_position = false;
        u.lost_contact_timer = 0.0;
        u.ai_waypoints.clear();
        u.ai_waypoint_idx = 0;
    }

    pub fn follower_count(&self) -> usize {
        self.units
            .iter()
            .filter(|u| u.alive && !u.is_player && u.order.is_some())
            .count()
    }

    pub(super) fn release_retinue_if_player_dead(&mut self) {
        if self.is_player_alive() {
            return;
        }
        let idxs: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.order.is_some())
            .map(|(i, _)| i)
            .collect();
        for i in idxs {
            self.release_unit(i);
        }
    }

    // ── Order issuance ───────────────────────────────────────────────────

    /// Command the retinue. Charge/Defend re-task non-committed followers;
    /// Dismiss releases everyone and sets their re-recruit cooldown.
    pub fn issue_order(&mut self, req: OrderRequest) -> OrderOutcome {
        let (player_x, player_y) = match self.player_unit() {
            Some(p) => (p.x, p.y),
            None => return OrderOutcome::NoPlayer,
        };

        let retinue: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && !u.is_player && u.order.is_some())
            .map(|(i, _)| i)
            .collect();
        if retinue.is_empty() {
            return OrderOutcome::NoFollowers;
        }

        if req == OrderRequest::Dismiss {
            let cooldown = self.config.re_recruit_cooldown_secs;
            let count = retinue.len();
            for idx in retinue {
                self.release_unit(idx);
                self.units[idx].re_recruit_cooldown = cooldown;
                self.units[idx].order_flash = self.config.order_flash_duration;
            }
            return OrderOutcome::Issued(count);
        }

        let order = match req {
            OrderRequest::Charge => {
                let aim = self.player_aim_dir;
                OrderKind::Charge {
                    target_x: player_x + aim.cos() * self.config.charge_distance_tiles * TILE_SIZE,
                    target_y: player_y + aim.sin() * self.config.charge_distance_tiles * TILE_SIZE,
                }
            }
            OrderRequest::Defend => OrderKind::Defend {
                anchor_x: player_x,
                anchor_y: player_y,
                facing_dir: self.player_aim_dir,
            },
            OrderRequest::Dismiss => unreachable!(),
        };
        let timer = match order {
            OrderKind::Charge { .. } => self.config.order_charge_timeout,
            OrderKind::Defend { .. } => self.config.order_defend_duration,
            OrderKind::Follow => 0.0,
        };

        let mut acknowledged = 0usize;
        for idx in retinue {
            if self.units[idx].is_committed() {
                continue;
            }
            let u = &mut self.units[idx];
            u.order = Some(order);
            u.order_timer = timer;
            u.order_flash = self.config.order_flash_duration;
            u.zone_lock_timer = 0.0;
            u.ai_waypoints.clear();
            u.ai_waypoint_idx = 0;
            u.ai_path_cooldown = 0.0;
            u.follow_arrived = false;
            u.defend_in_position = false;
            // Stable defend slot assigned at issue time
            if matches!(order, OrderKind::Defend { .. }) {
                let kind = self.units[idx].kind;
                let slot = self
                    .units
                    .iter()
                    .filter(|u| {
                        u.alive
                            && u.kind == kind
                            && matches!(u.order, Some(OrderKind::Defend { .. }))
                            && u.defend_slot.is_some()
                    })
                    .count() as u8;
                self.units[idx].defend_slot = Some(slot);
            } else {
                self.units[idx].defend_slot = None;
            }
            acknowledged += 1;
        }
        if acknowledged == 0 {
            OrderOutcome::NoFollowers
        } else {
            self.order_pulse = 0.6;
            self.order_pulse_radius = self.authority_command_radius();
            OrderOutcome::Issued(acknowledged)
        }
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

        let contact_leash = self.config.recruit_leash_tiles * TILE_SIZE;
        if self.units[ai_idx].distance_to_pos(player_x, player_y) > contact_leash {
            self.units[ai_idx].lost_contact_timer += dt;
            if self.units[ai_idx].lost_contact_timer >= self.config.recruit_lost_contact_secs {
                self.release_unit(ai_idx);
                return;
            }
        } else {
            self.units[ai_idx].lost_contact_timer = 0.0;
        }

        // Combat (leashed to player position)
        if self.ai_order_combat(
            ai_idx,
            player_x,
            player_y,
            self.config.order_leash_tiles * TILE_SIZE,
            dt,
        ) {
            return;
        }

        // Follow the player — spread out in a ring (with dead-band to prevent overshoot jitter)
        let dist = self.units[ai_idx].distance_to_pos(player_x, player_y);
        let inner = self.config.follow_distance_tiles * TILE_SIZE;
        let outer = inner + self.config.follow_deadband_tiles * TILE_SIZE;
        if dist < inner {
            self.units[ai_idx].follow_arrived = true;
        } else if dist > outer {
            self.units[ai_idx].follow_arrived = false;
        }
        if self.units[ai_idx].follow_arrived {
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            let slot = self.units[ai_idx].id as f32;
            let angle = slot * 2.39996; // golden angle for even spacing
            let offset_x = angle.cos() * self.config.follow_distance_tiles * TILE_SIZE * 0.7;
            let offset_y = angle.sin() * self.config.follow_distance_tiles * TILE_SIZE * 0.7;
            self.ai_move_toward_continuous(ai_idx, player_x + offset_x, player_y + offset_y, dt);
        }
    }

    /// Charge order AI: rush to target, fight enemies on the way, then switch to Follow.
    /// Archers and lancers stop to fight when enemies are in their attack range.
    pub(super) fn ai_order_charge_tick(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        let kind = self.units[ai_idx].kind;

        // Archers and lancers engage enemies within their own range (no leash)
        // Warriors keep charging — they need to close distance
        let leash = match kind {
            UnitKind::Archer | UnitKind::Lancer => {
                let range = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
                // Use unit position as leash center with their attack range
                let ax = self.units[ai_idx].x;
                let ay = self.units[ai_idx].y;
                if self.ai_order_combat(ai_idx, ax, ay, range, dt) {
                    return;
                }
                self.config.order_leash_tiles * TILE_SIZE // fallback for melee self-defense via normal leash
            }
            _ => self.config.order_leash_tiles * TILE_SIZE,
        };

        // Standard combat (leashed to charge target) — mostly for warriors
        if self.ai_order_combat(ai_idx, target_x, target_y, leash, dt) {
            return;
        }

        // Move toward charge target
        let dist = self.units[ai_idx].distance_to_pos(target_x, target_y);
        if dist < self.config.charge_arrival_tiles * TILE_SIZE {
            // Charge complete — transition to Follow so the group stays cohesive
            self.units[ai_idx].order = Some(OrderKind::Follow);
            self.units[ai_idx].order_timer = self.config.order_follow_duration;
            self.units[ai_idx].follow_arrived = false;
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
            UnitKind::Warrior => self.config.defend_line_warrior_tiles * TILE_SIZE,
            UnitKind::Lancer => self.config.defend_line_lancer_tiles * TILE_SIZE,
            UnitKind::Archer => self.config.defend_line_archer_tiles * TILE_SIZE,
            UnitKind::Monk => self.config.defend_line_monk_tiles * TILE_SIZE,
        };

        // Use stable slot assigned at order-issue time (prevents jumping on ally death)
        let slot = self.units[ai_idx].defend_slot.unwrap_or(0) as f32;
        let count = self
            .units
            .iter()
            .filter(|u| {
                u.alive && u.kind == kind && matches!(u.order, Some(OrderKind::Defend { .. }))
            })
            .count() as f32;

        // Behind direction (opposite of facing)
        let behind_dir = facing_dir + std::f32::consts::PI;
        // Perpendicular axis (90° from facing direction)
        let perp_x = -facing_dir.sin();
        let perp_y = facing_dir.cos();

        // Position: anchor + behind offset + perpendicular spread
        let behind_x = behind_dir.cos() * row_dist;
        let behind_y = behind_dir.sin() * row_dist;
        let lateral_offset =
            (slot - (count - 1.0) / 2.0) * self.config.defend_spacing_tiles * TILE_SIZE;
        let post_x = anchor_x + behind_x + perp_x * lateral_offset;
        let post_y = anchor_y + behind_y + perp_y * lateral_offset;

        // Combat (leashed to formation post — melee stays close, ranged has longer reach)
        let leash = match kind {
            UnitKind::Warrior | UnitKind::Lancer => {
                self.config.defend_leash_melee_tiles * TILE_SIZE
            }
            UnitKind::Archer => self.config.defend_leash_ranged_tiles * TILE_SIZE,
            UnitKind::Monk => self.config.defend_leash_melee_tiles * TILE_SIZE,
        };
        if self.ai_order_combat(ai_idx, post_x, post_y, leash, dt) {
            return;
        }

        // Move to formation post, idle when close
        let dist = self.units[ai_idx].distance_to_pos(post_x, post_y);
        if dist < TILE_SIZE * 0.5 {
            self.units[ai_idx].defend_in_position = true;
            self.units[ai_idx].set_anim(UnitAnim::Idle);
        } else {
            self.units[ai_idx].defend_in_position = false;
            self.ai_move_toward_continuous(ai_idx, post_x, post_y, dt);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Player at (20,20) with a guaranteed-accept authority profile.
    fn game_with_player(follow_chance: f32) -> Game {
        let mut game = Game::new(960.0, 640.0);
        game.config.authority_follow_base = follow_chance;
        game.config.authority_follow_slope = 0.0;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 20, 20, true);
        game
    }

    fn unit_by_id(game: &Game, id: UnitId) -> &Unit {
        game.units.iter().find(|u| u.id == id).unwrap()
    }

    #[test]
    fn recruitment_attaches_nearby_ally() {
        let mut game = game_with_player(1.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.recruitment_pass();
        assert!(matches!(
            unit_by_id(&game, ally).order,
            Some(OrderKind::Follow)
        ));
        assert!(unit_by_id(&game, ally).order_flash > 0.0);
    }

    #[test]
    fn recruitment_ignores_out_of_radius() {
        let mut game = game_with_player(1.0);
        // Command radius at authority 0 = 3 tiles; (40,20) is 20 tiles away
        let far = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 40, 20, false);
        game.recruitment_pass();
        assert!(unit_by_id(&game, far).order.is_none());
    }

    #[test]
    fn recruitment_ignores_enemies() {
        let mut game = game_with_player(1.0);
        let enemy = game.spawn_unit(UnitKind::Warrior, Faction::Red, 21, 20, false);
        game.recruitment_pass();
        assert!(unit_by_id(&game, enemy).order.is_none());
    }

    #[test]
    fn recruitment_respects_follower_cap() {
        let mut game = game_with_player(1.0);
        game.config.authority_max_followers_base = 1;
        game.config.authority_max_followers_slope = 0.0;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 19, 20, false);
        game.recruitment_pass();
        let followers = game
            .units
            .iter()
            .filter(|u| !u.is_player && u.order.is_some())
            .count();
        assert_eq!(followers, 1);
    }

    #[test]
    fn recruitment_is_silent_on_rejection() {
        let mut game = game_with_player(0.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.recruitment_pass();
        assert!(unit_by_id(&game, ally).order.is_none());
    }

    #[test]
    fn recruitment_skips_re_recruit_cooldown() {
        let mut game = game_with_player(1.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.units
            .iter_mut()
            .find(|u| u.id == ally)
            .unwrap()
            .re_recruit_cooldown = 5.0;
        game.recruitment_pass();
        assert!(unit_by_id(&game, ally).order.is_none());
    }

    #[test]
    fn follow_is_sticky_and_timed_orders_revert_to_follow() {
        let mut game = game_with_player(1.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.recruitment_pass();

        // Follow survives arbitrary time
        for _ in 0..1000 {
            game.units
                .iter_mut()
                .find(|u| u.id == ally)
                .unwrap()
                .tick_cooldowns(0.5);
        }
        assert!(matches!(
            unit_by_id(&game, ally).order,
            Some(OrderKind::Follow)
        ));

        // A timed order expires back into Follow, not release
        {
            let u = game.units.iter_mut().find(|u| u.id == ally).unwrap();
            u.order = Some(OrderKind::Defend {
                anchor_x: 0.0,
                anchor_y: 0.0,
                facing_dir: 0.0,
            });
            u.order_timer = 1.0;
            u.tick_cooldowns(2.0);
        }
        assert!(matches!(
            unit_by_id(&game, ally).order,
            Some(OrderKind::Follow)
        ));
    }

    #[test]
    fn lost_contact_releases_follower() {
        let mut game = game_with_player(1.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.recruitment_pass();

        // Teleport the follower far beyond the contact leash (15 tiles)
        let idx = game.units.iter().position(|u| u.id == ally).unwrap();
        game.units[idx].x += 30.0 * TILE_SIZE;

        // Under the lost-contact threshold: still a follower
        game.ai_order_follow_tick(idx, 1.0);
        assert!(game.units[idx].order.is_some());
        // Past the threshold (3s): released
        game.ai_order_follow_tick(idx, 1.0);
        game.ai_order_follow_tick(idx, 1.5);
        assert!(game.units[idx].order.is_none());
    }

    fn recruit_one(game: &mut Game, gx: u32, gy: u32) -> UnitId {
        let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, gx, gy, false);
        game.recruitment_pass();
        assert!(matches!(
            unit_by_id(game, id).order,
            Some(OrderKind::Follow)
        ));
        id
    }

    #[test]
    fn charge_targets_retinue_only() {
        let mut game = game_with_player(1.0);
        let follower = recruit_one(&mut game, 21, 20);
        game.config.authority_follow_base = 0.0;
        let stranger = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 19, 20, false);

        let outcome = game.issue_order(OrderRequest::Charge);
        assert_eq!(outcome, OrderOutcome::Issued(1));
        assert!(matches!(
            unit_by_id(&game, follower).order,
            Some(OrderKind::Charge { .. })
        ));
        assert!(unit_by_id(&game, stranger).order.is_none());
    }

    #[test]
    fn charging_unit_is_committed_until_arrival() {
        let mut game = game_with_player(1.0);
        let follower = recruit_one(&mut game, 21, 20);
        game.issue_order(OrderRequest::Charge);
        assert!(unit_by_id(&game, follower).is_committed());

        let outcome = game.issue_order(OrderRequest::Defend);
        assert_eq!(outcome, OrderOutcome::NoFollowers);
        assert!(matches!(
            unit_by_id(&game, follower).order,
            Some(OrderKind::Charge { .. })
        ));
    }

    #[test]
    fn posted_defender_is_retaskable() {
        let mut game = game_with_player(1.0);
        let follower = recruit_one(&mut game, 21, 20);
        game.issue_order(OrderRequest::Defend);
        assert!(unit_by_id(&game, follower).is_committed());

        game.units
            .iter_mut()
            .find(|u| u.id == follower)
            .unwrap()
            .defend_in_position = true;
        assert!(!unit_by_id(&game, follower).is_committed());

        let outcome = game.issue_order(OrderRequest::Charge);
        assert_eq!(outcome, OrderOutcome::Issued(1));
    }

    #[test]
    fn dismiss_releases_all_with_immunity() {
        let mut game = game_with_player(1.0);
        let a = recruit_one(&mut game, 21, 20);
        let b = recruit_one(&mut game, 19, 20);
        game.issue_order(OrderRequest::Charge);

        let outcome = game.issue_order(OrderRequest::Dismiss);
        assert_eq!(outcome, OrderOutcome::Issued(2));
        for id in [a, b] {
            assert!(unit_by_id(&game, id).order.is_none());
            assert!(unit_by_id(&game, id).re_recruit_cooldown > 0.0);
        }

        game.recruitment_pass();
        assert!(unit_by_id(&game, a).order.is_none());

        for u in game.units.iter_mut() {
            u.re_recruit_cooldown = 0.0;
        }
        game.recruitment_pass();
        assert!(unit_by_id(&game, a).order.is_some());
    }

    #[test]
    fn order_outcomes_without_retinue_or_player() {
        let mut game = game_with_player(1.0);
        assert_eq!(
            game.issue_order(OrderRequest::Charge),
            OrderOutcome::NoFollowers
        );
        game.units.clear();
        assert_eq!(
            game.issue_order(OrderRequest::Charge),
            OrderOutcome::NoPlayer
        );
    }

    #[test]
    fn player_death_releases_retinue() {
        let mut game = game_with_player(1.0);
        let ally = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 21, 20, false);
        game.recruitment_pass();
        assert!(unit_by_id(&game, ally).order.is_some());

        game.units.iter_mut().find(|u| u.is_player).unwrap().alive = false;
        game.release_retinue_if_player_dead();
        assert!(unit_by_id(&game, ally).order.is_none());
    }
}
