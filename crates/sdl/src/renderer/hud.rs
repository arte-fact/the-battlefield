#![allow(clippy::too_many_arguments)]

use battlefield_core::game::Game;
use battlefield_core::grid::{self, TILE_SIZE};
use battlefield_core::render_util;
use battlefield_core::unit::Faction;
use battlefield_core::zone::ZoneState;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::assets::Assets;
use super::draw_helpers::{draw_bar_3slice, draw_panel, draw_ribbon, fill_circle, stroke_circle};
use super::{ClickableButton, GameScreen};

pub(super) fn draw_hud(
    canvas: &mut Canvas<Window>,
    _tc: &TextureCreator<WindowContext>,
    game: &Game,
    assets: &mut Assets,
    _dpi_scale: f64,
) {
    let (w, _h) = canvas.output_size().unwrap_or((960, 640));

    // Player HP bar at top-left
    if let Some(player) = game.player_unit() {
        let bar_x = 10.0_f64;
        let bar_y = 6.0_f64;
        let bar_w = 200.0_f64;
        let bar_h = 46.0_f64;

        canvas.set_blend_mode(BlendMode::Blend);

        if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
            draw_bar_3slice(
                canvas, tex, bw as f64, bh as f64, bar_x, bar_y, bar_w, bar_h, 24.0,
            );
        }

        let ratio = player.hp as f64 / player.stats.max_hp as f64;
        let fill_left = 10.0_f64;
        let fill_right = 10.0_f64;
        let fill_top = 12.0_f64;
        let fill_bottom = 12.0_f64;
        let inner_w = bar_w - fill_left - fill_right;
        let fill_w = (inner_w * ratio).max(0.0);
        let fill_h = (bar_h - fill_top - fill_bottom).max(1.0);
        if fill_w > 0.0 {
            let (hr, hg, hb) = render_util::hp_bar_color(ratio);
            if let Some(ref mut fill_tex) = assets.ui_bar_fill {
                super::safe_set_color_mod(fill_tex, hr, hg, hb);
                let _ = canvas.copy(
                    fill_tex,
                    Rect::new(0, 20, 64, 24),
                    Rect::new(
                        (bar_x + fill_left) as i32,
                        (bar_y + fill_top) as i32,
                        fill_w as u32,
                        fill_h as u32,
                    ),
                );
                super::safe_set_color_mod(fill_tex, 255, 255, 255);
            } else {
                canvas.set_draw_color(Color::RGB(hr, hg, hb));
                let _ = canvas.fill_rect(Rect::new(
                    (bar_x + fill_left) as i32,
                    (bar_y + fill_top) as i32,
                    fill_w as u32,
                    fill_h as u32,
                ));
            }
        }

        if assets.ui_bar_base.is_none() {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 120));
            let _ = canvas.draw_rect(Rect::new(
                bar_x as i32,
                bar_y as i32,
                bar_w as u32,
                bar_h as u32,
            ));
        }

        // Authority bar
        let auth_x = bar_x;
        let auth_y = bar_y + bar_h + 6.0;
        let auth_w = bar_w;
        let auth_h = bar_h;

        if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
            draw_bar_3slice(
                canvas, tex, bw as f64, bh as f64, auth_x, auth_y, auth_w, auth_h, 24.0,
            );
        }

        let auth_ratio = game.authority as f64 / 100.0;
        let auth_fill_left = 10.0_f64;
        let auth_fill_right = 10.0_f64;
        let auth_fill_top = 12.0_f64;
        let auth_fill_bottom = 12.0_f64;
        let auth_inner_w = auth_w - auth_fill_left - auth_fill_right;
        let auth_fill_w = (auth_inner_w * auth_ratio).max(0.0);
        let auth_fill_h = (auth_h - auth_fill_top - auth_fill_bottom).max(1.0);
        if auth_fill_w > 0.0 {
            let (ar, ag, ab) = if game.authority >= 80.0 {
                (255u8, 200u8, 50u8)
            } else if game.authority >= 40.0 {
                (100, 200, 80)
            } else {
                (150, 150, 160)
            };
            if let Some(ref mut fill_tex) = assets.ui_bar_fill {
                super::safe_set_color_mod(fill_tex, ar, ag, ab);
                let _ = canvas.copy(
                    fill_tex,
                    Rect::new(0, 20, 64, 24),
                    Rect::new(
                        (auth_x + auth_fill_left) as i32,
                        (auth_y + auth_fill_top) as i32,
                        auth_fill_w as u32,
                        auth_fill_h as u32,
                    ),
                );
                super::safe_set_color_mod(fill_tex, 255, 255, 255);
            } else {
                canvas.set_draw_color(Color::RGB(ar, ag, ab));
                let _ = canvas.fill_rect(Rect::new(
                    (auth_x + auth_fill_left) as i32,
                    (auth_y + auth_fill_top) as i32,
                    auth_fill_w as u32,
                    auth_fill_h as u32,
                ));
            }
        }

        if assets.ui_bar_base.is_none() {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 60));
            let _ = canvas.draw_rect(Rect::new(
                auth_x as i32,
                auth_y as i32,
                auth_w as u32,
                auth_h as u32,
            ));
        }
    }

    // Zone control indicators at top-center on paper panel
    let zone_count = game.zone_manager.zones.len() as u32;
    if zone_count > 0 {
        let pip_r = 14i32;
        let pip_d = pip_r * 2;
        let gap_z = 12i32;
        let pad_z = 18i32;
        let n = zone_count as i32;
        let zone_panel_w = (n * pip_d + (n - 1) * gap_z + pad_z * 2) as u32;
        let zone_panel_h = (pip_d + pad_z * 2) as u32;
        let zone_panel_x = w as i32 / 2 - zone_panel_w as i32 / 2;
        let zone_panel_y = 10i32;

        if let Some((ref tex, bw, bh)) = assets.ui_special_paper {
            draw_panel(
                canvas,
                tex,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                bw as f64,
                bh as f64,
                zone_panel_x as f64,
                zone_panel_y as f64,
                zone_panel_w as f64,
                zone_panel_h as f64,
            );
        } else {
            canvas.set_blend_mode(BlendMode::Blend);
            canvas.set_draw_color(Color::RGBA(40, 30, 15, 220));
            let _ = canvas.fill_rect(Rect::new(
                zone_panel_x,
                zone_panel_y,
                zone_panel_w,
                zone_panel_h,
            ));
        }

        for (i, zone) in game.zone_manager.zones.iter().enumerate() {
            let cx = zone_panel_x + pad_z + pip_r + (i as i32) * (pip_d + gap_z);
            let cy = zone_panel_y + pad_z + pip_r;
            let progress = zone.progress.abs();

            let (cr, cg, cb, alpha) = match zone.state {
                ZoneState::Neutral => (100u8, 100, 100, 80u8),
                ZoneState::Contested => (255, 200, 0, 160),
                ZoneState::Capturing(Faction::Blue) => (60, 130, 255, 180),
                ZoneState::Controlled(Faction::Blue) => (60, 130, 255, 255),
                ZoneState::Capturing(Faction::Red) => (255, 60, 60, 180),
                ZoneState::Controlled(Faction::Red) => (255, 60, 60, 255),
            };

            // Background circle
            canvas.set_draw_color(Color::RGBA(30, 25, 20, 130));
            fill_circle(canvas, cx, cy, pip_r);

            // Inner fill scaled by progress
            let inner_r = ((pip_r as f32) * progress.max(0.15)) as i32;
            canvas.set_draw_color(Color::RGBA(cr, cg, cb, alpha));
            fill_circle(canvas, cx, cy, inner_r);

            // Ring
            canvas.set_draw_color(Color::RGBA(cr, cg, cb, alpha.saturating_sub(40)));
            stroke_circle(canvas, cx, cy, pip_r);
        }
    }
}

