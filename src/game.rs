use crate::animation::TurnEvent;
use crate::building::FactionBase;
use crate::camera::Camera;
use crate::combat;
use crate::grid::{self, Grid, TileKind, GRID_SIZE, TILE_SIZE};
use crate::input::SwipeDir;
use crate::mapgen;
use crate::particle::{Particle, Projectile};
use crate::turn::TurnState;
use crate::unit::{Facing, Faction, Unit, UnitAnim, UnitId, UnitKind, MELEE_RANGE, UNIT_RADIUS};
use crate::zone::ZoneManager;

/// Player vision radius in tiles.
const FOV_RADIUS: i32 = 15;

/// Half-angle of the player's attack cone (60° = PI/3 radians).
pub const ATTACK_CONE_HALF_ANGLE: f32 = std::f32::consts::FRAC_PI_3;

/// Knockback distance in pixels (roughly half a tile).
const KNOCKBACK_DIST: f32 = TILE_SIZE * 0.5;

/// Monks try to stay at least this far from enemies (3 tiles).
const MONK_SAFE_DIST: f32 = TILE_SIZE * 3.0;

/// Monks stop approaching allies once within this distance (2 tiles).
const MONK_FOLLOW_DIST: f32 = TILE_SIZE * 2.0;

pub struct Game {
    pub grid: Grid,
    pub units: Vec<Unit>,
    pub turn_state: TurnState,
    pub camera: Camera,
    pub particles: Vec<Particle>,
    pub projectiles: Vec<Projectile>,
    next_unit_id: UnitId,
    /// Tiles currently visible to the player this turn.
    pub visible: Vec<bool>,
    /// Tiles that have been seen at least once (revealed through fog).
    pub revealed: Vec<bool>,
    /// Set to true when FOV changes; renderer clears it after updating fog cache.
    pub fog_dirty: bool,
    /// Pre-computed: true if land tile is adjacent to water (for foam rendering).
    pub water_adjacency: Vec<bool>,
    /// Turn events recorded during game logic for animation playback.
    pub turn_events: Vec<TurnEvent>,
    /// Last grid cell where FOV was computed (optimization: skip if unchanged).
    pub last_fov_cell: (u32, u32),
    /// Player aim direction in radians (0 = right). Updated from movement input.
    pub player_aim_dir: f32,
    /// Strategic objective for Blue faction (world-space coords of Red spawn).
    pub blue_objective: (f32, f32),
    /// Strategic objective for Red faction (world-space coords of Blue spawn).
    pub red_objective: (f32, f32),
    /// Capture zone manager.
    pub zone_manager: ZoneManager,
    /// Faction bases with production buildings.
    pub bases: Vec<FactionBase>,
    /// Set when a faction wins (holds all zones for VICTORY_HOLD_TIME).
    pub winner: Option<Faction>,
}

impl Game {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut camera = Camera::new(viewport_w, viewport_h);
        let center = GRID_SIZE as f32 * TILE_SIZE * 0.5;
        camera.x = center;
        camera.y = center;

