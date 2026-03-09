use crate::camera::Camera;
use crate::combat;
use crate::grid::{self, Grid, GRID_SIZE, TILE_SIZE};
use crate::particle::{Particle, ParticleKind, Projectile};
use crate::turn::{TurnPhase, TurnState};
use crate::unit::{Facing, Faction, Unit, UnitAnim, UnitId, UnitKind};

/// Actions the player can perform.
#[derive(Debug, Clone)]
pub enum PlayerAction {
    Move { target_x: u32, target_y: u32 },
    Attack { target_id: UnitId },
    EndTurn,
}

/// Events emitted by the game for the renderer to visualize.
#[derive(Debug)]
pub enum GameEvent {
    UnitMoved {
        unit_id: UnitId,
        from_x: u32,
        from_y: u32,
        to_x: u32,
        to_y: u32,
    },
    MeleeAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
    },
    RangedAttack {
        attacker_id: UnitId,
        defender_id: UnitId,
        damage: i32,
        killed: bool,
    },
    UnitDied {
        unit_id: UnitId,
        x: u32,
        y: u32,
    },
    TurnChanged {
        turn: u32,
        phase: TurnPhase,
    },
}

pub struct Game {
    pub grid: Grid,
    pub units: Vec<Unit>,
    pub turn: TurnState,
    pub camera: Camera,
    pub particles: Vec<Particle>,
    pub projectiles: Vec<Projectile>,
    pub events: Vec<GameEvent>,
    next_unit_id: UnitId,
    /// Grid cell the player is hovering or has selected.
    pub selected_cell: Option<(u32, u32)>,
    /// Cells the player can move to.
    pub move_targets: Vec<(u32, u32)>,
    /// Unit IDs the player can attack.
    pub attack_targets: Vec<UnitId>,
}

impl Game {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut camera = Camera::new(viewport_w, viewport_h);
        // Center camera on the grid
        let center = GRID_SIZE as f32 * TILE_SIZE * 0.5;
        camera.x = center;
        camera.y = center;