pub(super) fn draw_minimap(canvas: &mut Canvas<Window>, game: &Game, assets: &Assets) {
    let (canvas_w, _canvas_h) = canvas.output_size().unwrap_or((960, 640));
    let mm_size = 240_u32;
    let pad = 20_i32;
    let panel_size = mm_size + pad as u32 * 2;
    let panel_margin = 10_i32;
    let panel_x = canvas_w as i32 - panel_margin - panel_size as i32;
    let panel_y = panel_margin;
    let mm_x = panel_x + pad;
    let mm_y = panel_y + pad;

    let grid_w = game.grid.width;
    let grid_h = game.grid.height;
    let scale_x = mm_size as f32 / grid_w as f32;
    let scale_y = mm_size as f32 / grid_h as f32;

    // Paper background
    if let Some((ref tex, tw, th)) = assets.ui_special_paper {
        draw_panel(
            canvas,
            tex,
            &render_util::NINE_SLICE_SPECIAL_PAPER,
            tw as f64,
            th as f64,
            panel_x as f64,
            panel_y as f64,
            panel_size as f64,
            panel_size as f64,
        );
    } else {
        canvas.set_blend_mode(BlendMode::Blend);
        canvas.set_draw_color(Color::RGBA(40, 30, 15, 220));
        let _ = canvas.fill_rect(Rect::new(panel_x, panel_y, panel_size, panel_size));
    }

    // Dark terrain background
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 160));
    let _ = canvas.fill_rect(Rect::new(mm_x, mm_y, mm_size, mm_size));

    // Terrain dots
    let step = 2_u32;
    let rect_w = (scale_x * step as f32).ceil() as u32;
    let rect_h = (scale_y * step as f32).ceil() as u32;
    let mut gy = 0_u32;
    while gy < grid_h {
        let mut gx = 0_u32;
        while gx < grid_w {
            let (r, g, b) = match game.grid.get(gx, gy) {
                grid::TileKind::Water => (30, 60, 120),
                grid::TileKind::Forest => (30, 80, 30),
                grid::TileKind::Rock => (90, 85, 75),
                grid::TileKind::Road => (160, 140, 100),
                grid::TileKind::Grass => {
                    if game.grid.elevation(gx, gy) >= 2 {
                        (100, 95, 70)
                    } else if game.grid.decoration(gx, gy) == Some(grid::Decoration::Bush) {
                        (55, 100, 45)
                    } else {
                        (70, 110, 50)
                    }
                }
            };
            let rx = mm_x + (gx as f32 * scale_x) as i32;
            let ry = mm_y + (gy as f32 * scale_y) as i32;
            canvas.set_draw_color(Color::RGB(r, g, b));
            let _ = canvas.fill_rect(Rect::new(rx, ry, rect_w.max(1), rect_h.max(1)));
            gx += step;
        }
        gy += step;
    }

    // Fog overlay on minimap
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 140));
    let mut gy = 0_u32;
    while gy < grid_h {
        let mut gx = 0_u32;
        while gx < grid_w {
            let idx = (gy * grid_w + gx) as usize;
            if idx < game.visible.len() && !game.visible[idx] {
                let rx = mm_x + (gx as f32 * scale_x) as i32;
                let ry = mm_y + (gy as f32 * scale_y) as i32;
                let _ = canvas.fill_rect(Rect::new(rx, ry, rect_w.max(1), rect_h.max(1)));
            }
            gx += step;
        }
        gy += step;
    }

    // Zone circles
    for zone in &game.zone_manager.zones {
        let zx = mm_x + (zone.center_gx as f32 * scale_x) as i32;
        let zy = mm_y + (zone.center_gy as f32 * scale_y) as i32;
        let zr = ((zone.radius as f32 * scale_x) as i32).max(2);

        let (r, g, b) = render_util::zone_pip_rgb(zone.state);
        canvas.set_draw_color(Color::RGBA(r, g, b, 200));
        fill_circle(canvas, zx, zy, zr);
    }

    // Unit dots
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = grid::world_to_grid(unit.x, unit.y);
        if unit.faction != Faction::Blue {
            let idx = (gy as u32 * grid_w + gx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] {
                continue;
            }
        }
        let ux = mm_x + (gx as f32 * scale_x) as i32;
        let uy = mm_y + (gy as f32 * scale_y) as i32;
        let ur = 1_u32.max((scale_x * 0.8) as u32);

        let color = match unit.faction {
            Faction::Blue => Color::RGB(74, 158, 255),
            Faction::Red => Color::RGB(255, 74, 74),
        };
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(Rect::new(ux - ur as i32, uy - ur as i32, ur * 2, ur * 2));
    }

    // Camera viewport rectangle
    let cam = &game.camera;
    let (vl, vt, vr, vb) = cam.visible_rect();
    let world_size = grid_w as f32 * TILE_SIZE;
    let vx = mm_x as f32 + (vl / world_size) * mm_size as f32;
    let vy = mm_y as f32 + (vt / world_size) * mm_size as f32;
    let vw = ((vr - vl) / world_size) * mm_size as f32;
    let vh = ((vb - vt) / world_size) * mm_size as f32;

    canvas.set_draw_color(Color::RGBA(255, 255, 255, 200));
    let _ = canvas.draw_rect(Rect::new(
        (vx.max(mm_x as f32)) as i32,
        (vy.max(mm_y as f32)) as i32,
        (vw.min(mm_size as f32 - (vx - mm_x as f32).max(0.0))) as u32,
        (vh.min(mm_size as f32 - (vy - mm_y as f32).max(0.0))) as u32,
    ));
}

