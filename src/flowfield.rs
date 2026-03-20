use crate::grid::Grid;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

/// Flow field: a grid of direction vectors pointing toward a goal.
/// Generated via Dijkstra from the goal outward.
pub struct FlowField {
    /// 8-directional direction per cell: (dx, dy) where each is -1, 0, or 1.
    /// (0, 0) means goal cell or unreachable.
    directions: Vec<(i8, i8)>,
    /// Integration cost to reach the goal from each cell. u32::MAX = unreachable.
    integration: Vec<u32>,
    pub goal_gx: u32,
    pub goal_gy: u32,
    width: u32,
    height: u32,
}

/// 8-directional neighbors: (dx, dy, cost_multiplier).
/// Cardinal = 2, diagonal = 3 (matches A* cost model).
const DIRS: [(i32, i32, u32); 8] = [
    (0, -1, 2),
    (1, 0, 2),
    (0, 1, 2),
    (-1, 0, 2),
    (1, -1, 3),
    (1, 1, 3),
    (-1, 1, 3),
    (-1, -1, 3),
];

impl FlowField {
    /// Generate a flow field from the given goal using Dijkstra.
    /// Uses the same cost model as A*: cardinal=2, diagonal=3, × tile movement_cost.
    pub fn generate(grid: &Grid, goal_gx: u32, goal_gy: u32) -> Self {
        let w = grid.width;
        let h = grid.height;
        let size = (w * h) as usize;
        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut integration = vec![u32::MAX; size];
        let mut directions = vec![(0i8, 0i8); size];

        if !grid.in_bounds(goal_gx as i32, goal_gy as i32) || !grid.is_passable(goal_gx, goal_gy) {
            return Self { directions, integration, goal_gx, goal_gy, width: w, height: h };
        }

        // Dijkstra from goal
        integration[idx(goal_gx, goal_gy)] = 0;
        let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
        heap.push(Reverse((0, goal_gx, goal_gy)));

        while let Some(Reverse((cost, x, y))) = heap.pop() {
            if cost > integration[idx(x, y)] {
                continue;
            }

            for &(dx, dy, dir_cost) in &DIRS {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if !grid.in_bounds(nx, ny) {
                    continue;
                }
                let nx = nx as u32;
                let ny = ny as u32;
                if !grid.is_passable(nx, ny) {
                    continue;
                }
                if !grid.can_move_diagonal(x, y, dx, dy) {
                    continue;
                }
                let tile_cost = grid.get(nx, ny).movement_cost().unwrap_or(1);
                let new_cost = cost + tile_cost * dir_cost;
                let ni = idx(nx, ny);
                if new_cost < integration[ni] {
                    integration[ni] = new_cost;
                    heap.push(Reverse((new_cost, nx, ny)));
                }
            }
        }

        // Build direction field: each cell points toward the neighbor with lowest integration cost
        for y in 0..h {
            for x in 0..w {
                let ci = idx(x, y);
                if integration[ci] == u32::MAX || (x == goal_gx && y == goal_gy) {
                    continue; // unreachable or goal: keep (0, 0)
                }
                let mut best_cost = integration[ci];
                let mut best_dir = (0i8, 0i8);
                for &(dx, dy, _) in &DIRS {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if !grid.in_bounds(nx, ny) {
                        continue;
                    }
                    let nx = nx as u32;
                    let ny = ny as u32;
                    if !grid.can_move_diagonal(x, y, dx, dy) {
                        continue;
                    }
                    let ni = idx(nx, ny);
                    if integration[ni] < best_cost {
                        best_cost = integration[ni];
                        best_dir = (dx as i8, dy as i8);
                    }
                }
                directions[ci] = best_dir;
            }
        }

        Self { directions, integration, goal_gx, goal_gy, width: w, height: h }
    }

    /// O(1) direction lookup at grid cell (gx, gy).
    /// Returns (0, 0) if out of bounds, unreachable, or at goal.
    pub fn direction_at(&self, gx: u32, gy: u32) -> (i8, i8) {
        if gx >= self.width || gy >= self.height {
            return (0, 0);
        }
        self.directions[(gy * self.width + gx) as usize]
    }

    /// Integration cost at cell. u32::MAX = unreachable.
    pub fn cost_at(&self, gx: u32, gy: u32) -> u32 {
        if gx >= self.width || gy >= self.height {
            return u32::MAX;
        }
        self.integration[(gy * self.width + gx) as usize]
    }
}

/// Per-faction cached flow field state.
pub struct FactionFlowState {
    pub field: Option<FlowField>,
    pub cached_goal: (f32, f32),
}

impl FactionFlowState {
    pub fn new() -> Self {
        Self {
            field: None,
            cached_goal: (0.0, 0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, TileKind};

    #[test]
    fn directions_point_toward_goal_on_clear_grid() {
        let grid = Grid::new_grass(16, 16);
        let ff = FlowField::generate(&grid, 8, 8);
        // Cell to the left of goal should point right
        assert_eq!(ff.direction_at(7, 8), (1, 0));
        // Cell above goal should point down
        assert_eq!(ff.direction_at(8, 7), (0, 1));
        // Cell diagonal should point diagonally
        assert_eq!(ff.direction_at(7, 7), (1, 1));
        // Goal itself should be (0, 0)
        assert_eq!(ff.direction_at(8, 8), (0, 0));
    }

    #[test]
    fn routes_around_impassable_wall() {
        let mut grid = Grid::new_grass(16, 16);
        // Wall of water blocking direct horizontal path at x=5, y=0..6
        for y in 0..6 {
            grid.set(5, y, TileKind::Water);
        }
        let ff = FlowField::generate(&grid, 8, 3);
        // Cell at (4, 3) can't go right (wall at 5,3), should route around
        let dir = ff.direction_at(4, 3);
        // Should not point directly right into the wall
        assert_ne!(dir, (1, 0));
        // Should still be reachable
        assert_ne!(ff.cost_at(4, 3), u32::MAX);
    }

    #[test]
    fn unreachable_cell_returns_zero() {
        let mut grid = Grid::new_grass(8, 8);
        // Surround cell (5, 4) with water
        for x in 4..7 {
            grid.set(x, 3, TileKind::Water);
            grid.set(x, 5, TileKind::Water);
        }
        grid.set(4, 4, TileKind::Water);
        grid.set(6, 4, TileKind::Water);
        // Goal is outside the pocket
        let ff = FlowField::generate(&grid, 0, 0);
        assert_eq!(ff.direction_at(5, 4), (0, 0));
        assert_eq!(ff.cost_at(5, 4), u32::MAX);
    }

    #[test]
    fn forest_cells_have_higher_integration_cost() {
        let mut grid = Grid::new_grass(16, 16);
        // Path through forest at (5, 8)
        grid.set(5, 8, TileKind::Forest);
        let ff = FlowField::generate(&grid, 8, 8);
        // Grass cell at same distance
        let grass_cost = ff.cost_at(5, 8);
        // Compare to a grass cell at symmetric position
        let sym_cost = ff.cost_at(11, 8);
        // Forest should cost more (movement_cost=2 vs 1 for grass)
        assert!(grass_cost > sym_cost);
    }
}
