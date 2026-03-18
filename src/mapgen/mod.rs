pub mod simplex;

use crate::building;
use crate::grid::{Decoration, Grid, TileKind, BORDER_SIZE, GRID_SIZE, PLAYABLE_SIZE};
use crate::zone::ZoneManager;
use simplex::Simplex;

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

    /// Random u32 in [0, max).
    fn range(&mut self, max: u32) -> u32 {
        self.next() % max
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

/// Configuration for battlefield generation.
pub struct TerrainConfig {
    /// Noise frequency — controls feature size (~14-tile features at 0.07).
    pub elevation_scale: f64,
    /// Noise values below this become water.
    pub water_threshold: f64,
    /// Noise values above this become impassable hills (elevation 2).
    pub hill_threshold: f64,
    /// Initial seed density for tree cellular automata.
    pub tree_density: f64,
    /// Initial seed density for bush cellular automata.
    pub bush_density: f64,
    /// Initial seed density for rock cellular automata.
    pub rock_density: f64,
    /// Noise frequency for rock seeding.
    pub rock_frequency: f64,
    /// Fraction of water tiles that get decorative rocks.
    pub water_rock_density: f32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            elevation_scale: 0.07,
            water_threshold: -0.3,
            hill_threshold: 0.45,
            tree_density: 0.3,
            bush_density: 0.25,
            rock_density: 0.20,
            rock_frequency: 0.10,
            water_rock_density: 0.12,
        }
    }
}

/// Returns true if the tile is within the border ring (outside the playable area).
fn in_border(x: u32, y: u32) -> bool {
    x < BORDER_SIZE
        || y < BORDER_SIZE
        || x >= BORDER_SIZE + PLAYABLE_SIZE
        || y >= BORDER_SIZE + PLAYABLE_SIZE
}

/// Generate a procedural battlefield grid.
pub fn generate_battlefield(seed: u32) -> Grid {
    generate_battlefield_with_config(seed, &TerrainConfig::default())
}

/// Generate a battlefield with custom configuration.
pub fn generate_battlefield_with_config(seed: u32, config: &TerrainConfig) -> Grid {
    let mut rng = Rng::new(seed);
    let w = GRID_SIZE;
    let h = GRID_SIZE;
    let mut grid = Grid::new_grass(w, h);
    let noise = Simplex::new(seed as u64);

    // --- Phase A: Simplex noise heightmap with edge elevation bias ---
    for y in 0..h {
        for x in 0..w {
            let val = noise.octave(
                x as f64 * config.elevation_scale,
                y as f64 * config.elevation_scale,
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

            if effective_val < config.water_threshold {
                grid.set(x, y, TileKind::Water);
            } else if effective_val > config.hill_threshold {
                grid.set_elevation(x, y, 2);
            }
            // else: remains Grass, elevation 0
        }
    }

    // --- Phase B: Cellular automata for vegetation & decorations ---

    // Trees: seeded from simplex noise at offset, placed on grass
    // In playable area: only on elev 0. In border: also on elevated tiles.
    let tree_seed = run_cellular_automaton(
        &seed_from_noise(&noise, w, h, 100.0, 0.12, config.tree_density),
        w,
        h,
        5,
        4,
        2,
    );
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if tree_seed[i]
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
            {
                grid.set(x, y, TileKind::Forest);
            }
        }
    }

    // Rocks: seeded from simplex noise at offset, placed on grass
    // In playable area: only on elev 0. In border: also on elevated tiles.
    let rock_seed = run_cellular_automaton(
        &seed_from_noise(&noise, w, h, 200.0, config.rock_frequency, config.rock_density),
        w,
        h,
        3,
        4,
        2,
    );
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if rock_seed[i]
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
            {
                grid.set(x, y, TileKind::Rock);
            }
        }
    }

    // Bushes: decoration on grass (not Forest/Rock)
    // In playable area: only on elev 0. In border: also on elevated tiles.
    let bush_seed = run_cellular_automaton(
        &seed_from_noise(&noise, w, h, 300.0, 0.10, config.bush_density),
        w,
        h,
        4,
        4,
        1,
    );
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if bush_seed[i]
                && grid.get(x, y) == TileKind::Grass
                && (grid.elevation(x, y) == 0 || in_border(x, y))
            {
                grid.set_decoration(x, y, Some(Decoration::Bush));
            }
        }
    }

    // Water rocks: simple random chance on water tiles
    for y in 0..h {
        for x in 0..w {
            if grid.get(x, y) == TileKind::Water && rng.chance(config.water_rock_density) {
                grid.set_decoration(x, y, Some(Decoration::WaterRock));
            }
        }
    }

    // --- Phase C: Post-processing ---

    // Clear deployment zones (diagonal corners offset into playable area)
    let b = BORDER_SIZE;
    let p = PLAYABLE_SIZE;
    // Blue base: top-left 12x12 of playable area
    clear_zone(&mut grid, b, b, 12, 12);
    // Red base: bottom-right 12x12 of playable area
    clear_zone(&mut grid, b + p - 12, b + p - 12, 12, 12);

    // Ensure a clear diagonal corridor from base to base
    ensure_diagonal_path(&mut grid, &mut rng);

    // Clear 5x5 areas around capture zone centers so units can always reach them
    clear_zone_centers(&mut grid);

    // Clear 5x5 areas around building footprints
    clear_building_footprints(&mut grid);

    grid
}

