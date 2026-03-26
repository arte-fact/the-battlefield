use crate::grid::{self, Grid, TileKind, TILE_SIZE};
use crate::sprite::AnimationState;
use crate::unit::{Facing, Faction};

pub const PAWN_FRAME_SIZE: u32 = 192;

const PAWN_WALK_SPEED: f32 = 60.0;
const PAWN_CHOP_TIME: f32 = 3.0;
const PAWN_RADIUS: f32 = 20.0;
const PAWN_ARRIVE_DIST_SQ: f32 = (TILE_SIZE * 0.5) * (TILE_SIZE * 0.5);
/// Max A* path length (grid steps).
const PAWN_PATH_MAX: u32 = 40;
/// Seconds without progress before giving up and resetting.
const STUCK_TIMEOUT: f32 = 4.0;

const IDLE_FRAMES: u32 = 8;
const RUN_FRAMES: u32 = 6;
const CHOP_FRAMES: u32 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PawnState {
    Idle,
    WalkingToTree,
    Chopping,
    WalkingHome,
}

pub struct Pawn {
    pub x: f32,
    pub y: f32,
    pub home_x: f32,
    pub home_y: f32,
    pub faction: Faction,
    pub facing: Facing,
    pub state: PawnState,
    pub animation: AnimationState,
    pub state_timer: f32,
    pub carrying_wood: bool,
    vel_x: f32,
    vel_y: f32,
    /// A* waypoints (grid coords), computed on state transition.
    waypoints: Vec<(u32, u32)>,
    waypoint_idx: usize,
    /// Target tree tile (grid coords).
    target_gx: u32,
    target_gy: u32,
    /// Stuck detection: resets when pawn moves more than 1 tile.
    stuck_timer: f32,
    last_x: f32,
    last_y: f32,
    rng_state: u32,
}

impl Pawn {
    pub fn new(home_x: f32, home_y: f32, faction: Faction, seed: u32) -> Self {
        let mut p = Self {
            x: home_x,
            y: home_y,
            home_x,
            home_y,
            faction,
            facing: Facing::Right,
            state: PawnState::Idle,
            animation: AnimationState::new(IDLE_FRAMES, 8.0),
            state_timer: 0.0,
            carrying_wood: false,
            vel_x: 0.0,
            vel_y: 0.0,
            waypoints: Vec::new(),
            waypoint_idx: 0,
            target_gx: 0,
            target_gy: 0,
            stuck_timer: 0.0,
            last_x: home_x,
            last_y: home_y,
            rng_state: if seed == 0 { 1 } else { seed },
        };
        p.state_timer = p.rand_range(0.5, 3.0);
        p
    }

