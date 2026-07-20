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
use crate::config::GameConfig;
use crate::flowfield::FactionFlowState;
use crate::grid::{self, Grid, TileKind, GRID_SIZE, TILE_SIZE};
use crate::mapgen;
use crate::particle::{Particle, ParticleKind, Projectile};
use crate::pawn::Pawn;
use crate::player_input::PlayerInput;
use crate::sheep::Sheep;
use crate::unit::{
    Facing, Faction, OrderKind, OrderOutcome, OrderRequest, Unit, UnitAnim, UnitId, UnitKind,
    MELEE_RANGE, UNIT_RADIUS,
};
use crate::zone::{ZoneManager, ZoneState};

pub use player::ATTACK_CONE_HALF_ANGLE;

/// Duration for floating authority text.
pub const FLOATING_TEXT_DURATION: f32 = 1.2;

/// Cell size for the per-frame spatial hash (pixels). 128px ≈ 2 tiles.
const SPATIAL_CELL: f32 = 128.0;

/// Lightweight spatial hash rebuilt each tick for O(1)-amortised neighbour queries.
/// Stores unit indices grouped by grid cell.
pub(crate) struct UnitSpatialGrid {
    cells: std::collections::HashMap<(i32, i32), Vec<usize>>,
}

impl UnitSpatialGrid {
    fn new() -> Self {
        Self {
            cells: std::collections::HashMap::new(),
        }
    }

    fn clear(&mut self) {
        for v in self.cells.values_mut() {
            v.clear();
        }
    }

    fn insert(&mut self, idx: usize, x: f32, y: f32) {
        let cx = (x / SPATIAL_CELL) as i32;
        let cy = (y / SPATIAL_CELL) as i32;
        self.cells.entry((cx, cy)).or_default().push(idx);
    }

    /// Return an iterator over unit indices within `radius` of `(x, y)`.
    /// Checks the minimal set of cells that could contain matches.
    fn query(&self, x: f32, y: f32, radius: f32) -> impl Iterator<Item = usize> + '_ {
        let r_cells = (radius / SPATIAL_CELL).ceil() as i32;
        let cx = (x / SPATIAL_CELL) as i32;
        let cy = (y / SPATIAL_CELL) as i32;
        (cy - r_cells..=cy + r_cells).flat_map(move |row| {
            (cx - r_cells..=cx + r_cells).flat_map(move |col| {
                self.cells
                    .get(&(col, row))
                    .map(|v| v.as_slice())
                    .unwrap_or(&[])
                    .iter()
                    .copied()
            })
        })
    }
}

