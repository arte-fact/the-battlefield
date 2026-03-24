//! Shared pure rendering helpers used by all frontends (wasm, SDL, server).
//!
//! These functions compute *what* to draw — frame indices, colors, opacity,
//! visibility predicates — without touching any graphics API.

use crate::camera::Camera;
use crate::grid::TILE_SIZE;
use crate::unit::{Faction, OrderKind, DEATH_FADE_DURATION};
use crate::zone::ZoneState;

// ---------------------------------------------------------------------------
// Animation frame selection
// ---------------------------------------------------------------------------

/// Compute a wave-gated animation frame.
///
/// A sine wave sweeps across the grid. When active at `(gx, gy)`, returns a
/// cycling frame index at 10 FPS; otherwise returns frame 0 (idle).
pub fn compute_wave_frame(elapsed: f64, gx: u32, gy: u32, frame_count: u32, speed: f64) -> u32 {
    let wave_pos = elapsed * speed + gx as f64 * 0.06 + gy as f64 * 0.04 + (gx ^ gy) as f64 * 0.01;
    if (wave_pos * std::f64::consts::TAU).sin() > 0.3 {
        ((elapsed * 10.0) as u32) % frame_count
    } else {
        0
    }
}

/// Compute the foam animation frame for a tile at `(gx, gy)`.
/// Synced to the wind wave system: foam only appears when the wave is active
/// at this tile (same gating as bushes/trees). Returns `None` when calm.
pub fn foam_frame(elapsed: f64, gx: u32, gy: u32) -> Option<u32> {
    const FOAM_FRAMES: u32 = 16;
    const FOAM_FPS: f64 = 8.0;

    // Same wind wave as bushes/trees — on/off gating
    let wave_pos = elapsed * 0.15 + gx as f64 * 0.06 + gy as f64 * 0.04 + (gx ^ gy) as f64 * 0.01;
    if (wave_pos * std::f64::consts::TAU).sin() <= 0.3 {
        return None; // wind is calm here — no foam
    }

    let tile_offset = (gx.wrapping_mul(7).wrapping_add(gy.wrapping_mul(13))) % FOAM_FRAMES;
    let global_frame = (elapsed * FOAM_FPS) as u32;
    Some((global_frame + tile_offset) % FOAM_FRAMES)
}

// ---------------------------------------------------------------------------
// Spatial helpers
// ---------------------------------------------------------------------------

/// Deterministic pseudo-random flip based on grid position.
/// Returns `true` for ~50% of tiles in a spatially uniform pattern.
pub fn tile_flip(gx: u32, gy: u32) -> bool {
    gx.wrapping_mul(48271).wrapping_add(gy.wrapping_mul(16807)) & 1 == 0
}

/// Pick a variant index from a set of `count` options, deterministically
/// seeded by grid position. Uses a proper hash to avoid diagonal stripe patterns.
pub fn variant_index(gx: u32, gy: u32, count: usize, seed_x: u32, seed_y: u32) -> usize {
    // xorshift-style hash to break linear patterns
    let mut h = gx.wrapping_mul(seed_x) ^ gy.wrapping_mul(seed_y);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1e995);
    h ^= h >> 15;
    (h as usize) % count
}