        Self {
            grid,
            units: Vec::new(),
            turn: TurnState::new(),
            camera,
            particles: Vec::new(),
            projectiles: Vec::new(),
            events: Vec::new(),
            next_unit_id: 1,
            selected_cell: None,
            move_targets: Vec::new(),
            attack_targets: Vec::new(),
        }
    }

    pub fn spawn_unit(
        &mut self,
        kind: UnitKind,
        faction: Faction,
        x: u32,
        y: u32,
        is_player: bool,
    ) -> UnitId {
        let id = self.next_unit_id;
        self.next_unit_id += 1;
        let unit = Unit::new(id, kind, faction, x, y, is_player);
        self.units.push(unit);
        id
    }

    pub fn player_unit(&self) -> Option<&Unit> {
        self.units.iter().find(|u| u.is_player && u.alive)
    }

    fn player_unit_mut(&mut self) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.is_player && u.alive)
    }

    pub fn unit_at(&self, x: u32, y: u32) -> Option<&Unit> {
        self.units
            .iter()
            .find(|u| u.alive && u.grid_x == x && u.grid_y == y)
    }

    pub fn find_unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }

    /// Compute valid move targets for the player.
    pub fn compute_player_targets(&mut self) {
        self.move_targets.clear();
        self.attack_targets.clear();

        let player = match self.player_unit() {
            Some(u) => u,
            None => return,
        };

        let px = player.grid_x;
        let py = player.grid_y;
        let mov = player.movement_left;
        let faction = player.faction;
        let range = player.stats.range;
        let has_attacked = player.has_attacked;

        // Movement: simple flood-fill within movement range
        if mov > 0 {
            for dy in 0..=(mov as i32) {
                for dx in 0..=(mov as i32 - dy) {
                    for &(sx, sy) in &[(1i32, 1i32), (1, -1), (-1, 1), (-1, -1)] {
                        let nx = px as i32 + dx * sx;
                        let ny = py as i32 + dy * sy;
                        if nx == px as i32 && ny == py as i32 {
                            continue;
                        }
                        if self.grid.in_bounds(nx, ny) {
                            let nx = nx as u32;
                            let ny = ny as u32;
                            if self.grid.is_passable(nx, ny)
                                && self.unit_at(nx, ny).is_none()
                                && !self.move_targets.contains(&(nx, ny))
                            {
                                self.move_targets.push((nx, ny));
                            }
                        }
                    }
                }
            }
        }

        // Attack targets
        if !has_attacked {
            for unit in &self.units {
                if unit.alive && unit.faction != faction {
                    let dist = {
                        let dx = (px as i32 - unit.grid_x as i32).unsigned_abs();
                        let dy = (py as i32 - unit.grid_y as i32).unsigned_abs();
                        dx.max(dy)
                    };
                    if dist <= range {
                        self.attack_targets.push(unit.id);
                    }
                }
            }
        }
    }

    /// Handle a player action. Returns true if the action was valid.
    pub fn handle_player_action(&mut self, action: PlayerAction) -> bool {
        if self.turn.phase != TurnPhase::PlayerTurn {
            return false;
        }

        match action {
            PlayerAction::Move { target_x, target_y } => {
                if !self.move_targets.contains(&(target_x, target_y)) {
                    return false;
                }
                let player = self.player_unit_mut().unwrap();
                let from_x = player.grid_x;
                let from_y = player.grid_y;
                let dist = {
                    let dx = (from_x as i32 - target_x as i32).unsigned_abs();
                    let dy = (from_y as i32 - target_y as i32).unsigned_abs();
                    dx + dy
                };
                player.movement_left = player.movement_left.saturating_sub(dist);
                player.grid_x = target_x;
                player.grid_y = target_y;
                player.set_anim(UnitAnim::Idle);

                // Face direction of movement
                if target_x > from_x {
                    player.facing = Facing::Right;
                } else if target_x < from_x {
                    player.facing = Facing::Left;
                }

                // Spawn dust particle
                let (wx, wy) = grid::grid_to_world(target_x, target_y);
                self.particles
                    .push(Particle::new(ParticleKind::Dust, wx, wy));

                self.events.push(GameEvent::UnitMoved {
                    unit_id: self.units.iter().find(|u| u.is_player).unwrap().id,
                    from_x,
                    from_y,
                    to_x: target_x,
                    to_y: target_y,
                });

                self.compute_player_targets();
                true
            }
            PlayerAction::Attack { target_id } => {
                if !self.attack_targets.contains(&target_id) {
                    return false;
                }
                self.execute_attack_by_player(target_id);
                self.compute_player_targets();
                true
            }
            PlayerAction::EndTurn => {
                self.turn.advance();
                self.events.push(GameEvent::TurnChanged {
                    turn: self.turn.turn_number,
                    phase: self.turn.phase,
                });
                true
            }
        }
    }

    fn execute_attack_by_player(&mut self, target_id: UnitId) {
        // Find player and target indices
        let player_idx = self.units.iter().position(|u| u.is_player && u.alive);
        let target_idx = self.units.iter().position(|u| u.id == target_id);
        let (player_idx, target_idx) = match (player_idx, target_idx) {
            (Some(p), Some(t)) => (p, t),
            _ => return,
        };

        let is_ranged = self.units[player_idx].kind == UnitKind::Archer
            && self.units[player_idx]
                .distance_to(self.units[target_idx].grid_x, self.units[target_idx].grid_y)
                > 1;

        // Split borrow
        let (attacker, defender) = if player_idx < target_idx {
            let (left, right) = self.units.split_at_mut(target_idx);
            (&mut left[player_idx], &mut right[0])
        } else {
            let (left, right) = self.units.split_at_mut(player_idx);
            (&mut right[0], &mut left[target_idx])
        };

        if is_ranged {
            let result = combat::execute_ranged(attacker, defender, &self.grid);
            // Spawn arrow projectile
            let (sx, sy) = grid::grid_to_world(attacker.grid_x, attacker.grid_y);
            let (tx, ty) = grid::grid_to_world(defender.grid_x, defender.grid_y);
            self.projectiles.push(Projectile::new(sx, sy, tx, ty));

            self.events.push(GameEvent::RangedAttack {
                attacker_id: attacker.id,
                defender_id: defender.id,
                damage: result.damage,
                killed: result.target_killed,
            });

            if result.target_killed {
                let (wx, wy) = grid::grid_to_world(defender.grid_x, defender.grid_y);
                self.particles
                    .push(Particle::new(ParticleKind::ExplosionLarge, wx, wy));
                self.events.push(GameEvent::UnitDied {
                    unit_id: defender.id,
                    x: defender.grid_x,
                    y: defender.grid_y,
                });
            }
        } else {
            let result = combat::execute_melee(attacker, defender, &self.grid);
            let (wx, wy) = grid::grid_to_world(defender.grid_x, defender.grid_y);
            self.particles
                .push(Particle::new(ParticleKind::ExplosionSmall, wx, wy));

            self.events.push(GameEvent::MeleeAttack {
                attacker_id: attacker.id,
                defender_id: defender.id,
                damage: result.damage,
                killed: result.target_killed,
            });

            if result.target_killed {
                self.particles
                    .push(Particle::new(ParticleKind::ExplosionLarge, wx, wy));
                self.events.push(GameEvent::UnitDied {
                    unit_id: defender.id,
                    x: defender.grid_x,
                    y: defender.grid_y,
                });
            }
        }
    }

    /// Run simple AI for all non-player units.
    pub fn run_ai_turn(&mut self) {
        if self.turn.phase != TurnPhase::AiTurn {
            return;
        }

        // Collect AI unit indices
        let ai_indices: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && !u.is_player)
            .map(|(i, _)| i)
            .collect();

        for ai_idx in ai_indices {
            let faction = self.units[ai_idx].faction;
            let ax = self.units[ai_idx].grid_x;
            let ay = self.units[ai_idx].grid_y;
            let range = self.units[ai_idx].stats.range;
            let kind = self.units[ai_idx].kind;

            // Find nearest enemy
            let nearest_enemy = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| u.alive && u.faction != faction)
                .min_by_key(|(_, u)| {
                    let dx = (ax as i32 - u.grid_x as i32).unsigned_abs();
                    let dy = (ay as i32 - u.grid_y as i32).unsigned_abs();
                    dx + dy
                })
                .map(|(i, u)| (i, u.grid_x, u.grid_y, u.id));

            let (enemy_idx, ex, ey, _enemy_id) = match nearest_enemy {
                Some(e) => e,
                None => continue,
            };

            let dist = {
                let dx = (ax as i32 - ex as i32).unsigned_abs();
                let dy = (ay as i32 - ey as i32).unsigned_abs();
                dx.max(dy)
            };

            // Attack if in range
            if dist <= range && !self.units[ai_idx].has_attacked {
                let is_ranged = kind == UnitKind::Archer && dist > 1;

                let (attacker, defender) = if ai_idx < enemy_idx {
                    let (left, right) = self.units.split_at_mut(enemy_idx);
                    (&mut left[ai_idx], &mut right[0])
                } else {
                    let (left, right) = self.units.split_at_mut(ai_idx);
                    (&mut right[0], &mut left[enemy_idx])
                };

                if is_ranged {
                    let result = combat::execute_ranged(attacker, defender, &self.grid);
                    let (sx, sy) = grid::grid_to_world(attacker.grid_x, attacker.grid_y);
                    let (tx, ty) = grid::grid_to_world(defender.grid_x, defender.grid_y);
                    self.projectiles.push(Projectile::new(sx, sy, tx, ty));

                    if result.target_killed {
                        let (wx, wy) = grid::grid_to_world(defender.grid_x, defender.grid_y);
                        self.particles
                            .push(Particle::new(ParticleKind::ExplosionLarge, wx, wy));
                    }
                } else {
                    let result = combat::execute_melee(attacker, defender, &self.grid);
                    let (wx, wy) = grid::grid_to_world(defender.grid_x, defender.grid_y);
                    self.particles
                        .push(Particle::new(ParticleKind::ExplosionSmall, wx, wy));

                    if result.target_killed {
                        self.particles
                            .push(Particle::new(ParticleKind::ExplosionLarge, wx, wy));
                    }
                }
            } else if self.units[ai_idx].movement_left > 0 {
                // Move toward nearest enemy (simple: one step closer)
                let mut best = (ax, ay);
                let mut best_dist = i32::MAX;

                for &(sdx, sdy) in &[(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                    let nx = ax as i32 + sdx;
                    let ny = ay as i32 + sdy;
                    if !self.grid.in_bounds(nx, ny) {
                        continue;
                    }
                    let nx = nx as u32;
                    let ny = ny as u32;
                    if !self.grid.is_passable(nx, ny) {
                        continue;
                    }
                    if self.unit_at(nx, ny).is_some() {
                        continue;
                    }
                    let d = (nx as i32 - ex as i32).abs() + (ny as i32 - ey as i32).abs();
                    if d < best_dist {
                        best_dist = d;
                        best = (nx, ny);
                    }
                }

                if best != (ax, ay) {
                    let unit = &mut self.units[ai_idx];
                    unit.grid_x = best.0;
                    unit.grid_y = best.1;
                    unit.movement_left -= 1;

                    if best.0 > ax {
                        unit.facing = Facing::Right;
                    } else if best.0 < ax {
                        unit.facing = Facing::Left;
                    }

                    let (wx, wy) = grid::grid_to_world(best.0, best.1);
                    self.particles
                        .push(Particle::new(ParticleKind::Dust, wx, wy));
                }
            }
        }

        self.turn.ai_done = true;
        self.turn.advance(); // -> Resolution
    }

    /// Resolve the turn: remove dead units, reset turn state, advance.
    pub fn resolve_turn(&mut self) {
        if self.turn.phase != TurnPhase::Resolution {
            return;
        }

        // Reset all living units for next turn
        for unit in &mut self.units {
            if unit.alive {
                unit.reset_turn();
            }
        }

        self.turn.resolution_done = true;
        self.turn.advance(); // -> PlayerTurn
        self.compute_player_targets();

        self.events.push(GameEvent::TurnChanged {
            turn: self.turn.turn_number,
            phase: self.turn.phase,
        });
    }

    /// Update animations, particles, projectiles, and death fades.
    pub fn update(&mut self, dt: f64) {
        for unit in &mut self.units {
            if unit.alive {
                unit.animation.update(dt);
            } else if unit.death_fade > 0.0 {
                unit.death_fade = (unit.death_fade - dt as f32).max(0.0);
                unit.animation.update(dt);
            }
        }

        // Update particles
        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| !p.finished);

        // Update projectiles
        for proj in &mut self.projectiles {
            proj.update(dt as f32);
        }
        self.projectiles.retain(|p| !p.finished);
    }

    /// Set up a simple demo battle for testing.
    pub fn setup_demo_battle(&mut self) {
        // Blue army (player side)
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 30, true);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 32, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 28, false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, 8, 30, false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, 8, 32, false);

        // Red army (enemy side)
        self.spawn_unit(UnitKind::Warrior, Faction::Red, 20, 30, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, 20, 32, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, 20, 28, false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, 22, 30, false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, 22, 32, false);

        // Some terrain variety
        for x in 14..17 {
            self.grid.set(x, 29, crate::grid::TileKind::Hill);
            self.grid.set(x, 30, crate::grid::TileKind::Hill);
            self.grid.set(x, 31, crate::grid::TileKind::Hill);
        }
        for x in 5..8 {
            self.grid.set(x, 25, crate::grid::TileKind::Water);
            self.grid.set(x, 26, crate::grid::TileKind::Water);
        }
        for x in 24..27 {
            self.grid.set(x, 33, crate::grid::TileKind::Forest);
            self.grid.set(x, 34, crate::grid::TileKind::Forest);
        }

        // Center camera on the battlefield action
        let (cx, cy) = grid::grid_to_world(15, 30);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = 1.0;

        self.compute_player_targets();
    }

    /// Check if player is alive.
    pub fn is_player_alive(&self) -> bool {
        self.player_unit().is_some()
    }

    /// Check if a faction has been eliminated.
    pub fn faction_eliminated(&self, faction: Faction) -> bool {
        !self.units.iter().any(|u| u.alive && u.faction == faction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_and_find_units() {
        let mut game = Game::new(960.0, 640.0);
        let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.find_unit(id).is_some());
        assert!(game.player_unit().is_some());
        assert!(game.unit_at(5, 5).is_some());
        assert!(game.unit_at(6, 6).is_none());
    }

    #[test]
    fn player_move() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_player_targets();

        assert!(game.handle_player_action(PlayerAction::Move {
            target_x: 6,
            target_y: 5
        }));

        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 6);
        assert_eq!(player.grid_y, 5);
    }

    #[test]
    fn player_cannot_move_out_of_range() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_player_targets();

        assert!(!game.handle_player_action(PlayerAction::Move {
            target_x: 20,
            target_y: 5
        }));
    }

    #[test]
    fn melee_attack_flow() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.compute_player_targets();

        assert!(game.attack_targets.contains(&enemy_id));
        assert!(game.handle_player_action(PlayerAction::Attack {
            target_id: enemy_id
        }));

        let enemy = game.find_unit(enemy_id).unwrap();
        assert!(enemy.hp < 10);
    }

    #[test]
    fn end_turn_advances_to_ai() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_player_targets();

        assert!(game.handle_player_action(PlayerAction::EndTurn));
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);
    }

    #[test]
    fn ai_attacks_nearby_enemy() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.turn.phase = TurnPhase::AiTurn;

        game.run_ai_turn();
        let player = game.player_unit().unwrap();
        assert!(player.hp < 10);
    }

    #[test]
    fn full_turn_cycle() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 20, 20, false);
        game.compute_player_targets();

        game.handle_player_action(PlayerAction::EndTurn);
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);

        game.run_ai_turn();
        assert_eq!(game.turn.phase, TurnPhase::Resolution);

        game.resolve_turn();
        assert_eq!(game.turn.phase, TurnPhase::PlayerTurn);
        assert_eq!(game.turn.turn_number, 2);
    }

    #[test]
    fn faction_elimination() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.faction_eliminated(Faction::Red));
        assert!(!game.faction_eliminated(Faction::Blue));
    }
}
