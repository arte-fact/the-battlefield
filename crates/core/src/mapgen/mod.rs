pub mod simplex;

use crate::grid::{Decoration, Grid, TileKind, BORDER_SIZE, GRID_SIZE, PLAYABLE_SIZE};
use simplex::Simplex;

/// BSP layout data returned from map generation.
pub struct MapLayout {
    pub blue_base: (u32, u32),
    pub red_base: (u32, u32),
    pub zone_centers: Vec<(u32, u32)>,
    /// Rally point for Blue: front-center of Blue base (toward battlefield).
    pub blue_gather: (u32, u32),
    /// Rally point for Red: front-center of Red base (toward battlefield).
    pub red_gather: (u32, u32),
}

/// A rectangle used during BSP partitioning.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

impl Rect {
    fn center(&self) -> (u32, u32) {
        (self.x + self.w / 2, self.y + self.h / 2)
    }
}

/// Recursively split a rectangle via BSP, returning leaf rects.
fn bsp_split(rng: &mut Rng, rect: Rect, depth: u32, max_depth: u32, min_size: u32) -> Vec<Rect> {
    if depth >= max_depth || (rect.w < min_size * 2 && rect.h < min_size * 2) {
        return vec![rect];
    }

    // Split along longer axis
    let split_horizontal = if rect.w > rect.h + 4 {
        false // split vertically (along x)
    } else if rect.h > rect.w + 4 {
        true // split horizontally (along y)
    } else {
        rng.chance(0.5)
    };

    if split_horizontal {
        if rect.h < min_size * 2 {
            return vec![rect];
        }
        // Split position: 40-60% of height
        let range = rect.h - 2 * min_size;
        let split_offset = min_size + (rng.next() % (range + 1));
        let top = Rect {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: split_offset,
        };
        let bottom = Rect {
            x: rect.x,
            y: rect.y + split_offset,
            w: rect.w,
            h: rect.h - split_offset,
        };
        let mut leaves = bsp_split(rng, top, depth + 1, max_depth, min_size);
        leaves.extend(bsp_split(rng, bottom, depth + 1, max_depth, min_size));
        leaves
    } else {
        if rect.w < min_size * 2 {
            return vec![rect];
        }
        let range = rect.w - 2 * min_size;
        let split_offset = min_size + (rng.next() % (range + 1));
        let left = Rect {
            x: rect.x,
            y: rect.y,
            w: split_offset,
            h: rect.h,
        };
        let right = Rect {
            x: rect.x + split_offset,
            y: rect.y,
            w: rect.w - split_offset,
            h: rect.h,
        };
        let mut leaves = bsp_split(rng, left, depth + 1, max_depth, min_size);
        leaves.extend(bsp_split(rng, right, depth + 1, max_depth, min_size));
        leaves
    }
}

/// Simple xorshift32 PRNG for deterministic terrain generation.
struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u32 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        self.state
    }

    /// Random f32 in [0.0, 1.0).
    fn f32(&mut self) -> f32 {
        (self.next() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }

    /// Returns true with probability `p` (0.0 to 1.0).
    fn chance(&mut self, p: f32) -> bool {
        self.f32() < p
    }
}

// Terrain generation constants
const ELEVATION_SCALE: f64 = 0.04;
const WATER_THRESHOLD: f64 = -0.3;
const HILL_THRESHOLD: f64 = 0.45;
const TREE_DENSITY: f64 = 0.3;
const BUSH_DENSITY: f64 = 0.04;
const ROCK_DENSITY: f64 = 0.03;
const WATER_ROCK_DENSITY: f32 = 0.12;

/// Returns true if the tile is within the border ring (outside the playable area).
fn in_border(x: u32, y: u32) -> bool {
    x < BORDER_SIZE
        || y < BORDER_SIZE
        || x >= BORDER_SIZE + PLAYABLE_SIZE
        || y >= BORDER_SIZE + PLAYABLE_SIZE
}

