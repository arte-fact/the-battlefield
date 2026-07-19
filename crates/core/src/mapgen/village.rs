//! Village layout planning for capture zones: houses, a production
//! building and a worked resource cluster per zone, described as data
//! (mapgen describes, setup spawns).

use super::{Rng, VILLAGE_CLEAR_RADIUS, VILLAGE_RING_MAX, VILLAGE_RING_MIN};
use crate::building::BuildingKind;
use crate::grid::{Grid, TileKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VillageTheme {
    Gold,
    Wood,
    Meat,
}

impl VillageTheme {
    pub fn production_building(self) -> BuildingKind {
        match self {
            VillageTheme::Gold => BuildingKind::Barracks,
            VillageTheme::Wood => BuildingKind::Archery,
            VillageTheme::Meat => BuildingKind::Monastery,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VillageSpec {
    pub zone_idx: u8,
    pub theme: VillageTheme,
    /// House anchor tiles.
    pub houses: Vec<(u32, u32)>,
    /// Production building anchors with their kind.
    pub production: Vec<((u32, u32), BuildingKind)>,
    /// Resource cluster tiles (gold stones / grove trees / pen ground).
    pub resources: Vec<(u32, u32)>,
}

/// Sixteen ring directions, deterministic order before shuffling.
const ANGLES: u32 = 16;

/// Plan a village for every zone. Wood groves are painted into the grid
/// here (they are terrain); gold stones and pens stay data until setup.
pub fn plan_villages(grid: &mut Grid, zone_centers: &[(u32, u32)], seed: u32) -> Vec<VillageSpec> {
    let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9).wrapping_add(0x5EED));

    // Theme pool guarantees all three themes on every map.
    let mut themes = vec![
        VillageTheme::Gold,
        VillageTheme::Wood,
        VillageTheme::Meat,
        VillageTheme::Gold,
        VillageTheme::Wood,
        VillageTheme::Meat,
        VillageTheme::Gold,
    ];
    themes.truncate(zone_centers.len().max(3));
    shuffle_themes(&mut rng, &mut themes);

    zone_centers
        .iter()
        .enumerate()
        .map(|(zi, &(cx, cy))| {
            plan_one(
                grid,
                zi as u8,
                cx,
                cy,
                themes[zi % themes.len()],
                &mut rng,
                zi == 3, // C2 gets the second production building
            )
        })
        .collect()
}

fn shuffle_themes(rng: &mut Rng, themes: &mut [VillageTheme]) {
    for i in (1..themes.len()).rev() {
        let j = (rng.next() as usize) % (i + 1);
        themes.swap(i, j);
    }
}

fn plan_one(
    grid: &mut Grid,
    zone_idx: u8,
    cx: u32,
    cy: u32,
    theme: VillageTheme,
    rng: &mut Rng,
    double_production: bool,
) -> VillageSpec {
    let (icx, icy) = (cx as i32, cy as i32);
    let mut occupied: Vec<(i32, i32)> = Vec::new();
    // Reserve the zone-center tower footprint.
    occupied.push((icx, icy));
    occupied.push((icx, icy - 1));

    // Direction of the road through the village: average of road tiles
    // near the center. Resources go on the opposite arc.
    let road_dir = road_direction(grid, icx, icy);

    let mut angle_slots: Vec<u32> = (0..ANGLES).collect();
    for i in (1..angle_slots.len()).rev() {
        let j = (rng.next() as usize) % (i + 1);
        angle_slots.swap(i, j);
    }

    let mut production = Vec::new();
    let kind = theme.production_building();
    if let Some(pos) = place_on_ring(grid, icx, icy, kind, &mut occupied, &angle_slots) {
        production.push((pos, kind));
    }
    if double_production {
        let second = match theme {
            VillageTheme::Gold => BuildingKind::Archery,
            VillageTheme::Wood => BuildingKind::Monastery,
            VillageTheme::Meat => BuildingKind::Barracks,
        };
        if let Some(pos) = place_on_ring(grid, icx, icy, second, &mut occupied, &angle_slots) {
            production.push((pos, second));
        }
    }

    let house_count = 3 + (rng.next() % 2) as usize;
    let mut houses = Vec::new();
    for _ in 0..house_count {
        if let Some(pos) = place_on_ring(
            grid,
            icx,
            icy,
            BuildingKind::House,
            &mut occupied,
            &angle_slots,
        ) {
            houses.push(pos);
        }
    }

    let resources = place_resources(grid, icx, icy, theme, road_dir, &mut occupied, rng);

    VillageSpec {
        zone_idx,
        theme,
        houses,
        production,
        resources,
    }
}

/// Average direction from the center toward nearby road tiles, if any.
fn road_direction(grid: &Grid, cx: i32, cy: i32) -> Option<(f32, f32)> {
    let r = VILLAGE_CLEAR_RADIUS as i32;
    let (mut sx, mut sy, mut n) = (0.0f32, 0.0f32, 0u32);
    for dy in -r..=r {
        for dx in -r..=r {
            if dx == 0 && dy == 0 {
                continue;
            }
            let (x, y) = (cx + dx, cy + dy);
            if grid.in_bounds(x, y) && grid.get(x as u32, y as u32) == TileKind::Road {
                let len = ((dx * dx + dy * dy) as f32).sqrt();
                sx += dx as f32 / len;
                sy += dy as f32 / len;
                n += 1;
            }
        }
    }
    if n == 0 {
        return None;
    }
    let len = (sx * sx + sy * sy).sqrt();
    if len < 0.1 {
        None
    } else {
        Some((sx / len, sy / len))
    }
}

/// Tiles a building blocks for placement: footprint plus the anchor tile.
fn block_tiles(kind: BuildingKind, x: i32, y: i32) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = kind
        .footprint_offsets()
        .iter()
        .map(|&(dx, dy)| (x + dx, y + dy))
        .collect();
    v.push((x, y));
    v
}

/// Try ring positions (shuffled angle slots × radii) until one passes the
/// rejection rules; commits the position into `occupied`.
fn place_on_ring(
    grid: &Grid,
    cx: i32,
    cy: i32,
    kind: BuildingKind,
    occupied: &mut Vec<(i32, i32)>,
    angle_slots: &[u32],
) -> Option<(u32, u32)> {
    for &slot in angle_slots {
        let angle = slot as f32 * (std::f32::consts::TAU / ANGLES as f32);
        for radius in VILLAGE_RING_MIN..=VILLAGE_RING_MAX {
            let x = cx + (angle.cos() * radius as f32).round() as i32;
            let y = cy + (angle.sin() * radius as f32).round() as i32;
            if fits(grid, kind, x, y, occupied) {
                let block = block_tiles(kind, x, y);
                occupied.extend(block);
                return Some((x as u32, y as u32));
            }
        }
    }
    None
}

fn fits(grid: &Grid, kind: BuildingKind, x: i32, y: i32, occupied: &[(i32, i32)]) -> bool {
    for (tx, ty) in block_tiles(kind, x, y) {
        // 1-tile apron: keep clear of roads, water, cliffs and neighbours.
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (ax, ay) = (tx + dx, ty + dy);
                if !grid.in_bounds(ax, ay) {
                    return false;
                }
                let (ax, ay) = (ax as u32, ay as u32);
                match grid.get(ax, ay) {
                    TileKind::Road | TileKind::Water => return false,
                    _ => {}
                }
                if grid.elevation(ax, ay) > 0 {
                    return false;
                }
            }
        }
        // 2-tile gaps between structures: units are nearly a tile wide,
        // single-tile corridors wedge and bump instead of flowing.
        if occupied
            .iter()
            .any(|&(ox, oy)| (ox - tx).abs() <= 2 && (oy - ty).abs() <= 2)
        {
            return false;
        }
    }
    true
}

