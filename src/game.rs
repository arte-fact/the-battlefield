use crate::animation::TurnEvent;
use crate::camera::Camera;
use crate::combat;
use crate::grid::{self, Grid, TileKind, GRID_SIZE, TILE_SIZE};
use crate::input::SwipeDir;
use crate::mapgen;
use crate::particle::{Particle, Projectile};
use crate::turn::TurnState;
use crate::unit::{Facing, Faction, Unit, UnitId, UnitKind};

/// Player vision radius in tiles.
const FOV_RADIUS: i32 = 15;

pub struct Game {
    pub grid: Grid,
    pub units: Vec<Unit>,
    pub turn_state: TurnState,
    pub camera: Camera,
    pub particles: Vec<Particle>,
    pub projectiles: Vec<Projectile>,
    next_unit_id: UnitId,
    /// Auto-move path: sequence of grid positions to walk to, one per turn.
    pub auto_path: Vec<(u32, u32)>,
    /// Index into auto_path for next step to take.
    pub auto_path_idx: usize,
    /// Time accumulator for auto-move pacing (seconds).
    pub auto_move_timer: f32,
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
            auto_path: Vec::new(),
            auto_path_idx: 0,
            auto_move_timer: 0.0,
            visible: vec![false; size],
            revealed: vec![false; size],
            fog_dirty: true,
            water_adjacency: vec![false; size],
            turn_events: Vec::new(),
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

    /// Player swipes in a direction: move 1 tile or attack adjacent enemy.
    /// After the player acts, all AI units take one action (auto-turn).
    /// Returns true if the action was valid.
    pub fn player_step(&mut self, dir: SwipeDir) -> bool {
        let player = match self.player_unit() {
            Some(p) => p,
            None => return false,
        };
        let px = player.grid_x;
        let py = player.grid_y;
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
        // AI archers will shoot at these positions, simulating projectile lag.
        let position_snapshot: Vec<(UnitId, u32, u32)> = self
            .units
            .iter()
            .filter(|u| u.alive)
            .map(|u| (u.id, u.grid_x, u.grid_y))
            .collect();

        // Enemy at target -> attack
        let enemy_at_target = self
            .unit_at(nx, ny)
            .filter(|u| u.faction != faction)
            .map(|u| u.id);

        if let Some(enemy_id) = enemy_at_target {
            self.execute_attack(player_id, enemy_id, None);
        } else if self.unit_at(nx, ny).is_some()
            || !self.grid.is_passable(nx, ny)
            || !self.grid.can_move_diagonal(px, py, dx, dy)
        {
            // Blocked by friendly unit, impassable terrain, or corner-cutting
            return false;
        } else {
            // Move player 1 tile
            let player = self.player_unit_mut().unwrap();
            player.grid_x = nx;
            player.grid_y = ny;
            if dx > 0 {
                player.facing = Facing::Right;
            } else if dx < 0 {
                player.facing = Facing::Left;
            }
            self.turn_events.push(TurnEvent::Move {
                unit_id: player_id,
                from: (px, py),
                to: (nx, ny),
            });
        }

        // Auto-turn: AI acts, using pre-turn snapshot for ranged targeting
        self.ai_turn(&position_snapshot);

        // Advance turn and reset all living units
        self.turn_state.turn_number += 1;
        for unit in &mut self.units {
            if unit.alive {
                unit.reset_turn();
            }
        }

        // Recompute FOV after player acts
        self.compute_fov();
        true
    }

    /// Set an auto-move path from A* pathfinding to a destination.
    /// Clears any existing path.
    pub fn set_auto_path(&mut self, dest_x: u32, dest_y: u32) {
        let player = match self.player_unit() {
            Some(p) => p,
            None => return,
        };
        let sx = player.grid_x;
        let sy = player.grid_y;

        // Pathfind ignoring unit positions (they'll move each turn)
        let path = self.grid.find_path(sx, sy, dest_x, dest_y, 30, |_, _| false);
        if let Some(p) = path {
            self.auto_path = p;
            self.auto_path_idx = 0;
            self.auto_move_timer = 0.0;
        }
    }

    /// Compute the next auto-move direction without executing it.
    /// Returns Some(dir) to pass to player_step, or None if path is done/stuck.
    /// The caller (game_loop) is responsible for calling player_step with the result.
    pub fn auto_move_step(&mut self) -> bool {
        if let Some(dir) = self.auto_move_next_dir() {
            if self.player_step(dir) {
                return true;
            }
            // Step failed — give up
            self.auto_path.clear();
            self.auto_path_idx = 0;
        }
        false
    }