/// Generate a procedural battlefield grid with BSP layout.
pub fn generate_battlefield(seed: u32) -> (Grid, MapLayout) {
    let mut rng = Rng::new(seed);
    let w = GRID_SIZE;
    let h = GRID_SIZE;
    let mut grid = Grid::new_grass(w, h);
    let noise = Simplex::new(seed as u64);

    // --- Phase A: Simplex noise heightmap with edge elevation bias ---
    for y in 0..h {
        for x in 0..w {
            let val = noise.octave(
                x as f64 * ELEVATION_SCALE,
                y as f64 * ELEVATION_SCALE,
                4,
                0.5,
            );

            // Distance from nearest edge, normalized to [0, 1] (0 = edge, 1 = center)
            let dx = (x as f64).min((w - 1 - x) as f64) / (BORDER_SIZE as f64);
            let dy = (y as f64).min((h - 1 - y) as f64) / (BORDER_SIZE as f64);
            let edge_dist = dx.min(dy).clamp(0.0, 1.0);

            // Quadratic bias: 0 in playable center, ramps up to ~1.0 at grid edge
            let edge_bias = if edge_dist < 16.0 {
                let t = 1.0 - edge_dist;
                t * t // smooth quadratic ramp
            } else {
                0.0
            };

            let effective_val = val + edge_bias;

            if effective_val < WATER_THRESHOLD {
                grid.set(x, y, TileKind::Water);
            } else if effective_val > HILL_THRESHOLD {
                grid.set_elevation(x, y, 2);
            }
            // else: remains Grass, elevation 0
        }
    }

    // Smooth water with cellular automata to remove small isolated chunks.
    // birth=5: land becomes water only if 5+ of 8 neighbors are water (conservative)
    // death=3: water becomes land if fewer than 3 neighbors are water (removes small pools)
    for _pass in 0..3 {
        let mut changes: Vec<(u32, u32, bool)> = Vec::new();
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let mut water_neighbors = 0u32;
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        if grid.get((x as i32 + dx) as u32, (y as i32 + dy) as u32)
                            == TileKind::Water
                        {
                            water_neighbors += 1;
                        }
                    }
                }
                let is_water = grid.get(x, y) == TileKind::Water;
                if is_water && water_neighbors < 3 {
                    changes.push((x, y, false)); // kill small water
                } else if !is_water && water_neighbors >= 5 {
                    changes.push((x, y, true)); // fill gaps
                }
            }
        }
        for (x, y, make_water) in changes {
            if make_water {
                grid.set(x, y, TileKind::Water);
                grid.set_elevation(x, y, 0);
                grid.set_decoration(x, y, None);
            } else {
                grid.set(x, y, TileKind::Grass);
            }
        }
    }

    // --- Phase B: Cellular automata for vegetation & decorations ---

    // Trees: seeded from simplex noise at offset, placed on grass
    let tree_seed = run_cellular_automaton(
        &seed_from_noise(&noise, w, h, 100.0, 0.07, TREE_DENSITY),
        w,
        h,
        5,
        4,
        2,
    );
    // Helper: true if tile is on or adjacent to an elevation cliff
    let near_cliff = |grid: &Grid, x: u32, y: u32| -> bool {
        let e = grid.elevation(x, y);
        for &(dx, dy) in &[(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if grid.in_bounds(nx, ny) && grid.elevation(nx as u32, ny as u32) != e {
                return true;
            }
        }
        false
    };

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if tree_seed[i]
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
                && !near_cliff(&grid, x, y)
            {
                grid.set(x, y, TileKind::Forest);
            }
        }
    }

    // Rocks: sparse random scatter on grass
    for y in 0..h {
        for x in 0..w {
            if rng.chance(ROCK_DENSITY as f32)
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
                && !near_cliff(&grid, x, y)
            {
                grid.set(x, y, TileKind::Rock);
            }
        }
    }

    // Bushes: sparse random scatter on grass
    for y in 0..h {
        for x in 0..w {
            if rng.chance(BUSH_DENSITY as f32)
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
                && !near_cliff(&grid, x, y)
            {
                grid.set_decoration(x, y, Some(Decoration::Bush));
            }
        }
    }

    // Water rocks: simple random chance on water tiles
    for y in 0..h {
        for x in 0..w {
            if grid.get(x, y) == TileKind::Water && rng.chance(WATER_ROCK_DENSITY) {
                grid.set_decoration(x, y, Some(Decoration::WaterRock));
            }
        }
    }

    // --- Phase C: Post-processing with BSP layout ---

    let b = BORDER_SIZE;
    let p = PLAYABLE_SIZE;

    // Run BSP on playable area
    let playable_rect = Rect {
        x: b,
        y: b,
        w: p,
        h: p,
    };
    let mut bsp_rng = Rng::new(seed.wrapping_add(0xBEEF));
    let leaves = bsp_split(&mut bsp_rng, playable_rect, 0, 4, 20);

    // Sort leaves by distance to top-left corner to assign bases
    let top_left = (b as f32, b as f32);
    let mut sorted_leaves = leaves.clone();
    sorted_leaves.sort_by(|a, b_leaf| {
        let (ax, ay) = a.center();
        let (bx, by) = b_leaf.center();
        let da = (ax as f32 - top_left.0).powi(2) + (ay as f32 - top_left.1).powi(2);
        let db = (bx as f32 - top_left.0).powi(2) + (by as f32 - top_left.1).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    });

    let blue_base = sorted_leaves[0].center();
    let red_base = sorted_leaves[sorted_leaves.len() - 1].center();

    // Place 5 zones: 3 along the diagonal between bases + 2 flanks
    let (bx, by) = (blue_base.0 as f32, blue_base.1 as f32);
    let (rx, ry) = (red_base.0 as f32, red_base.1 as f32);
    let mid_x = (bx + rx) * 0.5;
    let mid_y = (by + ry) * 0.5;
    let dx = rx - bx;
    let dy = ry - by;
    // 3 diagonal zones at 25%, 50%, 75% between bases
    let diag: Vec<(u32, u32)> = [0.25_f32, 0.50, 0.75]
        .iter()
        .map(|&t| ((bx + dx * t) as u32, (by + dy * t) as u32))
        .collect();
    // 2 flanks: perpendicular offset from midpoint
    let perp_x = -dy * 0.25;
    let perp_y = dx * 0.25;
    let flank1 = ((mid_x + perp_x) as u32, (mid_y + perp_y) as u32);
    let flank2 = ((mid_x - perp_x) as u32, (mid_y - perp_y) as u32);
    let mut zone_centers = diag;
    zone_centers.push(flank1);
    zone_centers.push(flank2);

    // Clear 24×28 rect around each base (wider for flank towers, deeper for rear village)
    clear_rect(
        &mut grid,
        blue_base.0.saturating_sub(12),
        blue_base.1.saturating_sub(14),
        24,
        28,
    );
    clear_rect(
        &mut grid,
        red_base.0.saturating_sub(12),
        red_base.1.saturating_sub(14),
        24,
        28,
    );

    // Clear 6-tile radius around each zone center
    for &(cx, cy) in &zone_centers {
        clear_circle(&mut grid, cx, cy, 6);
    }

    // Gather points: base center (open rally zone where units congregate).
    let blue_gather = blue_base;
    let red_gather = red_base;

    let layout = MapLayout {
        blue_base,
        red_base,
        zone_centers,
        blue_gather,
        red_gather,
    };

    // Generate 2-tile-wide roads connecting bases through capture zones
    generate_roads(&mut grid, &layout);

    (grid, layout)
}

