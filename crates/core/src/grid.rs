use std::cmp::Ordering;
use std::collections::BinaryHeap;

/// Tile types for the battlefield grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    Grass,
    Water,
    Forest,
    Rock,
    Road,
}

impl TileKind {
    pub fn movement_cost(self) -> Option<u32> {
        match self {
            TileKind::Grass => Some(5),
            TileKind::Road => Some(4), // slightly cheaper than grass
            TileKind::Forest => Some(10),
            TileKind::Water => None, // impassable
            TileKind::Rock => Some(5),
        }
    }

    pub fn defense_bonus(self) -> i32 {
        match self {
            TileKind::Forest => 1,
            _ => 0,
        }
    }

    /// Returns true for non-water terrain (Grass, Forest, Rock, Road).
    pub fn is_land(self) -> bool {
        self != TileKind::Water
    }
}

pub const PLAYABLE_SIZE: u32 = 160;
pub const BORDER_SIZE: u32 = 16;
pub const GRID_SIZE: u32 = PLAYABLE_SIZE + 2 * BORDER_SIZE; // 192

/// Decorative elements that sit on top of tiles without affecting gameplay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoration {
    Bush,
    WaterRock,
    /// Mineable gold outcrop (impassable), sprite variant 0-5.
    GoldStone(u8),
}

/// The battlefield grid: a GRID_SIZE x GRID_SIZE array of tiles.
pub struct Grid {
    tiles: Vec<TileKind>,
    elevations: Vec<u8>,
    pub decorations: Vec<Option<Decoration>>,
    /// Tiles occupied by buildings (impassable).
    building_occupied: Vec<bool>,
    /// Cached wide passability (tile + 4 cardinal neighbors all passable).
    wide_passable: Vec<bool>,
    /// Precomputed: true if tile blocks line-of-sight (Forest or elevation >= 2).
    pub vision_blocked: Vec<bool>,
    /// Precomputed: true if tile is passable (has movement cost and not building-occupied).
    pub passable: Vec<bool>,
    pub width: u32,
    pub height: u32,
}

impl Grid {
    pub fn new_grass(width: u32, height: u32) -> Self {
        let size = (width * height) as usize;
        Self {
            tiles: vec![TileKind::Grass; size],
            elevations: vec![0; size],
            decorations: vec![None; size],
            building_occupied: vec![false; size],
            vision_blocked: vec![false; size],
            passable: vec![true; size], // Grass is passable, no buildings
            wide_passable: vec![true; size],
            width,
            height,
        }
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height
    }

    pub fn get(&self, x: u32, y: u32) -> TileKind {
        self.tiles[(y * self.width + x) as usize]
    }

    pub fn set(&mut self, x: u32, y: u32, kind: TileKind) {
        let idx = (y * self.width + x) as usize;
        self.tiles[idx] = kind;
        // Keep caches in sync
        self.passable[idx] = kind.movement_cost().is_some() && !self.building_occupied[idx];
        self.vision_blocked[idx] = kind == TileKind::Forest || self.elevations[idx] >= 2;
        self.refresh_wide_around(x, y);
    }

    pub fn is_passable(&self, x: u32, y: u32) -> bool {
        self.passable[(y * self.width + x) as usize]
    }

    /// Check if movement between two adjacent tiles is blocked by a cliff
    /// (elevation difference > 0). Both tiles must be passable independently.
    pub fn is_cliff_between(&self, ax: u32, ay: u32, bx: u32, by: u32) -> bool {
        self.elevation(ax, ay) != self.elevation(bx, by)
    }

    /// Mark a tile as occupied by a building (impassable).
    pub fn mark_building(&mut self, x: u32, y: u32) {
        let idx = (y * self.width + x) as usize;
        self.building_occupied[idx] = true;
        self.passable[idx] = false;
        self.refresh_wide_around(x, y);
    }

    /// Recompute `vision_blocked` and `passable` caches from tile/elevation/building data.
    /// Call after map generation or any bulk terrain changes.
    pub fn recompute_caches(&mut self) {
        let size = (self.width * self.height) as usize;
        for i in 0..size {
            let tile = self.tiles[i];
            let elev = self.elevations[i];
            self.vision_blocked[i] = tile == TileKind::Forest || elev >= 2;
            self.passable[i] = tile.movement_cost().is_some()
                && !self.building_occupied[i]
                && !matches!(self.decorations[i], Some(Decoration::GoldStone(_)));
        }
        for y in 0..self.height {
            for x in 0..self.width {
                self.wide_passable[(y * self.width + x) as usize] = self.compute_wide_at(x, y);
            }
        }
    }