    fn rand_u32(&mut self) -> u32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        self.rng_state
    }

    fn rand_f32(&mut self) -> f32 {
        (self.rand_u32() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }

    fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.rand_f32() * (hi - lo)
    }

    pub fn sprite_index(&self) -> usize {
        match (self.state, self.carrying_wood) {
            (PawnState::Idle, false) => 0,
            (PawnState::WalkingToTree, _) => 1,
            (PawnState::Chopping, _) => 2,
            (PawnState::Idle, true) => 3,
            (PawnState::WalkingHome, _) => 4,
        }
    }

    pub fn anim_frame_count(&self) -> u32 {
        match self.state {
            PawnState::Idle => IDLE_FRAMES,
            PawnState::WalkingToTree | PawnState::WalkingHome => RUN_FRAMES,
            PawnState::Chopping => CHOP_FRAMES,
        }
    }

    fn set_anim(&mut self, frames: u32, fps: f64) {
        self.animation = AnimationState::new(frames, fps);
    }

    /// Compute A* path from current position to a grid goal.
    fn compute_path(&mut self, goal_gx: u32, goal_gy: u32, grid: &Grid) -> bool {
        let (sx, sy) = grid::world_to_grid(self.x, self.y);
        if sx < 0 || sy < 0 {
            return false;
        }
        if let Some(path) =
            grid.find_path(sx as u32, sy as u32, goal_gx, goal_gy, PAWN_PATH_MAX, |_, _| false)
        {
            self.waypoints = path;
            self.waypoint_idx = 0;
            self.stuck_timer = 0.0;
            self.last_x = self.x;
            self.last_y = self.y;
            true
        } else {
            false
        }
    }

    /// Walk toward the next A* waypoint. Returns true when all waypoints are reached.
    fn follow_waypoints(&mut self, dt: f32, grid: &Grid) -> bool {
        if self.waypoint_idx >= self.waypoints.len() {
            self.vel_x = 0.0;
            self.vel_y = 0.0;
            return true;
        }

        let (gx, gy) = self.waypoints[self.waypoint_idx];
        let (wx, wy) = grid::grid_to_world(gx, gy);

        let dx = wx - self.x;
        let dy = wy - self.y;
        let dist_sq = dx * dx + dy * dy;

        if dist_sq < PAWN_ARRIVE_DIST_SQ {
            self.waypoint_idx += 1;
            if self.waypoint_idx >= self.waypoints.len() {
                self.vel_x = 0.0;
                self.vel_y = 0.0;
                return true;
            }
            return false;
        }

        let dist = dist_sq.sqrt();
        self.vel_x = (dx / dist) * PAWN_WALK_SPEED;
        self.vel_y = (dy / dist) * PAWN_WALK_SPEED;

        let new_x = self.x + self.vel_x * dt;
        let new_y = self.y + self.vel_y * dt;

        if grid.is_circle_passable(new_x, new_y, PAWN_RADIUS) {
            self.x = new_x;
            self.y = new_y;
        } else if grid.is_circle_passable(new_x, self.y, PAWN_RADIUS) {
            self.x = new_x;
        } else if grid.is_circle_passable(self.x, new_y, PAWN_RADIUS) {
            self.y = new_y;
        }

        if self.vel_x > 0.5 {
            self.facing = Facing::Right;
        } else if self.vel_x < -0.5 {
            self.facing = Facing::Left;
        }
        false
    }

    /// Check if stuck (no significant movement in STUCK_TIMEOUT seconds).
    fn check_stuck(&mut self, dt: f32) -> bool {
        self.stuck_timer += dt;
        let moved_dx = self.x - self.last_x;
        let moved_dy = self.y - self.last_y;
        if moved_dx * moved_dx + moved_dy * moved_dy > TILE_SIZE * TILE_SIZE {
            self.stuck_timer = 0.0;
            self.last_x = self.x;
            self.last_y = self.y;
        }
        self.stuck_timer > STUCK_TIMEOUT
    }

    /// Find nearest Forest tile within a search radius. Returns grid coords.
    fn find_nearest_tree(&mut self, grid: &Grid) -> Option<(u32, u32)> {
        let (gx, gy) = grid::world_to_grid(self.x, self.y);
        let mut best: Option<((u32, u32), f32)> = None;

        for dy in -20i32..=20 {
            for dx in -20i32..=20 {
                let nx = gx + dx;
                let ny = gy + dy;
                if !grid.in_bounds(nx, ny) {
                    continue;
                }
                let ux = nx as u32;
                let uy = ny as u32;
                if grid.get(ux, uy) != TileKind::Forest {
                    continue;
                }
                let d = (dx * dx + dy * dy) as f32;
                if best.is_none() || d < best.unwrap().1 {
                    best = Some(((ux, uy), d));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    /// Find the nearest passable tile adjacent to the target tree (for pathfinding goal).
    fn passable_near(&self, gx: u32, gy: u32, grid: &Grid) -> Option<(u32, u32)> {
        // Check the 8 neighbors of the tree tile for a passable one
        let mut best: Option<((u32, u32), f32)> = None;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = gx as i32 + dx;
                let ny = gy as i32 + dy;
                if !grid.in_bounds(nx, ny) {
                    continue;
                }
                let ux = nx as u32;
                let uy = ny as u32;
                if !grid.is_passable(ux, uy) {
                    continue;
                }
                let wx = ux as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                let wy = uy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                let ddx = wx - self.x;
                let ddy = wy - self.y;
                let d = ddx * ddx + ddy * ddy;
                if best.is_none() || d < best.unwrap().1 {
                    best = Some(((ux, uy), d));
                }
            }
        }
        best.map(|(pos, _)| pos)
    }

    fn reset_to_idle(&mut self) {
        self.vel_x = 0.0;
        self.vel_y = 0.0;
        self.waypoints.clear();
        self.waypoint_idx = 0;
        self.carrying_wood = false;
        self.state = PawnState::Idle;
        self.state_timer = self.rand_range(2.0, 5.0);
        self.set_anim(IDLE_FRAMES, 8.0);
    }

    pub fn update(&mut self, dt: f32, grid: &Grid) {
        match self.state {
            PawnState::Idle => {
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    if let Some((tree_gx, tree_gy)) = self.find_nearest_tree(grid) {
                        // Path to a passable tile adjacent to the tree
                        let goal = self.passable_near(tree_gx, tree_gy, grid);
                        if let Some((pgx, pgy)) = goal {
                            if self.compute_path(pgx, pgy, grid) {
                                self.target_gx = tree_gx;
                                self.target_gy = tree_gy;
                                self.state = PawnState::WalkingToTree;
                                self.set_anim(RUN_FRAMES, 10.0);
                            } else {
                                self.state_timer = self.rand_range(3.0, 6.0);
                            }
                        } else {
                            self.state_timer = self.rand_range(3.0, 6.0);
                        }
                    } else {
                        self.state_timer = self.rand_range(3.0, 6.0);
                    }
                }
            }
            PawnState::WalkingToTree => {
                if self.check_stuck(dt) {
                    self.reset_to_idle();
                    return;
                }
                if self.follow_waypoints(dt, grid) {
                    self.state = PawnState::Chopping;
                    self.state_timer = PAWN_CHOP_TIME;
                    self.set_anim(CHOP_FRAMES, 10.0);
                    // Face the tree
                    let (twx, _) = grid::grid_to_world(self.target_gx, self.target_gy);
                    if twx > self.x + 0.5 {
                        self.facing = Facing::Right;
                    } else if twx < self.x - 0.5 {
                        self.facing = Facing::Left;
                    }
                }
            }
            PawnState::Chopping => {
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    self.carrying_wood = true;
                    // Path home
                    let (hgx, hgy) = grid::world_to_grid(self.home_x, self.home_y);
                    if hgx >= 0 && hgy >= 0 && self.compute_path(hgx as u32, hgy as u32, grid) {
                        self.state = PawnState::WalkingHome;
                        self.set_anim(RUN_FRAMES, 10.0);
                    } else {
                        // Can't path home — teleport as safety fallback
                        self.x = self.home_x;
                        self.y = self.home_y;
                        self.reset_to_idle();
                    }
                }
            }
            PawnState::WalkingHome => {
                if self.check_stuck(dt) {
                    // Stuck going home — teleport
                    self.x = self.home_x;
                    self.y = self.home_y;
                    self.reset_to_idle();
                    return;
                }
                if self.follow_waypoints(dt, grid) {
                    self.carrying_wood = false;
                    self.state = PawnState::Idle;
                    self.state_timer = self.rand_range(1.0, 2.0);
                    self.set_anim(IDLE_FRAMES, 8.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, GRID_SIZE};

    #[test]
    fn pawn_initializes_idle() {
        let p = Pawn::new(500.0, 500.0, Faction::Blue, 42);
        assert_eq!(p.state, PawnState::Idle);
        assert!(!p.carrying_wood);
        assert!(p.state_timer > 0.0);
    }

    #[test]
    fn pawn_finds_tree_and_walks() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        grid.set(10, 10, TileKind::Forest);
        let mut p = Pawn::new(500.0, 500.0, Faction::Blue, 42);
        p.state_timer = 0.0;
        p.update(0.016, &grid);
        assert_eq!(p.state, PawnState::WalkingToTree);
        assert!(!p.waypoints.is_empty());
    }

    #[test]
    fn pawn_idles_when_no_trees() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut p = Pawn::new(500.0, 500.0, Faction::Blue, 42);
        p.state_timer = 0.0;
        p.update(0.016, &grid);
        assert_eq!(p.state, PawnState::Idle);
        assert!(p.state_timer > 0.0);
    }

    #[test]
    fn pawn_paths_around_buildings() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        // Wall of buildings blocking direct path from (5,5) to (5,10)
        for y in 6..10 {
            grid.mark_building(5, y);
        }
        grid.set(5, 12, TileKind::Forest);
        let (wx, wy) = grid::grid_to_world(5, 5);
        let mut p = Pawn::new(wx, wy, Faction::Blue, 42);
        p.state_timer = 0.0;
        p.update(0.016, &grid);
        assert_eq!(p.state, PawnState::WalkingToTree);
        // Path should route around the building wall
        for &(gx, gy) in &p.waypoints {
            assert!(
                grid.is_passable(gx, gy),
                "Waypoint ({gx},{gy}) is not passable"
            );
        }
    }
}
