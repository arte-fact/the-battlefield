use crate::camera::Camera;
use crate::combat;
use crate::grid::{self, Grid, GRID_SIZE, TILE_SIZE};
use crate::input::SwipeDir;
use crate::particle::{Particle, ParticleKind, Projectile};
use crate::terrain_gen;
use crate::turn::{TurnPhase, TurnState};
use crate::unit::{Facing, Faction, Unit, UnitAnim, UnitId, UnitKind};

/// Read-only preview of a swipe path for rendering.
#[derive(Debug, Clone, Default)]
pub struct SwipePreview {
    /// Tiles the player would walk through.
    pub path: Vec<(u32, u32)>,
    /// Enemy grid position if walk would be interrupted.
    pub attack_target: Option<(u32, u32)>,
}

/// Result of a directional walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveResult {
    /// Reached end of range or obstacle; turn auto-ends.
    Completed,
    /// Enemy entered attack range; player can still attack.
    Interrupted,
    /// First tile blocked; nothing happened.
    NoMove,
}

/// Actions the player can perform.
#[derive(Debug, Clone)]
pub enum PlayerAction {
    Move { target_x: u32, target_y: u32 },
    MoveDirection { dir: SwipeDir },
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
    /// Live swipe preview (computed each frame from touch input).
    pub swipe_preview: Option<SwipePreview>,
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
            swipe_preview: None,
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

