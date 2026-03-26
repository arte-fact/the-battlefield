use super::*;

/// Knockback distance in pixels (roughly half a tile).
const KNOCKBACK_DIST: f32 = TILE_SIZE * 0.5;

impl Game {
    /// AI vision radius in tiles (converted to world distance when used).
    pub(super) const AI_VISION_RADIUS: u32 = 10;

    /// Execute an attack. For ranged attacks, `target_snapshot_pos` is the world position
    /// the target was at when the archer decided to shoot (for projectile lag/miss).
    pub(super) fn execute_attack(
        &mut self,
        attacker_id: UnitId,
        defender_id: UnitId,
        target_snapshot_pos: Option<(f32, f32)>,
    ) {
        // Guard: no self-attack, no attacking dead units
        if attacker_id == defender_id {
            return;
        }
        let attacker_idx = self.units.iter().position(|u| u.id == attacker_id);
        let defender_idx = self.units.iter().position(|u| u.id == defender_id);
        let (attacker_idx, defender_idx) = match (attacker_idx, defender_idx) {
            (Some(a), Some(d)) if self.units[a].alive && self.units[d].alive => (a, d),
            _ => return,
        };

        let dist = {
            let ax = self.units[attacker_idx].x;
            let ay = self.units[attacker_idx].y;
            let bx = self.units[defender_idx].x;
            let by = self.units[defender_idx].y;
            ((ax - bx) * (ax - bx) + (ay - by) * (ay - by)).sqrt()
        };

        let is_ranged = self.units[attacker_idx].kind == UnitKind::Archer && dist > MELEE_RANGE;

        if is_ranged {
            let def_x = self.units[defender_idx].x;
            let def_y = self.units[defender_idx].y;
            let (snap_x, snap_y) = target_snapshot_pos.unwrap_or((def_x, def_y));

            // Calculate damage but defer application to projectile impact
            let damage = crate_combat::calc_ranged_damage(
                &self.units[attacker_idx],
                &self.units[defender_idx],
                &self.grid,
            );
            let faction = self.units[attacker_idx].faction;
            let ax = self.units[attacker_idx].x;
            let ay = self.units[attacker_idx].y;

            // Start cooldown + attack anim on attacker
            self.units[attacker_idx].start_attack_cooldown();
            let anim = self.units[attacker_idx].next_attack_anim();
            self.units[attacker_idx].set_anim(anim);

            // Spawn ballistic projectile — damage applied on landing
            self.projectiles
                .push(Projectile::new(ax, ay, snap_x, snap_y, damage, faction));

            self.turn_events.push(TurnEvent::RangedAttack {
                attacker_id,
                defender_id,
                damage: 0, // deferred to impact
                killed: false,
                target_pos: (snap_x, snap_y),
                missed: false,
            });
        } else {
            let (attacker, defender) = if attacker_idx < defender_idx {
                let (left, right) = self.units.split_at_mut(defender_idx);
                (&mut left[attacker_idx], &mut right[0])
            } else {
                let (left, right) = self.units.split_at_mut(attacker_idx);
                (&mut right[0], &mut left[defender_idx])
            };
            let result = crate_combat::execute_melee(attacker, defender, &self.grid);
            self.turn_events.push(TurnEvent::MeleeAttack {
                attacker_id,
                defender_id,
                damage: result.damage,
                killed: result.target_killed,
            });
        }

        // Face the defender (AI only — player facing is controlled by aim-direction lock)
        if !self.units[attacker_idx].is_player {
            let dx = self.units[defender_idx].x - self.units[attacker_idx].x;
            if dx > 0.0 {
                self.units[attacker_idx].facing = Facing::Right;
            } else if dx < 0.0 {
                self.units[attacker_idx].facing = Facing::Left;
            }
        }
    }

    /// Push a unit away from a source position. Respects terrain collision.
    pub(super) fn apply_knockback(&mut self, target_idx: usize, from_x: f32, from_y: f32) {
        let tx = self.units[target_idx].x;
        let ty = self.units[target_idx].y;
        let dx = tx - from_x;
        let dy = ty - from_y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 0.01 {
            return;
        }
        let nx = dx / dist;
        let ny = dy / dist;
        let new_x = tx + nx * KNOCKBACK_DIST;
        let new_y = ty + ny * KNOCKBACK_DIST;
        if self.grid.is_circle_passable(new_x, new_y, UNIT_RADIUS) {
            self.units[target_idx].x = new_x;
            self.units[target_idx].y = new_y;
        }
    }