    fn compute_wide_at(&self, x: u32, y: u32) -> bool {
        if !self.is_passable(x, y) {
            return false;
        }
        for &(dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if !self.in_bounds(nx, ny) || !self.is_passable(nx as u32, ny as u32) {
                return false;
            }
        }
        true
    }

    /// Refresh the wide-passability cache for a tile and its cardinal
    /// neighbors after a local passability change.
    fn refresh_wide_around(&mut self, x: u32, y: u32) {
        for &(dx, dy) in &[(0i32, 0i32), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if self.in_bounds(nx, ny) {
                let i = (ny as u32 * self.width + nx as u32) as usize;
                self.wide_passable[i] = self.compute_wide_at(nx as u32, ny as u32);
            }
        }
    }

    /// Check if a tile is occupied by a building.
    pub fn is_building(&self, x: u32, y: u32) -> bool {
        self.building_occupied[(y * self.width + x) as usize]
    }

    /// Check if a tile is passable for a wide unit (radius > half tile).
    /// A wide unit at tile (x,y) overlaps into all 4 cardinal neighbors,
    /// so those must also be passable for the unit to stand there.
    pub fn is_wide_passable(&self, x: u32, y: u32) -> bool {
        self.wide_passable[(y * self.width + x) as usize]
    }

    /// Check if diagonal movement from (fx, fy) by (dx, dy) is allowed.
    /// Both adjacent cardinal tiles must be passable to prevent corner-cutting.
    pub fn can_move_diagonal(&self, fx: u32, fy: u32, dx: i32, dy: i32) -> bool {
        if dx == 0 || dy == 0 {
            return true; // cardinal, always fine
        }
        let cx = (fx as i32 + dx) as u32;
        let cy = fy;
        let rx = fx;
        let ry = (fy as i32 + dy) as u32;
        self.is_passable(cx, cy) && self.is_passable(rx, ry)
    }

    pub fn elevation(&self, x: u32, y: u32) -> u8 {
        self.elevations[(y * self.width + x) as usize]
    }

    pub fn set_elevation(&mut self, x: u32, y: u32, elev: u8) {
        let idx = (y * self.width + x) as usize;
        self.elevations[idx] = elev;
        self.vision_blocked[idx] = self.tiles[idx] == TileKind::Forest || elev >= 2;
    }

    pub fn decoration(&self, x: u32, y: u32) -> Option<Decoration> {
        self.decorations[(y * self.width + x) as usize]
    }

    pub fn set_decoration(&mut self, x: u32, y: u32, dec: Option<Decoration>) {
        self.decorations[(y * self.width + x) as usize] = dec;
    }

    /// Is a circle at (wx,wy) with given radius entirely on passable terrain?
    /// Checks center + 4 cardinal + 4 diagonal edge points.
    /// Also blocks if any edge point crosses an elevation cliff from the center.
    pub fn is_circle_passable(&self, wx: f32, wy: f32, radius: f32) -> bool {
        let d = radius * 0.707; // radius * cos(45°) for diagonal points
        let (cgx, cgy) = world_to_grid(wx, wy);
        if !self.in_bounds(cgx, cgy) || !self.is_passable(cgx as u32, cgy as u32) {
            return false;
        }
        let center_elev = self.elevation(cgx as u32, cgy as u32);
        let edge_points = [
            (wx + radius, wy),
            (wx - radius, wy),
            (wx, wy + radius),
            (wx, wy - radius),
            (wx + d, wy + d),
            (wx + d, wy - d),
            (wx - d, wy + d),
            (wx - d, wy - d),
        ];
        for &(px, py) in &edge_points {
            let (gx, gy) = world_to_grid(px, py);
            if !self.in_bounds(gx, gy) {
                return false;
            }
            let ux = gx as u32;
            let uy = gy as u32;
            if !self.is_passable(ux, uy) {
                return false;
            }
            if self.elevation(ux, uy) != center_elev {
                return false;
            }
        }
        true
    }