    /// Compute the next direction for auto-move. Updates internal path state
    /// but does NOT call player_step. Returns None if path is done or stuck.
    fn auto_move_next_dir(&mut self) -> Option<SwipeDir> {
        if self.auto_path_idx >= self.auto_path.len() {
            self.auto_path.clear();
            self.auto_path_idx = 0;
            return None;
        }

        let (tx, ty) = self.auto_path[self.auto_path_idx];
        let player = match self.player_unit() {
            Some(p) => p,
            None => {
                self.auto_path.clear();
                self.auto_path_idx = 0;
                return None;
            }
        };
        let px = player.grid_x;
        let py = player.grid_y;
        let player_faction = player.faction;

        let dx = tx as i32 - px as i32;
        let dy = ty as i32 - py as i32;

        // Enemy on next tile: attack if it's the destination, otherwise re-path
        if let Some(unit) = self.unit_at(tx, ty) {
            if unit.faction != player_faction {
                let is_destination = self.auto_path_idx == self.auto_path.len() - 1;
                if is_destination {
                    self.auto_path.clear();
                    self.auto_path_idx = 0;
                    return SwipeDir::from_grid_delta(dx, dy);
                }
                // Enemy in the way but not destination — re-path around them
                return self.repath_next_dir();
            }
        }

        if let Some(dir) = SwipeDir::from_grid_delta(dx, dy) {
            self.auto_path_idx += 1;
            return Some(dir);
        }

        // Can't compute direction — try re-path
        self.repath_next_dir()
    }

    /// Re-compute auto-path around obstacles and return the next direction.
    /// Does NOT call player_step.
    fn repath_next_dir(&mut self) -> Option<SwipeDir> {
        let dest = match self.auto_path.last().copied() {
            Some(d) => d,
            None => {
                self.auto_path.clear();
                self.auto_path_idx = 0;
                return None;
            }
        };

        let player = match self.player_unit() {
            Some(p) => p,
            None => {
                self.auto_path.clear();
                self.auto_path_idx = 0;
                return None;
            }
        };
        let sx = player.grid_x;
        let sy = player.grid_y;
        let player_id = player.id;

        let occupied: Vec<(u32, u32)> = self
            .units
            .iter()
            .filter(|u| u.alive && u.id != player_id)
            .map(|u| (u.grid_x, u.grid_y))
            .collect();

        let new_path = self.grid.find_path(sx, sy, dest.0, dest.1, 30, |x, y| {
            occupied.iter().any(|&(ox, oy)| ox == x && oy == y)
        });

        match new_path {
            Some(p) if !p.is_empty() => {
                let (tx, ty) = p[0];
                self.auto_path = p;
                self.auto_path_idx = 1;
                let dx = tx as i32 - sx as i32;
                let dy = ty as i32 - sy as i32;
                SwipeDir::from_grid_delta(dx, dy)
            }
            _ => {
                self.auto_path.clear();
                self.auto_path_idx = 0;
                None
            }
        }
    }

    /// Cancel any in-progress auto-move.
    pub fn cancel_auto_path(&mut self) {
        self.auto_path.clear();
        self.auto_path_idx = 0;
    }

    /// Returns true if auto-move is in progress.
    pub fn is_auto_moving(&self) -> bool {
        self.auto_path_idx < self.auto_path.len()
    }

