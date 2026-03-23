mod ai;
mod ai_movement;
mod authority;
mod combat;
mod fov;
mod movement;
mod orders;
mod player;
mod setup;

use crate::animation::TurnEvent;
use crate::building::{self, BaseBuilding};
use crate::camera::Camera;
use crate::combat as crate_combat;
use crate::flowfield::FactionFlowState;
use crate::grid::{self, Grid, TileKind, GRID_SIZE, TILE_SIZE};
use crate::mapgen;
use crate::particle::{Particle, Projectile};
use crate::player_input::PlayerInput;
use crate::unit::{
    Facing, Faction, OrderKind, Unit, UnitAnim, UnitId, UnitKind, MELEE_RANGE, UNIT_RADIUS,
};
use crate::zone::{ZoneManager, ZoneState, MAX_UNITS_PER_FACTION};
use std::collections::HashSet;

pub use orders::ORDER_FLASH_DURATION;
pub use player::ATTACK_CONE_HALF_ANGLE;

pub struct Game {
    pub grid: Grid,
    pub units: Vec<Unit>,
    pub camera: Camera,
    pub particles: Vec<Particle>,
    pub projectiles: Vec<Projectile>,
    next_unit_id: UnitId,
    /// Tiles currently visible to the player this turn.
    pub visible: Vec<bool>,
    /// Tiles that have been seen at least once (revealed through fog).
    pub revealed: Vec<bool>,
    /// Set to true when FOV changes; renderer clears it after updating fog cache.
    pub fog_dirty: bool,
    /// Pre-computed: true if land tile is adjacent to water (for foam rendering).
    pub water_adjacency: Vec<bool>,
    /// Turn events recorded during game logic for animation playback.
    pub turn_events: Vec<TurnEvent>,
    /// Last grid cell where FOV was computed (optimization: skip if unchanged).
    pub last_fov_cell: (u32, u32),
    /// Player aim direction in radians (0 = right). Updated from movement input.
    pub player_aim_dir: f32,
    /// Strategic objective for Blue faction (world-space coords of Red spawn).
    pub blue_objective: (f32, f32),
    /// Strategic objective for Red faction (world-space coords of Blue spawn).
    pub red_objective: (f32, f32),
    /// Production buildings at faction bases.
    pub buildings: Vec<BaseBuilding>,
    /// Capture zone manager.
    pub zone_manager: ZoneManager,
    /// Set when a faction wins (holds all zones for VICTORY_HOLD_TIME).
    pub winner: Option<Faction>,
    /// Reinforcement wave timers per faction [Blue, Red].
    reinforce_timer: [f32; 2],
    /// Pre-computed occupied grid cells for AI pathfinding (rebuilt each frame).
    ai_occupied_cache: HashSet<(u32, u32)>,
    /// Flow field for Blue faction objective marching.
    blue_flow: FactionFlowState,
    /// Flow field for Red faction objective marching.
    red_flow: FactionFlowState,
    /// Player authority level (0..100), governing order radius, follow chance, and rank.
    pub authority: f32,
}

impl Game {
    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut camera = Camera::new(viewport_w, viewport_h);
        let center = GRID_SIZE as f32 * TILE_SIZE * 0.5;
        camera.x = center;
        camera.y = center;

        let size = (GRID_SIZE * GRID_SIZE) as usize;
        Self {
            grid,
            units: Vec::new(),
            camera,
            particles: Vec::new(),
            projectiles: Vec::new(),
            next_unit_id: 1,
            visible: vec![false; size],
            revealed: vec![true; size],
            fog_dirty: true,
            water_adjacency: vec![false; size],
            turn_events: Vec::new(),
            last_fov_cell: (0, 0),
            player_aim_dir: 0.0,
            blue_objective: (0.0, 0.0),
            red_objective: (0.0, 0.0),
            buildings: Vec::new(),
            zone_manager: ZoneManager::empty(),
            winner: None,
            reinforce_timer: [0.0; 2],
            ai_occupied_cache: HashSet::new(),
            blue_flow: FactionFlowState::new(),
            red_flow: FactionFlowState::new(),
            authority: 0.0,
        }
    }

    /// Tick all alive units' cooldowns by dt seconds.
    pub fn tick_cooldowns(&mut self, dt: f32) {
        for unit in &mut self.units {
            if unit.alive {
                unit.tick_cooldowns(dt);
            }
        }
    }

    /// Update animations, particles, projectiles, death fades, and camera following.
    pub fn update(&mut self, dt: f64) {
        for unit in &mut self.units {
            if unit.alive {
                unit.animation.update(dt);
            } else if unit.death_fade > 0.0 {
                unit.death_fade = (unit.death_fade - dt as f32).max(0.0);
                unit.animation.update(dt);
            }
        }

        for particle in &mut self.particles {
            particle.update(dt);
        }
        self.particles.retain(|p| !p.finished);

        for proj in &mut self.projectiles {
            proj.update(dt as f32);
        }

        // Apply damage on arrow impact
        for proj in &self.projectiles {
            if proj.finished && proj.damage > 0 {
                if let Some(idx) = self.find_unit_near(proj.target_x, proj.target_y, proj.faction) {
                    self.units[idx].take_damage(proj.damage);
                }
            }
        }
        self.projectiles.retain(|p| !p.finished);

        // Remove dead units whose death fade has completed (keep player corpse)
        self.units
            .retain(|u| u.alive || u.death_fade > 0.0 || u.is_player);

        // Camera smoothly follows player's world position
        if let Some(player) = self.player_unit() {
            let (pvx, pvy) = (player.x, player.y);
            let lerp = (dt as f32 * 5.0).min(1.0);
            self.camera.x += (pvx - self.camera.x) * lerp;
            self.camera.y += (pvy - self.camera.y) * lerp;
        }
        let world_size = GRID_SIZE as f32 * TILE_SIZE;
        self.camera.clamp_to_world(world_size, world_size);

        // Recompute FOV every frame (friendly units move continuously)
        self.compute_fov();
    }

    /// Run one simulation tick: process player input, AI, combat, physics, zones.
    ///
    /// Attack and order commands are deliberately excluded so the client can
    /// inspect their return values (e.g. for haptic feedback). Call
    /// `player_attack()` and `issue_order()` separately after this method.
    pub fn tick(&mut self, input: &PlayerInput, dt: f32) {
        if self.winner.is_some() {
            return;
        }

        let old_positions: Vec<(f32, f32)> = self.units.iter().map(|u| (u.x, u.y)).collect();

        self.tick_cooldowns(dt);
        self.tick_ai(dt);
        self.tick_zones(dt);
        self.tick_production(dt);

        // Player movement
        if input.move_x != 0.0 || input.move_y != 0.0 {
            if !input.attack_held {
                self.player_aim_dir = input.move_y.atan2(input.move_x);
            }
            self.try_player_move(input.move_x, input.move_y, dt);
        }

        // Player facing from aim (skip when attack held to lock facing)
        if !input.attack_held {
            let aim_cos = self.player_aim_dir.cos();
            if let Some(player) = self.player_unit_mut() {
                if aim_cos > 0.01 {
                    player.facing = Facing::Right;
                } else if aim_cos < -0.01 {
                    player.facing = Facing::Left;
                }
            }
        }

        self.resolve_collisions();
        self.update_movement_anims(&old_positions);
        self.tick_authority();
    }
}