    /// Speed multiplier at world position (1.0 grass, 0.75 bush/rock, 0.5 forest, 0.0 out-of-bounds).
    pub fn speed_factor_at(&self, wx: f32, wy: f32) -> f32 {
        let (gx, gy) = world_to_grid(wx, wy);
        if !self.in_bounds(gx, gy) {
            return 0.0;
        }
        let ux = gx as u32;
        let uy = gy as u32;
        match self.get(ux, uy) {
            TileKind::Forest => 0.5,
            TileKind::Rock => 0.75,
            TileKind::Road => 1.25,
            TileKind::Grass => {
                if self.decoration(ux, uy) == Some(Decoration::Bush) {
                    0.75
                } else {
                    1.0
                }
            }
            _ => 0.0,
        }
    }

    /// A* pathfinding from (sx, sy) to (gx, gy).
    /// Returns the path as a list of grid positions excluding the start, or None if unreachable.
    /// `occupied` is called to check if a tile is blocked by a unit (not checked for the goal).
    /// Max path length is capped at `max_len` steps.
    pub fn find_path(
        &self,
        sx: u32,
        sy: u32,
        gx: u32,
        gy: u32,
        max_len: u32,
        occupied: impl Fn(u32, u32) -> bool,
    ) -> Option<Vec<(u32, u32)>> {
        if sx == gx && sy == gy {
            return Some(Vec::new());
        }
        if !self.in_bounds(gx as i32, gy as i32) || !self.is_passable(gx, gy) {
            return None;
        }
        // Use wide passability check: unit radius (28px) is nearly as wide as
        // a tile (32px), so units overlap into cardinal neighbors. A* must only
        // route through tiles where all neighbors are also passable.
        let passable = |x: u32, y: u32| -> bool { self.is_wide_passable(x, y) };
        // Allow start and goal even if not wide-passable (unit may already be there)
        let is_node_passable = |x: u32, y: u32| -> bool {
            (x == sx && y == sy) || (x == gx && y == gy) || passable(x, y)
        };

        let w = self.width;
        let h = self.height;
        let size = (w * h) as usize;
        let idx = |x: u32, y: u32| (y * w + x) as usize;

        // Generation-stamped scratch: reused across calls so each search
        // avoids allocating and zeroing two grid-sized vectors.
        thread_local! {
            static SCRATCH: std::cell::RefCell<PathScratch> =
                std::cell::RefCell::new(PathScratch::default());
        }

        SCRATCH.with(|scratch| {
            let mut scratch = scratch.borrow_mut();
            scratch.begin(size);
            scratch.set_g(idx(sx, sy), 0);

            // Octile heuristic for 8-directional movement (scaled by cost_mult 2/3)
            let heuristic = |x: u32, y: u32| -> u32 {
                let dx = (x as i32 - gx as i32).unsigned_abs();
                let dy = (y as i32 - gy as i32).unsigned_abs();
                let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
                // cardinal steps * 2 + diagonal steps * 3
                min * 3 + (max - min) * 2
            };

            let mut open = BinaryHeap::new();
            open.push(AStarNode {
                f: heuristic(sx, sy),
                g: 0,
                x: sx,
                y: sy,
            });

            // 8-directional: cardinal + diagonal
            // cost_mult: cardinal = 2, diagonal = 3 (approximates √2 ratio)
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

            // Search is bounded: an unreachable or distant goal must not flood
            // the whole grid (a full-map exhaustion costs ~1ms; the AI issues
            // several searches per frame). On hitting the cap, return a partial
            // path toward the closest approach found so far — callers truncate
            // to max_len anyway. A search exhausted below the cap (or a closest
            // approach identical to the start) is a genuine None.
            let expansion_cap = (max_len as usize).saturating_mul(8).max(64);
            let mut expansions = 0usize;
            let mut best_h = heuristic(sx, sy);
            let mut best_pos = (sx, sy);

            let reconstruct = |scratch: &PathScratch, tx: u32, ty: u32| -> Vec<(u32, u32)> {
                let mut path = Vec::new();
                let mut ci = idx(tx, ty);
                while ci != idx(sx, sy) {
                    let cx = (ci as u32) % w;
                    let cy = (ci as u32) / w;
                    path.push((cx, cy));
                    ci = scratch.parent(ci) as usize;
                }
                path.reverse();
                if path.len() > max_len as usize {
                    path.truncate(max_len as usize);
                }
                path
            };

            while let Some(node) = open.pop() {
                if node.x == gx && node.y == gy {
                    return Some(reconstruct(&scratch, gx, gy));
                }

                if node.g > scratch.g(idx(node.x, node.y)) {
                    continue; // stale entry
                }

                let h = heuristic(node.x, node.y);
                if h < best_h {
                    best_h = h;
                    best_pos = (node.x, node.y);
                }
                expansions += 1;
                if expansions >= expansion_cap {
                    if best_pos == (sx, sy) {
                        return None;
                    }
                    return Some(reconstruct(&scratch, best_pos.0, best_pos.1));
                }

                for &(dx, dy, dir_cost) in &DIRS {
                    let nx = node.x as i32 + dx;
                    let ny = node.y as i32 + dy;
                    if !self.in_bounds(nx, ny) {
                        continue;
                    }
                    let nx = nx as u32;
                    let ny = ny as u32;
                    if !is_node_passable(nx, ny) {
                        continue;
                    }
                    // Block movement across elevation cliffs
                    if self.is_cliff_between(node.x, node.y, nx, ny) {
                        continue;
                    }
                    // Prevent corner-cutting for diagonal moves
                    if !self.can_move_diagonal(node.x, node.y, dx, dy) {
                        continue;
                    }
                    // Skip occupied tiles unless it's the goal
                    if (nx != gx || ny != gy) && occupied(nx, ny) {
                        continue;
                    }
                    let tile_cost = self.get(nx, ny).movement_cost().unwrap_or(1);
                    let new_g = node.g + tile_cost * dir_cost;
                    let ni = idx(nx, ny);
                    if new_g < scratch.g(ni) {
                        scratch.set_g(ni, new_g);
                        scratch.set_parent(ni, idx(node.x, node.y) as u32);
                        open.push(AStarNode {
                            f: new_g + heuristic(nx, ny),
                            g: new_g,
                            x: nx,
                            y: ny,
                        });
                    }
                }
            }

            None
        })
    }
}

