use crate::animation::TurnAnimator;
use crate::game::{Game, ATTACK_CONE_HALF_ANGLE, ORDER_FLASH_DURATION};
use crate::grid::{self, Decoration, TileKind, TILE_SIZE};
use crate::renderer::{Canvas2dRenderer, Renderer};
use crate::unit::{Faction, OrderKind};
use crate::zone::ZoneState;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Draw HP bars and order labels on top of unit sprites.
pub(super) fn draw_unit_bars(
    r: &Canvas2dRenderer,
    game: &Game,
    animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // HP bars
    for unit in &game.units {
        let show = if animator.is_playing() {
            animator.is_visually_alive(unit.id)
        } else {
            unit.alive
        };
        if !show {
            continue;
        }
        // Hide enemy HP bars outside friendly line of sight
        if unit.faction != Faction::Blue {
            let (gx, gy) = unit.grid_cell();
            let idx = (gy * game.grid.width + gx) as usize;
            if !game.visible[idx] {
                continue;
            }
        }
        let (wx, wy) = (unit.x, unit.y);
        let bar_width = 48.0_f64;
        let bar_height = 6.0_f64;
        let bar_y = (wy as f64) - (TILE_SIZE as f64) * 0.85;
        let bar_x = (wx as f64) - bar_width / 2.0;

        r.set_alpha(0.8);
        r.set_fill_color("rgb(51,51,51)");
        r.fill_rect(bar_x, bar_y - bar_height / 2.0, bar_width, bar_height);

        let hp_ratio = unit.hp as f64 / unit.stats.max_hp as f64;
        let fill_width = bar_width * hp_ratio;
        let fill_color = if hp_ratio > 0.5 {
            "rgb(51,204,51)"
        } else if hp_ratio > 0.25 {
            "rgb(230,179,26)"
        } else {
            "rgb(230,51,26)"
        };
        r.set_alpha(0.9);
        r.set_fill_color(fill_color);
        r.fill_rect(
            bar_x,
            bar_y - (bar_height - 2.0) / 2.0,
            fill_width,
            bar_height - 2.0,
        );

        r.set_alpha(1.0);
    }

    // Order word indicators ("HOLD", "GO", "RETREAT") above units
    for unit in &game.units {
        if !unit.alive || unit.order_flash <= 0.0 {
            continue;
        }
        let label = match unit.order {
            Some(OrderKind::Hold { .. }) => "HOLD",
            Some(OrderKind::Go { .. }) => "GO",
            Some(OrderKind::Retreat { .. }) => "RETREAT",
            Some(OrderKind::Follow) => "FOLLOW",
            None => continue,
        };

        let alpha = (unit.order_flash / ORDER_FLASH_DURATION) as f64;
        let wx = unit.x as f64;
        let wy = unit.y as f64 - (TILE_SIZE as f64) * 1.0;

        r.set_alpha(alpha);
        r.set_font("bold 14px sans-serif");
        r.set_text_align("center");
        r.set_text_baseline("bottom");
        r.set_stroke_color("rgba(0,0,0,0.9)");
        r.set_line_width(3.0);
        r.stroke_text(label, wx, wy);
        r.set_fill_color("rgb(255,215,0)");
        r.fill_text(label, wx, wy);
    }
    r.set_alpha(1.0);

    Ok(())
}