/// Clear a circular area to grass.
fn clear_circle(grid: &mut Grid, cx: u32, cy: u32, radius: i32) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dy * dy > radius * radius {
                continue;
            }
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if grid.in_bounds(nx, ny) {
                grid.set(nx as u32, ny as u32, TileKind::Grass);
                grid.set_elevation(nx as u32, ny as u32, 0);
                grid.set_decoration(nx as u32, ny as u32, None);
            }
        }
    }
}

/// Seed a boolean grid from simplex noise at a given offset and frequency.
fn seed_from_noise(
    noise: &Simplex,
    w: u32,
    h: u32,
    seed_offset: f64,
    frequency: f64,
    threshold: f64,
) -> Vec<bool> {
    let size = (w * h) as usize;
    let mut grid = vec![false; size];
    for y in 0..h {
        for x in 0..w {
            let val = noise.octave(
                x as f64 * frequency + seed_offset,
                y as f64 * frequency + seed_offset,
                3,
                0.5,
            );
            let normalized = (val + 1.0) * 0.5;
            grid[(y * w + x) as usize] = normalized < threshold;
        }
    }
    grid
}

/// Run cellular automaton iterations on a boolean grid.
fn run_cellular_automaton(
    initial: &[bool],
    w: u32,
    h: u32,
    iterations: u32,
    birth_threshold: u32,
    death_threshold: u32,
) -> Vec<bool> {
    let size = (w * h) as usize;
    let mut current = initial.to_vec();
    let mut next = vec![false; size];

    for _ in 0..iterations {
        for y in 0..h {
            for x in 0..w {
                let neighbors = count_neighbors(&current, w, h, x, y);
                let i = (y * w + x) as usize;
                next[i] = if current[i] {
                    neighbors >= death_threshold
                } else {
                    neighbors >= birth_threshold
                };
            }
        }
        std::mem::swap(&mut current, &mut next);
    }

    current
}

