use super::*;

/// Half-angle of the player's attack cone (90° = PI/2 radians, 180° total arc).
pub const ATTACK_CONE_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_2;

impl Game {
    pub fn player_unit(&self) -> Option<&Unit> {
        self.units.iter().find(|u| u.is_player && u.alive)
    }

    pub fn player_unit_mut(&mut self) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.is_player && u.alive)
    }

    pub fn is_player_alive(&self) -> bool {
        self.player_unit().is_some()
    }

    /// Free-mode enlistment: an unaligned player standing at an
    /// army-owned production building converts to the unit kind it
    /// trains, in the owner's color, and the full player kit unlocks.
    pub(super) fn tick_conversion(&mut self) {
        if self.player_faction.is_some() {
            return;
        }
        let (px, py) = match self.player_unit() {
            Some(p) if p.alive => (p.x, p.y),
            _ => return,
        };
        let range = crate::grid::TILE_SIZE * 2.5;
        let mut best: Option<(usize, f32)> = None;
        for (i, b) in self.buildings.iter().enumerate() {
            if b.produces.is_none() {
                continue;
            }
            let (bx, by) = grid::grid_to_world(b.grid_x, b.grid_y);
            let d_sq = (px - bx).powi(2) + (py - by).powi(2);
            if d_sq <= range * range && best.map(|(_, bd)| d_sq < bd).unwrap_or(true) {
                best = Some((i, d_sq));
            }
        }
        let Some((bi, _)) = best else { return };
        let b = &self.buildings[bi];
        let owner = match b.zone_id {
            Some(z) => self
                .zone_manager
                .zones
                .get(z as usize)
                .and_then(|zone| zone.effective_faction())
                .unwrap_or(Faction::Villager),
            None => b.faction,
        };
        if owner == Faction::Villager {
            return; // this village serves no lord
        }
        let kind = self.buildings[bi].produces.expect("filtered above");
        let stats = kind.stats_from_config(&self.config);
        if let Some(p) = self.units.iter_mut().find(|u| u.is_player) {
            p.kind = kind;
            p.faction = owner;
            p.stats = stats;
            p.hp = stats.max_hp;
        }
        self.player_faction = Some(owner);
        // Ceremony: the command ring ripples out from the new soldier.
        self.order_pulse = 1.0;
        self.order_pulse_radius = self.authority_command_radius();
        log::info!("Player enlisted: {:?} {:?}", owner, kind);
    }

    /// Try to attack the nearest enemy in range. Returns true if an attack was executed.
    /// Called explicitly from attack key/button — never auto-attacks.
    /// Player attack: hit enemies in cone if any, otherwise whiff swing.
    /// Returns true if the attack hit at least one enemy.
    pub fn player_attack(&mut self) -> bool {
        // An unaligned villager has no blade worth swinging.
        if self.player_faction.is_none() {
            return false;
        }
        let player_idx = match self.units.iter().position(|u| u.is_player && u.alive) {
            Some(i) => i,
            None => return false,
        };

        if !self.units[player_idx].can_act() {
            return false;
        }

        let hits = self.swing_in_cone(player_idx, self.player_aim_dir);
        if hits == 0 {
            // Whiff: play attack anim with full cooldown (same rate as hits)
            let anim = self.units[player_idx].next_attack_anim();
            self.units[player_idx].set_anim(anim);
            let cd = self.units[player_idx]
                .kind
                .cooldown_from_config(&self.config);
            self.units[player_idx].start_attack_cooldown_cfg(cd);
        }
        hits > 0
    }

    /// Find ALL enemies within range AND within a cone (for cleave attacks).
    pub fn enemies_in_cone(
        &self,
        x: f32,
        y: f32,
        faction: Faction,
        range: f32,
        aim_dir: f32,
        half_angle: f32,
    ) -> Vec<UnitId> {
        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .filter_map(|u| {
                let dist = u.distance_to_pos(x, y);
                if dist > range {
                    return None;
                }
                let angle_to = (u.y - y).atan2(u.x - x);
                let mut diff = angle_to - aim_dir;
                diff = (diff + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                if diff.abs() <= half_angle {
                    Some(u.id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find the closest alive enemy unit near a world position (for arrow impact).
    /// Returns the index of the closest enemy of the opposing faction within hit radius.
    /// Uses the per-frame spatial hash to avoid scanning all units.
    pub(super) fn find_unit_near(
        &self,
        x: f32,
        y: f32,
        attacker_faction: Faction,
    ) -> Option<usize> {
        let hit_radius = TILE_SIZE * 0.75;
        let mut best: Option<(usize, f32)> = None;
        for i in self.spatial.query(x, y, hit_radius) {
            let u = &self.units[i];
            if u.faction == attacker_faction {
                continue;
            }
            let dist = u.distance_to_pos(x, y);
            if dist <= hit_radius && best.is_none_or(|b| dist < b.1) {
                best = Some((i, dist));
            }
        }
        best.map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_unit(game: &Game, id: UnitId) -> Option<&Unit> {
        game.units.iter().find(|u| u.id == id)
    }

    #[test]
    fn player_whiff_cooldown_matches_config() {
        let mut game = Game::new(960.0, 640.0);
        game.config.warrior_cooldown = 0.9;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // No enemies anywhere — guaranteed whiff. Rate fairness: a whiff must
        // charge the same (config-tuned) cooldown as a hit.
        assert!(!game.player_attack());
        assert_eq!(game.player_unit().unwrap().attack_cooldown, 0.9);
    }

    #[test]
    fn spawn_unit_and_find_player() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.player_unit().is_some());
    }

    #[test]
    fn player_attack_hits_in_range() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        // Adjacent = 64px, within MELEE_RANGE = 96px
        game.player_attack();
        let enemy = find_unit(&game, enemy_id).unwrap();
        assert!(
            enemy.hp < 10,
            "Enemy should have taken damage from auto-attack"
        );
    }

    #[test]
    fn player_attack_ranged() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Archer, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 9, 5, false);
        // 4 tiles = 256px, Archer range = 7 * 64 = 448px
        game.player_attack();
        // Arrow spawned — damage is deferred until projectile lands
        assert!(
            !game.projectiles.is_empty(),
            "Arrow projectile should be spawned"
        );
        // Advance time until arrow lands (distance ~256px / 600px/s ≈ 0.43s)
        for _ in 0..40 {
            game.update(0.016);
        }
        let enemy = find_unit(&game, enemy_id).unwrap();
        assert!(
            enemy.hp < 10,
            "Enemy should have taken ranged damage on arrow impact"
        );
    }

    // ---- Cleave tests ----

    #[test]
    fn enemies_in_cone_finds_all() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 4, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 6, false);
        let px = game.units[0].x;
        let py = game.units[0].y;
        let result = game.enemies_in_cone(
            px,
            py,
            Faction::Blue,
            MELEE_RANGE * 2.0,
            0.0,
            ATTACK_CONE_HALF_ANGLE,
        );
        assert_eq!(
            result.len(),
            3,
            "Should find all 3 enemies in cone, got {:?}",
            result
        );
    }

    #[test]
    fn enemies_in_cone_filters_behind() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false); // ahead (right)
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 4, 5, false); // behind (left)
        let px = game.units[0].x;
        let py = game.units[0].y;
        let result = game.enemies_in_cone(
            px,
            py,
            Faction::Blue,
            MELEE_RANGE * 2.0,
            0.0,
            ATTACK_CONE_HALF_ANGLE,
        );
        assert_eq!(result.len(), 1, "Should only find the enemy ahead");
        assert_eq!(result[0], 2, "Should be the enemy to the right");
    }

    #[test]
    fn player_cleave_hits_multiple() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let e1 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        let e2 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 5, 6, false);
        game.player_aim_dir = 0.0; // aim right
        game.player_attack();
        let enemy1 = find_unit(&game, e1).unwrap();
        let enemy2 = find_unit(&game, e2).unwrap();
        assert!(
            enemy1.hp < enemy1.stats.max_hp,
            "First enemy should be damaged"
        );
        assert!(
            enemy2.hp < enemy2.stats.max_hp,
            "Second enemy should be damaged"
        );
    }
}
