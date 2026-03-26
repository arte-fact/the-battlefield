use super::assets::LoadedTextures;
use crate::renderer::{Canvas2dRenderer, Renderer};
use battlefield_core::animation::TurnAnimator;
use battlefield_core::game::{Game, ATTACK_CONE_HALF_ANGLE, ORDER_FLASH_DURATION};
use battlefield_core::grid::{self, Decoration, TileKind, TILE_SIZE};
use battlefield_core::render_util;
use battlefield_core::asset_manifest;
use battlefield_core::unit::{Faction, UnitKind};
use battlefield_core::zone::ZoneState;
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
        let (gx, gy) = unit.grid_cell();
        if !render_util::is_visible_to_player(unit.faction, gx, gy, &game.visible, game.grid.width)
        {
            continue;
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
        let label = match render_util::order_label(unit.order.as_ref()) {
            Some(l) => l,
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

    // Dot markers: green for player, yellow for recruited units
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = unit.grid_cell();
        if !render_util::is_visible_to_player(unit.faction, gx, gy, &game.visible, game.grid.width)
        {
            continue;
        }

        let color = if unit.is_player {
            "rgba(50,220,50,0.85)"
        } else if game.recruited.contains(&unit.id) {
            "rgba(255,220,50,0.85)"
        } else {
            continue;
        };

        let wx = unit.x as f64;
        let wy = unit.y as f64 - (TILE_SIZE as f64) * 0.95;
        r.set_fill_color(color);
        r.begin_path();
        r.arc(wx, wy, 4.0, 0.0, std::f64::consts::TAU)?;
        r.fill();
    }

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
        r.fill_text(zone.name, cx, label_y);

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

/// Draw capture zone indicators on a paper panel at top-center.
pub(super) fn draw_zone_hud(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    canvas_w: f64,
    dpr: f64,
    _is_touch: bool,
) -> Result<(), JsValue> {
    let zones = &game.zone_manager.zones;
    if zones.is_empty() {
        return Ok(());
    }

    let pip_r = 14.0 * dpr; // circle radius
    let pip_d = pip_r * 2.0;
    let gap = 12.0 * dpr;
    let pad = 18.0 * dpr;
    let n = zones.len() as f64;
    let panel_w = n * pip_d + (n - 1.0) * gap + pad * 2.0;
    let panel_h = pip_d + pad * 2.0;
    let panel_x = (canvas_w - panel_w) / 2.0;
    let panel_y = 10.0 * dpr;

    // Paper background
    if let Some((ref atlas, aw, ah)) = loaded.ui_panel_atlas {
        use battlefield_core::render_util::NINE_SLICE_SPECIAL_PAPER;
        super::screens::draw_9slice_panel(
            r,
            atlas,
            &NINE_SLICE_SPECIAL_PAPER,
            aw as f64,
            ah as f64,
            panel_x,
            panel_y,
            panel_w,
            panel_h,
        )?;
    } else {
        r.set_fill_color("rgba(40,30,15,0.85)");
        r.fill_rect(panel_x, panel_y, panel_w, panel_h);
    }

    for (i, zone) in zones.iter().enumerate() {
        let cx = panel_x + pad + pip_r + i as f64 * (pip_d + gap);
        let cy = panel_y + pad + pip_r;
        let progress = zone.progress.abs() as f64;

        let (fill_color, ring_color, inner_alpha) = match zone.state {
            ZoneState::Neutral => ("rgba(100,100,100,0.3)", "rgba(160,160,160,0.4)", 0.3),
            ZoneState::Contested => ("rgba(255,200,0,0.6)", "rgba(255,220,60,0.7)", 0.6),
            ZoneState::Capturing(Faction::Blue) => {
                ("rgba(60,130,255,0.7)", "rgba(100,170,255,0.8)", 0.7)
            }
            ZoneState::Controlled(Faction::Blue) => {
                ("rgba(60,130,255,1.0)", "rgba(140,200,255,1.0)", 1.0)
            }
            ZoneState::Capturing(Faction::Red) => {
                ("rgba(255,60,60,0.7)", "rgba(255,110,110,0.8)", 0.7)
            }
            ZoneState::Controlled(Faction::Red) => {
                ("rgba(255,60,60,1.0)", "rgba(255,150,150,1.0)", 1.0)
            }
        };

        // Background circle
        r.begin_path();
        let _ = r.arc(cx, cy, pip_r, 0.0, std::f64::consts::TAU);
        r.set_fill_color("rgba(30,25,20,0.5)");
        r.fill();

        // Inner fill circle scaled by progress
        let inner_r = pip_r * progress.max(0.15);
        r.begin_path();
        let _ = r.arc(cx, cy, inner_r, 0.0, std::f64::consts::TAU);
        r.set_fill_color(fill_color);
        r.fill();

        // Outer ring
        r.begin_path();
        let _ = r.arc(cx, cy, pip_r, 0.0, std::f64::consts::TAU);
        r.set_stroke_color(ring_color);
        r.set_line_width(2.0 * dpr);
        r.stroke();

        // Glow for fully controlled
        if inner_alpha >= 1.0 {
            r.set_alpha(0.3);
            r.begin_path();
            let _ = r.arc(cx, cy, pip_r + 2.0 * dpr, 0.0, std::f64::consts::TAU);
            r.set_stroke_color(ring_color);
            r.set_line_width(2.0 * dpr);
            r.stroke();
            r.set_alpha(1.0);
        }
    }

    Ok(())
}

/// Draw the minimap HUD on a paper panel (bottom-left).
pub(super) fn draw_minimap(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    terrain_canvas: &web_sys::HtmlCanvasElement,
    _canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    _is_touch: bool,
) -> Result<(), JsValue> {
    let mm_size = (200.0 * dpr).min(canvas_h * 0.3);
    let pad = 20.0 * dpr;
    let panel_size = mm_size + pad * 2.0;
    let panel_margin = 10.0 * dpr;
    let panel_x = panel_margin;
    let panel_y = canvas_h - panel_margin - panel_size;
    let mm_x = panel_x + pad;
    let mm_y = panel_y + pad;

    let grid_w = game.grid.width as f64;
    let grid_h = game.grid.height as f64;
    let scale_x = mm_size / grid_w;
    let scale_y = mm_size / grid_h;

    // Paper background
    if let Some((ref atlas, aw, ah)) = loaded.ui_panel_atlas {
        use battlefield_core::render_util::NINE_SLICE_SPECIAL_PAPER;
        super::screens::draw_9slice_panel(
            r,
            atlas,
            &NINE_SLICE_SPECIAL_PAPER,
            aw as f64,
            ah as f64,
            panel_x,
            panel_y,
            panel_size,
            panel_size,
        )?;
    } else {
        r.set_fill_color("rgba(40,30,15,0.85)");
        r.fill_rect(panel_x, panel_y, panel_size, panel_size);
    }

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
    let remaining = ((1.0 - progress) * battlefield_core::zone::VICTORY_HOLD_TIME) as u32;
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
        let radius = battlefield_core::grid::TILE_SIZE as f64;
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

/// Draw the authority bar in screen space (below the HP bar), same sprite style as HP bar.
pub(super) fn draw_authority_bar(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    dpr: f64,
) -> Result<(), JsValue> {
    if game.player_unit().is_none() {
        return Ok(());
    }

    let bar_x = 10.0 * dpr;
    let bar_y = (8.0 + 24.0 + 6.0) * dpr; // below the HP bar
    let bar_w = 140.0 * dpr;
    let bar_h = 24.0 * dpr; // same height as HP bar

    let auth_ratio = game.authority as f64 / 100.0;
    let inset_x = 24.0 * dpr;
    let inset_y = 6.0 * dpr;
    let inner_w = bar_w - inset_x * 2.0;
    let fill_w = (inner_w * auth_ratio).max(0.0);

    // 1. Fill (behind the frame)
    if fill_w > 0.0 {
        let (ar, ag, ab) = if game.authority >= 80.0 {
            (255u8, 200u8, 50u8)
        } else if game.authority >= 40.0 {
            (100, 200, 80)
        } else {
            (150, 150, 160)
        };
        if let Some(tex) = loaded.ui_bar_fill {
            r.draw_texture(
                tex,
                0.0,
                0.0,
                64.0,
                64.0,
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            )?;
            r.set_fill_color(&format!("rgba({ar},{ag},{ab},0.7)"));
            r.set_composite_op("multiply");
            r.fill_rect(
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            );
            r.set_composite_op("source-over");
        } else {
            r.set_fill_color(&format!("rgb({ar},{ag},{ab})"));
            r.fill_rect(
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            );
        }
    }

    // 2. Bar frame (3-part, on top of fill)
    if let Some(tex) = loaded.ui_bar_base {
        let cap_w = 28.0 * dpr;
        let cap = cap_w.min(bar_w / 2.0);
        let mid_w = (bar_w - cap * 2.0).max(0.0);

        let (lsx, lsy, lsw, lsh) = render_util::BAR_LEFT;
        let (csx, csy, csw, csh) = render_util::BAR_CENTER;
        let (rsx, rsy, rsw, rsh) = render_util::BAR_RIGHT;

        r.draw_texture(tex, lsx, lsy, lsw, lsh, bar_x, bar_y, cap, bar_h)?;
        if mid_w > 0.0 {
            r.draw_texture(tex, csx, csy, csw, csh, bar_x + cap, bar_y, mid_w, bar_h)?;
        }
        r.draw_texture(
            tex,
            rsx,
            rsy,
            rsw,
            rsh,
            bar_x + cap + mid_w,
            bar_y,
            cap,
            bar_h,
        )?;
    } else {
        r.set_stroke_color("rgba(255,255,255,0.5)");
        r.set_line_width(1.0);
        r.stroke_rect(bar_x, bar_y, bar_w, bar_h);
    }

    Ok(())
}

/// Draw a panel showing recruited follower counts with avatar portraits on a paper background.
/// Includes rank name and follower count. Positioned at top-right.
pub(super) fn draw_follower_panel(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    canvas_w: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    let counts = game.recruited_counts();

    let icon_size = 48.0 * dpr;
    let pad = 12.0 * dpr;
    let gap = 6.0 * dpr;
    let count_font = 14.0 * dpr;
    let header_font = 11.0 * dpr;
    let header_h = 18.0 * dpr;
    let entry_w = icon_size + 4.0 * dpr;

    // Always show all 4 unit types
    let all_kinds: [(UnitKind, usize); 4] = [
        (UnitKind::Warrior, counts[0]),
        (UnitKind::Archer, counts[1]),
        (UnitKind::Lancer, counts[2]),
        (UnitKind::Monk, counts[3]),
    ];

    let cols = 4.0_f64;
    let panel_w = cols * entry_w + (cols - 1.0).max(0.0) * gap + pad * 2.0;
    let panel_h = header_h + icon_size + count_font + pad * 2.0 + gap;
    let panel_x = canvas_w - panel_w - 10.0 * dpr;
    let panel_y = 8.0 * dpr;

    // Paper 9-slice background
    if let Some((ref atlas, aw, ah)) = loaded.ui_panel_atlas {
        use battlefield_core::render_util::NINE_SLICE_SPECIAL_PAPER;
        super::screens::draw_9slice_panel(
            r,
            atlas,
            &NINE_SLICE_SPECIAL_PAPER,
            aw as f64,
            ah as f64,
            panel_x,
            panel_y,
            panel_w,
            panel_h,
        )?;
    } else {
        r.set_fill_color("rgba(40,30,15,0.85)");
        r.fill_rect(panel_x, panel_y, panel_w, panel_h);
    }

    // Header: rank + follower count
    let rank = game.authority_rank_name();
    let followers = game.follower_count();
    let max_followers = game.authority_max_followers();
    let header = format!("{rank}  {followers} of {max_followers}");
    r.set_font(&format!("bold {header_font}px sans-serif"));
    r.set_fill_color("rgba(255,255,255,0.95)");
    r.set_text_align("center");
    r.set_text_baseline("top");
    r.fill_text(&header, panel_x + panel_w / 2.0, panel_y + pad * 0.6);

    // Portraits + counts
    let row_y = panel_y + pad + header_h;
    r.set_font(&format!("bold {count_font}px sans-serif"));
    r.set_text_align("center");
    r.set_text_baseline("top");

    for (i, &(kind, count)) in all_kinds.iter().enumerate() {
        let cx = panel_x + pad + i as f64 * (entry_w + gap) + entry_w / 2.0;
        let ix = cx - icon_size / 2.0;

        // Dim avatars with 0 count
        if count == 0 {
            r.set_alpha(0.35);
        }

        let avatar_idx = asset_manifest::avatar_index(kind);
        if let Some(&tex_id) = loaded.avatar_textures.get(avatar_idx) {
            r.draw_texture(
                tex_id, 0.0, 0.0, 256.0, 256.0, ix, row_y, icon_size, icon_size,
            )?;
        }

        if count == 0 {
            r.set_alpha(1.0);
        }

        r.set_fill_color("rgba(255,255,255,0.95)");
        r.fill_text(&format!("{count}"), cx, row_y + icon_size + 2.0 * dpr);
    }

    Ok(())
}

/// Draw a sprite-based player HP bar in screen space (top-left corner).
///
/// Uses the BigBar_Base (3-slice frame) and BigBar_Fill (colored fill) textures
/// when available, falling back to plain colored rects.
pub(super) fn draw_player_hp_bar(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    dpr: f64,
) -> Result<(), JsValue> {
    let player = match game.player_unit() {
        Some(p) => p,
        None => return Ok(()),
    };

    let bar_x = 10.0 * dpr;
    let bar_y = 8.0 * dpr;
    let bar_w = 140.0 * dpr;
    let bar_h = 24.0 * dpr;

    let ratio = player.hp as f64 / player.stats.max_hp as f64;
    let inset_x = 24.0 * dpr;
    let inset_y = 6.0 * dpr;
    let inner_w = bar_w - inset_x * 2.0;
    let fill_w = (inner_w * ratio).max(0.0);

    // 1. Draw fill (behind the frame)
    if fill_w > 0.0 {
        let (hr, hg, hb) = render_util::hp_bar_color(ratio);
        if let Some(tex) = loaded.ui_bar_fill {
            r.draw_texture(
                tex,
                0.0,
                0.0,
                64.0,
                64.0,
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            )?;
            // Tint the fill by overlaying a colored rect with multiply compositing.
            // Canvas2D does not support color mod like SDL, so this approximates it.
            r.set_fill_color(&format!("rgba({hr},{hg},{hb},0.7)"));
            r.set_composite_op("multiply");
            r.fill_rect(
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            );
            r.set_composite_op("source-over");
        } else {
            r.set_fill_color(&format!("rgb({hr},{hg},{hb})"));
            r.fill_rect(
                bar_x + inset_x,
                bar_y + inset_y,
                fill_w,
                bar_h - inset_y * 2.0,
            );
        }
    }

    // 2. Draw bar frame (3-part, exact source rects, on top of fill)
    if let Some(tex) = loaded.ui_bar_base {
        let cap_w = 28.0 * dpr;
        let cap = cap_w.min(bar_w / 2.0);
        let mid_w = (bar_w - cap * 2.0).max(0.0);

        let (lsx, lsy, lsw, lsh) = render_util::BAR_LEFT;
        let (csx, csy, csw, csh) = render_util::BAR_CENTER;
        let (rsx, rsy, rsw, rsh) = render_util::BAR_RIGHT;

        r.draw_texture(tex, lsx, lsy, lsw, lsh, bar_x, bar_y, cap, bar_h)?;
        if mid_w > 0.0 {
            r.draw_texture(tex, csx, csy, csw, csh, bar_x + cap, bar_y, mid_w, bar_h)?;
        }
        r.draw_texture(
            tex,
            rsx,
            rsy,
            rsw,
            rsh,
            bar_x + cap + mid_w,
            bar_y,
            cap,
            bar_h,
        )?;
    } else {
        r.set_stroke_color("rgba(255,255,255,0.5)");
        r.set_line_width(1.0);
        r.stroke_rect(bar_x, bar_y, bar_w, bar_h);
    }

    Ok(())
}