/// A floating "+X" / "-X" authority indicator at a world position.
pub struct FloatingText {
    pub x: f32,
    pub y: f32,
    pub value: f32,
    pub remaining: f32,
}

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
    /// Rally point for Blue faction (grid coords, front-center of base).
    pub blue_gather: (u32, u32),
    /// Rally point for Red faction (grid coords, front-center of base).
    pub red_gather: (u32, u32),
    /// Production buildings at faction bases.
    pub buildings: Vec<BaseBuilding>,
    /// Ambient sheep at faction bases.
    pub sheep: Vec<Sheep>,
    /// Pawn workers at faction bases (one per house).
    pub pawns: Vec<Pawn>,
    /// Capture zone manager.
    pub zone_manager: ZoneManager,
    /// Banked peon deliveries per zone (fuels village production).
    pub village_stock: Vec<u8>,
    /// Per-zone garrison production timers.
    pub garrison_timer: Vec<f32>,
    /// Set when a faction wins (holds all zones for VICTORY_HOLD_TIME).
    pub winner: Option<Faction>,
    /// Remaining reinforcement manpower per army faction (ARMY_FACTIONS order).
    pub manpower: [f32; 4],
    /// Spawn queue per faction [Blue, Red] — units to spawn one-by-one.
    spawn_queue: [Vec<(UnitKind, Option<u8>)>; 4],
    /// Timer between individual unit spawns per army faction.
    spawn_timer: [f32; 4],
    /// Per-faction flag: skip rally_hold when dominating (all zones held).
    skip_rally: [bool; 4],
    /// Unified flow field for Blue faction (multi-source, all map objectives).
    blue_flow: FactionFlowState,
    /// Unified flow field for Red faction (multi-source, all map objectives).
    red_flow: FactionFlowState,
    /// Macro objectives per army faction: [(wx, wy, score); 3] each.
    pub macro_objectives: [Vec<(f32, f32, f32)>; 4],
    /// Sticky planner picks per faction: (defend, attack). A new candidate
    /// must clearly beat the current target before the army retargets.
    pub planner_targets: [(Option<u8>, Option<u8>); 4],
    /// Timer for periodic macro objective recomputation.
    objective_timer: f32,
    /// Timer for periodic auto-recruitment passes.
    recruit_timer: f32,
    /// Alternates each frame to stagger Blue/Red flow field updates.
    flow_field_turn: bool,
    /// Per-frame A* pathfind budget (reset each tick, decremented per find_path call).
    pub(crate) astar_budget: u8,
    /// Outcome of the most recent A* attempt: Some(found), None = deferred.
    pub(crate) last_path_result: Option<bool>,
    /// Rotates AI iteration order so the A* budget is shared fairly.
    ai_rotation: u32,
    /// Player authority level (0..100), governing order radius, follow chance, and rank.
    pub authority: f32,
    /// Runtime-tweakable AI configuration.
    pub config: GameConfig,
    /// Floating authority change indicators.
    pub floating_texts: Vec<FloatingText>,
    /// Command-radius pulse on order issue (seconds remaining).
    pub order_pulse: f32,
    /// Radius of the active order pulse in world pixels.
    pub order_pulse_radius: f32,
    /// Per-frame spatial hash of alive units (rebuilt in tick / update).
    pub(crate) spatial: UnitSpatialGrid,
    /// Frame counter for throttling expensive per-frame operations (e.g. FOV).
    fov_frame_counter: u8,
    /// Run scoring: player kills, zone captures witnessed, peak authority,
    /// seconds survived.
    pub score_kills: u32,
    pub score_zone_caps: u32,
    pub score_peak_authority: f32,
    pub survival_secs: f32,
}