/// Draw capture zone overlays in world space (fill, dashed border, label, progress bar).
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_zone_overlays(
    r: &Canvas2dRenderer,
    game: &Game,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    for zone in &game.zone_manager.zones {
        // Skip zones entirely outside the visible range (bounding box of circle)
        let zone_min_gx = zone.center_gx.saturating_sub(zone.radius);
        let zone_min_gy = zone.center_gy.saturating_sub(zone.radius);
        let zone_max_gx = zone.center_gx + zone.radius + 1;
        let zone_max_gy = zone.center_gy + zone.radius + 1;

        if zone_max_gx < min_gx
            || zone_min_gx > max_gx
            || zone_max_gy < min_gy
            || zone_min_gy > max_gy
        {
            continue;
        }

        let cx = zone.center_wx as f64;
        let cy = zone.center_wy as f64;
        let radius = zone.radius_world as f64;

        // Fill + border color by state
        let (fill_color, border_color) = match zone.state {
            ZoneState::Neutral => ("rgba(200,200,200,0.06)", "rgba(200,200,200,0.25)"),
            ZoneState::Contested => ("rgba(255,200,0,0.08)", "rgba(255,200,0,0.4)"),
            ZoneState::Capturing(Faction::Blue) => {
                ("rgba(60,120,255,0.08)", "rgba(60,120,255,0.4)")
            }
            ZoneState::Capturing(Faction::Red) => ("rgba(255,60,60,0.08)", "rgba(255,60,60,0.4)"),
            ZoneState::Controlled(Faction::Blue) => {
                ("rgba(60,120,255,0.12)", "rgba(60,120,255,0.5)")
            }
            ZoneState::Controlled(Faction::Red) => ("rgba(255,60,60,0.12)", "rgba(255,60,60,0.5)"),
            _ => ("rgba(200,200,200,0.06)", "rgba(200,200,200,0.25)"),
        };

        // Semi-transparent circular fill
        r.set_fill_color(fill_color);
        r.begin_path();
        r.arc(cx, cy, radius, 0.0, std::f64::consts::TAU)?;
        r.fill();

        // Dashed circular border
        r.set_stroke_color(border_color);
        r.set_line_width(2.0);
        r.set_line_dash(&[8.0, 4.0]);
        r.begin_path();
        r.arc(cx, cy, radius, 0.0, std::f64::consts::TAU)?;
        r.stroke();
        r.set_line_dash(&[]);

        // Zone name label (above circle)
        let label_y = cy - radius - 14.0;
        r.set_font("bold 11px monospace");
        r.set_text_align("center");
        r.set_text_baseline("bottom");
        r.set_fill_color("rgba(255,255,255,0.7)");
        r.fill_text(&zone.name, cx, label_y);

        // Progress bar (just below the label, above circle)
        let bar_w = radius;
        let bar_h = 4.0;
        let bar_x = cx - bar_w / 2.0;
        let bar_y = cy - radius - 6.0;

        // Bar background
        r.set_fill_color("rgba(0,0,0,0.4)");
        r.fill_rect(bar_x, bar_y, bar_w, bar_h);

        // Blue fills right from center, Red fills left from center
        let progress = zone.progress as f64;
        if progress > 0.01 {
            r.set_fill_color("rgba(60,120,255,0.85)");
            let fill_w = bar_w * 0.5 * progress;
            r.fill_rect(bar_x + bar_w * 0.5, bar_y, fill_w, bar_h);
        } else if progress < -0.01 {
            r.set_fill_color("rgba(255,60,60,0.85)");
            let fill_w = bar_w * 0.5 * (-progress);
            r.fill_rect(bar_x + bar_w * 0.5 - fill_w, bar_y, fill_w, bar_h);
        }

        // Center divider tick
        r.set_fill_color("rgba(255,255,255,0.5)");
        r.fill_rect(bar_x + bar_w * 0.5 - 0.5, bar_y - 1.0, 1.0, bar_h + 2.0);
    }

    Ok(())
}

