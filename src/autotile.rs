use crate::grid::{Grid, TileKind};

// Cardinal direction bits for the 4-bit bitmask.
const N: u8 = 1;
const E: u8 = 2;
const S: u8 = 4;
const W: u8 = 8;

/// Compute 4-bit cardinal bitmask: which neighbors satisfy `is_same`.
/// Out-of-bounds neighbors are treated as "same" so grid edges blend seamlessly.
fn cardinal_mask(grid: &Grid, x: u32, y: u32, is_same: impl Fn(u32, u32) -> bool) -> u8 {
    let mut mask = 0u8;
    if y == 0 || is_same(x, y - 1) {
        mask |= N;
    }
    if x + 1 >= grid.width || is_same(x + 1, y) {
        mask |= E;
    }
    if y + 1 >= grid.height || is_same(x, y + 1) {
        mask |= S;
    }
    if x == 0 || is_same(x - 1, y) {
        mask |= W;
    }
    mask
}

// ---------------------------------------------------------------------------
// Flat Ground autotile (cols 0-4 of Tilemap_colorN.png)
// ---------------------------------------------------------------------------
//
// Tilemap layout (4x4 composed block, cols 0-3, rows 0-3):
//   3x3 border (cols 0-2, rows 0-2):
//     Row 0: TL corner, Top edge, TR corner
//     Row 1: Left edge, Center,   Right edge
//     Row 2: BL corner, Bot edge, BR corner
//   1x3 V-strip (col 3, rows 0-2): V-top, V-mid, V-bottom
//   3x1 H-strip (cols 0-2, row 3): H-left, H-mid, H-right
//   1x1 isolated (col 3, row 3): single isolated tile
//
// Cardinal bitmask (N=1,E=2,S=4,W=8) → tilemap (col, row):
const FLAT_GROUND: [(u32, u32); 16] = [
    (3, 3), //  0: isolated (no neighbors)
    (3, 2), //  1: N only → V-bottom
    (0, 3), //  2: E only → H-left
    (0, 2), //  3: N+E → BL corner
    (3, 0), //  4: S only → V-top
    (3, 1), //  5: N+S → V-mid
    (0, 0), //  6: E+S → TL corner
    (0, 1), //  7: N+E+S → Left edge
    (2, 3), //  8: W only → H-right
    (2, 2), //  9: N+W → BR corner
    (1, 3), // 10: E+W → H-mid
    (1, 2), // 11: N+E+W → Bottom edge
    (2, 0), // 12: S+W → TR corner
    (2, 1), // 13: N+S+W → Right edge
    (1, 0), // 14: E+S+W → Top edge
    (1, 1), // 15: all → Center fill
];

// ---------------------------------------------------------------------------
// Elevated Ground autotile (cols 5-8 of Tilemap_colorN.png)
// ---------------------------------------------------------------------------
//
// Same 4x4 composed pattern at cols 5-8, rows 0-3:
//   3x3 border (cols 5-7, rows 0-2), V-strip (col 8, rows 0-2),
//   H-strip (cols 5-7, row 3), isolated (col 8, row 3).
const ELEVATED_GROUND: [(u32, u32); 16] = [
    (8, 3), //  0: isolated
    (8, 2), //  1: N only → V-bottom
    (5, 3), //  2: E only → H-left
    (5, 2), //  3: N+E → BL corner
    (8, 0), //  4: S only → V-top
    (8, 1), //  5: N+S → V-mid
    (5, 0), //  6: E+S → TL corner
    (5, 1), //  7: N+E+S → Left edge
    (7, 3), //  8: W only → H-right
    (7, 2), //  9: N+W → BR corner
    (6, 3), // 10: E+W → H-mid
    (6, 2), // 11: N+E+W → Bottom edge
    (7, 0), // 12: S+W → TR corner
    (7, 1), // 13: N+S+W → Right edge
    (6, 0), // 14: E+S+W → Top edge
    (6, 1), // 15: all → Center fill
];