impl Game {
    /// Army factions active in this battle: Blue + enemy_count rivals.
    pub fn active_factions(&self) -> &'static [Faction] {
        let n = (self.config.enemy_count.clamp(1, 3) as usize) + 1;
        &crate::unit::ARMY_FACTIONS[..n]
    }

    pub fn new(viewport_w: f32, viewport_h: f32) -> Self {
        let grid = Grid::new_grass(GRID_SIZE, GRID_SIZE);
        let mut camera = Camera::new(viewport_w, viewport_h);
        let center = GRID_SIZE as f32 * TILE_SIZE * 0.5;
        camera.x = center;
        camera.y = center;

        let size = (GRID_SIZE * GRID_SIZE) as usize;
        let config = GameConfig::default();
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
            blue_gather: (0, 0),
            red_gather: (0, 0),
            buildings: Vec::new(),
            sheep: Vec::new(),
            pawns: Vec::new(),
            zone_manager: ZoneManager::empty(),
            village_stock: Vec::new(),
            garrison_timer: Vec::new(),
            winner: None,
            manpower: [config.manpower_start; 4],
            spawn_queue: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            spawn_timer: [0.0; 4],
            skip_rally: [false; 4],
            blue_flow: FactionFlowState::new(),
            red_flow: FactionFlowState::new(),
            macro_objectives: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            planner_targets: [(None, None); 4],
            objective_timer: 0.0,
            recruit_timer: 0.0,
            flow_field_turn: false,
            astar_budget: 0,
            last_path_result: None,
            ai_rotation: 0,
            authority: 0.0,
            config,
            floating_texts: Vec::new(),
            order_pulse: 0.0,
            order_pulse_radius: 0.0,
            spatial: UnitSpatialGrid::new(),
            fov_frame_counter: 0,
            score_kills: 0,
            score_zone_caps: 0,
            score_peak_authority: 0.0,
            survival_secs: 0.0,
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

    /// Drain turn events and spawn visual particles (e.g. dust on movement).
    ///
    /// Call this between `tick()` and `update()` each frame.
    pub fn process_turn_events(&mut self) {
        for event in self.turn_events.drain(..) {
            if let TurnEvent::Move { from, .. } = event {
                self.particles
                    .push(Particle::new(ParticleKind::Dust, from.0, from.1));
            }
        }
    }

    /// Rebuild the spatial hash from all alive units. Call once per tick or update.
    pub(crate) fn rebuild_spatial(&mut self) {
        self.spatial.clear();
        for (i, u) in self.units.iter().enumerate() {
            if u.alive {
                self.spatial.insert(i, u.x, u.y);
            }
        }
    }

    /// Update animations, particles, projectiles, death fades, and camera following.
    pub fn update(&mut self, dt: f64) {
        // Rebuild spatial hash for projectile impact queries
        self.rebuild_spatial();

        for unit in &mut self.units {
            if unit.alive {
                unit.animation.update(dt);
            } else if unit.death_fade > 0.0 {
                unit.death_fade = (unit.death_fade - dt as f32).max(0.0);
                unit.animation.update(dt);
            }
        }

        // Build position lookup for follow-tracking (avoids O(n) scan per particle)
        let unit_positions: std::collections::HashMap<UnitId, (f32, f32)> = self
            .units
            .iter()
            .filter(|u| u.alive)
            .map(|u| (u.id, (u.x, u.y)))
            .collect();

        for particle in &mut self.particles {
            // Track following particles to their target unit
            if let Some(uid) = particle.follow_unit {
                if let Some(&(ux, uy)) = unit_positions.get(&uid) {
                    particle.world_x = ux;
                    particle.world_y = uy;
                } else {
                    particle.finished = true;
                }
            }
            particle.update(dt);
        }
        self.particles.retain(|p| !p.finished);

        // Float authority text upward and expire
        let dt_f = dt as f32;
        for ft in &mut self.floating_texts {
            ft.remaining -= dt_f;
            ft.y -= 30.0 * dt_f; // drift upward
        }
        self.floating_texts.retain(|ft| ft.remaining > 0.0);
        self.order_pulse = (self.order_pulse - dt_f).max(0.0);

        for sheep in &mut self.sheep {
            sheep.animation.update(dt);
        }
        for pawn in &mut self.pawns {
            pawn.animation.update(dt);
        }

        for proj in &mut self.projectiles {
            proj.update(dt as f32);
        }

        // Apply damage on arrow impact — prefer the original target if still alive and nearby
        for proj in &self.projectiles {
            if proj.finished && proj.damage > 0 {
                let hit_radius = TILE_SIZE * 0.75;
                let target_idx = proj
                    .target_unit
                    .and_then(|tid| {
                        self.units.iter().position(|u| {
                            u.id == tid
                                && u.alive
                                && u.distance_to_pos(proj.target_x, proj.target_y) <= hit_radius
                        })
                    })
                    .or_else(|| self.find_unit_near(proj.target_x, proj.target_y, proj.faction));
                if let Some(idx) = target_idx {
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
        let world_w = self.grid.width as f32 * TILE_SIZE;
        let world_h = self.grid.height as f32 * TILE_SIZE;
        self.camera.clamp_to_world(world_w, world_h);

        // Throttle FOV: recompute every 3rd frame (units don't move fast enough
        // for per-frame updates to matter visually, saves ~7k ops on other frames).
        self.fov_frame_counter = self.fov_frame_counter.wrapping_add(1);
        if self.fov_frame_counter.is_multiple_of(3) {
            self.compute_fov();
        }
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

        if self.is_player_alive() {
            self.survival_secs += dt;
        }
        if self.authority > self.score_peak_authority {
            self.score_peak_authority = self.authority;
        }

        self.tick_cooldowns(dt);
        self.tick_village_garrisons(dt);
        self.tick_ai(dt);
        self.tick_zones(dt);
        self.tick_production(dt);
        self.tick_building_combat(dt);

        // Player movement
        if input.move_x != 0.0 || input.move_y != 0.0 {
            if !input.aim_lock {
                self.player_aim_dir = input.move_y.atan2(input.move_x);
            }
            self.try_player_move(input.move_x, input.move_y, dt);
        }

        // Player facing from aim (skip when aim locked to preserve facing)
        if !input.aim_lock {
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
        self.tick_sheep(dt);
        self.tick_pawns(dt);
    }

    fn tick_sheep(&mut self, dt: f32) {
        let mut sheep = std::mem::take(&mut self.sheep);
        for s in &mut sheep {
            s.update(dt, &self.units, &self.grid);
        }
        self.sheep = sheep;
    }

    fn tick_pawns(&mut self, dt: f32) {
        let mut pawns = std::mem::take(&mut self.pawns);
        // Collect tiles already claimed by pawns (walking to or working)
        let claimed: Vec<(u32, u32)> = pawns.iter().filter_map(|p| p.claimed_target()).collect();
        let mut threats: Vec<(f32, f32)> = Vec::new();
        let sheep_tiles: Vec<(u32, u32)> = self
            .sheep
            .iter()
            .map(|s| {
                let (gx, gy) = grid::world_to_grid(s.x, s.y);
                (gx.max(0) as u32, gy.max(0) as u32)
            })
            .collect();
        for p in &mut pawns {
            if p.job == crate::pawn::PawnJob::Herd {
                let near_home: Vec<(u32, u32)> = sheep_tiles
                    .iter()
                    .copied()
                    .filter(|&(gx, gy)| {
                        let (wx, wy) = grid::grid_to_world(gx, gy);
                        let (dx, dy) = (wx - p.home_x, wy - p.home_y);
                        dx * dx + dy * dy < (TILE_SIZE * 12.0).powi(2)
                    })
                    .collect();
                p.set_herd_targets(near_home);
            }
            // A pawn panics near fighting units, or near enemies of its
            // owner (zone owner for village pawns, faction otherwise).
            let owner = match p.zone_id {
                None => Some(p.faction),
                Some(zid) => match self.zone_manager.zones.get(zid as usize).map(|z| z.state) {
                    Some(ZoneState::Controlled(f)) | Some(ZoneState::Capturing(f)) => Some(f),
                    _ => None,
                },
            };
            threats.clear();
            for idx in self
                .spatial
                .query(p.x, p.y, crate::pawn::PAWN_FLEE_RADIUS * 1.5)
            {
                let u = &self.units[idx];
                if !u.alive {
                    continue;
                }
                let fighting = u.current_anim.is_attack();
                let enemy = owner.is_some_and(|f| u.faction != f);
                if fighting || enemy {
                    threats.push((u.x, u.y));
                }
            }
            p.update(dt, &self.grid, &claimed, &threats);
            if p.delivered {
                p.delivered = false;
                if let Some(zid) = p.zone_id {
                    if let Some(stock) = self.village_stock.get_mut(zid as usize) {
                        *stock = (*stock + 1).min(self.config.village_stock_cap);
                    }
                }
            }
        }
        // Pawn-to-pawn collision: push overlapping pawns apart
        let radius = crate::pawn::PAWN_RADIUS;
        let min_dist = radius * 2.0;
        let min_dist_sq = min_dist * min_dist;
        for i in 0..pawns.len() {
            for j in (i + 1)..pawns.len() {
                let dx = pawns[j].x - pawns[i].x;
                let dy = pawns[j].y - pawns[i].y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < min_dist_sq && dist_sq > 0.01 {
                    let dist = dist_sq.sqrt();
                    let overlap = (min_dist - dist) * 0.5;
                    let nx = dx / dist;
                    let ny = dy / dist;
                    pawns[i].x -= nx * overlap;
                    pawns[i].y -= ny * overlap;
                    pawns[j].x += nx * overlap;
                    pawns[j].y += ny * overlap;
                }
            }
        }
        self.pawns = pawns;
    }
}