/// Draw capture zone HUD pips in screen space (top-center on touch, top-right on desktop).
pub(super) fn draw_zone_hud(
    r: &Canvas2dRenderer,
    game: &Game,
    canvas_w: f64,
    dpr: f64,
    is_touch: bool,
) -> Result<(), JsValue> {
    let zones = &game.zone_manager.zones;
    if zones.is_empty() {
        return Ok(());
    }

    let pip_size = if is_touch { 20.0 * dpr } else { 14.0 * dpr };
    let gap = if is_touch { 5.0 * dpr } else { 3.0 * dpr };
    let margin = 10.0 * dpr;
    let total_w = zones.len() as f64 * (pip_size + gap) - gap;
    // Center horizontally on touch, right-align on desktop
    let start_x = if is_touch {
        (canvas_w - total_w) / 2.0
    } else {
        canvas_w - margin - total_w
    };
    let y = margin;

    for (i, zone) in zones.iter().enumerate() {
        let x = start_x + i as f64 * (pip_size + gap);

        // Background pip
        r.set_fill_color("rgba(0,0,0,0.5)");
        r.fill_rect(x, y, pip_size, pip_size);

        // Color and fill ratio by state
        let (color, fill_ratio) = match zone.state {
            ZoneState::Neutral => ("rgba(150,150,150,0.5)", 0.0),
            ZoneState::Contested => ("rgba(255,200,0,0.7)", zone.progress.abs() as f64),
            ZoneState::Capturing(Faction::Blue) | ZoneState::Controlled(Faction::Blue) => {
                ("rgba(60,120,255,0.8)", zone.progress.abs() as f64)
            }
            ZoneState::Capturing(Faction::Red) | ZoneState::Controlled(Faction::Red) => {
                ("rgba(255,60,60,0.8)", zone.progress.abs() as f64)
            }
            _ => ("rgba(150,150,150,0.5)", 0.0),
        };

        if fill_ratio > 0.01 {
            r.set_fill_color(color);
            let fill_h = pip_size * fill_ratio;
            r.fill_rect(x, y + pip_size - fill_h, pip_size, fill_h);
        }

        // Border
        r.set_stroke_color("rgba(255,255,255,0.35)");
        r.set_line_width(1.0);
        r.stroke_rect(x, y, pip_size, pip_size);
    }

    r.set_alpha(1.0);
    Ok(())
}

