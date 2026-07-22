#![allow(clippy::too_many_arguments)]

use battlefield_core::camera::Camera;
use battlefield_core::game::Game;
use battlefield_core::grid::TILE_SIZE;
use battlefield_core::render_util;
use battlefield_core::unit::{Faction, OrderKind};
use battlefield_core::zone::ZoneState;
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::assets::Assets;
use super::draw_helpers::{draw_bar_3slice, draw_small_ribbon, fill_circle, stroke_circle};
use battlefield_core::rendering::{DrawBackend, SpriteInfo, SpriteKey};

use super::world_to_screen;

pub(super) fn draw_zones(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &mut Assets,
    game: &Game,
    cam: &Camera,
    ts: f32,
    dpi_scale: f64,
) {
    canvas.set_blend_mode(BlendMode::Blend);
    for zone in &game.zone_manager.zones {
        let (sx, sy) = world_to_screen(zone.center_wx, zone.center_wy, cam);
        let radius = (zone.radius as f32 * ts) as i32;

        let (fr, fg, fb, fa) = render_util::zone_fill_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(fr, fg, fb, fa));
        fill_circle(canvas, sx, sy, radius);

        let (br, bg, bb, ba) = render_util::zone_border_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(br, bg, bb, ba / 3));
        stroke_circle(canvas, sx, sy, radius - 2);
        canvas.set_draw_color(Color::RGBA(br, bg, bb, ba));
        stroke_circle(canvas, sx, sy, radius);
        stroke_circle(canvas, sx, sy, radius + 1);

        let zoom = cam.zoom as f64;
        let zone_font = (24.0 * dpi_scale as f32) * cam.zoom;
        let ribbon_h = 54.0 * zoom;

        let bar_w = 160.0 * zoom;
        let bar_h = 46.0 * zoom;
        let total_h = ribbon_h + 2.0 * zoom + bar_h;
        let top_y = sy as f64 - radius as f64 - total_h - 2.0 * zoom;
        let name_y = (top_y + ribbon_h / 2.0) as i32;
        let bar_x = sx as f64 - bar_w / 2.0;
        let bar_y = top_y + ribbon_h + 2.0 * zoom;

        let ribbon_row = match zone.state {
            ZoneState::Controlled(f) | ZoneState::Capturing(f) => match f {
                Faction::Blue => 1,
                Faction::Red => 3,
                Faction::Yellow => 5,
                Faction::Purple => 7,
                Faction::Villager => 9,
            },
            _ => 9,
        };

        if let Some(ref tex) = assets.ui_small_ribbons {
            let (tw, _th) = assets.text.measure_text(zone.name, zone_font);
            let center_w = tw as f64 + 4.0 * zoom;
            draw_small_ribbon(
                canvas,
                tex,
                ribbon_row,
                sx as f64,
                name_y as f64,
                center_w,
                zoom,
            );
        }

        assets.text.draw_text_centered(
            canvas,
            tc,
            zone.name,
            sx,
            name_y,
            zone_font,
            Color::RGBA(255, 255, 255, 220),
        );

        if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
            draw_bar_3slice(
                canvas,
                tex,
                bw as f64,
                bh as f64,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                24.0 * zoom,
            );
        } else {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 100));
            let _ = canvas.fill_rect(Rect::new(
                bar_x as i32,
                bar_y as i32,
                bar_w as u32,
                bar_h as u32,
            ));
        }

        let progress = zone.progress as f64;
        let acting = zone.owner.or(zone.capturing);
        let fill_inset_x = 10.0 * zoom;
        let fill_inset_y = 12.0 * zoom;
        let inner_w = bar_w - fill_inset_x * 2.0;
        let fill_h = (bar_h - fill_inset_y * 2.0).max(1.0);
        if let Some(acting) = acting.filter(|_| progress > 0.01) {
            let (fr, fg, fb) = acting.rgb();
            let fill_w = (inner_w * progress).max(0.0);
            if fill_w > 0.0 {
                let fill_x = bar_x + fill_inset_x;
                if let Some(ref mut fill_tex) = assets.ui_bar_fill {
                    super::safe_set_color_mod(fill_tex, fr, fg, fb);
                    let _ = canvas.copy(
                        fill_tex,
                        Rect::new(0, 20, 64, 24),
                        Rect::new(
                            fill_x as i32,
                            (bar_y + fill_inset_y) as i32,
                            fill_w as u32,
                            fill_h as u32,
                        ),
                    );
                    super::safe_set_color_mod(fill_tex, 255, 255, 255);
                } else {
                    canvas.set_draw_color(Color::RGBA(fr, fg, fb, 200));
                    let _ = canvas.fill_rect(Rect::new(
                        fill_x as i32,
                        (bar_y + fill_inset_y) as i32,
                        fill_w as u32,
                        fill_h as u32,
                    ));
                }
            }
        }
    }
}