/// Count alive neighbors in a Moore neighborhood (8 surrounding cells).
fn count_neighbors(grid: &[bool], w: u32, h: u32, x: u32, y: u32) -> u32 {
    let mut count = 0;
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i32 + dx;
            let ny = y as i32 + dy;
            if nx >= 0 && ny >= 0 && (nx as u32) < w && (ny as u32) < h {
                if grid[(ny as u32 * w + nx as u32) as usize] {
                    count += 1;
                }
            } else {
                count += 1;
            }
        }
    }
    count
}

/// Clear a rectangular zone back to grass with zero elevation and no decorations.
fn clear_rect(grid: &mut Grid, x: u32, y: u32, w: u32, h: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let nx = x + dx;
            let ny = y + dy;
            if grid.in_bounds(nx as i32, ny as i32) {
                grid.set(nx, ny, TileKind::Grass);
                grid.set_elevation(nx, ny, 0);
                grid.set_decoration(nx, ny, None);
            }
        }
    }
}

/// Generate 2-tile-wide roads connecting objectives via a Minimum Spanning Tree.
/// Uses Kruskal's algorithm (sorted edges + union-find) to produce a plausible
/// tree-shaped road network, then routes each MST edge through A*.
fn generate_roads(grid: &mut Grid, layout: &MapLayout) {
    // Collect all objectives (bases + zone centers)
    let mut nodes: Vec<(u32, u32)> = Vec::with_capacity(layout.zone_centers.len() + 2);
    nodes.push(layout.blue_base);
    nodes.extend(layout.zone_centers.iter().copied());
    nodes.push(layout.red_base);

    let n = nodes.len();
    if n < 2 {
        return;
    }

    // All candidate edges sorted by Euclidean distance
    let mut edges: Vec<(usize, usize, u32)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = nodes[i].0 as i32 - nodes[j].0 as i32;
            let dy = nodes[i].1 as i32 - nodes[j].1 as i32;
            edges.push((i, j, (dx * dx + dy * dy) as u32));
        }
    }
    edges.sort_by_key(|e| e.2);

    // Kruskal's MST via union-find
    let mut parent: Vec<usize> = (0..n).collect();
    let find = |parent: &mut Vec<usize>, mut x: usize| -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]]; // path compression
            x = parent[x];
        }
        x
    };

    let mut mst_edges = Vec::with_capacity(n - 1);
    for (i, j, _) in &edges {
        let ri = find(&mut parent, *i);
        let rj = find(&mut parent, *j);
        if ri != rj {
            parent[ri] = rj;
            mst_edges.push((*i, *j));
            if mst_edges.len() == n - 1 {
                break;
            }
        }
    }

    // Add extra edges so every node has at least 2 connections (creates loops
    // for flanks that would otherwise be dead-ends in the MST).
    let mut degree = vec![0u32; n];
    for &(i, j) in &mst_edges {
        degree[i] += 1;
        degree[j] += 1;
    }
    let mst_set: std::collections::HashSet<(usize, usize)> = mst_edges.iter().copied().collect();
    for &(i, j, _) in &edges {
        if mst_set.contains(&(i, j)) || mst_set.contains(&(j, i)) {
            continue;
        }
        if degree[i] < 2 || degree[j] < 2 {
            mst_edges.push((i, j));
            degree[i] += 1;
            degree[j] += 1;
        }
    }

    // Route each edge via A* and paint the road
    for (i, j) in mst_edges {
        if let Some(path) = road_astar(grid, nodes[i], nodes[j]) {
            paint_road_path(grid, &path);
        }
    }

    // Enforce 1-tile grass border: clear forest/rock adjacent to road tiles
    let w = grid.width;
    let h = grid.height;
    let mut to_clear: Vec<(u32, u32)> = Vec::new();
    for gy in 0..h {
        for gx in 0..w {
            if grid.get(gx, gy) != TileKind::Road {
                continue;
            }
            for &(dx, dy) in &[
                (-1i32, -1i32),
                (0, -1),
                (1, -1),
                (-1, 0),
                (1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ] {
                let nx = gx as i32 + dx;
                let ny = gy as i32 + dy;
                if !grid.in_bounds(nx, ny) {
                    continue;
                }
                let ux = nx as u32;
                let uy = ny as u32;
                let tile = grid.get(ux, uy);
                if tile == TileKind::Forest || tile == TileKind::Rock || grid.elevation(ux, uy) > 0
                {
                    to_clear.push((ux, uy));
                }
            }
        }
    }
    for (x, y) in to_clear {
        if grid.get(x, y) != TileKind::Road {
            grid.set(x, y, TileKind::Grass);
        }
        grid.set_decoration(x, y, None);
        grid.set_elevation(x, y, 0);
    }
}

