use crate::grid::Grid;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Flow field: a grid of direction vectors pointing toward the nearest/cheapest goal.
/// Generated via Dijkstra from one or more goals outward.
pub struct FlowField {
    /// 8-directional direction per cell: (dx, dy) where each is -1, 0, or 1.
    /// (0, 0) means goal cell or unreachable.
    directions: Vec<(i8, i8)>,
    /// Integration cost to reach the nearest goal from each cell. u32::MAX = unreachable.
    integration: Vec<u32>,
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
    /// Generate a flow field from a single goal. Thin wrapper around `generate_multi_source`.
    pub fn generate(grid: &Grid, goal_gx: u32, goal_gy: u32) -> Self {
        Self::generate_multi_source(grid, &[(goal_gx, goal_gy, 0)])
    }

    /// Generate a multi-source flow field from multiple goals using Dijkstra.
    /// Each goal is `(gx, gy, initial_cost)` — lower initial_cost = higher priority.
    /// Uses the same cost model as A*: cardinal=2, diagonal=3, × tile movement_cost.
    pub fn generate_multi_source(grid: &Grid, goals: &[(u32, u32, u32)]) -> Self {
        let w = grid.width;
        let h = grid.height;
        let size = (w * h) as usize;
        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut integration = vec![u32::MAX; size];
        let mut directions = vec![(0i8, 0i8); size];
        let mut heap: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();

        // Seed all valid goals
        for &(gx, gy, initial_cost) in goals {
            if grid.in_bounds(gx as i32, gy as i32) && grid.is_passable(gx, gy) {
                let i = idx(gx, gy);
                if initial_cost < integration[i] {
                    integration[i] = initial_cost;
                    heap.push(Reverse((initial_cost, gx, gy)));
                }
            }
        }

        if heap.is_empty() {
            return Self {
                directions,
                integration,
                width: w,
                height: h,
            };
        }

        // Dijkstra from all seeded goals simultaneously
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
                let is_goal = goals.iter().any(|&(gx, gy, _)| gx == x && gy == y);
                if integration[ci] == u32::MAX || is_goal {
                    continue; // unreachable or goal cell: keep (0, 0)
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

        Self {
            directions,
            integration,
            width: w,
            height: h,
        }
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
#[derive(Default)]
pub struct FactionFlowState {
    /// Unified multi-source field (fallback navigation).
    pub field: Option<FlowField>,
    /// Cached (gx, gy, initial_cost) tuples used to detect when regeneration is needed.
    pub cached_goals: Vec<(u32, u32, u32)>,
    /// Per-zone flow fields, indexed by zone id.
    pub zone_fields: Vec<Option<FlowField>>,
    /// Cached (gx, gy) per zone for change detection.
    pub cached_zone_goals: Vec<Option<(u32, u32)>>,
    /// Number of units assigned to each zone (updated each scoring cycle).
    pub zone_congestion: Vec<u32>,
}

impl FactionFlowState {
    pub fn new() -> Self {
        Self::default()
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

    #[test]
    fn multi_source_routes_to_nearest_goal() {
        let grid = Grid::new_grass(16, 16);
        // Two equal-weight goals: left (2, 8) and right (13, 8)
        let ff = FlowField::generate_multi_source(&grid, &[(2, 8, 0), (13, 8, 0)]);
        // Cell at (3, 8) is 1 tile from left goal — should point left
        assert_eq!(ff.direction_at(3, 8), (-1, 0));
        // Cell at (12, 8) is 1 tile from right goal — should point right
        assert_eq!(ff.direction_at(12, 8), (1, 0));
        // Both goal cells themselves are (0, 0)
        assert_eq!(ff.direction_at(2, 8), (0, 0));
        assert_eq!(ff.direction_at(13, 8), (0, 0));
    }

    #[test]
    fn multi_source_score_biases_routing() {
        let grid = Grid::new_grass(20, 10);
        // Goal A at (2, 5) initial_cost=0 (high priority)
        // Goal B at (17, 5) initial_cost=50 (low priority)
        // Cell at (10, 5): 8 tiles from A (cost ~16), 7 tiles from B (cost ~14 + 50 = 64)
        // Net: should flow toward A despite being slightly closer to B in raw distance
        let ff = FlowField::generate_multi_source(&grid, &[(2, 5, 0), (17, 5, 50)]);
        let dir = ff.direction_at(10, 5);
        assert_eq!(
            dir,
            (-1, 0),
            "score bias should redirect toward higher-priority goal A"
        );
    }
}
