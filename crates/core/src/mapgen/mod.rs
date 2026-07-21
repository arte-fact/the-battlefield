pub mod simplex;
pub mod village;

pub use village::{SettlementSpec, VillageTheme};

use crate::grid::{Decoration, Grid, TileKind, BORDER_SIZE, PLAYABLE_SIZE};
use crate::zone::SettlementTier;
use simplex::Simplex;

/// Village layout ring around each zone center (tiles). Starts beyond
/// the capture circle (zone_radius 6) so the fighting ground stays
/// open and the village spreads around it.
pub const VILLAGE_RING_MIN: u32 = 7;
pub const VILLAGE_RING_MAX: u32 = 10;
/// Largest building footprint half-extent placed on the ring.
pub const VILLAGE_MAX_FOOTPRINT: u32 = 2;
/// Cleared ground radius required by a village.
pub const VILLAGE_CLEAR_RADIUS: u32 = VILLAGE_RING_MAX + VILLAGE_MAX_FOOTPRINT + 1;

/// BSP layout data returned from map generation.
pub struct MapLayout {
    pub blue_base: (u32, u32),
    pub red_base: (u32, u32),
    pub zone_centers: Vec<(u32, u32)>,
    /// Rally point for Blue: front-center of Blue base (toward battlefield).
    pub blue_gather: (u32, u32),
    /// Rally point for Red: front-center of Red base (toward battlefield).
    pub red_gather: (u32, u32),
    /// Zone indices that are "home" zones for Blue (always capturable by Blue).
    pub blue_home_zones: Vec<u8>,
    /// Zone indices that are "home" zones for Red (always capturable by Red).
    pub red_home_zones: Vec<u8>,
    /// Adjacency: connections[i] = list of zone indices connected to zone i.
    pub connections: Vec<Vec<u8>>,
    /// One village per zone, same order as `zone_centers`.
    pub settlements: Vec<SettlementSpec>,
    /// Provisional capitals for third/fourth factions (Yellow, Purple),
    /// placed at the remaining map corners until the settlement network
    /// (roadmap items 4-5) replaces base handling entirely.
    pub extra_bases: Vec<(u32, u32)>,
}

/// Simple xorshift32 PRNG for deterministic terrain generation.
pub(crate) struct Rng {
    state: u32,
}