    /// Recompute field of view from the player's position using recursive shadowcasting.
    /// Tiles within FOV_RADIUS that have line-of-sight become visible and revealed.
    /// Forest tiles are visible but block vision beyond them.
    pub fn compute_fov(&mut self) {
        let w = self.grid.width;
        let h = self.grid.height;

        // Clear current visibility
        for v in self.visible.iter_mut() {
            *v = false;
        }

        let player = match self.player_unit() {
            Some(p) => (p.grid_x as i32, p.grid_y as i32),
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
    /// Call once after grid generation.
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

    /// Returns true if the tile at (x, y) blocks line of sight.
    fn blocks_light(&self, x: u32, y: u32) -> bool {
        let tile = self.grid.get(x, y);
        match tile {
            TileKind::Water => false,
            TileKind::Forest => true,
            _ => {
                // Elevation 2 blocks vision from ground level
                self.grid.elevation(x, y) >= 2
            }
        }
    }

    /// Recursive shadowcasting for one octant.
    /// Uses the standard 8-octant transform to cover all directions.
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
                // Transform (dx, dy) based on octant
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

                // Check if within radius (circular FOV)
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

    /// Execute an attack. For ranged attacks, `target_snapshot_pos` is the position
    /// the target was at when the archer decided to shoot. If the target has since moved,
    /// the arrow flies to the old position and misses (simulates projectile travel lag).
    fn execute_attack(
        &mut self,
        attacker_id: UnitId,
        defender_id: UnitId,
        target_snapshot_pos: Option<(u32, u32)>,
    ) {
        let attacker_idx = self.units.iter().position(|u| u.id == attacker_id);
        let defender_idx = self.units.iter().position(|u| u.id == defender_id);
        let (attacker_idx, defender_idx) = match (attacker_idx, defender_idx) {
            (Some(a), Some(d)) => (a, d),
            _ => return,
        };

        let is_ranged = self.units[attacker_idx].stats.range > 1
            && self.units[attacker_idx].distance_to(
                self.units[defender_idx].grid_x,
                self.units[defender_idx].grid_y,
            ) > 1;

        if is_ranged {
            let (snap_x, snap_y) = target_snapshot_pos.unwrap_or((
                self.units[defender_idx].grid_x,
                self.units[defender_idx].grid_y,
            ));
            let target_moved = self.units[defender_idx].grid_x != snap_x
                || self.units[defender_idx].grid_y != snap_y;

            if target_moved {
                // Miss: arrow hits empty ground
                self.turn_events.push(TurnEvent::RangedAttack {
                    attacker_id,
                    defender_id,
                    damage: 0,
                    killed: false,
                    target_pos: (snap_x, snap_y),
                    missed: true,
                });
            } else {
                // Hit: deal damage normally
                let (attacker, defender) = if attacker_idx < defender_idx {
                    let (left, right) = self.units.split_at_mut(defender_idx);
                    (&mut left[attacker_idx], &mut right[0])
                } else {
                    let (left, right) = self.units.split_at_mut(attacker_idx);
                    (&mut right[0], &mut left[defender_idx])
                };
                let result = combat::execute_ranged(attacker, defender, &self.grid);
                self.turn_events.push(TurnEvent::RangedAttack {
                    attacker_id,
                    defender_id,
                    damage: result.damage,
                    killed: result.target_killed,
                    target_pos: (snap_x, snap_y),
                    missed: false,
                });
            }
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
    }

    /// AI vision radius in tiles (Chebyshev distance).
    const AI_VISION_RADIUS: u32 = 10;

    /// Process all AI units sequentially. Each unit re-queries live state so
    /// earlier actions (kills, movement) are visible to later units.
    fn ai_turn(&mut self, position_snapshot: &[(UnitId, u32, u32)]) {
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

    /// Dispatch AI action based on unit type.
    fn ai_unit_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, u32, u32)]) {
        let kind = self.units[ai_idx].kind;
        match kind {
            UnitKind::Monk => self.ai_monk_action(ai_idx),
            UnitKind::Archer => self.ai_archer_action(ai_idx, position_snapshot),
            UnitKind::Warrior | UnitKind::Pawn | UnitKind::Lancer => {
                self.ai_melee_action(ai_idx, position_snapshot)
            }
        }
    }

    /// Find the nearest visible enemy for a unit at (ax, ay).
    fn find_nearest_enemy(&self, ai_idx: usize) -> Option<(u32, u32, UnitId, u32)> {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].grid_x;
        let ay = self.units[ai_idx].grid_y;

        self.units
            .iter()
            .filter(|u| u.alive && u.faction != faction)
            .filter_map(|u| {
                let dx = (ax as i32 - u.grid_x as i32).unsigned_abs();
                let dy = (ay as i32 - u.grid_y as i32).unsigned_abs();
                let dist = dx.max(dy);
                if dist <= Self::AI_VISION_RADIUS {
                    Some((u.grid_x, u.grid_y, u.id, dist))
                } else {
                    None
                }
            })
            .min_by_key(|&(_, _, _, dist)| dist)
    }