/// Reusable A* scratch: generation stamps avoid re-zeroing grid-sized
/// buffers on every search.
#[derive(Default)]
struct PathScratch {
    stamp: Vec<u32>,
    g_score: Vec<u32>,
    parents: Vec<u32>,
    generation: u32,
}

impl PathScratch {
    fn begin(&mut self, size: usize) {
        if self.stamp.len() != size {
            self.stamp = vec![0; size];
            self.g_score = vec![u32::MAX; size];
            self.parents = vec![u32::MAX; size];
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.stamp.fill(0);
            self.generation = 1;
        }
    }

    fn g(&self, i: usize) -> u32 {
        if self.stamp[i] == self.generation {
            self.g_score[i]
        } else {
            u32::MAX
        }
    }

    fn set_g(&mut self, i: usize, v: u32) {
        self.stamp[i] = self.generation;
        self.g_score[i] = v;
    }

    fn parent(&self, i: usize) -> u32 {
        self.parents[i]
    }

    fn set_parent(&mut self, i: usize, v: u32) {
        self.parents[i] = v;
    }
}

#[derive(Eq, PartialEq)]
struct AStarNode {
    f: u32,
    g: u32,
    x: u32,
    y: u32,
}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.cmp(&self.f).then_with(|| self.g.cmp(&other.g))
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// World-space constants: each tile is 64x64 pixels.
pub const TILE_SIZE: f32 = 64.0;

/// Convert grid coordinates to world-space pixel center.
pub fn grid_to_world(gx: u32, gy: u32) -> (f32, f32) {
    let wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    let wy = gy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    (wx, wy)
}

/// Convert world-space pixel position to grid coordinates.
pub fn world_to_grid(wx: f32, wy: f32) -> (i32, i32) {
    let gx = (wx / TILE_SIZE).floor() as i32;
    let gy = (wy / TILE_SIZE).floor() as i32;
    (gx, gy)
}