/// A* pathfinding for road generation. Prefers existing roads (zero extra cost)
/// and avoids water/elevation. Forest tiles are traversable but expensive.
fn road_astar(grid: &Grid, from: (u32, u32), to: (u32, u32)) -> Option<Vec<(u32, u32)>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let w = grid.width;
    let h = grid.height;
    let size = (w * h) as usize;
    let idx = |x: u32, y: u32| (y * w + x) as usize;

    let (sx, sy) = from;
    let (gx, gy) = to;

    let mut g_score = vec![u32::MAX; size];
    let mut came_from = vec![u32::MAX; size];
    g_score[idx(sx, sy)] = 0;

    // Octile heuristic (admissible with cardinal=2, diagonal=3, min tile cost=1)
    let heuristic = |x: u32, y: u32| -> u32 {
        let dx = (x as i32 - gx as i32).unsigned_abs();
        let dy = (y as i32 - gy as i32).unsigned_abs();
        let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
        min * 3 + (max - min) * 2
    };

    // Cardinal=2, Diagonal=3
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

    let mut open: BinaryHeap<Reverse<(u32, u32, u32)>> = BinaryHeap::new();
    open.push(Reverse((heuristic(sx, sy), sx, sy)));

    while let Some(Reverse((_, x, y))) = open.pop() {
        if x == gx && y == gy {
            // Reconstruct path
            let mut path = vec![(gx, gy)];
            let mut ci = idx(gx, gy);
            while ci != idx(sx, sy) {
                let cx = (ci as u32) % w;
                let cy = (ci as u32) / w;
                if path.last() != Some(&(cx, cy)) {
                    path.push((cx, cy));
                }
                ci = came_from[ci] as usize;
            }
            path.push((sx, sy));
            path.reverse();
            return Some(path);
        }

        let g = g_score[idx(x, y)];
        if g == u32::MAX {
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
            // Skip impassable (water) only; elevated terrain is traversable but costly
            if grid.get(nx, ny) == TileKind::Water {
                continue;
            }
            // Diagonal corner-cutting check
            if !grid.can_move_diagonal(x, y, dx, dy) {
                continue;
            }
            // Tile cost: existing road=1 (free merge), grass=3, forest=8, elevation=12
            // Tiles adjacent to water get a penalty so the 2×2 road stamp has room
            let base_cost = match grid.get(nx, ny) {
                TileKind::Road => 1,
                TileKind::Forest => 8,
                _ => 3,
            };
            let near_water = [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)]
                .iter()
                .any(|&(ddx, ddy)| {
                    let wx = nx as i32 + ddx;
                    let wy = ny as i32 + ddy;
                    grid.in_bounds(wx, wy) && grid.get(wx as u32, wy as u32) == TileKind::Water
                });
            let tile_cost = if grid.elevation(nx, ny) > 1 {
                12
            } else if near_water {
                15
            } else {
                base_cost
            };
            let new_g = g + tile_cost * dir_cost;
            let ni = idx(nx, ny);
            if new_g < g_score[ni] {
                g_score[ni] = new_g;
                came_from[ni] = idx(x, y) as u32;
                open.push(Reverse((new_g + heuristic(nx, ny), nx, ny)));
            }
        }
    }

    None // unreachable
}