impl Rng {
    pub(crate) fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    pub(crate) fn next(&mut self) -> u32 {
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

/// Generate a procedural battlefield grid (drains the budgeted pipeline).
pub fn generate_battlefield(seed: u32, playable_size: u32) -> (Grid, MapLayout) {
    generate_battlefield_n(seed, playable_size, 2)
}

/// Generate with `n_capitals` faction start positions (2-4).
pub fn generate_battlefield_n(seed: u32, playable_size: u32, n_capitals: u32) -> (Grid, MapLayout) {
    let mut job = MapGen::new(seed, playable_size, n_capitals);
    while !job.step() {}
    job.take_result()
}

/// Rough tile budget per generation step; row-banded stages size their
/// bands from this so one step stays well under a frame.
const STEP_TILES: u32 = 32_768;
/// A* expansions per step while routing a road: one long road at 1024²
/// would otherwise be a single 30+ ms chunk.
const ROAD_POPS_PER_STEP: u32 = 20_000;

enum GenStage {
    Heightmap { row: u32 },
    WaterScan { pass: u32, row: u32 },
    WaterApply { pass: u32 },
    TreeSeed { row: u32 },
    TreeCa { iter: u32, row: u32 },
    TreePlace { row: u32 },
    Rocks { row: u32 },
    Bushes { row: u32 },
    WaterRocks { row: u32 },
    Network,
    Clearing,
    RoadsPlan,
    RoadEdge { i: usize },
    RoadsBorder,
    Settlements,
    Done,
}

/// Budgeted map generation. The pipeline is a state machine advanced by
/// `step()`, each step bounded to roughly `STEP_TILES` tiles of work (or
/// one road A*), so a host can spread generation across frames behind a
/// loading bar. Draining it synchronously yields byte-identical output.
pub struct MapGen {
    seed: u32,
    playable: u32,
    n_capitals: u32,
    w: u32,
    h: u32,
    rows_per_step: u32,
    grid: Grid,
    rng: Rng,
    noise: Simplex,
    stage: GenStage,
    water_changes: Vec<(u32, u32, bool)>,
    tree_cur: Vec<bool>,
    tree_next: Vec<bool>,
    zone_centers: Vec<(u32, u32)>,
    tiers: Vec<SettlementTier>,
    capitals: Vec<(u32, u32)>,
    road_edges: Vec<(usize, usize)>,
    road_job: Option<RoadAstarJob>,
    result: Option<MapLayout>,
    steps_done: u32,
    steps_total: u32,
}

impl MapGen {
    pub fn new(seed: u32, playable_size: u32, n_capitals: u32) -> Self {
        let w = playable_size + 2 * BORDER_SIZE;
        let h = playable_size + 2 * BORDER_SIZE;
        let size = (w * h) as usize;
        let rows_per_step = (STEP_TILES / w).max(1);
        let bands = h.div_ceil(rows_per_step);
        // Band stages + fixed stages + a road-edge guess (corrected once
        // the network is planned). Only feeds the progress bar.
        let est_zones =
            ((playable_size as usize * playable_size as usize) / 3500).clamp(9, 28) as u32;
        let steps_total = bands * 13 + 3 + est_zones * 3 / 2 + 3;
        Self {
            seed,
            playable: playable_size,
            n_capitals,
            w,
            h,
            rows_per_step,
            grid: Grid::new_grass(w, h),
            rng: Rng::new(seed),
            noise: Simplex::new(seed as u64),
            stage: GenStage::Heightmap { row: 0 },
            water_changes: Vec::new(),
            tree_cur: vec![false; size],
            tree_next: vec![false; size],
            zone_centers: Vec::new(),
            tiers: Vec::new(),
            capitals: Vec::new(),
            road_edges: Vec::new(),
            road_job: None,
            result: None,
            steps_done: 0,
            steps_total,
        }
    }

    pub fn seed(&self) -> u32 {
        self.seed
    }

    pub fn is_done(&self) -> bool {
        matches!(self.stage, GenStage::Done)
    }

    /// Fraction complete, for the loading bar.
    pub fn progress(&self) -> f32 {
        if self.is_done() {
            return 1.0;
        }
        (self.steps_done as f32 / self.steps_total.max(1) as f32).min(0.99)
    }

    /// Grid and layout once `step()` has returned true.
    pub fn take_result(self) -> (Grid, MapLayout) {
        let layout = self.result.expect("MapGen::take_result before completion");
        (self.grid, layout)
    }

    /// Advance one bounded chunk of work. Returns true when generation is
    /// complete.
    pub fn step(&mut self) -> bool {
        self.steps_done += 1;
        let rps = self.rows_per_step;
        let (w, h) = (self.w, self.h);
        match self.stage {
            GenStage::Heightmap { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        let val = self.noise.octave(
                            x as f64 * ELEVATION_SCALE,
                            y as f64 * ELEVATION_SCALE,
                            4,
                            0.5,
                        );

                        // Distance from nearest edge, normalized (0 = edge)
                        let dx = (x as f64).min((w - 1 - x) as f64) / (BORDER_SIZE as f64);
                        let dy = (y as f64).min((h - 1 - y) as f64) / (BORDER_SIZE as f64);
                        let edge_dist = dx.min(dy).clamp(0.0, 1.0);

                        // Quadratic bias: 0 in playable center, ~1.0 at grid edge
                        let edge_bias = {
                            let t = 1.0 - edge_dist;
                            t * t
                        };

                        let effective_val = val + edge_bias;

                        if effective_val < WATER_THRESHOLD {
                            self.grid.set(x, y, TileKind::Water);
                        } else if effective_val > HILL_THRESHOLD {
                            self.grid.set_elevation(x, y, 2);
                        }
                        // else: remains Grass, elevation 0
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::WaterScan { pass: 0, row: 0 }
                } else {
                    GenStage::Heightmap { row: row + rps }
                };
            }
            // Smooth water with cellular automata to drop small isolated
            // chunks. birth=5: land floods only if 5+ of 8 neighbors are
            // water; death=3: water with fewer than 3 water neighbors dries.
            GenStage::WaterScan { pass, row } => {
                let y0 = row.max(1);
                let y1 = (row + rps).min(h - 1);
                for y in y0..y1 {
                    for x in 1..w - 1 {
                        let mut water_neighbors = 0u32;
                        for dy in -1i32..=1 {
                            for dx in -1i32..=1 {
                                if dx == 0 && dy == 0 {
                                    continue;
                                }
                                if self.grid.get((x as i32 + dx) as u32, (y as i32 + dy) as u32)
                                    == TileKind::Water
                                {
                                    water_neighbors += 1;
                                }
                            }
                        }
                        let is_water = self.grid.get(x, y) == TileKind::Water;
                        if is_water && water_neighbors < 3 {
                            self.water_changes.push((x, y, false)); // kill small water
                        } else if !is_water && water_neighbors >= 5 {
                            self.water_changes.push((x, y, true)); // fill gaps
                        }
                    }
                }
                self.stage = if row + rps >= h - 1 {
                    GenStage::WaterApply { pass }
                } else {
                    GenStage::WaterScan {
                        pass,
                        row: row + rps,
                    }
                };
            }
            GenStage::WaterApply { pass } => {
                for (x, y, make_water) in self.water_changes.drain(..) {
                    if make_water {
                        self.grid.set(x, y, TileKind::Water);
                        self.grid.set_elevation(x, y, 0);
                        self.grid.set_decoration(x, y, None);
                    } else {
                        self.grid.set(x, y, TileKind::Grass);
                    }
                }
                self.stage = if pass + 1 < 3 {
                    GenStage::WaterScan {
                        pass: pass + 1,
                        row: 0,
                    }
                } else {
                    GenStage::TreeSeed { row: 0 }
                };
            }
            // Trees: seeded from simplex noise at offset, grown by CA.
            GenStage::TreeSeed { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        let val = self.noise.octave(
                            x as f64 * 0.07 + 100.0,
                            y as f64 * 0.07 + 100.0,
                            3,
                            0.5,
                        );
                        let normalized = (val + 1.0) * 0.5;
                        self.tree_cur[(y * w + x) as usize] = normalized < TREE_DENSITY;
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::TreeCa { iter: 0, row: 0 }
                } else {
                    GenStage::TreeSeed { row: row + rps }
                };
            }
            GenStage::TreeCa { iter, row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        let neighbors = count_neighbors(&self.tree_cur, w, h, x, y);
                        let i = (y * w + x) as usize;
                        // birth >= 4 neighbors, survive >= 2
                        self.tree_next[i] = if self.tree_cur[i] {
                            neighbors >= 2
                        } else {
                            neighbors >= 4
                        };
                    }
                }
                self.stage = if row + rps >= h {
                    std::mem::swap(&mut self.tree_cur, &mut self.tree_next);
                    if iter + 1 < 5 {
                        GenStage::TreeCa {
                            iter: iter + 1,
                            row: 0,
                        }
                    } else {
                        GenStage::TreePlace { row: 0 }
                    }
                } else {
                    GenStage::TreeCa {
                        iter,
                        row: row + rps,
                    }
                };
            }
            GenStage::TreePlace { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        let i = (y * w + x) as usize;
                        if self.tree_cur[i]
                            && self.grid.get(x, y) == TileKind::Grass
                            && (self.grid.elevation(x, y) == 0 || in_border(x, y))
                            && !near_cliff(&self.grid, x, y)
                        {
                            self.grid.set(x, y, TileKind::Forest);
                        }
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::Rocks { row: 0 }
                } else {
                    GenStage::TreePlace { row: row + rps }
                };
            }
            // Rocks: sparse random scatter on grass
            GenStage::Rocks { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        if self.rng.chance(ROCK_DENSITY as f32)
                            && self.grid.get(x, y) == TileKind::Grass
                            && (self.grid.elevation(x, y) == 0 || in_border(x, y))
                            && !near_cliff(&self.grid, x, y)
                        {
                            self.grid.set(x, y, TileKind::Rock);
                        }
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::Bushes { row: 0 }
                } else {
                    GenStage::Rocks { row: row + rps }
                };
            }
            // Bushes: sparse random scatter on grass
            GenStage::Bushes { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        if self.rng.chance(BUSH_DENSITY as f32)
                            && self.grid.get(x, y) == TileKind::Grass
                            && (self.grid.elevation(x, y) == 0 || in_border(x, y))
                            && !near_cliff(&self.grid, x, y)
                        {
                            self.grid.set_decoration(x, y, Some(Decoration::Bush));
                        }
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::WaterRocks { row: 0 }
                } else {
                    GenStage::Bushes { row: row + rps }
                };
            }
            // Water rocks: simple random chance on water tiles
            GenStage::WaterRocks { row } => {
                for y in row..(row + rps).min(h) {
                    for x in 0..w {
                        if self.grid.get(x, y) == TileKind::Water
                            && self.rng.chance(WATER_ROCK_DENSITY)
                        {
                            self.grid.set_decoration(x, y, Some(Decoration::WaterRock));
                        }
                    }
                }
                self.stage = if row + rps >= h {
                    GenStage::Network
                } else {
                    GenStage::WaterRocks { row: row + rps }
                };
            }
            GenStage::Network => {
                self.plan_network();
                self.stage = GenStage::Clearing;
            }
            GenStage::Clearing => {
                // Clear settlement ground by tier.
                for (i, &(cx, cy)) in self.zone_centers.iter().enumerate() {
                    let r = if self.tiers[i] == SettlementTier::City {
                        crate::building::BASE_BAND_RADIUS + 4
                    } else {
                        VILLAGE_CLEAR_RADIUS as i32
                    };
                    clear_circle(&mut self.grid, cx, cy, r);
                }
                self.stage = GenStage::RoadsPlan;
            }
            GenStage::RoadsPlan => {
                self.road_edges = plan_road_edges(&self.zone_centers);
                // Correct the progress denominator now the edge count is real.
                self.steps_total = self.steps_done + self.road_edges.len() as u32 + 2;
                self.stage = GenStage::RoadEdge { i: 0 };
            }
            // Route roads with a bounded A* expansion budget per step;
            // long edges span several steps and resume where they left off.
            GenStage::RoadEdge { i } => {
                if let Some(&(a, b)) = self.road_edges.get(i) {
                    if self.road_job.is_none() {
                        self.road_job = Some(RoadAstarJob::new(
                            &self.grid,
                            self.zone_centers[a],
                            self.zone_centers[b],
                        ));
                    }
                    let job = self.road_job.as_mut().expect("created above");
                    match job.run(&self.grid, ROAD_POPS_PER_STEP) {
                        None => return self.is_done(), // still routing this edge
                        Some(path) => {
                            self.road_job = None;
                            if let Some(path) = path {
                                paint_road_path(&mut self.grid, &path);
                            }
                        }
                    }
                }
                self.stage = if i + 1 < self.road_edges.len() {
                    GenStage::RoadEdge { i: i + 1 }
                } else {
                    GenStage::RoadsBorder
                };
            }
            GenStage::RoadsBorder => {
                clear_road_borders(&mut self.grid);
                self.stage = GenStage::Settlements;
            }
            GenStage::Settlements => {
                let mut connections: Vec<Vec<u8>> = vec![Vec::new(); self.zone_centers.len()];
                for &(i, j) in &self.road_edges {
                    connections[i].push(j as u8);
                    connections[j].push(i as u8);
                }

                let mut layout = MapLayout {
                    blue_base: self.capitals[0],
                    red_base: self.capitals[1],
                    zone_centers: std::mem::take(&mut self.zone_centers),
                    blue_gather: self.capitals[0],
                    red_gather: self.capitals[1],
                    blue_home_zones: Vec::new(),
                    red_home_zones: Vec::new(),
                    connections,
                    settlements: Vec::new(),
                    extra_bases: self.capitals[2..].to_vec(),
                };

                layout.settlements = village::plan_settlements(
                    &mut self.grid,
                    &layout.zone_centers,
                    &self.tiers,
                    self.seed,
                );
                self.result = Some(layout);
                self.stage = GenStage::Done;
            }
            GenStage::Done => {}
        }
        self.is_done()
    }

    /// Settlement network: capitals by farthest-point sampling, the
    /// countryside by best-candidate sampling (spacing + terrain-damage
    /// scoring). Zone ids: capitals first (0..n), then the countryside.
    fn plan_network(&mut self) {
        let b = BORDER_SIZE;
        let p = self.playable;
        let grid = &self.grid;
        let mut net_rng = Rng::new(self.seed.wrapping_add(0xC17E));

        let margin_city = crate::building::BASE_BAND_RADIUS as u32 + 4;
        let margin_field = VILLAGE_CLEAR_RADIUS;

        let sample = |rng: &mut Rng, margin: u32| -> (u32, u32) {
            let span = p - 2 * margin;
            (
                b + margin + rng.next() % span,
                b + margin + rng.next() % span,
            )
        };
        let d2 = |a: (u32, u32), c: (u32, u32)| -> i64 {
            let dx = a.0 as i64 - c.0 as i64;
            let dy = a.1 as i64 - c.1 as i64;
            dx * dx + dy * dy
        };

        // Capitals: greedy farthest-point over a shared candidate pool,
        // terrain-damage penalized so cities avoid lakes and cliff fields.
        let n_capitals = self.n_capitals.clamp(2, 4) as usize;
        let pool: Vec<(u32, u32)> = (0..48).map(|_| sample(&mut net_rng, margin_city)).collect();
        let dmg = |c: (u32, u32)| terrain_damage(grid, c.0 as i32, c.1 as i32) as i64;
        let mut capitals: Vec<(u32, u32)> = Vec::new();
        {
            // Seed with the least-damaged pair among genuinely distant ones —
            // fairness first, then terrain quality.
            let min_sep = ((p as i64) * 6 / 10).pow(2);
            type CapitalPair = ((u32, u32), (u32, u32), i64);
            let mut best: Option<CapitalPair> = None;
            let mut best_far = (pool[0], pool[1], i64::MIN);
            for i in 0..pool.len() {
                for j in (i + 1)..pool.len() {
                    let dist = d2(pool[i], pool[j]);
                    if dist > best_far.2 {
                        best_far = (pool[i], pool[j], dist);
                    }
                    if dist < min_sep {
                        continue;
                    }
                    let score = -(dmg(pool[i]) + dmg(pool[j])) * 200 + dist / 8;
                    if best.map(|(_, _, bs)| score > bs).unwrap_or(true) {
                        best = Some((pool[i], pool[j], score));
                    }
                }
            }
            let (a, bcap) = best
                .map(|(a, b, _)| (a, b))
                .unwrap_or((best_far.0, best_far.1));
            capitals.push(a);
            capitals.push(bcap);
            while capitals.len() < n_capitals {
                let next = pool
                    .iter()
                    .copied()
                    .filter(|c| !capitals.contains(c))
                    .max_by_key(|&c| {
                        capitals.iter().map(|&k| d2(c, k)).min().unwrap_or(0) - dmg(c) * 200
                    });
                match next {
                    Some(c) => capitals.push(c),
                    None => break,
                }
            }
        }
        let blue_base = capitals[0];
        let red_base = capitals[1];

        // Countryside: total settlement count scales with playable area.
        let area = (p as usize) * (p as usize);
        let total = (area / 3500).max(n_capitals + 5);
        let mut n_field = (total - capitals.len()).clamp(5, 24);
        if n_capitals == 2 {
            // Mirrored 1v1 countryside needs an even count.
            n_field += n_field % 2;
        }
        let min_gap: i64 = 26 * 26;
        let cap_gap: i64 = 32 * 32;

        let mut field: Vec<(u32, u32)> = Vec::new();
        {
            let ok = |c: (u32, u32), field: &Vec<(u32, u32)>, capitals: &Vec<(u32, u32)>| -> bool {
                capitals.iter().all(|&k| d2(c, k) >= cap_gap)
                    && field.iter().all(|&f| d2(c, f) >= min_gap)
            };
            // 1v1 maps mirror the countryside about the capital midpoint for
            // fairness; bigger FFAs rely on best-candidate spread.
            let mirror_sum = (
                (blue_base.0 + red_base.0) as i64,
                (blue_base.1 + red_base.1) as i64,
            );
            let mirrored = n_capitals == 2;
            let mut attempts = 0;
            while field.len() < n_field && attempts < 3000 {
                attempts += 1;
                let mut best: Option<((u32, u32), i64)> = None;
                for _ in 0..24 {
                    let c = sample(&mut net_rng, margin_field);
                    if !ok(c, &field, &capitals) {
                        continue;
                    }
                    if mirrored {
                        let m = (mirror_sum.0 - c.0 as i64, mirror_sum.1 - c.1 as i64);
                        if m.0 < (b + margin_field) as i64
                            || m.1 < (b + margin_field) as i64
                            || m.0 >= (b + p - margin_field) as i64
                            || m.1 >= (b + p - margin_field) as i64
                        {
                            continue;
                        }
                        let m = (m.0 as u32, m.1 as u32);
                        if d2(c, m) < min_gap || !ok(m, &field, &capitals) {
                            continue;
                        }
                    }
                    let spread = field
                        .iter()
                        .chain(capitals.iter())
                        .map(|&e| d2(c, e))
                        .min()
                        .unwrap_or(i64::MAX / 4);
                    let score = spread - dmg(c) * 300;
                    if best.map(|(_, bs)| score > bs).unwrap_or(true) {
                        best = Some((c, score));
                    }
                }
                let Some((c, _)) = best else { continue };
                field.push(c);
                if mirrored {
                    // Pairs stay together — n_field is even for 1v1.
                    let m = (
                        (mirror_sum.0 - c.0 as i64) as u32,
                        (mirror_sum.1 - c.1 as i64) as u32,
                    );
                    field.push(m);
                }
            }
        }

        // Zone list: capitals first, then countryside. Tier split for the
        // countryside: ~1 town per 4, ~1 hamlet per 4, villages otherwise —
        // mirrored pairs share a tier so 1v1 stays fair.
        let mut zone_centers: Vec<(u32, u32)> = capitals.clone();
        zone_centers.extend(field.iter().copied());
        let mut tiers = vec![SettlementTier::City; capitals.len()];
        for i in 0..field.len() {
            let group = if n_capitals == 2 { i / 2 } else { i };
            tiers.push(match group % 4 {
                0 => SettlementTier::Town,
                3 => SettlementTier::Hamlet,
                _ => SettlementTier::Village,
            });
        }

        self.zone_centers = zone_centers;
        self.tiers = tiers;
        self.capitals = capitals;
    }
}