pub(super) fn draw_player_overlay(canvas: &mut Canvas<Window>, game: &Game, cam: &Camera) {
    let player = match game.player_unit() {
        Some(p) => p,
        None => return,
    };

    let (px, py) = world_to_screen(player.x, player.y, cam);

    let radius = (24.0 * cam.zoom) as i32;
    canvas.set_draw_color(Color::RGBA(255, 255, 51, 50));
    draw_filled_circle(canvas, px, py, radius);

    // Command-radius pulse (expanding ring when an order is issued)
    if game.order_pulse > 0.0 {
        let alpha = game.order_pulse / 0.6;
        let progress = 1.0 - alpha;
        let ring_r = (game.order_pulse_radius * progress * cam.zoom) as i32;
        if ring_r > 0 {
            canvas.set_draw_color(Color::RGBA(230, 217, 102, (alpha * 200.0) as u8));
            super::draw_helpers::stroke_circle(canvas, px, py, ring_r);
        }
    }
}

fn draw_filled_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
    for dy in -radius..=radius {
        let dx = ((radius * radius - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.draw_line((cx - dx, cy + dy), (cx + dx, cy + dy));
    }
}

// ───────────────────────────────────────────────────────────────────────────
// SdlBackend — implements core's DrawBackend; the Y-sorted foreground
// scene (units, buildings, pawns, particles, projectiles) is drawn by
// the shared renderer in core/rendering/foreground.rs.
// ───────────────────────────────────────────────────────────────────────────

pub(super) struct SdlBackend<'r, 'a> {
    pub canvas: &'r mut Canvas<Window>,
    pub assets: &'r mut Assets<'a>,
    pub cam: &'r Camera,
    pub ts: f32,
}

impl DrawBackend for SdlBackend<'_, '_> {
    fn draw_sprite(
        &mut self,
        key: SpriteKey,
        frame: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        flip: bool,
        alpha: f64,
    ) {
        let zoom = self.cam.zoom;
        let (sx, sy) = world_to_screen(x as f32, y as f32, self.cam);
        let dw = (w as f32 * zoom) as u32;
        let dh = (h as f32 * zoom) as u32;
        if dw == 0 || dh == 0 {
            return;
        }
        let Some((tex, fw, fh, _fc)) = self.assets.sprite_mut(key) else {
            return;
        };
        let src = Rect::new((frame * fw) as i32, 0, fw, fh);
        let dst = Rect::new(sx, sy, dw, dh);
        let a = (alpha.clamp(0.0, 1.0) * 255.0) as u8;
        super::safe_set_alpha(tex, a);
        let _ = self.canvas.copy_ex(tex, src, dst, 0.0, None, flip, false);
        super::safe_set_alpha(tex, 255);
    }

    fn draw_rotated(
        &mut self,
        key: SpriteKey,
        center_x: f64,
        center_y: f64,
        size: f64,
        angle: f64,
    ) {
        let zoom = self.cam.zoom;
        let s = (size as f32 * zoom) as u32;
        if s == 0 {
            return;
        }
        let (sx, sy) = world_to_screen(
            (center_x - size * 0.5) as f32,
            (center_y - size * 0.5) as f32,
            self.cam,
        );
        let Some((tex, _fw, _fh, _fc)) = self.assets.sprite_mut(key) else {
            return;
        };
        let dst = Rect::new(sx, sy, s, s);
        let _ = self
            .canvas
            .copy_ex(tex, None, dst, angle.to_degrees(), None, false, false);
    }

    fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo> {
        self.assets.sprite_dims(key).map(|(w, h, c)| SpriteInfo {
            frame_w: w,
            frame_h: h,
            frame_count: c,
        })
    }

    fn draw_elevated_tile(&mut self, game: &Game, gx: u32, gy: u32) {
        super::terrain::draw_elevated_tile(self.canvas, game, self.assets, self.cam, self.ts, gx, gy);
    }
}

pub(super) fn draw_foreground(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let mut backend = SdlBackend {
        canvas,
        assets,
        cam,
        ts,
    };
    battlefield_core::rendering::foreground::draw_foreground(
        &mut backend,
        game,
        (min_gx, min_gy, max_gx, max_gy),
        elapsed,
        |u| u.alive || u.death_fade > 0.0,
    );
}

pub(super) fn draw_unit_overlays(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    game: &Game,
    cam: &Camera,
    dpi_scale: f64,
) {
    canvas.set_blend_mode(BlendMode::Blend);
    let zoom = game.camera.zoom;
    let dot_r = (4.0 * zoom).max(2.0) as i32;

    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = unit.grid_cell();
        if !render_util::is_visible_to_player(unit.faction, gx, gy, &game.visible, game.grid.width)
        {
            continue;
        }

        let (sx, sy) = world_to_screen(unit.x, unit.y, cam);

        // HP bar
        let bar_w = (36.0 * zoom) as i32;
        let bar_h = (4.0 * zoom).max(2.0) as i32;
        let bar_y = sy - (TILE_SIZE * zoom * 0.7) as i32;
        let bar_x = sx - bar_w / 2;

        canvas.set_draw_color(Color::RGBA(40, 40, 40, 200));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        let ratio = unit.hp as f32 / unit.stats.max_hp as f32;
        let fill_w = (bar_w as f32 * ratio) as u32;
        let (hr, hg, hb) = render_util::hp_bar_color(ratio as f64);
        canvas.set_draw_color(Color::RGB(hr, hg, hb));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, fill_w, bar_h as u32));

        // Order progress bar — remaining time for timed orders, full for Follow
        if let Some(ref order) = unit.order {
            let remaining = match order {
                OrderKind::Follow => 1.0,
                OrderKind::Charge { .. } => {
                    (unit.order_timer / game.config.order_charge_timeout).clamp(0.0, 1.0)
                }
                OrderKind::Defend { .. } => {
                    (unit.order_timer / game.config.order_defend_duration).clamp(0.0, 1.0)
                }
                OrderKind::DefendZone { .. } => 1.0,
            };

            let ob_h = (3.0 * zoom).max(1.0) as i32;
            let ob_y = bar_y - ob_h - (1.0 * zoom) as i32;

            // Background
            canvas.set_draw_color(Color::RGBA(25, 25, 25, 180));
            let _ = canvas.fill_rect(Rect::new(bar_x, ob_y, bar_w as u32, ob_h as u32));

            // Fill — color coded by order type
            let fill_color = match order {
                OrderKind::Follow => Color::RGBA(64, 140, 255, 230),
                OrderKind::Charge { .. } => Color::RGBA(255, 190, 40, 230),
                OrderKind::Defend { .. } => Color::RGBA(128, 218, 128, 230),
                OrderKind::DefendZone { .. } => Color::RGBA(90, 200, 130, 230),
            };
            let fill_w = (bar_w as f32 * remaining) as u32;
            canvas.set_draw_color(fill_color);
            let _ = canvas.fill_rect(Rect::new(bar_x, ob_y, fill_w, ob_h as u32));

            // Order label above bar
            let label = match order {
                OrderKind::Follow => "F",
                OrderKind::Charge { .. } => "C",
                OrderKind::Defend { .. } => "D",
                OrderKind::DefendZone { .. } => "H",
            };
            let lbl_size = (12.0 * dpi_scale as f32) * zoom;
            let lbl_y = ob_y - (lbl_size * 0.8) as i32;
            let lbl_color = match order {
                OrderKind::Follow => Color::RGBA(100, 170, 255, 230),
                OrderKind::Charge { .. } => Color::RGBA(255, 200, 60, 230),
                OrderKind::Defend { .. } => Color::RGBA(140, 220, 140, 230),
                OrderKind::DefendZone { .. } => Color::RGBA(110, 210, 150, 230),
            };
            assets
                .text
                .draw_text_centered(canvas, tc, label, sx, lbl_y, lbl_size, lbl_color);
        }

        // Unit marker (player = green)
        let marker_color = if unit.is_player {
            Some(Color::RGBA(50, 220, 50, 220))
        } else {
            None
        };
        if let Some(color) = marker_color {
            let marker_y = sy - (TILE_SIZE * zoom * 0.95) as i32;
            canvas.set_draw_color(color);
            for dy in -dot_r..=dot_r {
                let dx = ((dot_r * dot_r - dy * dy) as f32).sqrt() as i32;
                let _ = canvas.draw_line((sx - dx, marker_y + dy), (sx + dx, marker_y + dy));
            }
        }

        // Order label (flashing text above unit)
        if unit.order_flash > 0.0 {
            if let Some(label) = render_util::order_label(unit.order.as_ref()) {
                let alpha = ((unit.order_flash / game.config.order_flash_duration) * 255.0) as u8;
                let label_y = sy - (TILE_SIZE * zoom) as i32;
                let font_size = (24.0 * dpi_scale as f32) * zoom;
                let ribbon_h = 54.0 * zoom as f64;
                let label_cy = label_y - (ribbon_h / 2.0) as i32;

                if let Some(ref tex) = assets.ui_small_ribbons {
                    let (tw, _th) = assets.text.measure_text(label, font_size);
                    let center_w = tw as f64 + 4.0 * zoom as f64;
                    draw_small_ribbon(
                        canvas,
                        tex,
                        5, // Yellow row
                        sx as f64,
                        label_cy as f64,
                        center_w,
                        zoom as f64,
                    );
                }

                assets.text.draw_text_centered(
                    canvas,
                    tc,
                    label,
                    sx,
                    label_cy,
                    font_size,
                    Color::RGBA(255, 215, 0, alpha),
                );
            }
        }
    }
}

pub(super) fn draw_fog(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let w = game.grid.width;

    let pixels = render_util::build_fog_pixels(&game.visible, w, game.grid.height);

    if let Some(ref mut tex) = assets.fog_texture {
        let pitch = (w * 4) as usize;
        let _ = tex.update(None, &pixels, pitch);

        let src_w = (max_gx - min_gx).max(1);
        let src_h = (max_gy - min_gy).max(1);
        let src = Rect::new(min_gx as i32, min_gy as i32, src_w, src_h);

        let (sx, sy) = world_to_screen(min_gx as f32 * TILE_SIZE, min_gy as f32 * TILE_SIZE, cam);
        let dst_w = (src_w as f32 * ts) as u32;
        let dst_h = (src_h as f32 * ts) as u32;
        let dst = Rect::new(sx, sy, dst_w, dst_h);

        let _ = canvas.copy(tex, src, dst);
    }
}