/// Seed a boolean grid from simplex noise at a given offset and frequency.
/// Cells where noise > threshold become true.
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
            // Map noise [-1,1] to [0,1] then compare to threshold
            let normalized = (val + 1.0) * 0.5;
            grid[(y * w + x) as usize] = normalized < threshold;
        }
    }
    grid
}

/// Run cellular automaton iterations on a boolean grid.
/// birth_threshold: number of alive neighbors to birth a dead cell.
/// death_threshold: number of alive neighbors below which a live cell dies.
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
                // Out-of-bounds counts as alive (encourages growth at edges)
                count += 1;
            }
        }
    }
    count
}

/// Clear a rectangular zone back to grass with zero elevation and no decorations.
fn clear_zone(grid: &mut Grid, x: u32, y: u32, w: u32, h: u32) {
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

/// Ensure there's a passable diagonal path from top-left base to bottom-right base.
/// Clears a 3-tile-wide corridor along the diagonal, offset by BORDER_SIZE.
fn ensure_diagonal_path(grid: &mut Grid, rng: &mut Rng) {
    let start = BORDER_SIZE + 10;
    let end = BORDER_SIZE + PLAYABLE_SIZE - 11;
    let steps = end - start;
    for i in 0..=steps {
        let cx = start + i;
        let cy = start + i;
        let wobble = (rng.range(3) as i32) - 1; // -1, 0, or 1
        for d in -1i32..=1 {
            let x = (cx as i32 + d + wobble).clamp(0, grid.width as i32 - 1) as u32;
            let y = (cy as i32 + d + wobble).clamp(0, grid.height as i32 - 1) as u32;
            let tile = grid.get(x, y);
            if tile == TileKind::Water || tile == TileKind::Rock || tile == TileKind::Forest {
                grid.set(x, y, TileKind::Grass);
            }
            grid.set_elevation(x, y, 0);
            grid.set_decoration(x, y, None);
        }
    }
}

/// Clear a 5x5 area around each capture zone center for tower footprint and surroundings.
fn clear_zone_centers(grid: &mut Grid) {
    for (cx, cy) in ZoneManager::default_zone_centers() {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if grid.in_bounds(nx, ny) {
                    let (ux, uy) = (nx as u32, ny as u32);
                    let tile = grid.get(ux, uy);
                    if tile == TileKind::Water || tile == TileKind::Rock || tile == TileKind::Forest {
                        grid.set(ux, uy, TileKind::Grass);
                    }
                    grid.set_elevation(ux, uy, 0);
                    grid.set_decoration(ux, uy, None);
                }
            }
        }
    }
}

/// Clear 5x5 areas around each building footprint and rally point.
fn clear_building_footprints(grid: &mut Grid) {
    let mut positions = building::all_building_positions();
    positions.extend(building::all_rally_positions());
    for (cx, cy) in positions {
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if grid.in_bounds(nx, ny) {
                    let (ux, uy) = (nx as u32, ny as u32);
                    let tile = grid.get(ux, uy);
                    if tile == TileKind::Water || tile == TileKind::Rock || tile == TileKind::Forest {
                        grid.set(ux, uy, TileKind::Grass);
                    }
                    grid.set_elevation(ux, uy, 0);
                    grid.set_decoration(ux, uy, None);
                }
            }
        }
    }
}

/// Suggested player spawn position (Blue base, top-left of playable area).
pub fn blue_spawn_area() -> (u32, u32) {
    (BORDER_SIZE + 5, BORDER_SIZE + 5) // (21, 21)
}