/// Draw the minimap HUD (top-left on touch devices, bottom-left on desktop).
pub(super) fn draw_minimap(
    r: &Canvas2dRenderer,
    game: &Game,
    terrain_canvas: &web_sys::HtmlCanvasElement,
    _canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    is_touch: bool,
) -> Result<(), JsValue> {
    let mm_size = (160.0 * dpr).min(canvas_h * 0.25);
    let mm_margin = 8.0 * dpr;
    let mm_x = mm_margin;
    // Top-left on touch (below zone HUD), bottom-left on desktop
    let mm_y = if is_touch {
        mm_margin + 34.0 * dpr // below zone pips
    } else {
        canvas_h - mm_margin - mm_size
    };

    let grid_w = game.grid.width as f64;
    let grid_h = game.grid.height as f64;
    let scale_x = mm_size / grid_w;
    let scale_y = mm_size / grid_h;

    // Background border
    r.set_fill_color("rgba(0,0,0,0.7)");
    let border = 2.0 * dpr;
    r.fill_rect(
        mm_x - border,
        mm_y - border,
        mm_size + border * 2.0,
        mm_size + border * 2.0,
    );

    // Draw pre-rendered terrain (nearest-neighbor for crisp pixels)
    r.save();
    r.set_image_smoothing(false);
    r.draw_canvas_scaled(terrain_canvas, mm_x, mm_y, mm_size, mm_size)?;
    r.restore();

    // Fog of war overlay (semi-transparent black for hidden tiles)
    // Use a coarse approach: sample every 2nd tile for performance
    let step = 2.0_f64;
    let rect_w = (scale_x * step).ceil().max(1.0);
    let rect_h = (scale_y * step).ceil().max(1.0);
    r.set_fill_color("rgba(0,0,0,0.55)");
    let mut gy = 0.0_f64;
    while gy < grid_h {
        let mut gx = 0.0_f64;
        while gx < grid_w {
            let idx = (gy as u32 * game.grid.width + gx as u32) as usize;
            if idx < game.visible.len() && !game.visible[idx] {
                let rx = mm_x + gx * scale_x;
                let ry = mm_y + gy * scale_y;
                r.fill_rect(rx, ry, rect_w, rect_h);
            }
            gx += step;
        }
        gy += step;
    }

    // Capture zones -- colored circles
    for zone in &game.zone_manager.zones {
        let zx = mm_x + zone.center_gx as f64 * scale_x;
        let zy = mm_y + zone.center_gy as f64 * scale_y;
        let zr = (zone.radius as f64 * scale_x).max(2.0 * dpr);

        let color = match zone.state {
            ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => {
                "rgba(60,130,255,0.8)"
            }
            ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => {
                "rgba(255,60,60,0.8)"
            }
            ZoneState::Contested => "rgba(255,200,0,0.8)",
            _ => "rgba(180,180,180,0.6)",
        };
        r.set_fill_color(color);
        r.begin_path();
        r.arc(zx, zy, zr, 0.0, std::f64::consts::TAU)?;
        r.fill();
    }

    // Buildings -- small faction-colored squares
    for b in &game.buildings {
        let bx = mm_x + b.grid_x as f64 * scale_x;
        let by = mm_y + b.grid_y as f64 * scale_y;
        let bs = (2.0 * dpr).max(2.0);
        let color = match b.faction {
            Faction::Blue => "rgba(80,150,255,0.9)",
            _ => "rgba(255,80,80,0.9)",
        };
        r.set_fill_color(color);
        r.fill_rect(bx - bs * 0.5, by - bs * 0.5, bs, bs);
    }

    // Units -- small dots (enemies hidden in fog)
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = grid::world_to_grid(unit.x, unit.y);
        // Hide enemy units outside player visibility
        if unit.faction != Faction::Blue {
            let idx = (gy as u32 * game.grid.width + gx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] {
                continue;
            }
        }
        let ux = mm_x + gx as f64 * scale_x;
        let uy = mm_y + gy as f64 * scale_y;
        let ur = (1.2 * dpr).max(1.0);

        let color = match unit.faction {
            Faction::Blue => "#4a9eff",
            _ => "#ff4a4a",
        };
        r.set_fill_color(color);
        r.fill_rect(ux - ur, uy - ur, ur * 2.0, ur * 2.0);
    }

    // Camera viewport rectangle
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let world_size = grid_w * TILE_SIZE as f64;
    let vx = mm_x + (vl as f64 / world_size) * mm_size;
    let vy = mm_y + (vt as f64 / world_size) * mm_size;
    let vw = ((vr - vl) as f64 / world_size) * mm_size;
    let vh = ((vb - vt) as f64 / world_size) * mm_size;

    r.set_stroke_color("rgba(255,255,255,0.85)");
    r.set_line_width(1.5 * dpr);
    r.stroke_rect(
        vx.max(mm_x),
        vy.max(mm_y),
        vw.min(mm_size - (vx - mm_x).max(0.0)),
        vh.min(mm_size - (vy - mm_y).max(0.0)),
    );

    Ok(())
}

/// Render the static terrain layer of the minimap (1 pixel per tile).
pub(super) fn render_minimap_terrain(
    canvas: &web_sys::HtmlCanvasElement,
    game: &Game,
) -> Result<(), JsValue> {
    let w = game.grid.width;
    let h = game.grid.height;
    let len = (w * h * 4) as usize;
    let mut pixels = vec![0u8; len];

    for gy in 0..h {
        for gx in 0..w {
            let idx = (gy * w + gx) as usize;
            let po = idx * 4;

            let (r, g, b) = match game.grid.get(gx, gy) {
                TileKind::Water => (30, 60, 120),
                TileKind::Forest => (30, 80, 30),
                TileKind::Rock => (90, 85, 75),
                TileKind::Road => (160, 140, 100),
                TileKind::Grass => {
                    if game.grid.elevation(gx, gy) >= 2 {
                        (100, 95, 70)
                    } else if game.grid.decoration(gx, gy) == Some(Decoration::Bush) {
                        (55, 100, 45)
                    } else {
                        (70, 110, 50)
                    }
                }
            };
            pixels[po] = r;
            pixels[po + 1] = g;
            pixels[po + 2] = b;
            pixels[po + 3] = 255;
        }
    }

    let mm_ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<web_sys::CanvasRenderingContext2d>()?;
    let clamped = wasm_bindgen::Clamped(&pixels[..]);
    let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, w, h)?;
    mm_ctx.put_image_data(&image_data, 0.0, 0.0)?;
    Ok(())
}

