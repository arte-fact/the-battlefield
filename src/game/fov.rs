use super::*;

/// Player vision radius in tiles.
const FOV_RADIUS: i32 = 15;

impl Game {
    /// Recompute field of view from the player's position using recursive shadowcasting.
    pub fn compute_fov(&mut self) {
        let w = self.grid.width;
        let h = self.grid.height;

        // Clear current visibility
        for v in self.visible.iter_mut() {
            *v = false;
        }

        // Collect grid positions of all alive friendly (Blue) units
        let friendly_positions: Vec<(i32, i32)> = self
            .units
            .iter()
            .filter(|u| u.alive && u.faction == Faction::Blue)
            .map(|u| {
                let (gx, gy) = u.grid_cell();
                (gx as i32, gy as i32)
            })
            .collect();

        // Compute vision from each friendly unit
        for (ox, oy) in &friendly_positions {
            let idx = (*oy as u32 * w + *ox as u32) as usize;
            self.visible[idx] = true;

            for octant in 0..8 {
                self.cast_light(*ox, *oy, FOV_RADIUS, 1, 1.0, 0.0, octant, w, h);
            }
        }

        self.fog_dirty = true;
    }

    /// Bresenham grid raycast: returns true if no intermediate tile blocks light.
    /// Skips the start and end tiles (units stand on them).
    pub(super) fn has_line_of_sight(&self, x1: f32, y1: f32, x2: f32, y2: f32) -> bool {
        let (gx1, gy1) = grid::world_to_grid(x1, y1);
        let (gx2, gy2) = grid::world_to_grid(x2, y2);

        let mut cx = gx1;
        let mut cy = gy1;
        let dx = (gx2 - gx1).abs();
        let dy = -(gy2 - gy1).abs();
        let sx: i32 = if gx1 < gx2 { 1 } else { -1 };
        let sy: i32 = if gy1 < gy2 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            // Skip start and end tiles
            if (cx != gx1 || cy != gy1) && (cx != gx2 || cy != gy2) {
                if !self.grid.in_bounds(cx, cy) {
                    return false;
                }
                if self.blocks_light(cx as u32, cy as u32) {
                    return false;
                }
            }
            if cx == gx2 && cy == gy2 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                cx += sx;
            }
            if e2 <= dx {
                err += dx;
                cy += sy;
            }
        }
        true
    }

    /// Returns true if the tile at (x, y) blocks line of sight.
    fn blocks_light(&self, x: u32, y: u32) -> bool {
        let tile = self.grid.get(x, y);
        match tile {
            TileKind::Water => false,
            TileKind::Forest => true,
            _ => self.grid.elevation(x, y) >= 2,
        }
    }

    /// Recursive shadowcasting for one octant.
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

                let dist_sq = dx * dx + dy * dy;
                if dist_sq <= radius * radius {
                    let idx = (map_y as u32 * w + map_x as u32) as usize;
                    self.visible[idx] = true;
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
                    self.cast_light(ox, oy, radius, j + 1, start_slope, l_slope, octant, w, h);
                    next_start_slope = r_slope;
                }
            }
            if blocked {
                return;
            }
        }
    }

    /// Pre-compute water adjacency for all land tiles (for foam rendering).
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fov_player_tile_visible() {
        let mut game = Game::new(960.0, 640.0);
        // Place in center of playable area
        let c = GRID_SIZE / 2;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, c, c, true);
        game.compute_fov();
        let idx = (c * GRID_SIZE + c) as usize;
        assert!(game.visible[idx]);
        assert!(game.revealed[idx]);
    }

    #[test]
    fn fov_nearby_tiles_visible() {
        let mut game = Game::new(960.0, 640.0);
        let c = GRID_SIZE / 2;
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, c, c, true);
        game.compute_fov();
        for &(dx, dy) in &[(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
            let x = (c as i32 + dx) as u32;
            let y = (c as i32 + dy) as u32;
            let idx = (y * GRID_SIZE + x) as usize;
            assert!(game.visible[idx], "Tile ({x},{y}) should be visible");
        }
    }

    #[test]
    fn fov_far_tiles_not_visible() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 20, 20, true);
        game.compute_fov();
        let idx = (80 * GRID_SIZE + 80) as usize;
        assert!(!game.visible[idx]);
    }

    #[test]
    fn has_los_open_field() {
        let game = Game::new(960.0, 640.0);
        let (x1, y1) = grid::grid_to_world(5, 5);
        let (x2, y2) = grid::grid_to_world(10, 5);
        assert!(
            game.has_line_of_sight(x1, y1, x2, y2),
            "Open grass should not block LOS"
        );
    }

    #[test]
    fn has_los_blocked_by_forest() {
        let mut game = Game::new(960.0, 640.0);
        game.grid.set(7, 5, TileKind::Forest);
        let (x1, y1) = grid::grid_to_world(5, 5);
        let (x2, y2) = grid::grid_to_world(10, 5);
        assert!(
            !game.has_line_of_sight(x1, y1, x2, y2),
            "Forest should block LOS"
        );
    }
}