pub(super) fn draw_screen_overlay(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    screen: GameScreen,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    dpi_scale: f64,
) -> Vec<ClickableButton> {
    let (w, h) = canvas.output_size().unwrap_or((960, 640));
    canvas.set_blend_mode(BlendMode::Blend);

    let layout = match screen {
        GameScreen::Playing => return Vec::new(),
        GameScreen::MainMenu => battlefield_core::ui::main_menu_layout(),
        GameScreen::PlayerDeath => battlefield_core::ui::death_layout(),
        GameScreen::GameWon => battlefield_core::ui::result_layout(true),
        GameScreen::GameLost => battlefield_core::ui::result_layout(false),
    };

    draw_layout_overlay(
        canvas,
        tc,
        assets,
        w,
        h,
        &layout,
        mouse_x,
        mouse_y,
        focused_button,
        gamepad_connected,
        dpi_scale,
    )
}

fn draw_layout_overlay(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    w: u32,
    h: u32,
    layout: &battlefield_core::ui::ScreenLayout,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    dpi_scale: f64,
) -> Vec<ClickableButton> {
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;

    let (or, og, ob, oa) = layout.overlay;
    canvas.set_draw_color(Color::RGBA(or, og, ob, oa));
    let _ = canvas.fill_rect(Rect::new(0, 0, w, h));

    let panel_y = if let Some((pw, ph)) = layout.panel_size {
        let px = cx - pw / 2.0;
        let py = cy - ph / 2.0;
        if let Some((ref tex, aw, ah)) = assets.ui_special_paper {
            draw_panel(
                canvas,
                tex,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                aw as f64,
                ah as f64,
                px,
                py,
                pw,
                ph,
            );
        }
        py
    } else {
        cy
    };

    if let Some((color_row, ribbon_offset_y, ribbon_w, ribbon_h)) = layout.title_ribbon {
        let ribbon_x = cx - ribbon_w / 2.0;
        let ribbon_y = panel_y + ribbon_offset_y;
        if let Some(ref tex) = assets.ui_big_ribbons {
            draw_ribbon(
                canvas, tex, color_row, ribbon_x, ribbon_y, ribbon_w, ribbon_h, ribbon_h,
            );
        }
    }

    if let Some(ref title) = layout.title {
        let tx = (cx + title.offset_x) as i32;
        let ty = (cy + title.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &title.text,
            tx,
            ty,
            (title.size * dpi_scale) as f32,
            Color::RGBA(title.r, title.g, title.b, title.a),
        );
    }

    if let Some(ref sub) = layout.subtitle {
        let sx = (cx + sub.offset_x) as i32;
        let sy = (cy + sub.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &sub.text,
            sx,
            sy,
            (sub.size * dpi_scale) as f32,
            Color::RGBA(sub.r, sub.g, sub.b, sub.a),
        );
    }

    let mut clickable_buttons = Vec::new();
    for (i, btn) in layout.buttons.iter().enumerate() {
        let bx = cx + btn.offset_x;
        let by = cy + btn.offset_y;
        let btn_x = bx - btn.w / 2.0;
        let btn_y = by - btn.h / 2.0;

        let is_focused = gamepad_connected && i == focused_button;
        let mouse_hovering = mouse_x as f64 >= btn_x
            && mouse_x as f64 <= btn_x + btn.w
            && mouse_y as f64 >= btn_y
            && mouse_y as f64 <= btn_y + btn.h;
        let hovering = mouse_hovering || is_focused;

        let btn_atlas = match btn.style {
            battlefield_core::ui::ButtonStyle::Blue => assets.ui_blue_btn.as_ref(),
            battlefield_core::ui::ButtonStyle::Red => assets.ui_red_btn.as_ref(),
        };

        if let Some((tex, aw, ah)) = btn_atlas {
            draw_panel(
                canvas,
                tex,
                &render_util::NINE_SLICE_BUTTON,
                *aw as f64,
                *ah as f64,
                btn_x,
                btn_y,
                btn.w,
                btn.h,
            );
        }

        if hovering {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 40));
            let _ = canvas.fill_rect(Rect::new(
                btn_x as i32,
                btn_y as i32,
                btn.w as u32,
                btn.h as u32,
            ));
        }

        assets.text.draw_text_centered(
            canvas,
            tc,
            btn.label,
            bx as i32,
            by as i32,
            24.0 * dpi_scale as f32,
            Color::RGB(255, 255, 255),
        );

        clickable_buttons.push(ClickableButton {
            x: btn_x,
            y: btn_y,
            w: btn.w,
            h: btn.h,
            action: btn.action,
        });
    }

    for hint in &layout.hints {
        let hx = (cx + hint.offset_x) as i32;
        let hy = (cy + hint.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &hint.text,
            hx,
            hy,
            (hint.size.max(24.0) * dpi_scale) as f32,
            Color::RGBA(hint.r, hint.g, hint.b, hint.a),
        );
    }

    clickable_buttons
}