/// Paint a 2-tile-wide road along an A*-generated path.
fn paint_road_path(grid: &mut Grid, path: &[(u32, u32)]) {
    for &(x, y) in path {
        // Stamp a 2×2 block
        for oy in 0..=1i32 {
            for ox in 0..=1i32 {
                let rx = x as i32 + ox;
                let ry = y as i32 + oy;
                if grid.in_bounds(rx, ry) {
                    let ux = rx as u32;
                    let uy = ry as u32;
                    let tile = grid.get(ux, uy);
                    if tile == TileKind::Grass || tile == TileKind::Forest || tile == TileKind::Rock
                    {
                        grid.set(ux, uy, TileKind::Road);
                        grid.set_decoration(ux, uy, None);
                        if grid.elevation(ux, uy) > 0 {
                            grid.set_elevation(ux, uy, 0);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let (g1, l1) = generate_battlefield(42);
        let (g2, l2) = generate_battlefield(42);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                assert_eq!(g1.get(x, y), g2.get(x, y));
                assert_eq!(g1.elevation(x, y), g2.elevation(x, y));
                assert_eq!(g1.decoration(x, y), g2.decoration(x, y));
            }
        }
        assert_eq!(l1.blue_base, l2.blue_base);
        assert_eq!(l1.red_base, l2.red_base);
        assert_eq!(l1.zone_centers, l2.zone_centers);
    }

    #[test]
    fn different_seeds_differ() {
        let (g1, _) = generate_battlefield(42);
        let (g2, _) = generate_battlefield(99);
        let mut differs = false;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if g1.get(x, y) != g2.get(x, y) || g1.elevation(x, y) != g2.elevation(x, y) {
                    differs = true;
                    break;
                }
            }
        }
        assert!(differs);
    }

    #[test]
    fn deployment_zones_clear() {
        let (grid, layout) = generate_battlefield(42);
        // Check 7-tile radius around each base center is clear
        for &(cx, cy) in &[layout.blue_base, layout.red_base] {
            for dy in 0..14i32 {
                for dx in 0..14i32 {
                    let x = cx.saturating_sub(7) + dx as u32;
                    let y = cy.saturating_sub(7) + dy as u32;
                    if grid.in_bounds(x as i32, y as i32) {
                        let tile = grid.get(x, y);
                        assert!(
                            tile == TileKind::Grass || tile == TileKind::Road,
                            "Unexpected tile {tile:?} at ({x},{y}) in deploy zone around ({cx},{cy})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn zone_centers_clear() {
        let (grid, layout) = generate_battlefield(42);
        for &(cx, cy) in &layout.zone_centers {
            for dy in -3i32..=3 {
                for dx in -3i32..=3 {
                    let x = (cx as i32 + dx) as u32;
                    let y = (cy as i32 + dy) as u32;
                    assert!(
                        grid.is_passable(x, y),
                        "Zone center ({cx},{cy}) blocked at ({x},{y}): {:?}",
                        grid.get(x, y)
                    );
                }
            }
        }
    }

    #[test]
    fn has_terrain_variety() {
        let (grid, _) = generate_battlefield(42);
        let mut has_forest = false;
        let mut has_water = false;
        let mut has_elevation = false;
        let mut has_bush = false;
        let mut has_rock = false;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                match grid.get(x, y) {
                    TileKind::Forest => has_forest = true,
                    TileKind::Water => has_water = true,
                    TileKind::Rock => has_rock = true,
                    _ => {}
                }
                if grid.elevation(x, y) > 0 {
                    has_elevation = true;
                }
                if grid.decoration(x, y) == Some(Decoration::Bush) {
                    has_bush = true;
                }
            }
        }
        assert!(has_forest, "No forests generated");
        assert!(has_water, "No water generated");
        assert!(has_elevation, "No elevated terrain generated");
        assert!(has_bush, "No bushes generated");
        assert!(has_rock, "No rocks generated");
    }

    #[test]
    fn elevation_from_noise() {
        let (grid, _) = generate_battlefield(42);
        let mut has_elev0 = false;
        let mut has_elev2 = false;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                match grid.elevation(x, y) {
                    0 => has_elev0 = true,
                    2 => has_elev2 = true,
                    _ => {}
                }
            }
        }
        assert!(has_elev0, "No elevation 0 tiles");
        assert!(has_elev2, "No elevation 2 tiles");
    }

    #[test]
    fn decorations_on_correct_terrain() {
        let (grid, _) = generate_battlefield(42);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                match grid.decoration(x, y) {
                    Some(Decoration::Bush) => {
                        assert_eq!(
                            grid.get(x, y),
                            TileKind::Grass,
                            "Bush on non-grass at ({x},{y})"
                        );
                        if !in_border(x, y) {
                            assert_eq!(
                                grid.elevation(x, y),
                                0,
                                "Bush on elevated tile in playable area at ({x},{y})"
                            );
                        }
                    }
                    Some(Decoration::WaterRock) => {
                        assert_eq!(
                            grid.get(x, y),
                            TileKind::Water,
                            "Water rock on non-water at ({x},{y})"
                        );
                    }
                    None => {}
                }
            }
        }
    }

    #[test]
    fn trees_form_clusters() {
        let (grid, _) = generate_battlefield(42);
        let mut clustered = 0;
        let mut total_forest = 0;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if grid.get(x, y) == TileKind::Forest {
                    total_forest += 1;
                    let has_neighbor =
                        [(-1i32, 0), (1, 0), (0, -1), (0, 1)]
                            .iter()
                            .any(|&(dx, dy)| {
                                let nx = x as i32 + dx;
                                let ny = y as i32 + dy;
                                grid.in_bounds(nx, ny)
                                    && grid.get(nx as u32, ny as u32) == TileKind::Forest
                            });
                    if has_neighbor {
                        clustered += 1;
                    }
                }
            }
        }
        assert!(total_forest > 0, "No forest tiles at all");
        let ratio = clustered as f32 / total_forest as f32;
        assert!(
            ratio > 0.6,
            "Trees not clustered enough: {clustered}/{total_forest} = {ratio:.2}"
        );
    }

    #[test]
    fn rng_f32_in_range() {
        let mut rng = Rng::new(12345);
        for _ in 0..1000 {
            let v = rng.f32();
            assert!((0.0..1.0).contains(&v));
        }
    }

    #[test]
    fn bsp_produces_leaves() {
        let mut rng = Rng::new(42);
        let rect = Rect {
            x: 16,
            y: 16,
            w: 128,
            h: 128,
        };
        let leaves = bsp_split(&mut rng, rect, 0, 4, 20);
        assert!(
            leaves.len() >= 4,
            "BSP should produce at least 4 leaves, got {}",
            leaves.len()
        );
        assert!(
            leaves.len() <= 16,
            "BSP should produce at most 16 leaves, got {}",
            leaves.len()
        );
    }

    #[test]
    fn bsp_bases_at_opposing_corners() {
        let (_, layout) = generate_battlefield(42);
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let mid = b + p / 2;
        // Blue base should be in top-left quadrant
        assert!(
            layout.blue_base.0 < mid,
            "Blue base x={} should be < {mid}",
            layout.blue_base.0
        );
        assert!(
            layout.blue_base.1 < mid,
            "Blue base y={} should be < {mid}",
            layout.blue_base.1
        );
        // Red base should be in bottom-right quadrant
        assert!(
            layout.red_base.0 > mid,
            "Red base x={} should be > {mid}",
            layout.red_base.0
        );
        assert!(
            layout.red_base.1 > mid,
            "Red base y={} should be > {mid}",
            layout.red_base.1
        );
    }

    #[test]
    fn layout_has_five_zones() {
        let (_, layout) = generate_battlefield(42);
        assert_eq!(
            layout.zone_centers.len(),
            5,
            "Should have exactly 5 zones (3 diagonal + 2 flanks)"
        );
    }

    #[test]
    fn border_has_vegetation() {
        let (grid, _) = generate_battlefield(42);
        let mut border_forest = 0u32;
        let mut border_rock = 0u32;
        let mut border_bush = 0u32;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if !in_border(x, y) {
                    continue;
                }
                match grid.get(x, y) {
                    TileKind::Forest => border_forest += 1,
                    TileKind::Rock => border_rock += 1,
                    _ => {}
                }
                if grid.decoration(x, y) == Some(Decoration::Bush) {
                    border_bush += 1;
                }
            }
        }
        assert!(border_forest > 0, "Border should have forest tiles");
        assert!(border_rock > 0, "Border should have rock tiles");
        assert!(border_bush > 0, "Border should have bush decorations");
    }

    #[test]
    fn cellular_automaton_produces_change() {
        let w = 20;
        let h = 20;
        let initial: Vec<bool> = (0..w * h).map(|i| i % 3 == 0).collect();
        let result = run_cellular_automaton(&initial, w as u32, h as u32, 5, 4, 2);
        assert_ne!(initial, result, "CA should modify the grid");
    }
}
