#![allow(clippy::too_many_arguments)]

mod assets;
mod draw_helpers;
mod foreground;
mod hud;
mod terrain;
mod text;
mod touch;

pub use assets::Assets;
pub use battlefield_core::ui::GameScreen;

use battlefield_core::camera::Camera;
use battlefield_core::game::Game;
use battlefield_core::grid::TILE_SIZE;
use battlefield_core::render_util;
use battlefield_core::unit::{Faction, UnitAnim, UnitKind};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};

/// Safe wrapper around `Texture::set_alpha_mod` that does not panic.
///
/// The sdl2-rs 0.37 implementation panics when the underlying
/// `SDL_SetTextureAlphaMod` returns an error (e.g. invalid texture on
/// Emscripten/WebGL). This wrapper silently ignores such errors.
#[inline(always)]
fn safe_set_alpha(tex: &mut Texture, alpha: u8) {
    unsafe {
        sdl2::sys::SDL_SetTextureAlphaMod(tex.raw(), alpha);
    }
}

/// Safe wrapper around `Texture::set_color_mod` that does not panic.
#[inline(always)]
fn safe_set_color_mod(tex: &mut Texture, r: u8, g: u8, b: u8) {
    unsafe {
        sdl2::sys::SDL_SetTextureColorMod(tex.raw(), r, g, b);
    }
}

/// Safe `canvas.set_draw_color` + `canvas.clear` that won't panic on WebGL context loss.
#[inline(always)]
fn safe_clear(canvas: &mut Canvas<Window>, r: u8, g: u8, b: u8) {
    unsafe {
        sdl2::sys::SDL_SetRenderDrawColor(canvas.raw(), r, g, b, 255);
        sdl2::sys::SDL_RenderClear(canvas.raw());
    }
}

/// A clickable button region returned by the renderer for hit-testing.
pub struct ClickableButton {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub action: battlefield_core::ui::ButtonAction,
}