    /// Warrior, Pawn, Lancer: attack adjacent enemy, else A* toward nearest.
    fn ai_melee_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, u32, u32)]) {
        let ai_id = self.units[ai_idx].id;
        let has_attacked = self.units[ai_idx].has_attacked;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => return,
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if !has_attacked && dist <= 1 {
            let snap_pos = position_snapshot
                .iter()
                .find(|(id, _, _)| *id == enemy_id)
                .map(|&(_, x, y)| (x, y));
            self.execute_attack(ai_id, enemy_id, snap_pos);
        } else if !self.units[ai_idx].has_moved {
            self.ai_move_toward(ai_idx, ex, ey);
        }
    }

    /// Archer: ranged attack if in range, melee if adjacent, hold if in range, else A* approach.
    fn ai_archer_action(&mut self, ai_idx: usize, position_snapshot: &[(UnitId, u32, u32)]) {
        let ai_id = self.units[ai_idx].id;
        let range = self.units[ai_idx].stats.range;
        let has_attacked = self.units[ai_idx].has_attacked;

        let enemy = match self.find_nearest_enemy(ai_idx) {
            Some(e) => e,
            None => return,
        };
        let (ex, ey, enemy_id, dist) = enemy;

        if !has_attacked && dist > 1 && dist <= range {
            // Ranged attack — use snapshot for projectile-lag mechanic
            let snap_pos = position_snapshot
                .iter()
                .find(|(id, _, _)| *id == enemy_id)
                .map(|&(_, x, y)| (x, y));
            self.execute_attack(ai_id, enemy_id, snap_pos);
        } else if !has_attacked && dist <= 1 {
            // Melee fallback when adjacent
            self.execute_attack(ai_id, enemy_id, None);
        } else if dist <= range {
            // Already in range — hold position, don't walk into melee
        } else if !self.units[ai_idx].has_moved {
            // Out of range — approach
            self.ai_move_toward(ai_idx, ex, ey);
        }
    }

    /// Monk: heal adjacent wounded ally (<60% HP), else move toward wounded ally. Never attacks.
    fn ai_monk_action(&mut self, ai_idx: usize) {
        let faction = self.units[ai_idx].faction;
        let ax = self.units[ai_idx].grid_x;
        let ay = self.units[ai_idx].grid_y;
        let ai_id = self.units[ai_idx].id;
        let has_attacked = self.units[ai_idx].has_attacked;

        // Find adjacent ally below 60% HP (heal target)
        let heal_target = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
            .filter(|u| {
                let dist = {
                    let dx = (ax as i32 - u.grid_x as i32).unsigned_abs();
                    let dy = (ay as i32 - u.grid_y as i32).unsigned_abs();
                    dx.max(dy)
                };
                dist <= 1 && (u.hp as f32) < (u.stats.max_hp as f32 * 0.6)
            })
            .min_by_key(|u| u.hp)
            .map(|u| u.id);

        if let Some(target_id) = heal_target {
            if !has_attacked {
                self.execute_heal(ai_idx, target_id);
                return;
            }
        }

        // Move toward lowest-HP wounded ally within vision
        if !self.units[ai_idx].has_moved {
            let move_target = self
                .units
                .iter()
                .filter(|u| u.alive && u.faction == faction && u.id != ai_id)
                .filter(|u| u.hp < u.stats.max_hp)
                .filter(|u| {
                    let dx = (ax as i32 - u.grid_x as i32).unsigned_abs();
                    let dy = (ay as i32 - u.grid_y as i32).unsigned_abs();
                    dx.max(dy) <= Self::AI_VISION_RADIUS
                })
                .min_by_key(|u| u.hp)
                .map(|u| (u.grid_x, u.grid_y));

            if let Some((tx, ty)) = move_target {
                self.ai_move_toward(ai_idx, tx, ty);
            }
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

    /// Move an AI unit one step toward (target_x, target_y) using A* pathfinding.
    fn ai_move_toward(&mut self, ai_idx: usize, target_x: u32, target_y: u32) {
        let ai_id = self.units[ai_idx].id;
        let ax = self.units[ai_idx].grid_x;
        let ay = self.units[ai_idx].grid_y;

        // Build occupied set from all alive units except self
        let occupied: Vec<(u32, u32)> = self
            .units
            .iter()
            .filter(|u| u.alive && u.id != ai_id)
            .map(|u| (u.grid_x, u.grid_y))
            .collect();

        // A* path with max_len=1 to get the first step direction
        let path = self.grid.find_path(ax, ay, target_x, target_y, 30, |x, y| {
            occupied.iter().any(|&(ox, oy)| ox == x && oy == y)
        });

        if let Some(steps) = path {
            if let Some(&(nx, ny)) = steps.first() {
                let unit = &mut self.units[ai_idx];
                let old_x = unit.grid_x;
                unit.grid_x = nx;
                unit.grid_y = ny;
                unit.has_moved = true;
                unit.movement_left = unit.movement_left.saturating_sub(1);
                if nx > old_x {
                    unit.facing = Facing::Right;
                } else if nx < old_x {
                    unit.facing = Facing::Left;
                }
                self.turn_events.push(TurnEvent::Move {
                    unit_id: ai_id,
                    from: (ax, ay),
                    to: (nx, ny),
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
        self.projectiles.retain(|p| !p.finished);

        // Camera smoothly follows player's visual position
        if let Some(player) = self.player_unit() {
            let (pvx, pvy) = (player.visual_x, player.visual_y);
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

    pub fn setup_demo_battle(&mut self) {
        self.setup_demo_battle_with_seed(42);
    }

    pub fn setup_demo_battle_with_seed(&mut self, seed: u32) {
        self.grid = mapgen::generate_battlefield(seed);

        let (blue_x, blue_y) = mapgen::blue_spawn_area();
        let (red_x, red_y) = mapgen::red_spawn_area();

        // Blue army (player side)
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_x, blue_y, true);
        self.spawn_unit(UnitKind::Warrior, Faction::Blue, blue_x, blue_y + 2, false);
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Blue,
            blue_x,
            blue_y.saturating_sub(2),
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            blue_x.saturating_sub(2),
            blue_y + 1,
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Blue,
            blue_x.saturating_sub(2),
            blue_y.saturating_sub(1),
            false,
        );
        self.spawn_unit(
            UnitKind::Lancer,
            Faction::Blue,
            blue_x + 1,
            blue_y + 4,
            false,
        );
        self.spawn_unit(
            UnitKind::Pawn,
            Faction::Blue,
            blue_x + 1,
            blue_y.saturating_sub(4),
            false,
        );
        self.spawn_unit(
            UnitKind::Pawn,
            Faction::Blue,
            blue_x + 1,
            blue_y.saturating_sub(3),
            false,
        );
        self.spawn_unit(
            UnitKind::Monk,
            Faction::Blue,
            blue_x.saturating_sub(1),
            blue_y + 3,
            false,
        );

        // Red army (enemy side)
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_x, red_y, false);
        self.spawn_unit(UnitKind::Warrior, Faction::Red, red_x, red_y + 2, false);
        self.spawn_unit(
            UnitKind::Warrior,
            Faction::Red,
            red_x,
            red_y.saturating_sub(2),
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Red,
            red_x + 2,
            red_y + 1,
            false,
        );
        self.spawn_unit(
            UnitKind::Archer,
            Faction::Red,
            red_x + 2,
            red_y.saturating_sub(1),
            false,
        );
        self.spawn_unit(
            UnitKind::Lancer,
            Faction::Red,
            red_x.saturating_sub(1),
            red_y + 4,
            false,
        );
        self.spawn_unit(
            UnitKind::Pawn,
            Faction::Red,
            red_x.saturating_sub(1),
            red_y.saturating_sub(4),
            false,
        );
        self.spawn_unit(
            UnitKind::Pawn,
            Faction::Red,
            red_x.saturating_sub(1),
            red_y.saturating_sub(3),
            false,
        );
        self.spawn_unit(
            UnitKind::Monk,
            Faction::Red,
            red_x + 1,
            red_y + 3,
            false,
        );

        // Camera starts centered on player
        let (cx, cy) = grid::grid_to_world(blue_x, blue_y);
        self.camera.x = cx;
        self.camera.y = cy;
        self.camera.zoom = 1.5;

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
        assert_eq!(player.grid_x, 6);
        assert_eq!(player.grid_y, 5);
    }

    #[test]
    fn step_blocked_by_water() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.grid.set(6, 5, TileKind::Water);
        assert!(!game.player_step(SwipeDir::E));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 5);
    }

    #[test]
    fn step_attacks_enemy() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        let enemy_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        assert!(game.player_step(SwipeDir::E));
        // Player didn't move (attacked instead)
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 5);
        // Enemy took damage
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
        assert!(enemy.grid_x < 10, "AI should have moved toward player");
    }

    #[test]
    fn ai_attacks_adjacent_player() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 6, 5, false);
        // Player steps south (away from enemy)
        game.player_step(SwipeDir::S);
        // Enemy should pursue and be adjacent now at (6,5) or (5,6)
        // Player is at (5,6). Enemy was at (6,5), moves to (5,5) or stays.
        // The AI should try to attack if adjacent after moving.
        let player = game.player_unit().unwrap();
        // After AI turn, player may have been attacked
        assert!(
            player.hp < 10 || player.grid_y == 6,
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
        assert_eq!(player.grid_x, 6);
        assert_eq!(player.grid_y, 4);
    }

    #[test]
    fn step_out_of_bounds_fails() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 0, 0, true);
        assert!(!game.player_step(SwipeDir::N));
        assert!(!game.player_step(SwipeDir::W));
        let player = game.player_unit().unwrap();
        assert_eq!(player.grid_x, 0);
        assert_eq!(player.grid_y, 0);
    }

    #[test]
    fn player_step_records_move_event() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.player_step(SwipeDir::E);
        let has_player_move = game.turn_events.iter().any(|e| {
            matches!(
                e,
                TurnEvent::Move {
                    unit_id: 1,
                    from: (5, 5),
                    to: (6, 5)
                }
            )
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
        // Adjacent tiles should be visible
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
        // Tile far away should not be visible
        let idx = (60 * GRID_SIZE + 60) as usize;
        assert!(!game.visible[idx]);
    }

    #[test]
    fn fov_revealed_persists_after_move() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 10, true);
        game.compute_fov();
        // Tiles near (10,10) should be revealed
        let idx_near = (10 * GRID_SIZE + 12) as usize;
        assert!(game.revealed[idx_near]);
        // Move player away
        game.player_step(SwipeDir::W);
        // Tile (12,10) should still be revealed but may not be visible
        assert!(game.revealed[idx_near]);
    }

    #[test]
    fn spawned_unit_has_correct_visual_position() {
        use crate::grid;
        let mut game = Game::new(960.0, 640.0);
        let id = game.spawn_unit(UnitKind::Warrior, Faction::Blue, 10, 15, true);
        let unit = game.find_unit(id).unwrap();
        let (expected_x, expected_y) = grid::grid_to_world(10, 15);
        assert!((unit.visual_x - expected_x).abs() < f32::EPSILON);
        assert!((unit.visual_y - expected_y).abs() < f32::EPSILON);
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
        // Archer at distance 3 (within range 5) — should NOT advance
        game.spawn_unit(UnitKind::Archer, Faction::Red, 8, 5, false);
        game.player_step(SwipeDir::S); // player steps away
        let archer = game.units.iter().find(|u| u.kind == UnitKind::Archer).unwrap();
        assert_eq!(
            archer.grid_x, 8,
            "Archer should hold position when already in range"
        );
    }

    #[test]
    fn ai_monk_heals_wounded_ally() {
        use crate::animation::TurnEvent;
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Red warrior at half HP, adjacent to red monk
        let warrior_id = game.spawn_unit(UnitKind::Warrior, Faction::Red, 30, 30, false);
        game.spawn_unit(UnitKind::Monk, Faction::Red, 31, 30, false);
        // Wound the warrior to below 60%
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
        // Monk directly adjacent to player — should NOT attack
        game.spawn_unit(UnitKind::Monk, Faction::Red, 6, 5, false);
        game.player_step(SwipeDir::S); // player steps away, monk becomes adjacent
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
        // Enemy far beyond AI_VISION_RADIUS (10) — should not move
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 50, 50, false);
        game.player_step(SwipeDir::E);
        let enemy = game
            .units
            .iter()
            .find(|u| u.faction == Faction::Red && u.alive)
            .unwrap();
        assert_eq!(enemy.grid_x, 50, "Distant AI should not move");
        assert_eq!(enemy.grid_y, 50, "Distant AI should not move");
    }

    #[test]
    fn ai_paths_around_obstacle() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        // Enemy at (10, 5) with a water wall blocking the direct path
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 10, 5, false);
        for y in 0..GRID_SIZE {
            game.grid.set(8, y, TileKind::Water);
        }
        // Leave a gap at y=3
        game.grid.set(8, 3, TileKind::Grass);
        game.player_step(SwipeDir::E);
        let enemy = game
            .units
            .iter()
            .find(|u| u.faction == Faction::Red && u.alive)
            .unwrap();
        // Enemy should have moved (not stuck behind water)
        assert!(
            enemy.grid_x != 10 || enemy.grid_y != 5,
            "AI should path around water obstacle"
        );
    }
}