/// Compute the visible tile range from the camera frustum, clamped to the grid.
pub fn visible_tile_range(camera: &Camera, grid_w: u32, grid_h: u32) -> (u32, u32, u32, u32) {
    let (vl, vt, vr, vb) = camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(grid_w as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(grid_h as i32) as u32;
    (min_gx, min_gy, max_gx, max_gy)
}

// ---------------------------------------------------------------------------
// Unit rendering helpers
// ---------------------------------------------------------------------------

/// Compute the draw opacity for a unit (death fade + hit flash blink).
pub fn unit_opacity(alive: bool, death_fade: f32, hit_flash: f32) -> f64 {
    if !alive {
        (death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0) as f64
    } else if hit_flash > 0.0 && (hit_flash * 30.0) as i32 % 2 == 0 {
        0.3
    } else {
        1.0
    }
}

/// Check whether a non-Blue unit at `(gx, gy)` should be drawn (FOV check).
pub fn is_visible_to_player(
    faction: Faction,
    gx: u32,
    gy: u32,
    visible: &[bool],
    grid_w: u32,
) -> bool {
    if faction == Faction::Blue {
        return true;
    }
    let idx = (gy * grid_w + gx) as usize;
    visible.get(idx).copied().unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tree rendering
// ---------------------------------------------------------------------------

/// Compute tree alpha based on distance to the player (fade when near).
pub fn tree_alpha(tree_wx: f64, tree_wy: f64, player_pos: Option<(f64, f64)>, ts: f64) -> f64 {
    if let Some((px, py)) = player_pos {
        let dx = tree_wx - px;
        let dy = tree_wy - py;
        let dist = (dx * dx + dy * dy).sqrt();
        let fade_start = ts * 2.5;
        let fade_end = ts * 1.0;
        if dist < fade_end {
            0.3
        } else if dist < fade_start {
            0.3 + (dist - fade_end) / (fade_start - fade_end) * 0.7
        } else {
            1.0
        }
    } else {
        1.0
    }
}

// ---------------------------------------------------------------------------
// HUD / color helpers
// ---------------------------------------------------------------------------

/// HP bar fill color as (R, G, B) based on HP ratio (0.0–1.0).
pub fn hp_bar_color(ratio: f64) -> (u8, u8, u8) {
    if ratio > 0.5 {
        (51, 204, 51) // green
    } else if ratio > 0.25 {
        (230, 179, 26) // yellow
    } else {
        (230, 51, 26) // red
    }
}

/// Order label text for the given order kind.
pub fn order_label(order: Option<&OrderKind>) -> Option<&'static str> {
    match order {
        Some(OrderKind::Follow) => Some("FOLLOW"),
        Some(OrderKind::Charge { .. }) => Some("CHARGE"),
        Some(OrderKind::Defend { .. }) => Some("DEFEND"),
        None => None,
    }
}

/// Zone overlay fill color as (R, G, B, A) for a given zone state.
pub fn zone_fill_rgba(state: ZoneState) -> (u8, u8, u8, u8) {
    match state {
        ZoneState::Neutral => (200, 200, 200, 15),
        ZoneState::Contested => (255, 200, 0, 20),
        ZoneState::Capturing(Faction::Blue) => (60, 120, 255, 20),
        ZoneState::Capturing(Faction::Red) => (255, 60, 60, 20),
        ZoneState::Controlled(Faction::Blue) => (60, 120, 255, 30),
        ZoneState::Controlled(Faction::Red) => (255, 60, 60, 30),
    }
}

/// Zone border color as (R, G, B, A) for a given zone state.
pub fn zone_border_rgba(state: ZoneState) -> (u8, u8, u8, u8) {
    match state {
        ZoneState::Neutral => (200, 200, 200, 64),
        ZoneState::Contested => (255, 200, 0, 100),
        ZoneState::Capturing(Faction::Blue) => (60, 120, 255, 100),
        ZoneState::Capturing(Faction::Red) => (255, 60, 60, 100),
        ZoneState::Controlled(Faction::Blue) => (60, 120, 255, 128),
        ZoneState::Controlled(Faction::Red) => (255, 60, 60, 128),
    }
}

/// Zone pip fill color as (R, G, B) for zone HUD indicators.
pub fn zone_pip_rgb(state: ZoneState) -> (u8, u8, u8) {
    match state {
        ZoneState::Neutral => (150, 150, 150),
        ZoneState::Contested => (255, 200, 0),
        ZoneState::Capturing(Faction::Blue) | ZoneState::Controlled(Faction::Blue) => {
            (60, 120, 255)
        }
        ZoneState::Capturing(Faction::Red) | ZoneState::Controlled(Faction::Red) => (255, 60, 60),
    }
}

// ---------------------------------------------------------------------------
// Fog of war
// ---------------------------------------------------------------------------

/// Compute fog alpha for a tile. Returns 0 (visible), 160 (revealed), or 240 (hidden).
pub fn fog_alpha(visible: bool, revealed: bool) -> u8 {
    if visible {
        0
    } else if revealed {
        160
    } else {
        240
    }
}

/// Count visible neighbors (8-directional) for a tile. No allocations.
pub fn visible_neighbor_count(visible: &[bool], gx: u32, gy: u32, w: u32, h: u32) -> u32 {
    let mut count = 0u32;
    let x = gx as i32;
    let y = gy as i32;
    let wi = w as i32;
    let hi = h as i32;
    for &(ndx, ndy) in &[
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        let nx = x + ndx;
        let ny = y + ndy;
        if nx >= 0 && ny >= 0 && nx < wi && ny < hi && visible[(ny as u32 * w + nx as u32) as usize]
        {
            count += 1;
        }
    }
    count
}

/// Compute smooth fog alpha for a single tile, using neighbor visibility for soft edges.
/// Returns a value 0–255 suitable for a black RGBA pixel's alpha channel.
pub fn smooth_fog_alpha(visible: &[bool], gx: u32, gy: u32, w: u32, h: u32) -> u8 {
    let idx = (gy * w + gx) as usize;
    let is_vis = visible.get(idx).copied().unwrap_or(false);

    if is_vis {
        // Visible tile — add soft edge darkening near fog boundary
        let fog_n = 8 - visible_neighbor_count(visible, gx, gy, w, h);
        if fog_n >= 3 {
            ((fog_n - 2) * 10).min(255) as u8
        } else {
            0
        }
    } else {
        // Not visible — dim fog, softer near visible tiles
        let vis_n = visible_neighbor_count(visible, gx, gy, w, h);
        let base = 140i32;
        (base - (vis_n as i32) * 15).max(38) as u8
    }
}

/// Build a complete fog RGBA pixel buffer (1 pixel per tile) with smooth edges.
/// Returns a Vec<u8> of length `w * h * 4` in RGBA order.
pub fn build_fog_pixels(visible: &[bool], w: u32, h: u32) -> Vec<u8> {
    let len = (w * h * 4) as usize;
    let mut pixels = vec![0u8; len];
    for gy in 0..h {
        for gx in 0..w {
            let po = ((gy * w + gx) * 4) as usize;
            let alpha = smooth_fog_alpha(visible, gx, gy, w, h);
            // Black with computed alpha
            pixels[po] = 0;
            pixels[po + 1] = 0;
            pixels[po + 2] = 0;
            pixels[po + 3] = alpha;
        }
    }
    pixels
}

// ---------------------------------------------------------------------------
// 9-slice (9-patch) panel rendering
// ---------------------------------------------------------------------------

/// A source rectangle (f64 for cross-platform compat with both SDL and Canvas2D).
#[derive(Clone, Copy, Debug)]
pub struct SrcDst {
    pub sx: f64,
    pub sy: f64,
    pub sw: f64,
    pub sh: f64,
    pub dx: f64,
    pub dy: f64,
    pub dw: f64,
    pub dh: f64,
}

/// Standard 9-slice definition using border insets from image edges.
///
/// The source image must be **gapless** — the 9 cells are contiguous.
/// If the original sprite has transparent gaps between cells, pre-process it
/// at load time by blitting the 9 cells into a tight atlas.
///
/// The slice defines 4 border widths measured inward from the image edges.
/// Corners keep their source size; edges stretch in one axis; center stretches in both.
#[derive(Clone, Copy, Debug)]
pub struct NineSlice {
    /// Pixels from left edge to end of left column.
    pub left: f64,
    /// Pixels from top edge to end of top row.
    pub top: f64,
    /// Pixels from right edge to start of right column.
    pub right: f64,
    /// Pixels from bottom edge to start of bottom row.
    pub bottom: f64,
}

/// 9-slice borders for the pre-processed SpecialPaper atlas (162x151).
/// Left/right corners = 54px, top = 44px, bottom = 43px, center = 64x64.
pub const NINE_SLICE_SPECIAL_PAPER: NineSlice = NineSlice {
    left: 54.0,
    top: 44.0,
    right: 54.0,
    bottom: 43.0,
};

/// 9-slice borders for the pre-processed Button atlas (154x158).
/// Left/right corners = 45px, top/bottom = 47px, center = 64x64.
pub const NINE_SLICE_BUTTON: NineSlice = NineSlice {
    left: 45.0,
    top: 47.0,
    right: 45.0,
    bottom: 47.0,
};

/// Cell source rectangles for SpecialPaper.png (320x320) — used during
/// atlas pre-processing. Each tuple: (sx, sy, sw, sh).
pub const SPECIAL_PAPER_CELLS: [[f64; 4]; 9] = [
    [10.0, 20.0, 54.0, 44.0],   // TL
    [128.0, 20.0, 64.0, 44.0],  // TC
    [256.0, 20.0, 54.0, 44.0],  // TR
    [10.0, 128.0, 54.0, 64.0],  // ML
    [128.0, 128.0, 64.0, 64.0], // MC
    [256.0, 128.0, 54.0, 64.0], // MR
    [10.0, 256.0, 54.0, 43.0],  // BL
    [128.0, 256.0, 64.0, 43.0], // BC
    [256.0, 256.0, 54.0, 43.0], // BR
];

/// Cell source rectangles for BigBlueButton/BigRedButton (320x320).
pub const BUTTON_CELLS: [[f64; 4]; 9] = [
    [19.0, 17.0, 45.0, 47.0],   // TL
    [128.0, 17.0, 64.0, 47.0],  // TC
    [256.0, 17.0, 45.0, 47.0],  // TR
    [19.0, 128.0, 45.0, 64.0],  // ML
    [128.0, 128.0, 64.0, 64.0], // MC
    [256.0, 128.0, 45.0, 64.0], // MR
    [19.0, 256.0, 45.0, 47.0],  // BL
    [128.0, 256.0, 64.0, 47.0], // BC
    [256.0, 256.0, 45.0, 47.0], // BR
];

/// Cell source rectangles for WoodTable.png (448x448).
pub const WOOD_TABLE_CELLS: [[f64; 4]; 9] = [
    [45.0, 43.0, 83.0, 85.0],    // TL
    [192.0, 49.0, 64.0, 79.0],   // TC (use TL height for consistency → 85)
    [320.0, 43.0, 83.0, 85.0],   // TR
    [49.0, 192.0, 79.0, 64.0],   // ML
    [192.0, 192.0, 64.0, 64.0],  // MC
    [320.0, 192.0, 79.0, 64.0],  // MR
    [44.0, 320.0, 84.0, 103.0],  // BL
    [192.0, 320.0, 64.0, 103.0], // BC
    [320.0, 320.0, 84.0, 103.0], // BR
];

/// 9-slice borders for the pre-processed WoodTable atlas.
pub const NINE_SLICE_WOOD_TABLE: NineSlice = NineSlice {
    left: 83.0,
    top: 85.0,
    right: 83.0,
    bottom: 103.0,
};

/// Compute the total atlas size for a pre-processed 9-cell image.
/// Returns (width, height) of the tightly packed atlas.
pub fn nine_cell_atlas_size(cells: &[[f64; 4]; 9]) -> (u32, u32) {
    let w = cells[0][2] + cells[1][2] + cells[2][2]; // TL.w + TC.w + TR.w
    let h = cells[0][3] + cells[3][3] + cells[6][3]; // TL.h + ML.h + BL.h
    (w.ceil() as u32, h.ceil() as u32)
}

/// Compute where each of the 9 source cells should be placed in the tight atlas.
/// Returns 9 destination (dx, dy) offsets within the atlas.
pub fn nine_cell_atlas_positions(cells: &[[f64; 4]; 9]) -> [(f64, f64); 9] {
    let col_w = [cells[0][2], cells[1][2], cells[2][2]]; // widths of 3 columns
    let row_h = [cells[0][3], cells[3][3], cells[6][3]]; // heights of 3 rows
    let mut positions = [(0.0, 0.0); 9];
    for row in 0..3 {
        for col in 0..3 {
            let x: f64 = col_w[..col].iter().sum();
            let y: f64 = row_h[..row].iter().sum();
            positions[row * 3 + col] = (x, y);
        }
    }
    positions
}

/// Source rects for BigRibbons.png (448x640, 3 cols × 5 color rows).
/// Each color row: left end, center (stretchable), right end.
/// Row heights: 128px each. Colors: Blue=0, Red=1, Yellow=2, Purple=3, Black=4.
pub const RIBBON_CELL_W: f64 = 149.0;
pub const RIBBON_CELL_H: f64 = 128.0;
/// Actual content bounds within each ribbon cell (for tight drawing).
pub const RIBBON_LEFT: (f64, f64, f64, f64) = (30.0, 20.0, 98.0, 103.0); // sx, sy_off, sw, sh
pub const RIBBON_CENTER: (f64, f64, f64, f64) = (192.0, 22.0, 64.0, 89.0);
pub const RIBBON_RIGHT: (f64, f64, f64, f64) = (320.0, 20.0, 97.0, 101.0);

/// SmallRibbons.png (320x640, 10 rows of 64px each).
/// Small variants at rows 1,3,5,7,9. Colors: Blue=1, Red=3, Yellow=5, Purple=7, Black=9.
pub const SMALL_RIBBON_CELL_H: f64 = 64.0;
pub const SMALL_RIBBON_LEFT: (f64, f64, f64, f64) = (3.0, 4.0, 61.0, 54.0);
pub const SMALL_RIBBON_CENTER: (f64, f64, f64, f64) = (128.0, 4.0, 64.0, 54.0);
pub const SMALL_RIBBON_RIGHT: (f64, f64, f64, f64) = (256.0, 4.0, 61.0, 54.0);

/// Map a faction to SmallRibbon small-variant row index.
/// Blue=1, Red=3. Yellow=5 for neutral/orders.
pub fn small_ribbon_row(faction: Faction) -> u32 {
    match faction {
        Faction::Blue => 1,
        Faction::Red => 3,
    }
}

/// Source rects for BigBar_Base.png (320x64, 3-part horizontal).
pub const BAR_LEFT: (f64, f64, f64, f64) = (40.0, 9.0, 24.0, 51.0);
pub const BAR_CENTER: (f64, f64, f64, f64) = (128.0, 9.0, 64.0, 51.0);
pub const BAR_RIGHT: (f64, f64, f64, f64) = (256.0, 9.0, 24.0, 51.0);

impl NineSlice {
    /// Compute the 9 source→dest draw commands for a **gapless** atlas image.
    ///
    /// `img_w`, `img_h`: dimensions of the pre-processed atlas texture.
    /// `dx, dy, dw, dh`: destination rectangle on screen.
    ///
    /// Source regions are derived from the border insets:
    ///   srcX = [0, left, img_w - right]
    ///   srcY = [0, top, img_h - bottom]
    ///
    /// Destination seam positions are snapped to integer pixels to prevent gaps.
    pub fn compute(
        &self,
        img_w: f64,
        img_h: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    ) -> [SrcDst; 9] {
        let l = self.left;
        let t = self.top;
        let r = self.right;
        let b = self.bottom;

        // Source slice boundaries (contiguous, no gaps)
        let sx = [0.0, l, img_w - r];
        let sw = [l, img_w - l - r, r];
        let sy = [0.0, t, img_h - b];
        let sh = [t, img_h - t - b, b];

        // Destination boundaries, snapped to integer pixels
        let x0 = dx.round();
        let y0 = dy.round();
        let x1 = (dx + l.min(dw / 2.0)).round();
        let y1 = (dy + t.min(dh / 2.0)).round();
        let x2 = (dx + dw - r.min(dw / 2.0)).round();
        let y2 = (dy + dh - b.min(dh / 2.0)).round();
        let x3 = (dx + dw).round();
        let y3 = (dy + dh).round();

        let dx_arr = [x0, x1, x2];
        let dw_arr = [x1 - x0, x2 - x1, x3 - x2];
        let dy_arr = [y0, y1, y2];
        let dh_arr = [y1 - y0, y2 - y1, y3 - y2];

        let mut result = [SrcDst {
            sx: 0.0,
            sy: 0.0,
            sw: 0.0,
            sh: 0.0,
            dx: 0.0,
            dy: 0.0,
            dw: 0.0,
            dh: 0.0,
        }; 9];

        for row in 0..3 {
            for col in 0..3 {
                result[row * 3 + col] = SrcDst {
                    sx: sx[col],
                    sy: sy[row],
                    sw: sw[col],
                    sh: sh[row],
                    dx: dx_arr[col],
                    dy: dy_arr[row],
                    dw: dw_arr[col],
                    dh: dh_arr[row],
                };
            }
        }
        result
    }
}

/// Compute the 3 source→dest draw commands for a horizontal 3-part bar.
/// Source layout: left cap | center (stretchable) | right cap, each `cap_w × bar_h`.
pub fn bar_3slice(
    src_cap_w: f64,
    src_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    cap_w: f64,
) -> [SrcDst; 3] {
    let cw = cap_w.min(dw / 2.0);
    let mid_w = dw - cw * 2.0;
    [
        // Left cap
        SrcDst {
            sx: 0.0,
            sy: 0.0,
            sw: src_cap_w,
            sh: src_h,
            dx,
            dy,
            dw: cw,
            dh,
        },
        // Center stretch
        SrcDst {
            sx: src_cap_w,
            sy: 0.0,
            sw: src_cap_w,
            sh: src_h,
            dx: dx + cw,
            dy,
            dw: mid_w,
            dh,
        },
        // Right cap
        SrcDst {
            sx: src_cap_w * 2.0,
            sy: 0.0,
            sw: src_cap_w,
            sh: src_h,
            dx: dx + cw + mid_w,
            dy,
            dw: cw,
            dh,
        },
    ]
}

/// Compute source rect for a ribbon from the BigRibbons/SmallRibbons sprite sheet.
/// Ribbons are laid out as 3 columns × 5 rows (Blue=0, Red=1, Yellow=2, Purple=3, Black=4).
/// Each ribbon has left end, center (stretchable), right end.
pub fn ribbon_src(color_row: u32, col: u32, cell_w: f64, cell_h: f64) -> (f64, f64, f64, f64) {
    let sx = col as f64 * cell_w;
    let sy = color_row as f64 * cell_h;
    (sx, sy, cell_w, cell_h)
}

/// Map a faction to its ribbon color row in BigRibbons/SmallRibbons (Blue=0, Red=1).
pub fn faction_ribbon_row(faction: Faction) -> u32 {
    match faction {
        Faction::Blue => 0,
        Faction::Red => 1,
    }
}