        let size = (GRID_SIZE * GRID_SIZE) as usize;
        Self {
            grid,
            units: Vec::new(),
            turn_state: TurnState::new(),
            camera,
            particles: Vec::new(),
            projectiles: Vec::new(),
            next_unit_id: 1,
            visible: vec![false; size],
            revealed: vec![false; size],
            fog_dirty: true,
            water_adjacency: vec![false; size],
            turn_events: Vec::new(),
            last_fov_cell: (0, 0),
            player_aim_dir: 0.0,
            blue_objective: (0.0, 0.0),
            red_objective: (0.0, 0.0),
            zone_manager: ZoneManager::empty(),
            bases: Vec::new(),
            winner: None,
        }
    }

    pub fn turn_number(&self) -> u32 {
        self.turn_state.turn_number
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
        let mut unit = Unit::new(id, kind, faction, x, y, is_player);
        // Stagger AI initial attack cooldowns to prevent all acting on the same frame
        if !is_player {
            unit.attack_cooldown = (id as f32 * 0.05) % 0.3;
        }
        self.units.push(unit);
        id
    }

    pub fn player_unit(&self) -> Option<&Unit> {
        self.units.iter().find(|u| u.is_player && u.alive)
    }

    pub fn player_unit_mut(&mut self) -> Option<&mut Unit> {
        self.units.iter_mut().find(|u| u.is_player && u.alive)
    }

    pub fn unit_at(&self, x: u32, y: u32) -> Option<&Unit> {
        self.units
            .iter()
            .find(|u| u.alive && u.grid_cell() == (x, y))
    }

    pub fn find_unit(&self, id: UnitId) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }

    /// Find the closest alive enemy unit near a world position (for arrow impact).
    /// Returns the index of the closest enemy of the opposing faction within hit radius.
    fn find_unit_near(&self, x: f32, y: f32, attacker_faction: Faction) -> Option<usize> {
        let hit_radius = TILE_SIZE * 0.75;
        self.units
            .iter()
            .enumerate()
            .filter(|(_, u)| u.alive && u.faction != attacker_faction)
            .filter_map(|(i, u)| {
                let dist = u.distance_to_pos(x, y);
                if dist <= hit_radius {
                    Some((i, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    /// Find nearest enemy unit within range of a world position.
    pub fn enemy_in_range(&self, x: f32, y: f32, faction: Faction, range: f32) -> Option<UnitId> {
        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .filter_map(|u| {
                let dist = u.distance_to_pos(x, y);
                if dist <= range {
                    Some((u.id, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
    }

    /// Find nearest enemy within range AND within a cone defined by aim direction and half-angle.
    pub fn enemy_in_cone(
        &self,
        x: f32,
        y: f32,
        faction: Faction,
        range: f32,
        aim_dir: f32,
        half_angle: f32,
    ) -> Option<UnitId> {
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
                // Normalize to [-PI, PI]
                diff = (diff + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                if diff.abs() <= half_angle {
                    Some((u.id, dist))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id)
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

    // ---- Legacy test shim ----

    /// Legacy test shim: player swipes in a direction, AI acts synchronously,
    /// turns reset. Zeroes cooldowns before acting so tests pass unchanged.
    pub fn player_step(&mut self, dir: SwipeDir) -> bool {
        // Zero cooldowns so the action always succeeds (legacy turn-based mode)
        for unit in &mut self.units {
            unit.attack_cooldown = 0.0;
        }

        let player = match self.player_unit() {
            Some(p) => p,
            None => return false,
        };
        let (px, py) = player.grid_cell();
        let faction = player.faction;
        let player_id = player.id;
        let (dx, dy) = dir.delta();
        let nx = px as i32 + dx;
        let ny = py as i32 + dy;

        if !self.grid.in_bounds(nx, ny) {
            return false;
        }
        let nx = nx as u32;
        let ny = ny as u32;

        // Snapshot positions BEFORE anyone acts this turn.
        let position_snapshot: Vec<(UnitId, f32, f32)> = self
            .units
            .iter()
            .filter(|u| u.alive)
            .map(|u| (u.id, u.x, u.y))
            .collect();

        // Enemy at target -> attack
        let enemy_at_target = self
            .unit_at(nx, ny)
            .filter(|u| u.faction != faction)
            .map(|u| u.id);

        if let Some(enemy_id) = enemy_at_target {
            self.execute_attack(player_id, enemy_id, None);
            if let Some(p) = self.player_unit_mut() {
                p.has_attacked = true;
            }
        } else if self.unit_at(nx, ny).is_some()
            || !self.grid.is_passable(nx, ny)
            || !self.grid.can_move_diagonal(px, py, dx, dy)
        {
            // Blocked by friendly unit, impassable terrain, or corner-cutting
            return false;
        } else {
            // Move player to the target tile
            let (wx, wy) = grid::grid_to_world(nx, ny);
            let player = self.player_unit_mut().unwrap();
            let from = (player.x, player.y);
            player.x = wx;
            player.y = wy;
            if dx > 0 {
                player.facing = Facing::Right;
            } else if dx < 0 {
                player.facing = Facing::Left;
            }
            self.turn_events.push(TurnEvent::Move {
                unit_id: player_id,
                from,
                to: (wx, wy),
            });
        }

        // Auto-turn: AI acts, using pre-turn snapshot for ranged targeting
        self.ai_turn(&position_snapshot);

        // Advance turn and reset all living units
        self.turn_state.turn_number += 1;
        for unit in &mut self.units {
            if unit.alive {
                unit.reset_turn();
                unit.attack_cooldown = 0.0;
            }
        }

        // Recompute FOV after player acts
        self.compute_fov();
        true
    }

    // ---- Real-time methods ----

    /// Tick all alive units' cooldowns by dt seconds.
    pub fn tick_cooldowns(&mut self, dt: f32) {
        for unit in &mut self.units {
            if unit.alive {
                unit.tick_cooldowns(dt);
            }
        }
    }

    /// Move a unit continuously in a direction with split-axis terrain collision.
    fn move_unit(&mut self, idx: usize, dir_x: f32, dir_y: f32, dt: f32) {
        let speed = self.units[idx].move_speed()
            * self.grid.speed_factor_at(self.units[idx].x, self.units[idx].y);
        let vx = dir_x * speed * dt;
        let vy = dir_y * speed * dt;

        let old_x = self.units[idx].x;
        let old_y = self.units[idx].y;

        // Split-axis collision: try X first
        let new_x = old_x + vx;
        if self.grid.is_circle_passable(new_x, old_y, UNIT_RADIUS) {
            self.units[idx].x = new_x;
        }

        // Then try Y
        let cur_x = self.units[idx].x;
        let new_y = old_y + vy;
        if self.grid.is_circle_passable(cur_x, new_y, UNIT_RADIUS) {
            self.units[idx].y = new_y;
        }

        // Update facing from movement (player facing is managed by the game loop)
        if !self.units[idx].is_player {
            if vx > 0.01 {
                self.units[idx].facing = Facing::Right;
            } else if vx < -0.01 {
                self.units[idx].facing = Facing::Left;
            }
        }
    }

    /// Resolve circle-circle collisions between all alive units.
    /// Only pushes a unit if the destination is passable terrain.
    pub fn resolve_collisions(&mut self) {
        let min_dist = UNIT_RADIUS * 2.0;

        for i in 0..self.units.len() {
            if !self.units[i].alive {
                continue;
            }
            for j in (i + 1)..self.units.len() {
                if !self.units[j].alive {
                    continue;
                }
                let dx = self.units[j].x - self.units[i].x;
                let dy = self.units[j].y - self.units[i].y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < min_dist && dist > 0.001 {
                    let overlap = (min_dist - dist) / 2.0;
                    let nx = dx / dist;
                    let ny = dy / dist;
                    let new_ix = self.units[i].x - nx * overlap;
                    let new_iy = self.units[i].y - ny * overlap;
                    let new_jx = self.units[j].x + nx * overlap;
                    let new_jy = self.units[j].y + ny * overlap;
                    let i_ok = self.grid.is_circle_passable(new_ix, new_iy, UNIT_RADIUS);
                    let j_ok = self.grid.is_circle_passable(new_jx, new_jy, UNIT_RADIUS);
                    if i_ok && j_ok {
                        self.units[i].x = new_ix;
                        self.units[i].y = new_iy;
                        self.units[j].x = new_jx;
                        self.units[j].y = new_jy;
                    } else if i_ok {
                        // Only push i (j is against terrain)
                        let dbl_ix = self.units[i].x - nx * overlap * 2.0;
                        let dbl_iy = self.units[i].y - ny * overlap * 2.0;
                        if self.grid.is_circle_passable(dbl_ix, dbl_iy, UNIT_RADIUS) {
                            self.units[i].x = dbl_ix;
                            self.units[i].y = dbl_iy;
                        }
                    } else if j_ok {
                        // Only push j (i is against terrain)
                        let dbl_jx = self.units[j].x + nx * overlap * 2.0;
                        let dbl_jy = self.units[j].y + ny * overlap * 2.0;
                        if self.grid.is_circle_passable(dbl_jx, dbl_jy, UNIT_RADIUS) {
                            self.units[j].x = dbl_jx;
                            self.units[j].y = dbl_jy;
                        }
                    }
                    // If neither is passable, don't move either
                }
            }
        }
    }

    /// Update run/idle animations based on whether units moved since last frame.
    pub fn update_movement_anims(&mut self, old_positions: &[(f32, f32)]) {
        for (i, unit) in self.units.iter_mut().enumerate() {
            if !unit.alive {
                continue;
            }
            if unit.current_anim == UnitAnim::Attack {
                if unit.attack_cooldown <= 0.0 {
                    unit.set_anim(UnitAnim::Idle);
                }
                continue;
            }
            if i < old_positions.len() {
                let (ox, oy) = old_positions[i];
                let moved = (unit.x - ox).abs() > 0.1 || (unit.y - oy).abs() > 0.1;
                if moved {
                    unit.set_anim(UnitAnim::Run);
                } else if unit.current_anim == UnitAnim::Run {
                    unit.set_anim(UnitAnim::Idle);
                }
            }
        }
    }

    /// Real-time player movement: continuous movement only.
    pub fn try_player_move(&mut self, dir_x: f32, dir_y: f32, dt: f32) {
        let player_idx = match self.units.iter().position(|u| u.is_player && u.alive) {
            Some(i) => i,
            None => return,
        };

        // Move
        self.move_unit(player_idx, dir_x, dir_y, dt);

        // FOV check: recompute only when crossing a tile boundary
        let new_cell = self.units[player_idx].grid_cell();
        if new_cell != self.last_fov_cell {
            self.last_fov_cell = new_cell;
            self.compute_fov();
        }
    }

    /// Try to attack the nearest enemy in range. Returns true if an attack was executed.
    /// Called explicitly from attack key/button — never auto-attacks.
    /// Player attack: hit enemies in cone if any, otherwise whiff swing.
    pub fn player_attack(&mut self) {
        let player_idx = match self.units.iter().position(|u| u.is_player && u.alive) {
            Some(i) => i,
            None => return,
        };

        if !self.units[player_idx].can_act() {
            return;
        }

        let player_id = self.units[player_idx].id;
        let player_faction = self.units[player_idx].faction;
        let px = self.units[player_idx].x;
        let py = self.units[player_idx].y;

        let attack_range = if self.units[player_idx].stats.range > 1 {
            self.units[player_idx].stats.range as f32 * TILE_SIZE
        } else {
            MELEE_RANGE
        };

        let targets = self.enemies_in_cone(
            px,
            py,
            player_faction,
            attack_range,
            self.player_aim_dir,
            ATTACK_CONE_HALF_ANGLE,
        );

        if targets.is_empty() {
            // Whiff: play attack anim with half cooldown
            self.units[player_idx].set_anim(UnitAnim::Attack);
            self.units[player_idx].attack_cooldown =
                self.units[player_idx].kind.base_attack_cooldown() * 0.5;
        } else {
            for enemy_id in targets {
                self.execute_attack(player_id, enemy_id, None);
                if let Some(idx) = self.units.iter().position(|u| u.id == enemy_id) {
                    self.apply_knockback(idx, px, py);
                }
            }
        }
    }

    /// Real-time AI tick: each AI unit acts independently with continuous movement.
    pub fn tick_ai(&mut self, dt: f32) {
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
        let kind = self.units[ai_idx].kind;
        match kind {
            UnitKind::Monk => self.ai_monk_tick(ai_idx, dt),
            UnitKind::Archer => self.ai_archer_tick(ai_idx, dt),
            UnitKind::Warrior | UnitKind::Lancer => {
                self.ai_melee_tick(ai_idx, dt)
            }
        }
    }

    /// Real-time melee AI: attack if in melee range and can_act, else move toward.
    fn ai_melee_tick(&mut self, ai_idx: usize, dt: f32) {
        let ai_id = self.units[ai_idx].id;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => {
                let objective = self.faction_objective(self.units[ai_idx].faction);
                self.ai_move_toward_continuous(ai_idx, objective.0, objective.1, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        let melee_reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
        let melee_reach = melee_reach.max(MELEE_RANGE);
        if self.units[ai_idx].can_act() && dist <= melee_reach {
            self.execute_attack(ai_id, enemy_id, None);
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
                let objective = self.faction_objective(self.units[ai_idx].faction);
                self.ai_move_toward_continuous(ai_idx, objective.0, objective.1, dt);
                return;
            }
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if self.units[ai_idx].can_act() && dist > MELEE_RANGE && dist <= range_world {
            self.execute_attack(ai_id, enemy_id, None);
        } else if self.units[ai_idx].can_act() && dist <= MELEE_RANGE {
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
        let heal_range = self.units[ai_idx].stats.range as f32 * TILE_SIZE;

        // Find nearby wounded ally within heal range
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
            if self.units[ai_idx].can_act() {
                self.execute_heal(ai_idx, target_id);
                return;
            }
        }

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

        // Move directly toward wounded ally to get in heal range (no standoff)
        let vision_range = Self::AI_VISION_RADIUS as f32 * TILE_SIZE;
        let wounded_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
            .filter(|u| u.hp < u.stats.max_hp)
            .filter(|u| u.distance_to_pos(ax, ay) <= vision_range)
            .filter(|u| self.nearest_enemy_dist(u.x, u.y, faction) >= MONK_SAFE_DIST)
            .min_by_key(|u| u.hp)
            .map(|u| (u.x, u.y));

        if let Some((tx, ty)) = wounded_target {
            self.ai_move_toward_continuous(ai_idx, tx, ty, dt);
            return;
        }

        // Fallback: follow nearest friendly combatant — only if safe, keep standoff distance
        let follow_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id && u.kind != UnitKind::Monk)
            .filter(|u| self.nearest_enemy_dist(u.x, u.y, faction) >= MONK_SAFE_DIST)
            .min_by(|a, b| {
                let da = a.distance_to_pos(ax, ay);
                let db = b.distance_to_pos(ax, ay);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|u| (u.x, u.y));
        if let Some((tx, ty)) = follow_target {
            let (sx, sy) = Self::monk_standoff_point(ax, ay, tx, ty);
            self.ai_move_toward_continuous(ai_idx, sx, sy, dt);
        }
    }

    /// Move AI unit continuously toward target using waypoint-following with A*.
    fn ai_move_toward_continuous(
        &mut self,
        ai_idx: usize,
        target_x: f32,
        target_y: f32,
        dt: f32,
    ) {
        let ai_id = self.units[ai_idx].id;

        // Tick path cooldown
        self.units[ai_idx].ai_path_cooldown =
            (self.units[ai_idx].ai_path_cooldown - dt).max(0.0);

        // Re-path if cooldown expired or path exhausted
        let needs_repath = self.units[ai_idx].ai_path_cooldown <= 0.0
            || self.units[ai_idx].ai_waypoint_idx >= self.units[ai_idx].ai_waypoints.len();

        if needs_repath {
            let (ax, ay) = self.units[ai_idx].grid_cell();
            let (gx, gy) = grid::world_to_grid(target_x, target_y);
            let gx = gx.max(0) as u32;
            let gy = gy.max(0) as u32;

            let occupied: Vec<(u32, u32)> = self
                .units
                .iter()
                .filter(|u| u.alive && u.id != ai_id)
                .map(|u| u.grid_cell())
                .collect();

            let path = self.grid.find_path(ax, ay, gx, gy, 60, |x, y| {
                occupied.iter().any(|&(ox, oy)| ox == x && oy == y)
            });

            if let Some(steps) = path {
                self.units[ai_idx].ai_waypoints = steps
                    .iter()
                    .map(|&(x, y)| grid::grid_to_world(x, y))
                    .collect();
                self.units[ai_idx].ai_waypoint_idx = 0;
            } else {
                self.units[ai_idx].ai_waypoints.clear();
                self.units[ai_idx].ai_waypoint_idx = 0;
            }
            self.units[ai_idx].ai_path_cooldown = 0.5;
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

            if dist < TILE_SIZE / 3.0 {
                self.units[ai_idx].ai_waypoint_idx += 1;
            } else if dist > 0.01 {
                let dir_x = ddx / dist;
                let dir_y = ddy / dist;
                self.move_unit(ai_idx, dir_x, dir_y, dt);
            }
        }
    }

    /// Recompute field of view from the player's position using recursive shadowcasting.
    pub fn compute_fov(&mut self) {
        let w = self.grid.width;
        let h = self.grid.height;

        // Clear current visibility
        for v in self.visible.iter_mut() {
            *v = false;
        }

        let player = match self.player_unit() {
            Some(p) => {
                let (gx, gy) = p.grid_cell();
                (gx as i32, gy as i32)
            }
            None => return,
        };

        // Player's own tile is always visible
        let idx = (player.1 as u32 * w + player.0 as u32) as usize;
        self.visible[idx] = true;
        self.revealed[idx] = true;

        // Run shadowcasting for all 8 octants
        for octant in 0..8 {
            self.cast_light(player.0, player.1, FOV_RADIUS, 1, 1.0, 0.0, octant, w, h);
        }

        self.fog_dirty = true;
    }

    /// Pre-compute water adjacency for all land tiles (for foam rendering).
    pub fn compute_water_adjacency(&mut self) {
        let w = self.grid.width;
        let h = self.grid.height;
        self.water_adjacency = vec![false; (w * h) as usize];
        for gy in 0..h {
            for gx in 0..w {
                if !self.grid.get(gx, gy).is_land() {
                    continue;
                }
                let has = (gy > 0 && self.grid.get(gx, gy - 1) == TileKind::Water)
                    || (gx + 1 < w && self.grid.get(gx + 1, gy) == TileKind::Water)
                    || (gy + 1 < h && self.grid.get(gx, gy + 1) == TileKind::Water)
                    || (gx > 0 && self.grid.get(gx - 1, gy) == TileKind::Water);
                self.water_adjacency[(gy * w + gx) as usize] = has;
            }
        }
    }

    /// Return the strategic objective for a faction (world-space coordinates).
    /// Prioritizes capture zones; falls back to enemy base if all zones are controlled.
    fn faction_objective(&self, faction: Faction) -> (f32, f32) {
        if let Some(zone) = self.zone_manager.best_target_zone(faction) {
            return (zone.center_wx, zone.center_wy);
        }
        match faction {
            Faction::Blue => self.blue_objective,
            _ => self.red_objective,
        }
    }

    /// Bresenham grid raycast: returns true if no intermediate tile blocks light.
    /// Skips the start and end tiles (units stand on them).
    fn has_line_of_sight(&self, x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
        let (gx1, gy1) = grid::world_to_grid(x1, y1);
        let (gx2, gy2) = grid::world_to_grid(x2, y2);

        let mut cx = gx1;
        let mut cy = gy1;
        let dx = (gx2 - gx1).abs();
        let dy = -(gy2 - gy1).abs();
        let sx: i32 = if gx1 < gx2 { 1 } else { -1 };
        let sy: i32 = if gy1 < gy2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            // Skip start and end tiles
            if (cx != gx1 || cy != gy1) && (cx != gx2 || cy != gy2) {
                if !self.grid.in_bounds(cx, cy) {
                    return false;
                }
                if self.blocks_light(cx as u32, cy as u32) {
                    return false;
                }
            }
            if cx == gx2 && cy == gy2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                cx += sx;
            }
            if e2 <= dx {
                err += dx;
                cy += sy;
            }
        }
        true
    }

    /// Returns true if the tile at (x, y) blocks line of sight.
    fn blocks_light(&self, x: u32, y: u32) -> bool {
        let tile = self.grid.get(x, y);
        match tile {
            TileKind::Water => false,
            TileKind::Forest => true,
            _ => {
                self.grid.elevation(x, y) >= 2
            }
        }
    }

    /// Recursive shadowcasting for one octant.
    #[allow(clippy::too_many_arguments)]
    fn cast_light(
        &mut self,
        ox: i32,
        oy: i32,
        radius: i32,
        row: i32,
        mut start_slope: f64,
        end_slope: f64,
        octant: u8,
        w: u32,
        h: u32,
    ) {
        if start_slope < end_slope || row > radius {
            return;
        }

        let mut blocked = false;
        let mut next_start_slope = start_slope;

        for j in row..=radius {
            if blocked {
                return;
            }
            let dy = -j;
            for dx in -j..=0 {
                let (tx, ty) = match octant {
                    0 => (dx, dy),
                    1 => (dy, dx),
                    2 => (-dy, dx),
                    3 => (-dx, dy),
                    4 => (-dx, -dy),
                    5 => (-dy, -dx),
                    6 => (dy, -dx),
                    _ => (dx, -dy),
                };

                let map_x = ox + tx;
                let map_y = oy + ty;

                if map_x < 0 || map_y < 0 || map_x >= w as i32 || map_y >= h as i32 {
                    continue;
                }

                let l_slope = (dx as f64 - 0.5) / (dy as f64 + 0.5);
                let r_slope = (dx as f64 + 0.5) / (dy as f64 - 0.5);

                if start_slope < r_slope {
                    continue;
                }
                if end_slope > l_slope {
                    break;
                }

                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= radius * radius {
                    let idx = (map_y as u32 * w + map_x as u32) as usize;
                    self.visible[idx] = true;
                    self.revealed[idx] = true;
                }

                let ux = map_x as u32;
                let uy = map_y as u32;
                let is_blocking = self.blocks_light(ux, uy);

                if blocked {
                    if is_blocking {
                        next_start_slope = r_slope;
                    } else {
                        blocked = false;
                        start_slope = next_start_slope;
                    }
                } else if is_blocking && j < radius {
                    blocked = true;
                    self.cast_light(
                        ox,
                        oy,
                        radius,
                        j + 1,
                        start_slope,
                        l_slope,
                        octant,
                        w,
                        h,
                    );
                    next_start_slope = r_slope;
                }
            }
            if blocked {
                return;
            }
        }
    }

    /// Push a unit away from a source position. Respects terrain collision.
    fn apply_knockback(&mut self, target_idx: usize, from_x: f32, from_y: f32) {
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

    /// Execute an attack. For ranged attacks, `target_snapshot_pos` is the world position
    /// the target was at when the archer decided to shoot (for projectile lag/miss).
    fn execute_attack(
        &mut self,
        attacker_id: UnitId,
        defender_id: UnitId,
        target_snapshot_pos: Option<(f32, f32)>,
    ) {
        let attacker_idx = self.units.iter().position(|u| u.id == attacker_id);
        let defender_idx = self.units.iter().position(|u| u.id == defender_id);
        let (attacker_idx, defender_idx) = match (attacker_idx, defender_idx) {
            (Some(a), Some(d)) => (a, d),
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
            let damage = combat::calc_ranged_damage(
                &self.units[attacker_idx],
                &self.units[defender_idx],
                &self.grid,
            );
            let faction = self.units[attacker_idx].faction;
            let ax = self.units[attacker_idx].x;
            let ay = self.units[attacker_idx].y;

            // Start cooldown + attack anim on attacker
            self.units[attacker_idx].start_attack_cooldown();
            self.units[attacker_idx].set_anim(UnitAnim::Attack);

            // Spawn ballistic projectile — damage applied on landing
            self.projectiles.push(Projectile::new(
                ax, ay, snap_x, snap_y, damage, faction,
            ));

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
            let result = combat::execute_melee(attacker, defender, &self.grid);
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

    /// AI vision radius in tiles (converted to world distance when used).
    const AI_VISION_RADIUS: u32 = 10;

    /// Distance from a world position to the nearest enemy of the given faction.
    /// Returns `f32::MAX` if no enemies exist.
    fn nearest_enemy_dist(&self, x: f32, y: f32, faction: Faction) -> f32 {
        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .map(|u| u.distance_to_pos(x, y))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(f32::MAX)
    }

    /// Find the nearest visible enemy for a unit (Euclidean distance in world pixels).
    fn find_nearest_enemy(&self, ai_idx: usize) -> Option<(f32, f32, UnitId, f32)> {
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

    // ---- Turn-based AI (used by player_step test shim) ----

    /// Process all AI units sequentially (turn-based).
    fn ai_turn(&mut self, position_snapshot: &[(UnitId, f32, f32)]) {
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
            self.ai_unit_action(ai_idx, position_snapshot);
        }
    }

    fn ai_unit_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, f32, f32)]) {
        let kind = self.units[ai_idx].kind;
        match kind {
            UnitKind::Monk => self.ai_monk_action(ai_idx),
            UnitKind::Archer => self.ai_archer_action(ai_idx, position_snapshot),
            UnitKind::Warrior | UnitKind::Lancer => {
                self.ai_melee_action(ai_idx, position_snapshot)
            }
        }
    }

    fn ai_melee_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, f32, f32)]) {
        let ai_id = self.units[ai_idx].id;
        let has_attacked = self.units[ai_idx].has_attacked;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => return,
        };
        let (ex, ey, enemy_id, dist) = enemy;

        let melee_reach = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
        let melee_reach = melee_reach.max(MELEE_RANGE);
        if !has_attacked && dist <= melee_reach {
            let snap_pos = position_snapshot
                .iter()
                .find(|(id, _, _)| *id == enemy_id)
                .map(|&(_, x, y)| (x, y));
            self.execute_attack(ai_id, enemy_id, snap_pos);
            self.units[ai_idx].has_attacked = true;
        } else {
            let (gx, gy) = grid::world_to_grid(ex, ey);
            self.ai_move_toward(ai_idx, gx.max(0) as u32, gy.max(0) as u32);
        }
    }

    fn ai_archer_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, f32, f32)]) {
        let ai_id = self.units[ai_idx].id;
        let range_world = self.units[ai_idx].stats.range as f32 * TILE_SIZE;
        let has_attacked = self.units[ai_idx].has_attacked;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => return,
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if !has_attacked && dist > MELEE_RANGE && dist <= range_world {
            let snap_pos = position_snapshot
                .iter()
                .find(|(id, _, _)| *id == enemy_id)
                .map(|&(_, x, y)| (x, y));
            self.execute_attack(ai_id, enemy_id, snap_pos);
            self.units[ai_idx].has_attacked = true;
        } else if !has_attacked && dist <= MELEE_RANGE {
            self.execute_attack(ai_id, enemy_id, None);
            self.units[ai_idx].has_attacked = true;
        } else if dist <= range_world {
            // In range — hold position
        } else {
            let (gx, gy) = grid::world_to_grid(ex, ey);
            self.ai_move_toward(ai_idx, gx.max(0) as u32, gy.max(0) as u32);
        }
    }

    fn ai_monk_action(&mut self, ai_idx: usize) {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].x;
        let ay = self.units[ai_idx].y;
        let ai_id = self.units[ai_idx].id;
        let has_attacked = self.units[ai_idx].has_attacked;
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
            if !has_attacked {
                self.execute_heal(ai_idx, target_id);
                self.units[ai_idx].has_attacked = true;
                return;
            }
        }

        // Flee if an enemy is too close
        let enemy_dist = self.nearest_enemy_dist(ax, ay, faction);
        if enemy_dist < MONK_SAFE_DIST {
            if let Some(enemy) = self.find_nearest_enemy(ai_idx) {
                let (ex, ey, _, _) = enemy;
                let flee_x = ax + (ax - ex);
                let flee_y = ay + (ay - ey);
                let (gx, gy) = grid::world_to_grid(flee_x, flee_y);
                self.ai_move_toward(ai_idx, gx.max(0) as u32, gy.max(0) as u32);
                return;
            }
        }

        // Move directly toward wounded ally to get in heal range (no standoff)
        let vision_range = Self::AI_VISION_RADIUS as f32 * TILE_SIZE;
        let wounded_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
            .filter(|u| u.hp < u.stats.max_hp)
            .filter(|u| u.distance_to_pos(ax, ay) <= vision_range)
            .filter(|u| self.nearest_enemy_dist(u.x, u.y, faction) >= MONK_SAFE_DIST)
            .min_by_key(|u| u.hp)
            .map(|u| {
                let (gx, gy) = u.grid_cell();
                (gx, gy)
            });

        if let Some((tx, ty)) = wounded_target {
            self.ai_move_toward(ai_idx, tx, ty);
        }
    }

    /// Execute a heal action between healer at ai_idx and target unit.
    fn execute_heal(&mut self, healer_idx: usize, target_id: UnitId) {
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

        let amount = combat::execute_heal(healer, target);
        self.turn_events.push(TurnEvent::Heal {
            healer_id,
            target_id,
            amount,
        });
    }

    /// Move an AI unit one step toward target using A* (turn-based, for player_step shim).
    fn ai_move_toward(&mut self, ai_idx: usize, target_x: u32, target_y: u32) {
        let ai_id = self.units[ai_idx].id;
        let (ax, ay) = self.units[ai_idx].grid_cell();

        let occupied: Vec<(u32, u32)> = self
            .units
            .iter()
            .filter(|u| u.alive && u.id != ai_id)
            .map(|u| u.grid_cell())
            .collect();

        let path = self.grid.find_path(ax, ay, target_x, target_y, 30, |x, y| {
            occupied.iter().any(|&(ox, oy)| ox == x && oy == y)
        });

        if let Some(steps) = path {
            if let Some(&(nx, ny)) = steps.first() {
                let (wx, wy) = grid::grid_to_world(nx, ny);
                let unit = &mut self.units[ai_idx];
                let from = (unit.x, unit.y);
                let old_x = unit.x;
                unit.x = wx;
                unit.y = wy;
                if wx > old_x {
                    unit.facing = Facing::Right;
                } else if wx < old_x {
                    unit.facing = Facing::Left;
                }
                self.turn_events.push(TurnEvent::Move {
                    unit_id: ai_id,
                    from,
                    to: (wx, wy),
                });
            }
        }
    }

    /// Update animations, particles, projectiles, death fades, and camera following.
    pub fn update(&mut self, dt: f64) {
        for unit in &mut self.units {
            if unit.alive {
                unit.animation.update(dt);
            } else if unit.death_fade > 0.0 {
                unit.death_fade = (unit.death_fade - dt as f32).max(0.0);
                unit.animation.update(dt);
            }
        }

        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| !p.finished);

        for proj in &mut self.projectiles {
            proj.update(dt as f32);
        }

        // Apply damage on arrow impact
        for proj in &self.projectiles {
            if proj.finished && proj.damage > 0 {
                if let Some(idx) = self.find_unit_near(proj.target_x, proj.target_y, proj.faction)
                {
                    self.units[idx].take_damage(proj.damage);
                }
            }
        }
        self.projectiles.retain(|p| !p.finished);

        // Camera smoothly follows player's world position
        if let Some(player) = self.player_unit() {
            let (pvx, pvy) = (player.x, player.y);
            let lerp = (dt as f32 * 5.0).min(1.0);
            self.camera.x += (pvx - self.camera.x) * lerp;
            self.camera.y += (pvy - self.camera.y) * lerp;
        }
    }

    pub fn is_player_alive(&self) -> bool {
        self.player_unit().is_some()
    }

    pub fn faction_eliminated(&self, faction: Faction) -> bool {
        !self.units.iter().any(|u| u.alive && u.faction == faction)
    }

    /// Tick capture zone progress based on unit positions.
    pub fn tick_zones(&mut self, dt: f32) {
        self.zone_manager.count_units(&self.units);
        self.zone_manager.tick_capture(dt);
        if self.winner.is_none() {
            if let Some(faction) = self.zone_manager.tick_victory(dt) {
                self.winner = Some(faction);
            }
        }
    }

    /// Tick production buildings and dispatch groups from faction bases.
    pub fn tick_production(&mut self, dt: f32) {
        use crate::zone::MAX_UNITS_PER_FACTION;

        for base_idx in 0..self.bases.len() {
            let faction = self.bases[base_idx].faction;

            // Count alive units + staged units for faction cap check
            let alive_count = self.units.iter().filter(|u| u.alive && u.faction == faction).count();
            let staged = self.bases[base_idx].total_staged() as usize;
            let at_cap = alive_count + staged >= MAX_UNITS_PER_FACTION;

            // Tick production buildings (only if under cap)
            if !at_cap {
                let mut produced: Vec<UnitKind> = Vec::new();
                for building in &mut self.bases[base_idx].buildings {
                    if let Some(kind) = building.tick(dt) {
                        produced.push(kind);
                    }
                }
                for kind in produced {
                    self.bases[base_idx].receive_unit(kind);
                }
            }

            // Tick staging timer
            self.bases[base_idx].tick_staging(dt);

            // Check for dispatch
            if self.bases[base_idx].group_ready() || self.bases[base_idx].should_force_dispatch() {
                let rally_gx = self.bases[base_idx].rally_gx;
                let rally_gy = self.bases[base_idx].rally_gy;
                let (warriors, lancers, archers, monks) = self.bases[base_idx].dispatch_group();

                // Spawn units at rally point with slight offsets
                let mut offset = 0u32;
                for _ in 0..warriors {
                    let gy = (rally_gy as i32 + (offset as i32) - (warriors as i32 / 2))
                        .clamp(0, GRID_SIZE as i32 - 1) as u32;
                    self.spawn_unit(UnitKind::Warrior, faction, rally_gx, gy, false);
                    offset += 1;
                }
                offset = 0;
                for _ in 0..lancers {
                    let gx = if faction == Faction::Blue { rally_gx.saturating_sub(1) } else { rally_gx + 1 };
                    let gy = (rally_gy as i32 + (offset as i32) - (lancers as i32 / 2))
                        .clamp(0, GRID_SIZE as i32 - 1) as u32;
                    self.spawn_unit(UnitKind::Lancer, faction, gx, gy, false);
                    offset += 1;
                }
                offset = 0;
                for _ in 0..archers {
                    let gx = if faction == Faction::Blue { rally_gx.saturating_sub(2) } else { rally_gx + 2 };
                    let gy = (rally_gy as i32 + (offset as i32) - (archers as i32 / 2))
                        .clamp(0, GRID_SIZE as i32 - 1) as u32;
                    self.spawn_unit(UnitKind::Archer, faction, gx, gy, false);
                    offset += 1;
                }
                offset = 0;
                for _ in 0..monks {
                    let gx = if faction == Faction::Blue { rally_gx.saturating_sub(3) } else { rally_gx + 3 };
                    let gy = (rally_gy as i32 + (offset as i32) - (monks as i32 / 2))
                        .clamp(0, GRID_SIZE as i32 - 1) as u32;
                    self.spawn_unit(UnitKind::Monk, faction, gx, gy, false);
                    offset += 1;
                }
            }
        }
    }

    pub fn setup_demo_battle(&mut self) {
        let seed = (js_sys::Math::random() * u32::MAX as f64) as u32;
        self.setup_demo_battle_with_seed(seed);
    }

    pub fn setup_demo_battle_with_seed(&mut self, seed: u32) {
        self.grid = mapgen::generate_battlefield(seed);

        let (blue_x, blue_y) = mapgen::blue_spawn_area(); // (5, 5)
        let (red_x, red_y) = mapgen::red_spawn_area();     // (58, 58)

        // Each faction's objective is the other faction's base
        self.blue_objective = grid::grid_to_world(red_x, red_y);
        self.red_objective = grid::grid_to_world(blue_x, blue_y);

        // Initialize capture zones (diagonal)
        self.zone_manager = ZoneManager::create_default_zones();

        // Initialize faction bases with production buildings
        self.bases = vec![
            FactionBase::create_blue_base(),
            FactionBase::create_red_base(),
        ];

        // Blue rally point for initial army spawn
        let blue_rally_gx = self.bases[0].rally_gx; // 5
        let blue_rally_gy = self.bases[0].rally_gy; // 10

        // Blue army (player side) — 16 units at rally point
        // Front line: player warrior + 4 warriors
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_rally_gx + 1, blue_rally_gy, true);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_rally_gx + 1, blue_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_rally_gx + 1, blue_rally_gy.saturating_sub(1), false);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_rally_gx + 1, blue_rally_gy + 2, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_rally_gx + 1, blue_rally_gy.saturating_sub(2), false);
        // Lancers: second line
        for i in 0..3u32 {
            self.spawn_unit(UnitKind::Lancer, Faction::Blue, blue_rally_gx, blue_rally_gy + i, false);
            self.spawn_unit(UnitKind::Lancer, Faction::Blue, blue_rally_gx, blue_rally_gy.saturating_sub(1 + i), false);
        }
        // Archers: third line
        self.spawn_unit(UnitKind::Archer, Faction::Blue, blue_rally_gx.saturating_sub(2), blue_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, blue_rally_gx.saturating_sub(2), blue_rally_gy.saturating_sub(1), false);
        self.spawn_unit(UnitKind::Archer, Faction::Blue, blue_rally_gx.saturating_sub(2), blue_rally_gy, false);
        // Monks: rear
        self.spawn_unit(UnitKind::Monk, Faction::Blue, blue_rally_gx.saturating_sub(3), blue_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Monk, Faction::Blue, blue_rally_gx.saturating_sub(3), blue_rally_gy.saturating_sub(1), false);

        // Red rally point for initial army spawn
        let red_rally_gx = self.bases[1].rally_gx; // 58
        let red_rally_gy = self.bases[1].rally_gy; // 53

        // Red army (enemy side) — 15 units at rally point
        // Front line: 5 warriors
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_rally_gx.saturating_sub(1), red_rally_gy, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_rally_gx.saturating_sub(1), red_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_rally_gx.saturating_sub(1), red_rally_gy.saturating_sub(1), false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_rally_gx.saturating_sub(1), red_rally_gy + 2, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_rally_gx.saturating_sub(1), red_rally_gy.saturating_sub(2), false);
        // Lancers: second line
        for i in 0..3u32 {
            self.spawn_unit(UnitKind::Lancer, Faction::Red, red_rally_gx, red_rally_gy + i, false);
            self.spawn_unit(UnitKind::Lancer, Faction::Red, red_rally_gx, red_rally_gy.saturating_sub(1 + i), false);
        }
        // Archers: third line
        self.spawn_unit(UnitKind::Archer, Faction::Red, red_rally_gx + 2, red_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, red_rally_gx + 2, red_rally_gy.saturating_sub(1), false);
        self.spawn_unit(UnitKind::Archer, Faction::Red, red_rally_gx + 2, red_rally_gy, false);
        // Monks: rear
        self.spawn_unit(UnitKind::Monk, Faction::Red, red_rally_gx + 3, red_rally_gy + 1, false);
        self.spawn_unit(UnitKind::Monk, Faction::Red, red_rally_gx + 3, red_rally_gy.saturating_sub(1), false);

        // Camera starts centered on player (Blue rally point)
        let (cx, cy) = grid::grid_to_world(blue_rally_gx, blue_rally_gy);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = 1.0;

        // Pre-compute caches
        self.compute_water_adjacency();
        self.compute_fov();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::TileKind;

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
    fn step_moves_one_tile() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.player_step(SwipeDir::E));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_cell(), (6, 5));
    }

    #[test]
    fn step_blocked_by_water() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.grid.set(6, 5, TileKind::Water);
        assert!(!game.player_step(SwipeDir::E));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_cell(), (5, 5));
    }

    #[test]
    fn step_attacks_enemy() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        assert!(game.player_step(SwipeDir::E));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_cell(), (5, 5));
        let enemy = game.find_unit(enemy_id).unwrap();
        assert!(enemy.hp < 10);
    }

    #[test]
    fn step_blocked_by_friendly() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 6, 5, false);
        assert!(!game.player_step(SwipeDir::E));
    }

    #[test]
    fn turn_advances_after_step() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert_eq!(game.turn_number(), 1);
        game.player_step(SwipeDir::E);
        assert_eq!(game.turn_number(), 2);
    }

    #[test]
    fn ai_moves_toward_player() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 10, 5, false);
        game.player_step(SwipeDir::E);
        let enemy = game
            .units
            .iter()
            .find(|u| !u.is_player && u.alive)
            .unwrap();
        let (egx, _) = enemy.grid_cell();
        assert!(egx < 10, "AI should have moved toward player");
    }

    #[test]
    fn ai_attacks_adjacent_player() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.player_step(SwipeDir::S);
        let player = game.player_unit().unwrap();
        let (_, pgy) = player.grid_cell();
        assert!(
            player.hp < 10 || pgy == 6,
            "AI should have pursued"
        );
    }

    #[test]
    fn faction_elimination() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.faction_eliminated(Faction::Red));
        assert!(!game.faction_eliminated(Faction::Blue));
    }

    #[test]
    fn diagonal_step() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        assert!(game.player_step(SwipeDir::NE));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_cell(), (6, 4));
    }

    #[test]
    fn step_out_of_bounds_fails() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 0, 0, true);
        assert!(!game.player_step(SwipeDir::N));
        assert!(!game.player_step(SwipeDir::W));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_cell(), (0, 0));
    }

    #[test]
    fn player_step_records_move_event() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.player_step(SwipeDir::E);
        let has_player_move = game.turn_events.iter().any(|e| {
            matches!(e, TurnEvent::Move { unit_id: 1, .. })
        });
        assert!(
            has_player_move,
            "Expected Move event for player, got: {:?}",
            game.turn_events
        );
    }

    #[test]
    fn player_step_records_melee_attack_event() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.player_step(SwipeDir::E);
        let has_melee = game
            .turn_events
            .iter()
            .any(|e| matches!(e, TurnEvent::MeleeAttack { attacker_id: 1, .. }));
        assert!(
            has_melee,
            "Expected MeleeAttack event, got: {:?}",
            game.turn_events
        );
    }

    #[test]
    fn fov_player_tile_visible() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 32, 32, true);
        game.compute_fov();
        let idx = (32 * GRID_SIZE + 32) as usize;
        assert!(game.visible[idx]);
        assert!(game.revealed[idx]);
    }

    #[test]
    fn fov_nearby_tiles_visible() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 32, 32, true);
        game.compute_fov();
        for &(dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let x = (32 + dx) as u32;
            let y = (32 + dy) as u32;
            let idx = (y * GRID_SIZE + x) as usize;
            assert!(game.visible[idx], "Tile ({x},{y}) should be visible");
        }
    }

    #[test]
    fn fov_far_tiles_not_visible() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.compute_fov();
        let idx = (60 * GRID_SIZE + 60) as usize;
        assert!(!game.visible[idx]);
    }

    #[test]
    fn fov_revealed_persists_after_move() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 10, true);
        game.compute_fov();
        let idx_near = (10 * GRID_SIZE + 12) as usize;
        assert!(game.revealed[idx_near]);
        game.player_step(SwipeDir::W);
        assert!(game.revealed[idx_near]);
    }

    #[test]
    fn spawned_unit_has_correct_position() {
        use crate::grid;
        let mut game = Game::new(960.0, 640.0);
        let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 15, true);
        let unit = game.find_unit(id).unwrap();
        let (expected_x, expected_y) = grid::grid_to_world(10, 15);
        assert!((unit.x - expected_x).abs() < f32::EPSILON);
        assert!((unit.y - expected_y).abs() < f32::EPSILON);
    }

    #[test]
    fn turn_events_accumulate_player_and_ai() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 15, 5, false);
        game.player_step(SwipeDir::E);
        let move_count = game
            .turn_events
            .iter()
            .filter(|e| matches!(e, TurnEvent::Move { .. }))
            .count();
        assert!(
            move_count >= 2,
            "Expected at least 2 Move events (player + AI), got {move_count}"
        );
        let events: Vec<_> = game.turn_events.drain(..).collect();
        assert!(game.turn_events.is_empty());
        assert!(events.len() >= 2);
    }

    #[test]
    fn ai_archer_holds_position_in_range() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Archer, Faction::Red, 8, 5, false);
        game.player_step(SwipeDir::S);
        let archer = game.units.iter().find(|u| u.kind == UnitKind::Archer).unwrap();
        let (agx, _) = archer.grid_cell();
        assert_eq!(
            agx, 8,
            "Archer should hold position when already in range"
        );
    }

    #[test]
    fn ai_monk_heals_wounded_ally() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let warrior_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 30, 30, false);
        game.spawn_unit(UnitKind::Monk, Faction::Red, 31, 30, false);
        game.units.iter_mut().find(|u| u.id == warrior_id).unwrap().hp = 3;
        game.player_step(SwipeDir::E);
        let has_heal = game
            .turn_events
            .iter()
            .any(|e| matches!(e, TurnEvent::Heal { .. }));
        assert!(has_heal, "Monk should heal wounded adjacent ally");
        let warrior = game.find_unit(warrior_id).unwrap();
        assert!(warrior.hp > 3, "Warrior HP should have increased");
    }

    #[test]
    fn ai_monk_does_not_attack() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Monk, Faction::Red, 6, 5, false);
        game.player_step(SwipeDir::S);
        let has_attack = game.turn_events.iter().any(|e| {
            matches!(
                e,
                TurnEvent::MeleeAttack { .. } | TurnEvent::RangedAttack { .. }
            )
        });
        assert!(!has_attack, "Monk should never attack enemies");
    }

    #[test]
    fn ai_ignores_distant_enemies() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 50, 50, false);
        game.player_step(SwipeDir::E);
        let enemy = game
            .units
            .iter()
            .find(|u| u.faction == Faction::Red && u.alive)
            .unwrap();
        assert_eq!(enemy.grid_cell(), (50, 50), "Distant AI should not move");
    }

    #[test]
    fn ai_paths_around_obstacle() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 10, 5, false);
        for y in 0..GRID_SIZE {
            game.grid.set(8, y, TileKind::Water);
        }
        game.grid.set(8, 3, TileKind::Grass);
        game.player_step(SwipeDir::E);
        let enemy = game
            .units
            .iter()
            .find(|u| u.faction == Faction::Red && u.alive)
            .unwrap();
        assert!(
            enemy.grid_cell() != (10, 5),
            "AI should path around water obstacle"
        );
    }

    // ---- Real-time tests ----

    #[test]
    fn player_attack_hits_in_range() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        // Adjacent = 64px, within MELEE_RANGE = 96px
        game.player_attack();
        let enemy = game.find_unit(enemy_id).unwrap();
        assert!(enemy.hp < 10, "Enemy should have taken damage from auto-attack");
    }

    #[test]
    fn player_attack_ranged() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Archer, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 9, 5, false);
        // 4 tiles = 256px, Archer range = 7 * 64 = 448px
        game.player_attack();
        // Arrow spawned — damage is deferred until projectile lands
        assert!(!game.projectiles.is_empty(), "Arrow projectile should be spawned");
        // Advance time until arrow lands (distance ~256px / 600px/s ≈ 0.43s)
        for _ in 0..40 {
            game.update(0.016);
        }
        let enemy = game.find_unit(enemy_id).unwrap();
        assert!(enemy.hp < 10, "Enemy should have taken ranged damage on arrow impact");
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
        assert!(enemy.x < start_x, "AI melee should have moved toward player");
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
        let archer = game.units.iter().find(|u| u.kind == UnitKind::Archer).unwrap();
        assert!(
            (archer.x - start_x).abs() < 1.0,
            "Archer should hold position when in range"
        );
    }

    #[test]
    fn cooldowns_tick_down() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.units[0].attack_cooldown = 1.0;
        game.tick_cooldowns(0.3);
        assert!((game.units[0].attack_cooldown - 0.7).abs() < 0.001);
        game.tick_cooldowns(1.0);
        assert!(game.units[0].attack_cooldown.abs() < f32::EPSILON);
    }

    #[test]
    fn resolve_collisions_pushes_apart() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 5, 5, false);
        // Both at same position
        let (wx, wy) = grid::grid_to_world(5, 5);
        game.units[0].x = wx;
        game.units[0].y = wy;
        game.units[1].x = wx + 1.0; // slightly offset to avoid zero-distance
        game.units[1].y = wy;
        game.resolve_collisions();
        let dist = game.units[0].distance_to_unit(&game.units[1]);
        assert!(
            dist >= UNIT_RADIUS * 2.0 - 0.1,
            "Units should be pushed apart, dist={dist}"
        );
    }

    #[test]
    fn enemy_in_range_finds_nearest() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 8, 5, false);
        let px = game.units[0].x;
        let py = game.units[0].y;
        let nearest = game.enemy_in_range(px, py, Faction::Blue, MELEE_RANGE);
        assert_eq!(nearest, Some(2), "Should find the closer enemy");
    }

    // ---- Cone hitbox tests ----

    #[test]
    fn enemy_in_cone_finds_enemy_ahead() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        let px = game.units[0].x;
        let py = game.units[0].y;
        // Aim right (0 rad), enemy is to the right
        let result = game.enemy_in_cone(px, py, Faction::Blue, MELEE_RANGE, 0.0, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result, Some(2), "Should hit enemy directly ahead");
    }

    #[test]
    fn enemy_in_cone_misses_enemy_behind() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 4, 5, false); // enemy to the left
        let px = game.units[0].x;
        let py = game.units[0].y;
        // Aim right (0 rad), enemy is to the left (behind)
        let result = game.enemy_in_cone(px, py, Faction::Blue, MELEE_RANGE, 0.0, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result, None, "Should miss enemy behind");
    }

    #[test]
    fn enemy_in_cone_diagonal_aim() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 6, false); // SE
        let px = game.units[0].x;
        let py = game.units[0].y;
        // Aim SE (PI/4 rad)
        let aim_se = std::f32::consts::FRAC_PI_4;
        let result = game.enemy_in_cone(px, py, Faction::Blue, MELEE_RANGE * 2.0, aim_se, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result, Some(2), "Should hit enemy in SE when aiming SE");
    }

    #[test]
    fn enemy_in_cone_wraps_around_pi() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 4, 5, false); // enemy to the left
        let px = game.units[0].x;
        let py = game.units[0].y;
        // Aim left (PI rad) — tests wrap-around at -PI/PI boundary
        let aim_left = std::f32::consts::PI;
        let result = game.enemy_in_cone(px, py, Faction::Blue, MELEE_RANGE, aim_left, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result, Some(2), "Should hit enemy to the left when aiming left");
    }

    // ---- Line-of-sight tests ----

    #[test]
    fn has_los_open_field() {
        let game = Game::new(960.0, 640.0);
        let (x1, y1) = grid::grid_to_world(5, 5);
        let (x2, y2) = grid::grid_to_world(10, 5);
        assert!(game.has_line_of_sight(x1, y1, x2, y2), "Open grass should not block LOS");
    }

    #[test]
    fn has_los_blocked_by_forest() {
        let mut game = Game::new(960.0, 640.0);
        game.grid.set(7, 5, TileKind::Forest);
        let (x1, y1) = grid::grid_to_world(5, 5);
        let (x2, y2) = grid::grid_to_world(10, 5);
        assert!(!game.has_line_of_sight(x1, y1, x2, y2), "Forest should block LOS");
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
        assert!(result.is_none(), "Enemy behind forest should not be visible");
    }

    #[test]
    fn ai_melee_marches_to_objective() {
        let mut game = Game::new(960.0, 640.0);
        // Set up objective to the right
        game.blue_objective = grid::grid_to_world(50, 5);
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

    // ---- Cleave & knockback tests ----

    #[test]
    fn enemies_in_cone_finds_all() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 4, false);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 6, false);
        let px = game.units[0].x;
        let py = game.units[0].y;
        let result = game.enemies_in_cone(px, py, Faction::Blue, MELEE_RANGE * 2.0, 0.0, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result.len(), 3, "Should find all 3 enemies in cone, got {:?}", result);
    }

    #[test]
    fn enemies_in_cone_filters_behind() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false); // ahead (right)
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 4, 5, false); // behind (left)
        let px = game.units[0].x;
        let py = game.units[0].y;
        let result = game.enemies_in_cone(px, py, Faction::Blue, MELEE_RANGE * 2.0, 0.0, ATTACK_CONE_HALF_ANGLE);
        assert_eq!(result.len(), 1, "Should only find the enemy ahead");
        assert_eq!(result[0], 2, "Should be the enemy to the right");
    }

    #[test]
    fn player_cleave_hits_multiple() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let e1 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        let e2 = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 4, false);
        game.player_aim_dir = 0.0; // aim right
        game.player_attack();
        let enemy1 = game.find_unit(e1).unwrap();
        let enemy2 = game.find_unit(e2).unwrap();
        assert!(enemy1.hp < enemy1.stats.max_hp, "First enemy should be damaged");
        assert!(enemy2.hp < enemy2.stats.max_hp, "Second enemy should be damaged");
    }

    #[test]
    fn knockback_pushes_enemy_away() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        let before_x = game.find_unit(enemy_id).unwrap().x;
        game.player_aim_dir = 0.0;
        game.player_attack();
        let after_x = game.find_unit(enemy_id).unwrap().x;
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
        let before_x = game.find_unit(enemy_id).unwrap().x;
        game.player_aim_dir = 0.0;
        game.player_attack();
        let after_x = game.find_unit(enemy_id).unwrap().x;
        assert!(
            (after_x - before_x).abs() < 0.01,
            "Enemy should NOT be pushed into water, before={before_x} after={after_x}"
        );
    }

    #[test]
    fn tick_zones_updates_capture_progress() {
        let mut game = Game::new(960.0, 640.0);
        game.zone_manager = ZoneManager::create_default_zones();
        // Place a Blue unit inside zone 0 (center_gx=16, center_gy=16)
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 16, 16, false);
        game.tick_zones(4.0);
        assert!(
            game.zone_manager.zones[0].progress > 0.0,
            "Zone progress should advance with a Blue unit inside"
        );
    }

    #[test]
    fn ai_targets_zone_not_spawn() {
        let mut game = Game::new(960.0, 640.0);
        game.zone_manager = ZoneManager::create_default_zones();
        game.blue_objective = grid::grid_to_world(58, 58);
        // All zones neutral — Blue should target nearest zone (16,16), not enemy base
        let obj = game.faction_objective(Faction::Blue);
        let (base_wx, _) = grid::grid_to_world(58, 58);
        assert!(
            obj.0 < base_wx,
            "Blue should target a zone (x < {base_wx}), got x={}", obj.0
        );
    }

    #[test]
    fn production_spawns_units() {
        let mut game = Game::new(960.0, 640.0);
        game.bases = vec![FactionBase::create_blue_base()];
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 10, true);
        let count_before = game.units.len();
        // Tick production for enough time to produce a full group (~40s) + dispatch
        for _ in 0..420 {
            game.tick_production(0.1);
        }
        let count_after = game.units.len();
        assert!(
            count_after > count_before,
            "Production should have spawned units ({count_before} -> {count_after})"
        );
    }
}
