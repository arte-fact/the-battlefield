/// Tile types for the battlefield grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileKind {
    Grass,
    Hill,
    Water,
    Forest,
    Rock,
}

impl TileKind {
    pub fn movement_cost(self) -> Option<u32> {
        match self {
            TileKind::Grass => Some(1),
            TileKind::Hill => Some(2),
            TileKind::Forest => Some(2),
            TileKind::Water | TileKind::Rock => None, // impassable
        }
    }

    pub fn defense_bonus(self) -> i32 {
        match self {
            TileKind::Hill | TileKind::Forest => 1,
            _ => 0,
        }
    }
}

pub const GRID_SIZE: u32 = 64;

/// The battlefield grid: a 64x64 array of tiles.
pub struct Grid {
    tiles: Vec<TileKind>,
    pub width: u32,
    pub height: u32,
}

impl Grid {
    pub fn new_grass(width: u32, height: u32) -> Self {
        Self {
            tiles: vec![TileKind::Grass; (width * height) as usize],
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
        self.tiles[(y * self.width + x) as usize] = kind;
    }

    pub fn is_passable(&self, x: u32, y: u32) -> bool {
        self.get(x, y).movement_cost().is_some()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grass_grid() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        assert_eq!(grid.get(0, 0), TileKind::Grass);
        assert_eq!(grid.get(63, 63), TileKind::Grass);
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
        assert!(grid.in_bounds(63, 63));
        assert!(!grid.in_bounds(-1, 0));
        assert!(!grid.in_bounds(64, 0));
    }

    #[test]
    fn movement_costs() {
        assert_eq!(TileKind::Grass.movement_cost(), Some(1));
        assert_eq!(TileKind::Hill.movement_cost(), Some(2));
        assert_eq!(TileKind::Water.movement_cost(), None);
        assert_eq!(TileKind::Rock.movement_cost(), None);
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
}