/// Suggested enemy spawn area center (Red base, bottom-right of playable area).
pub fn red_spawn_area() -> (u32, u32) {
    (BORDER_SIZE + PLAYABLE_SIZE - 6, BORDER_SIZE + PLAYABLE_SIZE - 6) // (106, 106)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_generation() {
        let g1 = generate_battlefield(42);
        let g2 = generate_battlefield(42);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                assert_eq!(g1.get(x, y), g2.get(x, y));
                assert_eq!(g1.elevation(x, y), g2.elevation(x, y));
                assert_eq!(g1.decoration(x, y), g2.decoration(x, y));
            }
        }
    }

    #[test]
    fn different_seeds_differ() {
        let g1 = generate_battlefield(42);
        let g2 = generate_battlefield(99);
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
        let grid = generate_battlefield(42);
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        // Blue base: 12x12 at (b, b)
        for y in b..b + 12 {
            for x in b..b + 12 {
                assert_eq!(
                    grid.get(x, y),
                    TileKind::Grass,
                    "Non-grass at ({x},{y}) in Blue deploy zone"
                );
                assert_eq!(
                    grid.elevation(x, y),
                    0,
                    "Non-zero elevation at ({x},{y}) in Blue deploy zone"
                );
                assert_eq!(
                    grid.decoration(x, y),
                    None,
                    "Decoration at ({x},{y}) in Blue deploy zone"
                );
            }
        }
        // Red base: 12x12 at (b+p-12, b+p-12)
        for y in (b + p - 12)..(b + p) {
            for x in (b + p - 12)..(b + p) {
                assert_eq!(
                    grid.get(x, y),
                    TileKind::Grass,
                    "Non-grass at ({x},{y}) in Red deploy zone"
                );
                assert_eq!(
                    grid.elevation(x, y),
                    0,
                    "Non-zero elevation at ({x},{y}) in Red deploy zone"
                );
                assert_eq!(
                    grid.decoration(x, y),
                    None,
                    "Decoration at ({x},{y}) in Red deploy zone"
                );
            }
        }
    }

    #[test]
    fn diagonal_path_passable() {
        let grid = generate_battlefield(42);
        let start = BORDER_SIZE + 10;
        let end = BORDER_SIZE + PLAYABLE_SIZE - 11;
        // Diagonal corridor should have passable tiles
        for i in start..=end {
            let passable = (i.saturating_sub(2)..=i + 2).any(|d| {
                let x = d.min(GRID_SIZE - 1);
                let y = d.min(GRID_SIZE - 1);
                grid.is_passable(x, y)
            });
            assert!(
                passable,
                "No passable tile near diagonal at step {i}"
            );
        }
    }

    #[test]
    fn has_terrain_variety() {
        let grid = generate_battlefield(42);
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
        let grid = generate_battlefield(42);
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
        let grid = generate_battlefield(42);
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                match grid.decoration(x, y) {
                    Some(Decoration::Bush) => {
                        assert_eq!(
                            grid.get(x, y),
                            TileKind::Grass,
                            "Bush on non-grass at ({x},{y})"
                        );
                        // In playable area bushes are only on elev 0;
                        // in the border ring they can be on elevated tiles too
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
        let grid = generate_battlefield(42);
        // Count forest tiles that have at least one forest neighbor
        let mut clustered = 0;
        let mut total_forest = 0;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if grid.get(x, y) == TileKind::Forest {
                    total_forest += 1;
                    let has_neighbor = [(-1i32, 0), (1, 0), (0, -1), (0, 1)]
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
        // At least 60% of forest tiles should be clustered (CA produces groups)
        let ratio = clustered as f32 / total_forest as f32;
        assert!(
            ratio > 0.6,
            "Trees not clustered enough: {clustered}/{total_forest} = {ratio:.2}"
        );
    }

    #[test]
    fn rng_range_in_bounds() {
        let mut rng = Rng::new(12345);
        for _ in 0..1000 {
            let v = rng.range(10);
            assert!(v < 10);
        }
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
    fn spawn_areas_in_deployment_zones() {
        let b = BORDER_SIZE;
        let p = PLAYABLE_SIZE;
        let (bx, by) = blue_spawn_area();
        assert!(bx >= b && bx < b + 12, "Blue spawn x should be in top-left zone");
        assert!(by >= b && by < b + 12, "Blue spawn y should be in top-left zone");
        let (rx, ry) = red_spawn_area();
        assert!(rx >= b + p - 12 && rx < b + p, "Red spawn x should be in bottom-right zone");
        assert!(ry >= b + p - 12 && ry < b + p, "Red spawn y should be in bottom-right zone");
    }

    #[test]
    fn zone_centers_clear() {
        let grid = generate_battlefield(42);
        for (cx, cy) in crate::zone::ZoneManager::default_zone_centers() {
            for dy in -2i32..=2 {
                for dx in -2i32..=2 {
                    let x = (cx as i32 + dx) as u32;
                    let y = (cy as i32 + dy) as u32;
                    assert!(
                        grid.is_passable(x, y),
                        "Zone center ({cx},{cy}) blocked at ({x},{y}): {:?}",
                        grid.get(x, y)
                    );
                    assert_eq!(
                        grid.elevation(x, y),
                        0,
                        "Zone center ({cx},{cy}) has elevation at ({x},{y})"
                    );
                    assert_eq!(
                        grid.decoration(x, y),
                        None,
                        "Zone center ({cx},{cy}) has decoration at ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn border_has_vegetation() {
        let grid = generate_battlefield(42);
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
