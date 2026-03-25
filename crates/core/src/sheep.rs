use crate::grid::{Grid, TILE_SIZE};
use crate::sprite::AnimationState;
use crate::unit::{Facing, Unit};

pub const SHEEP_FRAME_SIZE: u32 = 128;
pub const SHEEP_IDLE_FRAMES: u32 = 6;
pub const SHEEP_MOVE_FRAMES: u32 = 4;
pub const SHEEP_GRASS_FRAMES: u32 = 12;
/// Distance at which sheep start fleeing from a unit.
pub const SHEEP_FLEE_RADIUS: f32 = TILE_SIZE * 3.0;
/// Movement speed when fleeing (pixels/sec).
pub const SHEEP_FLEE_SPEED: f32 = 120.0;
/// Collision radius for terrain checks.
pub const SHEEP_RADIUS: f32 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheepState {
    Idle,
    Grazing,
    Fleeing,
}

pub struct Sheep {
    pub x: f32,
    pub y: f32,
    pub facing: Facing,
    pub state: SheepState,
    pub animation: AnimationState,
    pub state_timer: f32,
    pub vel_x: f32,
    pub vel_y: f32,
    rng_state: u32,
}

impl Sheep {
    pub fn new(x: f32, y: f32, seed: u32) -> Self {
        let mut s = Self {
            x,
            y,
            facing: Facing::Right,
            state: SheepState::Idle,
            animation: AnimationState::new(SHEEP_IDLE_FRAMES, 8.0),
            state_timer: 0.0,
            vel_x: 0.0,
            vel_y: 0.0,
            rng_state: if seed == 0 { 1 } else { seed },
        };
        // Randomize initial timer so sheep don't all transition at once
        s.state_timer = s.rand_range(2.0, 5.0);
        s
    }

    /// Simple xorshift32 PRNG.
    fn rand_u32(&mut self) -> u32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 17;
        self.rng_state ^= self.rng_state << 5;
        self.rng_state
    }

    fn rand_f32(&mut self) -> f32 {
        (self.rand_u32() & 0x00FF_FFFF) as f32 / 16_777_216.0
    }

    fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.rand_f32() * (hi - lo)
    }

    fn set_state(&mut self, state: SheepState) {
        self.state = state;
        let (frames, fps) = match state {
            SheepState::Idle => (SHEEP_IDLE_FRAMES, 8.0),
            SheepState::Grazing => (SHEEP_GRASS_FRAMES, 10.0),
            SheepState::Fleeing => (SHEEP_MOVE_FRAMES, 12.0),
        };
        self.animation = AnimationState::new(frames, fps);
    }

    /// Returns the frame count for the current animation (used by renderers).
    pub fn anim_frame_count(&self) -> u32 {
        match self.state {
            SheepState::Idle => SHEEP_IDLE_FRAMES,
            SheepState::Grazing => SHEEP_GRASS_FRAMES,
            SheepState::Fleeing => SHEEP_MOVE_FRAMES,
        }
    }

    /// Returns the sprite sheet index: 0=Idle, 1=Move, 2=Grass.
    pub fn sprite_index(&self) -> usize {
        match self.state {
            SheepState::Idle => 0,
            SheepState::Fleeing => 1,
            SheepState::Grazing => 2,
        }
    }

    pub fn update(&mut self, dt: f32, units: &[Unit], grid: &Grid) {
        // Check for nearby units — flee if too close
        let mut nearest_dist_sq = f32::MAX;
        let mut nearest_dx = 0.0_f32;
        let mut nearest_dy = 0.0_f32;
        for u in units {
            if !u.alive {
                continue;
            }
            let dx = self.x - u.x;
            let dy = self.y - u.y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < nearest_dist_sq {
                nearest_dist_sq = dist_sq;
                nearest_dx = dx;
                nearest_dy = dy;
            }
        }

        let flee_radius_sq = SHEEP_FLEE_RADIUS * SHEEP_FLEE_RADIUS;
        if nearest_dist_sq < flee_radius_sq && nearest_dist_sq > 0.01 {
            // Start or continue fleeing
            let dist = nearest_dist_sq.sqrt();
            self.vel_x = (nearest_dx / dist) * SHEEP_FLEE_SPEED;
            self.vel_y = (nearest_dy / dist) * SHEEP_FLEE_SPEED;
            if self.state != SheepState::Fleeing {
                self.set_state(SheepState::Fleeing);
                self.state_timer = self.rand_range(1.0, 2.0);
            }
        }

        match self.state {
            SheepState::Fleeing => {
                // Move away
                let new_x = self.x + self.vel_x * dt;
                let new_y = self.y + self.vel_y * dt;
                if grid.is_circle_passable(new_x, new_y, SHEEP_RADIUS) {
                    self.x = new_x;
                    self.y = new_y;
                } else {
                    // Hit impassable terrain — stop fleeing
                    self.vel_x = 0.0;
                    self.vel_y = 0.0;
                }

                // Update facing
                if self.vel_x > 0.5 {
                    self.facing = Facing::Right;
                } else if self.vel_x < -0.5 {
                    self.facing = Facing::Left;
                }

                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    self.vel_x = 0.0;
                    self.vel_y = 0.0;
                    self.set_state(SheepState::Idle);
                    self.state_timer = self.rand_range(2.0, 4.0);
                }
            }
            SheepState::Idle => {
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    self.set_state(SheepState::Grazing);
                    self.state_timer = self.rand_range(4.0, 8.0);
                }
            }
            SheepState::Grazing => {
                self.state_timer -= dt;
                if self.state_timer <= 0.0 {
                    self.set_state(SheepState::Idle);
                    self.state_timer = self.rand_range(3.0, 6.0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{Grid, GRID_SIZE};
    use crate::unit::{Faction, UnitKind};

    #[test]
    fn sheep_initializes_idle() {
        let sheep = Sheep::new(100.0, 100.0, 42);
        assert_eq!(sheep.state, SheepState::Idle);
        assert!(sheep.state_timer > 0.0);
    }

    #[test]
    fn sheep_flees_from_nearby_unit() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut sheep = Sheep::new(500.0, 500.0, 42);
        let unit = Unit::new(1, UnitKind::Warrior, Faction::Blue, 7, 7, false);
        // Unit at grid (7,7) = world (480, 480), ~28px away — well within flee radius
        sheep.update(0.016, &[unit], &grid);
        assert_eq!(sheep.state, SheepState::Fleeing);
        assert!(sheep.vel_x != 0.0 || sheep.vel_y != 0.0);
    }

    #[test]
    fn sheep_transitions_idle_to_grazing() {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut sheep = Sheep::new(500.0, 500.0, 42);
        sheep.state_timer = 0.01;
        sheep.update(0.1, &[], &grid);
        assert_eq!(sheep.state, SheepState::Grazing);
    }
}