        // Movement: simple flood-fill within movement range (only if not yet moved)
        if mov > 0 && !player.has_moved {
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
                player.has_moved = true;
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

                // Auto-end turn after move
                self.auto_end_turn();
                true
            }
            PlayerAction::MoveDirection { dir } => {
                // Block if already acted
                let player = match self.player_unit() {
                    Some(p) => p,
                    None => return false,
                };
                if player.has_moved || player.has_attacked {
                    return false;
                }

                match self.walk_straight_line(dir) {
                    MoveResult::NoMove => false,
                    MoveResult::Completed => {
                        self.auto_end_turn();
                        true
                    }
                    MoveResult::Interrupted => {
                        // Turn continues — player can still attack
                        true
                    }
                }
            }
            PlayerAction::Attack { target_id } => {
                if !self.attack_targets.contains(&target_id) {
                    return false;
                }
                self.execute_attack_by_player(target_id);
                self.compute_player_targets();
                // Auto-end turn after attack
                self.auto_end_turn();
                true
            }
            PlayerAction::EndTurn => {
                self.end_turn();
                true
            }
        }
    }

    fn end_turn(&mut self) {
        self.turn.advance();
        self.events.push(GameEvent::TurnChanged {
            turn: self.turn.turn_number,
            phase: self.turn.phase,
        });
    }

    fn auto_end_turn(&mut self) {
        self.end_turn();
    }

    /// Walk tile-by-tile in a straight line. Returns how the walk ended.
    pub fn walk_straight_line(&mut self, dir: SwipeDir) -> MoveResult {
        let player = match self.player_unit() {
            Some(p) => p,
            None => return MoveResult::NoMove,
        };
        let mut px = player.grid_x;
        let mut py = player.grid_y;
        let mut movement_left = player.movement_left;
        let range = player.stats.range;
        let faction = player.faction;
        let player_id = player.id;
        let (ddx, ddy) = dir.delta();

        let mut steps = 0u32;

        loop {
            let nx = px as i32 + ddx;
            let ny = py as i32 + ddy;

            // Bounds check
            if !self.grid.in_bounds(nx, ny) {
                break;
            }
            let nx = nx as u32;
            let ny = ny as u32;

            // Passability check
            let cost = match self.grid.get(nx, ny).movement_cost() {
                Some(c) => c,
                None => break,
            };

            // Movement cost check
            if cost > movement_left {
                break;
            }

            // Occupied check
            if self.unit_at(nx, ny).is_some() {
                break;
            }

            // Move one step
            movement_left -= cost;
            let from_x = px;
            let from_y = py;
            px = nx;
            py = ny;
            steps += 1;

            // Update the actual unit
            let player = self.player_unit_mut().unwrap();
            player.grid_x = px;
            player.grid_y = py;
            player.movement_left = movement_left;

            // Face direction
            if ddx > 0 {
                player.facing = Facing::Right;
            } else if ddx < 0 {
                player.facing = Facing::Left;
            }

            // Dust particle
            let (wx, wy) = grid::grid_to_world(px, py);
            self.particles
                .push(Particle::new(ParticleKind::Dust, wx, wy));

            self.events.push(GameEvent::UnitMoved {
                unit_id: player_id,
                from_x,
                from_y,
                to_x: px,
                to_y: py,
            });

            // Check if an enemy is now in attack range
            if self.has_enemy_in_range(px, py, range, faction) {
                let player = self.player_unit_mut().unwrap();
                player.movement_left = 0;
                player.has_moved = true;
                self.compute_player_targets();
                return MoveResult::Interrupted;
            }
        }

        if steps == 0 {
            return MoveResult::NoMove;
        }

        let player = self.player_unit_mut().unwrap();
        player.has_moved = true;
        player.movement_left = 0;
        self.compute_player_targets();
        MoveResult::Completed
    }

    fn has_enemy_in_range(&self, x: u32, y: u32, range: u32, faction: Faction) -> bool {
        self.units.iter().any(|u| {
            u.alive && u.faction != faction && {
                let dx = (x as i32 - u.grid_x as i32).unsigned_abs();
                let dy = (y as i32 - u.grid_y as i32).unsigned_abs();
                dx.max(dy) <= range
            }
        })
    }

    fn find_nearest_enemy_in_range(
        &self,
        x: u32,
        y: u32,
        range: u32,
        faction: Faction,
    ) -> Option<(u32, u32)> {
        self.units
            .iter()
            .filter(|u| {
                u.alive && u.faction != faction && {
                    let dx = (x as i32 - u.grid_x as i32).unsigned_abs();
                    let dy = (y as i32 - u.grid_y as i32).unsigned_abs();
                    dx.max(dy) <= range
                }
            })
            .min_by_key(|u| {
                let dx = (x as i32 - u.grid_x as i32).unsigned_abs();
                let dy = (y as i32 - u.grid_y as i32).unsigned_abs();
                dx + dy
            })
            .map(|u| (u.grid_x, u.grid_y))
    }

    /// Compute a read-only swipe preview (does NOT mutate game state).
    pub fn compute_swipe_preview(&self, dir: SwipeDir) -> SwipePreview {
        let player = match self.player_unit() {
            Some(p) => p,
            None => return SwipePreview::default(),
        };
        if player.has_moved || player.has_attacked {
            return SwipePreview::default();
        }

        let mut px = player.grid_x;
        let mut py = player.grid_y;
        let mut movement_left = player.movement_left;
        let range = player.stats.range;
        let faction = player.faction;
        let (ddx, ddy) = dir.delta();

        let mut path = Vec::new();

        loop {
            let nx = px as i32 + ddx;
            let ny = py as i32 + ddy;

            if !self.grid.in_bounds(nx, ny) {
                break;
            }
            let nx = nx as u32;
            let ny = ny as u32;

            let cost = match self.grid.get(nx, ny).movement_cost() {
                Some(c) => c,
                None => break,
            };

            if cost > movement_left {
                break;
            }

            if self.unit_at(nx, ny).is_some() {
                break;
            }

            movement_left -= cost;
            px = nx;
            py = ny;
            path.push((px, py));

            // Check if enemy entered attack range
            if let Some(enemy_pos) = self.find_nearest_enemy_in_range(px, py, range, faction) {
                return SwipePreview {
                    path,
                    attack_target: Some(enemy_pos),
                };
            }
        }

        SwipePreview {
            path,
            attack_target: None,
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

    /// Find the nearest attackable enemy in the given swipe direction.
    pub fn find_attack_target_in_direction(&self, dir: SwipeDir) -> Option<UnitId> {
        let player = self.player_unit()?;
        let px = player.grid_x as i32;
        let py = player.grid_y as i32;

        self.attack_targets
            .iter()
            .filter_map(|&target_id| {
                let unit = self.find_unit(target_id)?;
                let rx = unit.grid_x as i32 - px;
                let ry = unit.grid_y as i32 - py;
                let target_dir = SwipeDir::from_grid_delta(rx, ry)?;
                if target_dir == dir {
                    let dist = rx.unsigned_abs() + ry.unsigned_abs();
                    Some((target_id, dist))
                } else {
                    None
                }
            })
            .min_by_key(|&(_, dist)| dist)
            .map(|(id, _)| id)
    }

    /// Find the farthest reachable move target in the given swipe direction.
    pub fn find_farthest_move_target(&self, dir: SwipeDir) -> Option<(u32, u32)> {
        let player = self.player_unit()?;
        let px = player.grid_x as i32;
        let py = player.grid_y as i32;
        let (dir_dx, dir_dy) = dir.delta();

        self.move_targets
            .iter()
            .filter(|&&(mx, my)| {
                let rx = mx as i32 - px;
                let ry = my as i32 - py;
                SwipeDir::from_grid_delta(rx, ry) == Some(dir)
            })
            .max_by_key(|&&(mx, my)| {
                let rx = mx as i32 - px;
                let ry = my as i32 - py;
                rx * dir_dx + ry * dir_dy
            })
            .copied()
    }

    /// Set up a demo battle with procedural terrain.
    pub fn setup_demo_battle(&mut self) {
        self.setup_demo_battle_with_seed(42);
    }

    /// Set up a demo battle with a specific seed for terrain generation.
    pub fn setup_demo_battle_with_seed(&mut self, seed: u32) {
        self.grid = terrain_gen::generate_battlefield(seed);

        let (blue_x, blue_y) = terrain_gen::blue_spawn_area();
        let (red_x, red_y) = terrain_gen::red_spawn_area();

        // Blue army (player side) — spread around spawn point
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_x, blue_y, true);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_x, blue_y + 2, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_x, blue_y.saturating_sub(2), false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, blue_x.saturating_sub(2), blue_y, false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, blue_x.saturating_sub(2), blue_y + 2, false);

        // Red army (enemy side)
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_x, red_y, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_x, red_y + 2, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_x, red_y.saturating_sub(2), false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, red_x + 2, red_y, false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, red_x + 2, red_y + 2, false);

        // Center camera on the player with a view of surrounding terrain
        let (cx, cy) = grid::grid_to_world(blue_x + 5, blue_y);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = 0.8;

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
    fn player_move_auto_ends_turn() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_player_targets();

        assert!(game.handle_player_action(PlayerAction::Move {
            target_x: 6,
            target_y: 5
        }));

        // Move auto-ends the turn
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);
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
    fn melee_attack_auto_ends_turn() {
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
        // Attack auto-ends the turn
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);
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
    fn swipe_finds_farthest_move_target() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_player_targets();

        // Should find a target to the east
        let target = game.find_farthest_move_target(SwipeDir::E);
        assert!(target.is_some());
        let (tx, _ty) = target.unwrap();
        assert!(tx > 5); // moved east
    }

    #[test]
    fn swipe_finds_attack_target_in_direction() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.compute_player_targets();

        // Enemy is to the east
        assert_eq!(
            game.find_attack_target_in_direction(SwipeDir::E),
            Some(enemy_id)
        );
        // No enemy to the west
        assert_eq!(game.find_attack_target_in_direction(SwipeDir::W), None);
    }

    #[test]
    fn faction_elimination() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.faction_eliminated(Faction::Red));
        assert!(!game.faction_eliminated(Faction::Blue));
    }

    // -- walk_straight_line tests --

    #[test]
    fn walk_straight_line_on_grass() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Warrior has mov=3, all grass (cost 1 each)
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::Completed);
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 8); // 5 + 3 steps
        assert_eq!(player.grid_y, 5);
    }

    #[test]
    fn walk_respects_terrain_cost() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Hill at (7,5) costs 2
        game.grid.set(7, 5, crate::grid::TileKind::Hill);
        // mov=3: (6,5) costs 1, (7,5) costs 2 = total 3
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::Completed);
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 7);
    }

    #[test]
    fn walk_blocked_by_impassable() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.grid.set(6, 5, crate::grid::TileKind::Water);
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::NoMove);
    }

    #[test]
    fn walk_blocked_by_unit() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 6, 5, false); // ally blocking
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::NoMove);
    }

    #[test]
    fn walk_interrupted_by_enemy() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 8, 5, false);
        // Warrior range=1, swipe E: walks (6,5), (7,5) then enemy at (8,5) is in range
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::Interrupted);
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 7); // stopped adjacent to enemy
        assert!(player.has_moved);
    }

    #[test]
    fn after_interrupt_can_attack() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 8, 5, false);

        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::Interrupted);
        // Should still be player's turn
        assert_eq!(game.turn.phase, TurnPhase::PlayerTurn);
        // Attack targets should include the enemy
        assert!(game.attack_targets.contains(&enemy_id));
        // Attack should work and auto-end turn
        assert!(game.handle_player_action(PlayerAction::Attack {
            target_id: enemy_id
        }));
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);
    }

    #[test]
    fn after_interrupt_cannot_move() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 8, 5, false);

        game.walk_straight_line(SwipeDir::E); // interrupted
        // Trying to move again should fail
        let result = game.handle_player_action(PlayerAction::MoveDirection { dir: SwipeDir::W });
        assert!(!result);
    }

    #[test]
    fn completed_move_auto_ends_turn() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);

        assert!(game.handle_player_action(PlayerAction::MoveDirection { dir: SwipeDir::E }));
        assert_eq!(game.turn.phase, TurnPhase::AiTurn);
    }

    #[test]
    fn archer_interrupted_at_range() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Archer, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 13, 5, false);
        // Archer range=5, mov=3: walks to (6,5), (7,5), (8,5)
        // At (8,5), enemy at (13,5) is distance 5 = in range
        let result = game.walk_straight_line(SwipeDir::E);
        assert_eq!(result, MoveResult::Interrupted);
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 8);
    }

    #[test]
    fn diagonal_walk() {
        use crate::input::SwipeDir;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // NE = (+1, -1) per step, mov=3
        let result = game.walk_straight_line(SwipeDir::NE);
        assert_eq!(result, MoveResult::Completed);
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 8);
        assert_eq!(player.grid_y, 2);
    }
}