/// True if the tile is on or adjacent to an elevation cliff.
fn near_cliff(grid: &Grid, x: u32, y: u32) -> bool {
    let e = grid.elevation(x, y);
    for &(dx, dy) in &[(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if grid.in_bounds(nx, ny) && grid.elevation(nx as u32, ny as u32) != e {
            return true;
        }
    }
    false
}

/// Water/cliff tiles a village clearing at (cx, cy) would bulldoze.
fn terrain_damage(grid: &Grid, cx: i32, cy: i32) -> u32 {
    let r = VILLAGE_CLEAR_RADIUS as i32;
    let mut damage = 0;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy > r * r {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if !grid.in_bounds(x, y) {
                damage += 1;
                continue;
            }
            let (x, y) = (x as u32, y as u32);
            if grid.get(x, y) == TileKind::Water || grid.elevation(x, y) > 0 {
                damage += 1;
            }
        }
    }
    damage
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

/// Plan road edges connecting all settlements: a Minimum Spanning Tree
/// plus loop edges (every node reaches degree >= 2 where possible). The
/// edge list is also the settlement adjacency graph.
fn plan_road_edges(nodes: &[(u32, u32)]) -> Vec<(usize, usize)> {
    let n = nodes.len();
    if n < 2 {
        return Vec::new();
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

    mst_edges
}

/// Enforce a 1-tile grass border: clear forest/rock/cliff adjacent to
/// road tiles so the 2-wide stamp is always walkable edge to edge.
fn clear_road_borders(grid: &mut Grid) {
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

/// Resumable A* for road generation. Prefers existing roads (zero extra
/// cost) and avoids water/elevation; forest is traversable but expensive.
/// `run` expands at most `max_pops` nodes per call so generation steps
/// stay frame-sized on large maps.
struct RoadAstarJob {
    from: (u32, u32),
    to: (u32, u32),
    g_score: Vec<u32>,
    came_from: Vec<u32>,
    open: std::collections::BinaryHeap<std::cmp::Reverse<(u32, u32, u32)>>,
}

impl RoadAstarJob {
    fn new(grid: &Grid, from: (u32, u32), to: (u32, u32)) -> Self {
        let size = (grid.width * grid.height) as usize;
        let mut job = Self {
            from,
            to,
            g_score: vec![u32::MAX; size],
            came_from: vec![u32::MAX; size],
            open: std::collections::BinaryHeap::new(),
        };
        job.g_score[(from.1 * grid.width + from.0) as usize] = 0;
        job.open
            .push(std::cmp::Reverse((job.heuristic(from.0, from.1), from.0, from.1)));
        job
    }

    // Octile heuristic (admissible with cardinal=2, diagonal=3, min tile cost=1)
    fn heuristic(&self, x: u32, y: u32) -> u32 {
        let dx = (x as i32 - self.to.0 as i32).unsigned_abs();
        let dy = (y as i32 - self.to.1 as i32).unsigned_abs();
        let (min, max) = if dx < dy { (dx, dy) } else { (dy, dx) };
        min * 3 + (max - min) * 2
    }

    /// Expand up to `max_pops` nodes. Returns None while still running,
    /// Some(None) when unreachable, Some(Some(path)) on arrival.
    #[allow(clippy::option_option)]
    fn run(&mut self, grid: &Grid, max_pops: u32) -> Option<Option<Vec<(u32, u32)>>> {
        use std::cmp::Reverse;

        let w = grid.width;
        let idx = |x: u32, y: u32| (y * w + x) as usize;
        let (sx, sy) = self.from;
        let (gx, gy) = self.to;

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

        let mut pops = 0u32;
        while let Some(Reverse((_, x, y))) = self.open.pop() {
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
                    ci = self.came_from[ci] as usize;
                }
                path.push((sx, sy));
                path.reverse();
                return Some(Some(path));
            }

            let g = self.g_score[idx(x, y)];
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
                if new_g < self.g_score[ni] {
                    self.g_score[ni] = new_g;
                    self.came_from[ni] = idx(x, y) as u32;
                    self.open
                        .push(Reverse((new_g + self.heuristic(nx, ny), nx, ny)));
                }
            }

            pops += 1;
            if pops >= max_pops {
                return None; // budget spent; resume next step
            }
        }

        Some(None) // open set exhausted: unreachable
    }
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
    use crate::grid::GRID_SIZE;

    #[test]
    fn zone_spacing_and_bounds_invariants() {
        for seed in [42, 77, 123, 777, 1234, 5555, 9999] {
            let (_, layout) = generate_battlefield(seed, PLAYABLE_SIZE);
            let zones = &layout.zone_centers;
            let total = ((PLAYABLE_SIZE as usize * PLAYABLE_SIZE as usize) / 3500).max(7);
            let mut n_field = (total - 2).clamp(5, 24);
            n_field += n_field % 2; // mirrored 1v1 pairs
            assert_eq!(zones.len(), 2 + n_field, "seed {seed}: settlement count");
            let margin = BORDER_SIZE; // every tier keeps at least this
            for (i, &z) in zones.iter().enumerate() {
                assert!(
                    z.0 >= margin
                        && z.1 >= margin
                        && z.0 < GRID_SIZE - margin
                        && z.1 < GRID_SIZE - margin,
                    "seed {seed}: settlement {i} at {z:?} too close to map edge"
                );
                for (j, &other) in zones.iter().enumerate().skip(i + 1) {
                    let dx = z.0 as i64 - other.0 as i64;
                    let dy = z.1 as i64 - other.1 as i64;
                    assert!(
                        dx * dx + dy * dy >= 26 * 26,
                        "seed {seed}: settlements {i} and {j} too close ({z:?} vs {other:?})"
                    );
                }
            }
        }
    }

    #[test]
    fn every_zone_gets_a_livable_village() {
        for seed in [42, 77, 123, 777, 1234, 5555, 9999] {
            let (grid, layout) = generate_battlefield(seed, PLAYABLE_SIZE);
            assert_eq!(layout.settlements.len(), layout.zone_centers.len());
            let mut themes = std::collections::HashSet::new();
            for v in &layout.settlements {
                themes.insert(format!("{:?}", v.theme));
                if v.tier == crate::zone::SettlementTier::City {
                    // City buildings come from the band generator at setup;
                    // the spec still carries a worked resource ring.
                    assert!(
                        !v.resources.is_empty(),
                        "seed {seed}: city {} has no resources",
                        v.zone_idx
                    );
                    continue;
                }
                assert!(
                    v.houses.len() >= 2,
                    "seed {seed}: village {} has {} houses",
                    v.zone_idx,
                    v.houses.len()
                );
                assert!(
                    !v.production.is_empty(),
                    "seed {seed}: village {} has no production building",
                    v.zone_idx
                );
                assert!(
                    v.resources.len() >= 3,
                    "seed {seed}: village {} has {} resource tiles",
                    v.zone_idx,
                    v.resources.len()
                );
                // Buildings must not sit on roads or water.
                for &(x, y) in v.houses.iter().chain(v.production.iter().map(|(p, _)| p)) {
                    let t = grid.get(x, y);
                    assert!(
                        t != TileKind::Road && t != TileKind::Water,
                        "seed {seed}: village {} building on {t:?} at ({x},{y})",
                        v.zone_idx
                    );
                }
            }
            assert_eq!(themes.len(), 3, "seed {seed}: themes missing: {themes:?}");
        }
    }

    #[test]
    fn zone_placement_is_mirror_fair() {
        // 1v1 countryside is placed in mirrored pairs about the capital
        // midpoint: every field settlement's mirror is also a settlement.
        for seed in [42, 777, 9999] {
            let (_, layout) = generate_battlefield(seed, PLAYABLE_SIZE);
            let sum = (
                (layout.blue_base.0 + layout.red_base.0) as i64,
                (layout.blue_base.1 + layout.red_base.1) as i64,
            );
            let field = &layout.zone_centers[2..];
            for &(x, y) in field {
                let m = (sum.0 - x as i64, sum.1 - y as i64);
                let found = field
                    .iter()
                    .any(|&(fx, fy)| (fx as i64 - m.0).abs() <= 1 && (fy as i64 - m.1).abs() <= 1);
                assert!(
                    found,
                    "seed {seed}: settlement ({x},{y}) has no mirror at {m:?}"
                );
            }
        }
    }

    #[test]
    fn all_zones_reachable_from_bases() {
        for seed in [1, 5, 7, 21, 42, 99, 777, 1234, 31337] {
            let (grid, layout) = generate_battlefield(seed, PLAYABLE_SIZE);

            // Flood fill from the blue base under the PLANNER's traversal
            // rules (passable + no cliff crossing) — what flow fields, A*,
            // and unit movement all enforce. Tile passability alone is not
            // enough: an elevation step or dropped road edge partitions the
            // map for actual units.
            let mut visited = vec![false; (grid.width * grid.height) as usize];
            let idx = |x: u32, y: u32| (y * grid.width + x) as usize;
            let mut stack = vec![layout.blue_base];
            visited[idx(layout.blue_base.0, layout.blue_base.1)] = true;
            while let Some((x, y)) = stack.pop() {
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if !grid.in_bounds(nx, ny) {
                        continue;
                    }
                    let (ux, uy) = (nx as u32, ny as u32);
                    if visited[idx(ux, uy)]
                        || !grid.is_passable(ux, uy)
                        || grid.is_cliff_between(x, y, ux, uy)
                    {
                        continue;
                    }
                    visited[idx(ux, uy)] = true;
                    stack.push((ux, uy));
                }
            }

            assert!(
                visited[idx(layout.red_base.0, layout.red_base.1)],
                "seed {seed}: red base unreachable from blue base"
            );
            for (i, &(zx, zy)) in layout.zone_centers.iter().enumerate() {
                assert!(
                    visited[idx(zx, zy)],
                    "seed {seed}: zone {i} at ({zx},{zy}) unreachable from blue base"
                );
            }
        }
    }

    #[test]
    fn deterministic_generation() {
        let (g1, l1) = generate_battlefield(42, PLAYABLE_SIZE);
        let (g2, l2) = generate_battlefield(42, PLAYABLE_SIZE);
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
        let (g1, _) = generate_battlefield(42, PLAYABLE_SIZE);
        let (g2, _) = generate_battlefield(99, PLAYABLE_SIZE);
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
        let (grid, layout) = generate_battlefield(42, PLAYABLE_SIZE);
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
        let (grid, layout) = generate_battlefield(42, PLAYABLE_SIZE);
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
        let (grid, _) = generate_battlefield(42, PLAYABLE_SIZE);
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
        let (grid, _) = generate_battlefield(42, PLAYABLE_SIZE);
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
        let (grid, _) = generate_battlefield(42, PLAYABLE_SIZE);
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
                    Some(Decoration::GoldStone(_)) => {
                        assert_eq!(
                            grid.get(x, y),
                            TileKind::Grass,
                            "Gold stone on non-grass at ({x},{y})"
                        );
                    }
                    None => {}
                }
            }
        }
    }

    #[test]
    fn trees_form_clusters() {
        let (grid, _) = generate_battlefield(42, PLAYABLE_SIZE);
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
    fn capitals_far_apart() {
        for seed in [42, 777, 9999] {
            let (_, layout) = generate_battlefield(seed, PLAYABLE_SIZE);
            let dx = layout.blue_base.0 as f32 - layout.red_base.0 as f32;
            let dy = layout.blue_base.1 as f32 - layout.red_base.1 as f32;
            let dist = (dx * dx + dy * dy).sqrt();
            assert!(
                dist >= PLAYABLE_SIZE as f32 * 0.55,
                "seed {seed}: capitals only {dist:.0} tiles apart"
            );
        }
    }

    #[test]
    fn network_adjacency_matches_roads() {
        let (_, layout) = generate_battlefield(42, PLAYABLE_SIZE);
        assert_eq!(layout.connections.len(), layout.zone_centers.len());
        // Every settlement is on the road network (degree >= 1, mostly 2+).
        for (i, adj) in layout.connections.iter().enumerate() {
            assert!(
                !adj.is_empty(),
                "settlement {i} is not connected to the road network"
            );
            for &n in adj {
                assert!(
                    layout.connections[n as usize].contains(&(i as u8)),
                    "adjacency not symmetric: {i} -> {n}"
                );
            }
        }
    }

    #[test]
    fn border_has_vegetation() {
        let (grid, _) = generate_battlefield(42, PLAYABLE_SIZE);
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

}