impl ClickableButton {
    /// Returns true if the given point is inside this button's rectangle.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let px = px as f64;
        let py = py as f64;
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Internal shared types
// ───────────────────────────────────────────────────────────────────────────

#[derive(Hash, Eq, PartialEq)]
struct UnitTexKey {
    faction: Faction,
    kind: UnitKind,
    anim: UnitAnim,
}

enum Drawable {
    Unit(usize),
    Tree(u32, u32),
    WaterRock(u32, u32),
    BaseBuilding(usize),
    Particle(usize),
    Sheep(usize),
    Pawn(usize),
    ElevatedTile(u32, u32),
}

// ───────────────────────────────────────────────────────────────────────────
// Coordinate helpers
// ───────────────────────────────────────────────────────────────────────────

fn world_to_screen(wx: f32, wy: f32, cam: &Camera) -> (i32, i32) {
    let offset_x = (cam.viewport_w * 0.5 - cam.x * cam.zoom).round();
    let offset_y = (cam.viewport_h * 0.5 - cam.y * cam.zoom).round();
    let sx = (wx * cam.zoom + offset_x) as i32;
    let sy = (wy * cam.zoom + offset_y) as i32;
    (sx, sy)
}

fn src_rect(sx: f64, sy: f64, sw: f64, sh: f64) -> Rect {
    Rect::new(sx as i32, sy as i32, sw as u32, sh as u32)
}

// ───────────────────────────────────────────────────────────────────────────
// Main render entry point
// ───────────────────────────────────────────────────────────────────────────

pub fn render_frame<'a>(
    canvas: &mut Canvas<Window>,
    tc: &'a TextureCreator<WindowContext>,
    game: &Game,
    assets: &mut Assets<'a>,
    screen: GameScreen,
    elapsed: f64,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    dpi_scale: f64,
    input_state: &crate::input::InputState,
    touch_dpr: f64,
) -> Vec<ClickableButton> {
    let ts = TILE_SIZE * game.camera.zoom;
    let cam = &game.camera;
    let (min_gx, min_gy, max_gx, max_gy) =
        render_util::visible_tile_range(cam, game.grid.width, game.grid.height);

    // 1. Clear
    safe_clear(canvas, 26, 26, 38);

    // 2-4. Static terrain (water + ground + rocks): cached to render-target texture.
    // Only redrawn when the visible tile range or zoom changes.
    let zoom_key = (cam.zoom * 1000.0) as u32;
    let cache_key = (min_gx, min_gy, max_gx, max_gy, zoom_key);
    let cache_w = ((max_gx - min_gx) as f32 * ts).ceil() as u32;
    let cache_h = ((max_gy - min_gy) as f32 * ts).ceil() as u32;

    if cache_w > 0 && cache_h > 0 {
        let need_redraw = assets.terrain_cache_key != cache_key || assets.terrain_cache.is_none();

        if need_redraw {
            // (Re)create the render-target texture if size changed
            let size_changed = assets.terrain_cache.as_ref().is_none_or(|tex| {
                let q = tex.query();
                q.width != cache_w || q.height != cache_h
            });
            if size_changed {
                assets.terrain_cache =
                    tc.create_texture_target(None, cache_w, cache_h)
                        .ok()
                        .map(|mut t| {
                            t.set_blend_mode(sdl2::render::BlendMode::Blend);
                            t
                        });
            }

            // Render static terrain layers to the cache texture.
            // Take the texture out to avoid borrow conflict (draw functions need &mut assets).
            if let Some(mut cache_tex) = assets.terrain_cache.take() {
                let offset_cam = {
                    let mut c = *cam;
                    let half_w = cache_w as f32 / 2.0;
                    let half_h = cache_h as f32 / 2.0;
                    c.viewport_w = cache_w as f32;
                    c.viewport_h = cache_h as f32;
                    c.x = min_gx as f32 * TILE_SIZE + half_w / cam.zoom;
                    c.y = min_gy as f32 * TILE_SIZE + half_h / cam.zoom;
                    c
                };

                let _ = canvas.with_texture_canvas(&mut cache_tex, |c| {
                    c.set_draw_color(sdl2::pixels::Color::RGBA(0, 0, 0, 0));
                    c.clear();
                    terrain::draw_water(
                        c,
                        game,
                        assets,
                        &offset_cam,
                        ts,
                        min_gx,
                        min_gy,
                        max_gx,
                        max_gy,
                    );
                    terrain::draw_terrain(
                        c,
                        game,
                        assets,
                        &offset_cam,
                        ts,
                        min_gx,
                        min_gy,
                        max_gx,
                        max_gy,
                    );
                    terrain::draw_rocks(
                        c,
                        game,
                        assets,
                        &offset_cam,
                        ts,
                        min_gx,
                        min_gy,
                        max_gx,
                        max_gy,
                    );
                });
                assets.terrain_cache = Some(cache_tex);
                assets.terrain_cache_key = cache_key;
            }
        }

        // Blit cached terrain to screen
        if assets.terrain_cache.is_some() {
            let (sx, sy) =
                world_to_screen(min_gx as f32 * TILE_SIZE, min_gy as f32 * TILE_SIZE, cam);
            let dst = Rect::new(sx, sy, cache_w, cache_h);
            // Temporarily take the texture out to avoid borrow conflict with canvas
            let cache_tex = assets.terrain_cache.take().unwrap();
            let _ = canvas.copy(&cache_tex, None, dst);
            assets.terrain_cache = Some(cache_tex);
        }
    } else {
        // Fallback: draw directly if cache dimensions are zero
        terrain::draw_water(
            canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
        );
        terrain::draw_terrain(
            canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
        );
        terrain::draw_rocks(
            canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
        );
    }

    // 3b. Foam animation (animated, cannot be cached)
    terrain::draw_foam(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 5. Zone overlays (in world space)
    foreground::draw_zones(canvas, tc, assets, game, cam, ts, dpi_scale);

    // 6. Bushes (animated, cannot be cached)
    terrain::draw_bushes(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 8. Player aim cone overlay
    foreground::draw_player_overlay(canvas, game, cam);

    // 9. Y-sorted foreground (units, trees, water rocks, towers, buildings, particles)
    foreground::draw_foreground(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 10. Projectiles (fly above everything)
    foreground::draw_projectiles(canvas, game, assets, cam);

    // 11. HP bars, unit markers, and order labels (merged into single pass)
    foreground::draw_unit_overlays(canvas, tc, assets, game, cam, dpi_scale);

    // 12. Fog of war
    foreground::draw_fog(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 13. Screen-space HUD
    hud::draw_hud(canvas, tc, game, assets, dpi_scale);

    // 14. Victory progress bar
    foreground::draw_victory_progress(canvas, tc, assets, game, dpi_scale);

    // 15. Minimap
    hud::draw_minimap(canvas, game, assets);

    // 16. Touch controls (during gameplay)
    if screen == GameScreen::Playing {
        touch::draw_touch_controls(canvas, tc, input_state, &assets.text, touch_dpr);
    }

    // 17. Screen overlays (menu, death, result)
    let buttons = hud::draw_screen_overlay(
        canvas,
        tc,
        assets,
        screen,
        mouse_x,
        mouse_y,
        focused_button,
        gamepad_connected,
        dpi_scale,
    );

    canvas.present();

    buttons
}