/// Place the resource cluster on the arc opposite the road entry.
/// Wood groves are painted as Forest tiles immediately (they are terrain);
/// gold and pasture tiles stay grass and are recorded for setup.
fn place_resources(
    grid: &mut Grid,
    cx: i32,
    cy: i32,
    theme: VillageTheme,
    road_dir: Option<(f32, f32)>,
    occupied: &mut Vec<(i32, i32)>,
    rng: &mut Rng,
) -> Vec<(u32, u32)> {
    let count = match theme {
        VillageTheme::Gold => 3 + (rng.next() % 3) as usize,
        VillageTheme::Wood => 4 + (rng.next() % 3) as usize,
        VillageTheme::Meat => 4,
    };
    // Base direction: away from the road, or seeded if the village is roadless.
    let (bx, by) = match road_dir {
        Some((x, y)) => (-x, -y),
        None => {
            let a = (rng.next() % ANGLES) as f32 * (std::f32::consts::TAU / ANGLES as f32);
            (a.cos(), a.sin())
        }
    };
    let base_angle = by.atan2(bx);

    let mut out: Vec<(u32, u32)> = Vec::new();
    let mut tries = 0;
    while out.len() < count && tries < 200 {
        tries += 1;
        // Jitter within ±80° of the opposite-arc direction; clusters pack
        // tightly (only buildings keep an apron), so retries converge.
        let jitter = ((rng.next() % 1000) as f32 / 1000.0 - 0.5) * 2.8;
        let angle = base_angle + jitter;
        let max_r = (VILLAGE_CLEAR_RADIUS - 1) as f32;
        let radius = VILLAGE_RING_MIN as f32
            + ((rng.next() % 1000) as f32 / 1000.0) * (max_r - VILLAGE_RING_MIN as f32);
        let x = cx + (angle.cos() * radius).round() as i32;
        let y = cy + (angle.sin() * radius).round() as i32;
        if !grid.in_bounds(x, y) {
            continue;
        }
        let (ux, uy) = (x as u32, y as u32);
        if grid.get(ux, uy) != TileKind::Grass
            || grid.elevation(ux, uy) > 0
            || occupied
                .iter()
                .any(|&(ox, oy)| (ox - x).abs() <= 1 && (oy - y).abs() <= 1)
            || out.contains(&(ux, uy))
        {
            continue;
        }
        if theme == VillageTheme::Wood {
            grid.set(ux, uy, TileKind::Forest);
        }
        out.push((ux, uy));
    }
    // Resources block building placement for later villages sharing edges.
    occupied.extend(out.iter().map(|&(x, y)| (x as i32, y as i32)));
    out
}