/// Compute source pixel rectangle (sx, sy, sw, sh) for a tile at (col, row) in the tilemap.
/// Tilemap is 576x384 pixels (9 cols x 6 rows of 64x64 tiles).
pub fn tilemap_src_rect(col: u32, row: u32) -> (f64, f64, f64, f64) {
    let sx = (col as f64) * (TILE_SIZE as f64);
    let sy = (row as f64) * (TILE_SIZE as f64);
    (sx, sy, TILE_SIZE as f64, TILE_SIZE as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grass_grid() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        assert_eq!(grid.get(0, 0), TileKind::Grass);
        assert_eq!(grid.get(GRID_SIZE - 1, GRID_SIZE - 1), TileKind::Grass);
    }

    #[test]
    fn set_and_get_tile() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        grid.set(10, 20, TileKind::Water);
        assert_eq!(grid.get(10, 20), TileKind::Water);
        assert_eq!(grid.get(10, 19), TileKind::Grass);
    }

    #[test]
    fn in_bounds() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        assert!(grid.in_bounds(0, 0));
        assert!(grid.in_bounds(GRID_SIZE as i32 - 1, GRID_SIZE as i32 - 1));
        assert!(!grid.in_bounds(-1, 0));
        assert!(!grid.in_bounds(GRID_SIZE as i32, 0));
    }

    #[test]
    fn movement_costs() {
        assert_eq!(TileKind::Grass.movement_cost(), Some(5));
        assert_eq!(TileKind::Forest.movement_cost(), Some(10));
        assert_eq!(TileKind::Water.movement_cost(), None);
        assert_eq!(TileKind::Rock.movement_cost(), Some(5));
        assert_eq!(TileKind::Road.movement_cost(), Some(4));
    }

    #[test]
    fn grid_world_conversion_roundtrip() {
        let (wx, wy) = grid_to_world(5, 10);
        let (gx, gy) = world_to_grid(wx, wy);
        assert_eq!(gx, 5);
        assert_eq!(gy, 10);
    }

    #[test]
    fn grid_to_world_center() {
        let (wx, wy) = grid_to_world(0, 0);
        assert!((wx - 32.0).abs() < f32::EPSILON);
        assert!((wy - 32.0).abs() < f32::EPSILON);
    }

    #[test]
    fn is_land() {
        assert!(TileKind::Grass.is_land());
        assert!(TileKind::Forest.is_land());
        assert!(TileKind::Rock.is_land());
        assert!(TileKind::Road.is_land());
        assert!(!TileKind::Water.is_land());
    }

    #[test]
    fn elevation_defaults_to_zero() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        assert_eq!(grid.elevation(0, 0), 0);
        assert_eq!(grid.elevation(GRID_SIZE - 1, GRID_SIZE - 1), 0);
    }

    #[test]
    fn set_and_get_elevation() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        grid.set_elevation(10, 20, 2);
        assert_eq!(grid.elevation(10, 20), 2);
        assert_eq!(grid.elevation(10, 19), 0);
    }

    #[test]
    fn find_path_straight_line() {
        let grid = Grid::new_grass(16, 16);
        // Use interior tiles (wide passability requires all cardinal neighbors passable)
        let path = grid.find_path(2, 2, 7, 2, 30, |_, _| false);
        assert!(path.is_some());
        let path = path.unwrap();
        assert_eq!(path.len(), 5);
        assert_eq!(path[0], (3, 2));
        assert_eq!(path[4], (7, 2));
    }

    #[test]
    fn find_path_around_water() {
        let mut grid = Grid::new_grass(16, 16);
        // Wall of water blocking direct path
        for y in 0..5 {
            grid.set(3, y, TileKind::Water);
        }
        let path = grid.find_path(0, 2, 5, 2, 30, |_, _| false);
        assert!(path.is_some());
        let path = path.unwrap();
        // Path must go around the wall
        assert!(path.len() > 5);
        // All tiles in path must be passable
        for &(x, y) in &path {
            assert!(grid.is_passable(x, y));
        }
    }

    #[test]
    fn find_path_far_goal_is_bounded_best_effort() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        // Goal ~120 tiles away: the search must return a truncated partial
        // path without flooding the map.
        let path = grid.find_path(5, 5, 125, 125, 40, |_, _| false).unwrap();
        assert!(!path.is_empty() && path.len() <= 40);
        // Path must make real progress toward the goal
        let (lx, ly) = *path.last().unwrap();
        let d0 = (125 - 5) + (125 - 5);
        let d1 = (125 - lx as i32).unsigned_abs() + (125 - ly as i32).unsigned_abs();
        assert!((d1 as i32) < d0 - 20);
    }

    #[test]
    fn find_path_none_when_pressed_against_barrier() {
        let mut grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        for y in 0..GRID_SIZE {
            grid.set(30, y, TileKind::Water);
        }
        grid.recompute_caches();
        // Start right at the wall, goal across it: no approach is closer
        // than the start, so the search reports unreachable.
        assert!(grid.find_path(28, 20, 33, 20, 40, |_, _| false).is_none());
    }

    #[test]
    fn find_path_unreachable() {
        let mut grid = Grid::new_grass(8, 8);
        // Surround destination with water
        for x in 4..7 {
            grid.set(x, 3, TileKind::Water);
            grid.set(x, 5, TileKind::Water);
        }
        grid.set(4, 4, TileKind::Water);
        grid.set(6, 4, TileKind::Water);
        let path = grid.find_path(0, 0, 5, 4, 30, |_, _| false);
        assert!(path.is_none());
    }

    #[test]
    fn find_path_same_position() {
        let grid = Grid::new_grass(8, 8);
        let path = grid.find_path(3, 3, 3, 3, 30, |_, _| false);
        assert_eq!(path, Some(Vec::new()));
    }

    #[test]
    fn find_path_respects_max_len() {
        let grid = Grid::new_grass(16, 16);
        let path = grid.find_path(0, 0, 10, 0, 5, |_, _| false);
        assert!(path.is_some());
        assert_eq!(path.unwrap().len(), 5); // truncated
    }

    #[test]
    fn find_path_avoids_occupied() {
        let grid = Grid::new_grass(8, 8);
        // Block (2,0) with a unit — path should route around it
        let path = grid.find_path(0, 0, 4, 0, 30, |x, y| x == 2 && y == 0);
        assert!(path.is_some());
        let path = path.unwrap();
        assert!(!path.contains(&(2, 0)));
        assert_eq!(*path.last().unwrap(), (4, 0));
    }

    #[test]
    fn find_path_diagonal() {
        let grid = Grid::new_grass(16, 16);
        // Use interior tiles for wide passability
        let path = grid.find_path(3, 3, 8, 8, 30, |_, _| false).unwrap();
        assert_eq!(path.len(), 5); // 5 diagonal steps
        assert_eq!(*path.last().unwrap(), (8, 8));
    }

    #[test]
    fn find_path_no_corner_cutting() {
        let mut grid = Grid::new_grass(16, 16);
        // Create an L-shaped wall in interior: water at (6,5) and (5,6)
        // Moving from (5,5) to (6,6) diagonally would cut the corner
        grid.set(6, 5, TileKind::Water);
        grid.set(5, 6, TileKind::Water);
        let path = grid.find_path(5, 5, 6, 6, 30, |_, _| false).unwrap();
        // Path must not go directly (5,5)->(6,6), it must route around
        assert!(path.len() > 1);
        // No step should cut a corner past water
        let mut prev = (1u32, 1u32);
        for &(px, py) in &path {
            let dx = px as i32 - prev.0 as i32;
            let dy = py as i32 - prev.1 as i32;
            if dx != 0 && dy != 0 {
                // Diagonal: both cardinal neighbors must be passable
                assert!(grid.is_passable((prev.0 as i32 + dx) as u32, prev.1));
                assert!(grid.is_passable(prev.0, (prev.1 as i32 + dy) as u32));
            }
            prev = (px, py);
        }
    }

    #[test]
    fn circle_passable_on_grass() {
        let grid = Grid::new_grass(16, 16);
        let (wx, wy) = grid_to_world(5, 5);
        assert!(grid.is_circle_passable(wx, wy, 22.0));
    }

    #[test]
    fn circle_not_passable_on_water() {
        let mut grid = Grid::new_grass(16, 16);
        grid.set(5, 5, TileKind::Water);
        let (wx, wy) = grid_to_world(5, 5);
        assert!(!grid.is_circle_passable(wx, wy, 22.0));
    }

    #[test]
    fn speed_factor_grass_and_forest() {
        let mut grid = Grid::new_grass(16, 16);
        let (wx, wy) = grid_to_world(5, 5);
        assert!((grid.speed_factor_at(wx, wy) - 1.0).abs() < f32::EPSILON);
        grid.set(5, 5, TileKind::Forest);
        assert!((grid.speed_factor_at(wx, wy) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn can_move_diagonal_blocked() {
        let mut grid = Grid::new_grass(8, 8);
        grid.set(2, 1, TileKind::Water);
        // Moving from (1,1) NE to (2,0): cardinal neighbor (2,1) is water
        assert!(!grid.can_move_diagonal(1, 1, 1, -1));
        // Moving from (1,1) SE to (2,2): cardinal neighbor (2,1) is water
        assert!(!grid.can_move_diagonal(1, 1, 1, 1));
        // Moving from (1,1) NW to (0,0): both cardinals passable
        assert!(grid.can_move_diagonal(1, 1, -1, -1));
    }
}