/// Draw a progress bar when a faction is holding all zones toward victory.
pub(super) fn draw_victory_progress(
    r: &Canvas2dRenderer,
    game: &Game,
    canvas_w: f64,
    _canvas_h: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    let progress = game.zone_manager.victory_progress();
    if progress < f32::EPSILON || game.winner.is_some() {
        return Ok(());
    }

    let faction = match game.zone_manager.victory_candidate {
        Some(f) => f,
        None => return Ok(()),
    };

    let bar_w = 300.0 * dpr;
    let bar_h = 24.0 * dpr;
    let bar_x = (canvas_w - bar_w) / 2.0;
    let bar_y = 60.0 * dpr;
    let radius = 6.0 * dpr;

    // Background
    r.set_fill_color("rgba(0, 0, 0, 0.6)");
    r.begin_path();
    r.round_rect(bar_x, bar_y, bar_w, bar_h, radius)?;
    r.fill();

    // Fill
    let color = match faction {
        Faction::Blue => "rgba(70, 130, 230, 0.9)",
        _ => "rgba(220, 60, 60, 0.9)",
    };
    r.set_fill_color(color);
    let fill_w = bar_w * progress as f64;
    if fill_w > 0.5 {
        r.begin_path();
        r.round_rect(bar_x, bar_y, fill_w, bar_h, radius)?;
        r.fill();
    }

    // Label
    let font_size = 14.0 * dpr;
    r.set_font(&format!("bold {font_size}px sans-serif"));
    r.set_fill_color("white");
    r.set_text_align("center");
    r.set_text_baseline("middle");
    let remaining = ((1.0 - progress) * crate::zone::VICTORY_HOLD_TIME) as u32;
    let label = match faction {
        Faction::Blue => format!("Blue holds all zones - Victory in {remaining}s"),
        _ => format!("Red holds all zones - Victory in {remaining}s"),
    };
    r.fill_text(&label, canvas_w / 2.0, bar_y + bar_h / 2.0);

    Ok(())
}

/// Draw overlays: player highlight circle + attack cone.
pub(super) fn draw_overlays(
    r: &Canvas2dRenderer,
    game: &Game,
    _min_gx: u32,
    _min_gy: u32,
    _max_gx: u32,
    _max_gy: u32,
    _ts: f64,
    _animator: &TurnAnimator,
) -> Result<(), JsValue> {
    // Player position indicator (circle under player)
    if let Some(player) = game.player_unit() {
        r.set_fill_color("rgba(255,255,51,0.2)");
        r.begin_path();
        r.arc(
            player.x as f64,
            player.y as f64,
            24.0,
            0.0,
            std::f64::consts::TAU,
        )?;
        r.fill();

        // Aim direction indicator (wedge showing attack cone)
        let aim = game.player_aim_dir as f64;
        let half = ATTACK_CONE_HALF_ANGLE as f64;
        let radius = 40.0_f64;
        let px = player.x as f64;
        let py = player.y as f64;

        r.set_fill_color("rgba(255,255,100,0.12)");
        r.begin_path();
        r.move_to(px, py);
        r.arc(px, py, radius, aim - half, aim + half)?;
        r.close_path();
        r.fill();

        r.set_stroke_color("rgba(255,255,100,0.35)");
        r.set_line_width(1.0);
        r.begin_path();
        r.move_to(px, py);
        r.arc(px, py, radius, aim - half, aim + half)?;
        r.close_path();
        r.stroke();
    }

    r.set_alpha(1.0);

    Ok(())
}
