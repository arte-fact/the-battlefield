use crate::grid::{Grid, TileKind, GRID_SIZE};

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
    /// Number of hill clusters.
    pub hill_clusters: u32,
    /// Number of forest patches.
    pub forest_patches: u32,
    /// Number of water bodies.
    pub water_bodies: u32,
    /// Number of rock clusters.
    pub rock_clusters: u32,
}

impl Default for TerrainConfig {
    fn default() -> Self {
        Self {
            hill_clusters: 4,
            forest_patches: 5,
            water_bodies: 2,
            rock_clusters: 3,
        }
    }
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

    // Define deployment zones (left 10 cols = blue, right 10 cols = red)
    // Keep these mostly clear for unit placement.
    let deploy_left = 10;
    let deploy_right = w - 10;

    // Place hill clusters in the contested middle area
    for _ in 0..config.hill_clusters {
        let cx = rng.range(deploy_right - deploy_left - 6) + deploy_left + 3;
        let cy = rng.range(h - 10) + 5;
        let radius = rng.range(3) + 2; // 2-4 tile radius
        place_cluster(&mut grid, &mut rng, cx, cy, radius, TileKind::Hill, 0.7);
    }

    // Place forest patches — some in middle, some on flanks
    for _ in 0..config.forest_patches {
        let cx = rng.range(w - 8) + 4;
        let cy = rng.range(h - 8) + 4;
        let radius = rng.range(3) + 2;
        place_cluster(&mut grid, &mut rng, cx, cy, radius, TileKind::Forest, 0.6);
    }

    // Place water bodies — elongated horizontal or vertical
    for _ in 0..config.water_bodies {
        let cx = rng.range(deploy_right - deploy_left - 10) + deploy_left + 5;
        let cy = rng.range(h - 16) + 8;
        let is_horizontal = rng.chance(0.5);
        if is_horizontal {
            let length = rng.range(6) + 4; // 4-9 tiles
            let width = rng.range(2) + 2; // 2-3 tiles
            place_rect(&mut grid, cx.saturating_sub(length / 2), cy, length, width, TileKind::Water);
        } else {
            let length = rng.range(6) + 4;
            let width = rng.range(2) + 2;
            place_rect(&mut grid, cx, cy.saturating_sub(length / 2), width, length, TileKind::Water);
        }
    }

    // Place rock clusters — small impassable obstacles
    for _ in 0..config.rock_clusters {
        let cx = rng.range(w - 8) + 4;
        let cy = rng.range(h - 8) + 4;
        let radius = rng.range(2) + 1; // 1-2 tile radius
        place_cluster(&mut grid, &mut rng, cx, cy, radius, TileKind::Rock, 0.5);
    }

    // Clear deployment zones — ensure armies can deploy
    clear_zone(&mut grid, 0, 0, deploy_left, h);
    clear_zone(&mut grid, deploy_right, 0, w - deploy_right, h);

    // Ensure a clear corridor through the center for the main engagement
    ensure_center_path(&mut grid, &mut rng);

    grid
}

/// Place a roughly circular cluster of tiles.
fn place_cluster(
    grid: &mut Grid,
    rng: &mut Rng,
    cx: u32,
    cy: u32,
    radius: u32,
    kind: TileKind,
    density: f32,
) {
    let r = radius as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let nx = cx as i32 + dx;
            let ny = cy as i32 + dy;
            if !grid.in_bounds(nx, ny) {
                continue;
            }
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= r * r && rng.chance(density) {
                grid.set(nx as u32, ny as u32, kind);
            }
        }
    }
}

/// Place a rectangular area of tiles.
fn place_rect(grid: &mut Grid, x: u32, y: u32, w: u32, h: u32, kind: TileKind) {
    for dy in 0..h {
        for dx in 0..w {
            let nx = x + dx;
            let ny = y + dy;
            if grid.in_bounds(nx as i32, ny as i32) {
                grid.set(nx, ny, kind);
            }
        }
    }
}

/// Clear a rectangular zone back to grass (for deployment areas).
fn clear_zone(grid: &mut Grid, x: u32, y: u32, w: u32, h: u32) {
    for dy in 0..h {
        for dx in 0..w {
            let nx = x + dx;
            let ny = y + dy;
            if grid.in_bounds(nx as i32, ny as i32) {
                grid.set(nx, ny, TileKind::Grass);
            }
        }
    }
}

/// Ensure there's a passable path through the center of the battlefield.
/// Clears a 3-tile-wide corridor at the vertical center.
fn ensure_center_path(grid: &mut Grid, rng: &mut Rng) {
    let center_y = grid.height / 2;
    // Clear a winding path through the center rows
    for x in 0..grid.width {
        let wobble = (rng.range(3) as i32) - 1; // -1, 0, or 1
        for dy in -1..=1 {
            let y = (center_y as i32 + dy + wobble).clamp(0, grid.height as i32 - 1) as u32;
            let tile = grid.get(x, y);
            if tile == TileKind::Water || tile == TileKind::Rock {
                grid.set(x, y, TileKind::Grass);
            }
        }
    }
}

/// Suggested player spawn position (left deployment zone center).
pub fn blue_spawn_area() -> (u32, u32) {
    (5, GRID_SIZE / 2)
}

/// Suggested enemy spawn area center (right deployment zone).
pub fn red_spawn_area() -> (u32, u32) {
    (GRID_SIZE - 6, GRID_SIZE / 2)
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
                if g1.get(x, y) != g2.get(x, y) {
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
        // Left deployment zone (first 10 columns) should be all grass
        for y in 0..GRID_SIZE {
            for x in 0..10 {
                assert_eq!(
                    grid.get(x, y),
                    TileKind::Grass,
                    "Non-grass at ({x},{y}) in left deploy zone"
                );
            }
        }
        // Right deployment zone (last 10 columns)
        for y in 0..GRID_SIZE {
            for x in (GRID_SIZE - 10)..GRID_SIZE {
                assert_eq!(
                    grid.get(x, y),
                    TileKind::Grass,
                    "Non-grass at ({x},{y}) in right deploy zone"
                );
            }
        }
    }

    #[test]
    fn center_path_passable() {
        let grid = generate_battlefield(42);
        let center_y = GRID_SIZE / 2;
        // At least one of the center rows should be passable across the entire width
        for x in 0..GRID_SIZE {
            let passable = (center_y.saturating_sub(2)..=center_y + 2)
                .any(|y| grid.is_passable(x, y));
            assert!(
                passable,
                "No passable tile near center at column {x}"
            );
        }
    }

    #[test]
    fn has_terrain_variety() {
        let grid = generate_battlefield(42);
        let mut has_hill = false;
        let mut has_forest = false;
        let mut has_water = false;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                match grid.get(x, y) {
                    TileKind::Hill => has_hill = true,
                    TileKind::Forest => has_forest = true,
                    TileKind::Water => has_water = true,
                    _ => {}
                }
            }
        }
        assert!(has_hill, "No hills generated");
        assert!(has_forest, "No forests generated");
        assert!(has_water, "No water generated");
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
        let (bx, by) = blue_spawn_area();
        assert!(bx < 10);
        assert!(by < GRID_SIZE);
        let (rx, ry) = red_spawn_area();
        assert!(rx >= GRID_SIZE - 10);
        assert!(ry < GRID_SIZE);
    }
}