// ---------------------------------------------------------------------------
// Cliff faces (row 4, cols 5-8 of Tilemap_colorN.png)
// ---------------------------------------------------------------------------
//
// Cliff is drawn on the tile BELOW an elevated tile when that tile has
// lower elevation. Horizontal mask: which horizontal neighbors are also
// elevated at this level?
//   bit 0 = E neighbor elevated, bit 1 = W neighbor elevated
const CLIFF_LAND: [(u32, u32); 4] = [
    (7, 4), // 0: neither → isolated cliff
    (5, 4), // 1: E only → left cliff cap
    (8, 4), // 2: W only → right cliff cap
    (6, 4), // 3: both → center cliff
];

// Water-facing cliff (row 5, cols 5-8)
const CLIFF_WATER: [(u32, u32); 4] = [
    (7, 5), // 0: neither → isolated
    (5, 5), // 1: E only → left
    (8, 5), // 2: W only → right
    (6, 5), // 3: both → center
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns tilemap (col, row) for flat ground at the given grid position.
/// "Same" = any land tile that is not a road (not water, not road).
/// This makes grass tiles next to roads use the bordered edge tiles,
/// creating a natural grass-to-road transition using the same art as grass-to-water.
pub fn flat_ground_src(grid: &Grid, x: u32, y: u32) -> (u32, u32) {
    let tile = grid.get(x, y);
    if tile == TileKind::Road {
        // Road tiles: "same" = other road tiles (edges where road meets non-road)
        let mask = cardinal_mask(grid, x, y, |nx, ny| grid.get(nx, ny) == TileKind::Road);
        FLAT_GROUND[mask as usize]
    } else {
        // Non-road land tiles: treat road as "not same" so edges appear next to roads
        let mask = cardinal_mask(grid, x, y, |nx, ny| {
            let t = grid.get(nx, ny);
            t.is_land() && t != TileKind::Road
        });
        FLAT_GROUND[mask as usize]
    }
}

/// Returns tilemap (col, row) for elevated ground top surface.
/// "Same" = neighbor at same or higher elevation level.
pub fn elevated_top_src(grid: &Grid, x: u32, y: u32, level: u8) -> (u32, u32) {
    let mask = cardinal_mask(grid, x, y, |nx, ny| grid.elevation(nx, ny) >= level);
    ELEVATED_GROUND[mask as usize]
}

/// Returns tilemap (col, row) for the cliff face below an elevated tile,
/// or None if no cliff is visible (tile below is at same or higher elevation).
/// Uses land-facing or water-facing cliff depending on what's below.
pub fn cliff_src(grid: &Grid, x: u32, y: u32, level: u8) -> Option<(u32, u32)> {
    // Cliff only visible if the tile below (y+1) has lower elevation
    if y + 1 >= grid.height || grid.elevation(x, y + 1) >= level {
        return None;
    }

    // Horizontal mask: which horizontal neighbors are also elevated?
    let mut h_mask = 0u8;
    if x + 1 < grid.width && grid.elevation(x + 1, y) >= level {
        h_mask |= 1; // E
    }
    if x > 0 && grid.elevation(x - 1, y) >= level {
        h_mask |= 2; // W
    }

    // Choose land-facing or water-facing cliff
    let table = if grid.get(x, y + 1) == TileKind::Water {
        &CLIFF_WATER
    } else {
        &CLIFF_LAND
    };

    Some(table[h_mask as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid(width: u32, height: u32, tiles: &[(u32, u32, TileKind)]) -> Grid {
        let mut grid = Grid::new_grass(width, height);
        for &(x, y, kind) in tiles {
            grid.set(x, y, kind);
        }
        grid
    }

    #[test]
    fn isolated_grass_tile() {
        let grid = make_grid(3, 3, &[
            (0, 0, TileKind::Water), (1, 0, TileKind::Water), (2, 0, TileKind::Water),
            (0, 1, TileKind::Water), /* (1,1) Grass */        (2, 1, TileKind::Water),
            (0, 2, TileKind::Water), (1, 2, TileKind::Water), (2, 2, TileKind::Water),
        ]);
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (3, 3)); // isolated tile
    }

    #[test]
    fn center_fill() {
        let grid = Grid::new_grass(3, 3);
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (1, 1)); // center fill (all neighbors land)
    }

    #[test]
    fn boundary_treated_as_same() {
        let grid = Grid::new_grass(3, 3);
        // Corner tile at (0,0): N and W are out of bounds (treated as same),
        // E=(1,0)=Grass, S=(0,1)=Grass → all 4 same → center fill
        let (col, row) = flat_ground_src(&grid, 0, 0);
        assert_eq!((col, row), (1, 1));
    }

    #[test]
    fn top_edge() {
        // Water above, land on all other sides
        let grid = make_grid(3, 3, &[
            (0, 0, TileKind::Water), (1, 0, TileKind::Water), (2, 0, TileKind::Water),
        ]);
        // (1,1): N=water, E=grass, S=grass, W=grass → mask = E+S+W = 14 → top edge
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (1, 0));
    }

    #[test]
    fn tl_corner() {
        let grid = make_grid(3, 3, &[
            (0, 0, TileKind::Water), (1, 0, TileKind::Water), (2, 0, TileKind::Water),
            (0, 1, TileKind::Water),
            (0, 2, TileKind::Water),
        ]);
        // (1,1): N=water, E=grass, S=grass, W=water → mask = E+S = 6 → TL corner
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (0, 0));
    }

    #[test]
    fn horizontal_strip() {
        let grid = make_grid(5, 3, &[
            (0, 0, TileKind::Water), (1, 0, TileKind::Water), (2, 0, TileKind::Water),
            (3, 0, TileKind::Water), (4, 0, TileKind::Water),
            (0, 2, TileKind::Water), (1, 2, TileKind::Water), (2, 2, TileKind::Water),
            (3, 2, TileKind::Water), (4, 2, TileKind::Water),
        ]);
        // (2,1): N=water, E=grass, S=water, W=grass → mask = E+W = 10 → H-mid
        let (col, row) = flat_ground_src(&grid, 2, 1);
        assert_eq!((col, row), (1, 3));
    }

    #[test]
    fn cliff_when_lower_below() {
        let mut grid = Grid::new_grass(3, 3);
        grid.set_elevation(0, 0, 2);
        grid.set_elevation(1, 0, 2);
        grid.set_elevation(2, 0, 2);
        // row 1 stays at elevation 0
        let cliff = cliff_src(&grid, 1, 0, 2);
        assert!(cliff.is_some());
        let (_col, row) = cliff.unwrap();
        assert_eq!(row, 4); // land-facing cliff row
    }

    #[test]
    fn cliff_water_facing() {
        let mut grid = Grid::new_grass(3, 3);
        grid.set_elevation(1, 0, 2);
        grid.set(1, 1, TileKind::Water);
        grid.set_elevation(1, 1, 0);
        let cliff = cliff_src(&grid, 1, 0, 2);
        assert!(cliff.is_some());
        let (_, row) = cliff.unwrap();
        assert_eq!(row, 5); // water-facing cliff row
    }

    #[test]
    fn no_cliff_when_same_elevation_below() {
        let mut grid = Grid::new_grass(3, 3);
        grid.set_elevation(1, 0, 2);
        grid.set_elevation(1, 1, 2);
        assert!(cliff_src(&grid, 1, 0, 2).is_none());
    }

    #[test]
    fn grass_next_to_road_gets_edge() {
        // Road to the east of a grass tile → grass should get a right edge
        let grid = make_grid(3, 3, &[
            (2, 0, TileKind::Road), (2, 1, TileKind::Road), (2, 2, TileKind::Road),
        ]);
        // (1,1): N=grass, E=road, S=grass, W=grass → mask = N+S+W = 13 → Right edge
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (2, 1)); // Right edge
    }

    #[test]
    fn road_tile_autotile() {
        // Vertical road strip: road tiles see other road tiles as "same"
        let grid = make_grid(3, 3, &[
            (1, 0, TileKind::Road), (1, 1, TileKind::Road), (1, 2, TileKind::Road),
        ]);
        // (1,1): N=road, E=grass(not same), S=road, W=grass(not same) → mask = N+S = 5 → V-mid
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (3, 1)); // V-mid
    }

    #[test]
    fn road_does_not_count_as_same_for_grass() {
        // A grass tile surrounded by grass on 3 sides and road on 1 should NOT be center fill
        let grid = make_grid(3, 3, &[
            (1, 0, TileKind::Road),
        ]);
        // (1,1): N=road(not same), E=grass, S=grass, W=grass → mask = E+S+W = 14 → Top edge
        let (col, row) = flat_ground_src(&grid, 1, 1);
        assert_eq!((col, row), (1, 0)); // Top edge
    }
}