    /// Execute a heal action between healer at ai_idx and target unit.
    pub(super) fn execute_heal(&mut self, healer_idx: usize, target_id: UnitId) {
        let target_idx = match self.units.iter().position(|u| u.id == target_id) {
            Some(i) => i,
            None => return,
        };
        let healer_id = self.units[healer_idx].id;

        let (healer, target) = if healer_idx < target_idx {
            let (left, right) = self.units.split_at_mut(target_idx);
            (&mut left[healer_idx], &mut right[0])
        } else {
            let (left, right) = self.units.split_at_mut(healer_idx);
            (&mut right[0], &mut left[target_idx])
        };

        let amount = crate_combat::execute_heal(healer, target);
        // Spawn heal effect particle that follows the healed unit
        let tid = target.id;
        let tx = target.x;
        let ty = target.y;
        self.particles
            .push(Particle::new_follow(ParticleKind::HealEffect, tid, tx, ty));
        self.turn_events.push(TurnEvent::Heal {
            healer_id,
            target_id,
            amount,
        });
    }

    /// Find the nearest visible enemy for a unit (Euclidean distance in world pixels).
    pub(super) fn find_nearest_enemy(&self, ai_idx: usize) -> Option<(f32, f32, UnitId, f32)> {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let vision_range = Self::AI_VISION_RADIUS as f32 * TILE_SIZE;

        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .filter_map(|u| {
                let dist = u.distance_to_pos(ax, ay);
                if dist <= vision_range && self.has_line_of_sight(ax, ay, u.x, u.y) {
                    Some((u.x, u.y, u.id, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Distance from a world position to the nearest enemy of the given faction.
    /// Returns `f32::MAX` if no enemies exist.
    pub(super) fn nearest_enemy_dist(&self, x: f32, y: f32, faction: Faction) -> f32 {
        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .map(|u| u.distance_to_pos(x, y))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(f32::MAX)
    }
    /// Tick building combat: find nearest enemy, fire projectiles.
    /// Zone-linked buildings derive their faction from zone capture progress.
    pub(super) fn tick_building_combat(&mut self, dt: f32) {
        for i in 0..self.buildings.len() {
            if !self.buildings[i].kind.is_combat() {
                continue;
            }

            // Zone-linked buildings: derive faction from zone progress
            let faction = if let Some(zid) = self.buildings[i].zone_id {
                let progress = self.zone_manager.zones[zid as usize].progress;
                if progress > 0.01 {
                    Faction::Blue
                } else if progress < -0.01 {
                    Faction::Red
                } else {
                    continue; // neutral — no tower defense
                }
            } else {
                self.buildings[i].faction
            };

            self.buildings[i].attack_cooldown -= dt;
            if self.buildings[i].attack_cooldown > 0.0 {
                continue;
            }

            let bx = grid::grid_to_world(self.buildings[i].grid_x, self.buildings[i].grid_y);
            let range = self.buildings[i].kind.attack_range();
            let damage = self.buildings[i].kind.attack_damage();

            if let Some((tx, ty)) = self.find_nearest_enemy_from(bx.0, bx.1, faction, range) {
                self.buildings[i].attack_cooldown = self.buildings[i].kind.base_cooldown();
                self.projectiles
                    .push(Projectile::new(bx.0, bx.1, tx, ty, damage, faction));
            }
        }
    }

    /// Find the nearest alive enemy of the opposing faction within range from a world position.
    fn find_nearest_enemy_from(
        &self,
        x: f32,
        y: f32,
        faction: Faction,
        range: f32,
    ) -> Option<(f32, f32)> {
        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .filter_map(|u| {
                let dist = u.distance_to_pos(x, y);
                if dist <= range {
                    Some((u.x, u.y, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(x, y, _)| (x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_unit(game: &Game, id: UnitId) -> Option<&Unit> {
        game.units.iter().find(|u| u.id == id)
    }

    #[test]
    fn knockback_pushes_enemy_away() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        let before_x = find_unit(&game, enemy_id).unwrap().x;
        game.player_aim_dir = 0.0;
        game.player_attack();
        let after_x = find_unit(&game, enemy_id).unwrap().x;
        assert!(
            after_x > before_x,
            "Enemy should be pushed away (right) from player, before={before_x} after={after_x}"
        );
    }

    #[test]
    fn knockback_blocked_by_terrain() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        // Place water right behind the enemy so knockback destination is impassable
        game.grid.set(7, 5, TileKind::Water);
        let before_x = find_unit(&game, enemy_id).unwrap().x;
        game.player_aim_dir = 0.0;
        game.player_attack();
        let after_x = find_unit(&game, enemy_id).unwrap().x;
        assert!(
            (after_x - before_x).abs() < 0.01,
            "Enemy should NOT be pushed into water, before={before_x} after={after_x}"
        );
    }

    #[test]
    fn find_nearest_enemy_blocked_by_forest() {
        let mut game = Game::new(960.0, 640.0);
        // Place a forest wall between the two units
        game.grid.set(7, 5, TileKind::Forest);
        game.grid.set(7, 4, TileKind::Forest);
        game.grid.set(7, 6, TileKind::Forest);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 9, 5, false);
        // Enemy is within AI_VISION_RADIUS (10 tiles) but behind forest
        let result = game.find_nearest_enemy(0);
        assert!(
            result.is_none(),
            "Enemy behind forest should not be visible"
        );
    }
}
