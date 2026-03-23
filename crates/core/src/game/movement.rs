use super::*;

impl Game {
    /// Move a unit continuously in a direction with wall-sliding collision.
    /// Tries full diagonal → X-only → Y-only, matching `try_push()` pattern.
    /// If completely stuck, nudges toward tile center to escape corners.
    pub(super) fn move_unit(&mut self, idx: usize, dir_x: f32, dir_y: f32, dt: f32) {
        let speed = self.units[idx].move_speed()
            * self
                .grid
                .speed_factor_at(self.units[idx].x, self.units[idx].y)
                .max(0.25);
        let vx = dir_x * speed * dt;
        let vy = dir_y * speed * dt;

        let old_x = self.units[idx].x;
        let old_y = self.units[idx].y;

        // 1. Try full diagonal
        if self
            .grid
            .is_circle_passable(old_x + vx, old_y + vy, UNIT_RADIUS)
        {
            self.units[idx].x = old_x + vx;
            self.units[idx].y = old_y + vy;
        }
        // 2. Wall-slide X
        else if vx.abs() > 0.001 && self.grid.is_circle_passable(old_x + vx, old_y, UNIT_RADIUS) {
            self.units[idx].x = old_x + vx;
        }
        // 3. Wall-slide Y
        else if vy.abs() > 0.001 && self.grid.is_circle_passable(old_x, old_y + vy, UNIT_RADIUS) {
            self.units[idx].y = old_y + vy;
        }
        // 4. Completely stuck — nudge toward current tile center to escape corners
        else {
            let (gx, gy) = self.units[idx].grid_cell();
            let (cx, cy) = grid::grid_to_world(gx, gy);
            let nudge_x = (cx - old_x) * 0.1;
            let nudge_y = (cy - old_y) * 0.1;
            if (nudge_x.abs() > 0.01 || nudge_y.abs() > 0.01)
                && self
                    .grid
                    .is_circle_passable(old_x + nudge_x, old_y + nudge_y, UNIT_RADIUS)
            {
                self.units[idx].x = old_x + nudge_x;
                self.units[idx].y = old_y + nudge_y;
            }
        }

        // Update facing only when horizontal movement is dominant.
        // Prevents flickering when moving vertically with tiny vx oscillations.
        if !self.units[idx].is_player && vx.abs() > vy.abs() * 0.5 {
            if vx > 0.0 {
                self.units[idx].facing = Facing::Right;
            } else {
                self.units[idx].facing = Facing::Left;
            }
        }
    }

    /// Try to push a unit by (push_x, push_y). Falls back to axis-aligned sliding
    /// if the full push hits terrain.
    pub(super) fn try_push(grid: &crate::grid::Grid, unit: &mut Unit, push_x: f32, push_y: f32) {
        let ox = unit.x;
        let oy = unit.y;
        // Try full push
        if grid.is_circle_passable(ox + push_x, oy + push_y, UNIT_RADIUS) {
            unit.x = ox + push_x;
            unit.y = oy + push_y;
            return;
        }
        // Wall slide: try X only
        if push_x.abs() > 0.001 && grid.is_circle_passable(ox + push_x, oy, UNIT_RADIUS) {
            unit.x = ox + push_x;
            return;
        }
        // Wall slide: try Y only
        if push_y.abs() > 0.001 && grid.is_circle_passable(ox, oy + push_y, UNIT_RADIUS) {
            unit.y = oy + push_y;
        }
    }

    /// Resolve circle-circle collisions between all alive units.
    pub fn resolve_collisions(&mut self) {
        let min_dist = UNIT_RADIUS * 2.0;

        for i in 0..self.units.len() {
            if !self.units[i].alive {
                continue;
            }
            for j in (i + 1)..self.units.len() {
                if !self.units[j].alive {
                    continue;
                }
                let dx = self.units[j].x - self.units[i].x;
                let dy = self.units[j].y - self.units[i].y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < min_dist && dist > 0.001 {
                    let overlap = (min_dist - dist) / 2.0;
                    let nx = dx / dist;
                    let ny = dy / dist;

                    let strength = if self.units[i].faction == self.units[j].faction {
                        0.4
                    } else {
                        1.0
                    };
                    let push = overlap * strength;

                    let (left, right) = self.units.split_at_mut(j);
                    Self::try_push(&self.grid, &mut left[i], -nx * push, -ny * push);
                    Self::try_push(&self.grid, &mut right[0], nx * push, ny * push);
                }
            }
        }
    }

    /// Update run/idle animations based on whether units moved since last frame.
    pub fn update_movement_anims(&mut self, old_positions: &[(f32, f32)]) {
        for (i, unit) in self.units.iter_mut().enumerate() {
            if !unit.alive {
                continue;
            }
            if unit.current_anim == UnitAnim::Attack {
                if unit.attack_cooldown <= 0.0 {
                    unit.set_anim(UnitAnim::Idle);
                }
                continue;
            }
            if i < old_positions.len() {
                let (ox, oy) = old_positions[i];
                let moved = (unit.x - ox).abs() > 0.1 || (unit.y - oy).abs() > 0.1;
                if moved {
                    unit.set_anim(UnitAnim::Run);
                } else if unit.current_anim == UnitAnim::Run {
                    unit.set_anim(UnitAnim::Idle);
                }
            }
        }
    }

    /// Real-time player movement: continuous movement only.
    pub fn try_player_move(&mut self, dir_x: f32, dir_y: f32, dt: f32) {
        let player_idx = match self.units.iter().position(|u| u.is_player && u.alive) {
            Some(i) => i,
            None => return,
        };

        // Move
        self.move_unit(player_idx, dir_x, dir_y, dt);

        // FOV check: recompute only when crossing a tile boundary
        let new_cell = self.units[player_idx].grid_cell();
        if new_cell != self.last_fov_cell {
            self.last_fov_cell = new_cell;
            self.compute_fov();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_collisions_pushes_apart() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.spawn_unit(UnitKind::Warrior, Faction::Red, 5, 5, false);
        // Both at same position
        let (wx, wy) = grid::grid_to_world(5, 5);
        game.units[0].x = wx;
        game.units[0].y = wy;
        game.units[1].x = wx + 1.0; // slightly offset to avoid zero-distance
        game.units[1].y = wy;
        game.resolve_collisions();
        let dist = game.units[0].distance_to_unit(&game.units[1]);
        assert!(
            dist >= UNIT_RADIUS * 2.0 - 0.1,
            "Units should be pushed apart, dist={dist}"
        );
    }

    #[test]
    fn cooldowns_tick_down() {
        let mut game = Game::new(960.0, 640.0);
        game.spawn_unit(UnitKind::Warrior, Faction::Blue, 5, 5, true);
        game.units[0].attack_cooldown = 1.0;
        game.tick_cooldowns(0.3);
        assert!((game.units[0].attack_cooldown - 0.7).abs() < 0.001);
        game.tick_cooldowns(1.0);
        assert!(game.units[0].attack_cooldown.abs() < f32::EPSILON);
    }
}
