#![allow(clippy::too_many_arguments)]

mod assets;
mod draw_helpers;
mod foreground;
mod hud;
mod terrain;
mod text;

pub use assets::Assets;
pub use battlefield_core::ui::GameScreen;

use battlefield_core::camera::Camera;
use battlefield_core::game::Game;
use battlefield_core::grid::TILE_SIZE;
use battlefield_core::render_util;
use battlefield_core::unit::{Faction, UnitAnim, UnitKind};
use sdl2::pixels::Color;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

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
    Tower(u8),
    BaseBuilding(usize),
    Particle(usize),
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

pub fn render_frame(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    game: &Game,
    assets: &mut Assets,
    screen: GameScreen,
    elapsed: f64,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
    dpi_scale: f64,
) -> Vec<ClickableButton> {
    let ts = TILE_SIZE * game.camera.zoom;
    let cam = &game.camera;
    let (min_gx, min_gy, max_gx, max_gy) =
        render_util::visible_tile_range(cam, game.grid.width, game.grid.height);

    // 1. Clear
    canvas.set_draw_color(Color::RGB(26, 26, 38));
    canvas.clear();

    // 2. Water background
    terrain::draw_water(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 3. Foam animation
    terrain::draw_foam(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 4. Terrain (autotiled ground, roads, elevation)
    terrain::draw_terrain(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 5. Zone overlays (in world space)
    foreground::draw_zones(canvas, tc, assets, game, cam, ts, dpi_scale);

    // 6. Bushes (ground level, behind units)
    terrain::draw_bushes(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 7. Rocks (ground level, behind units)
    terrain::draw_rocks(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 8. Player aim cone overlay
    foreground::draw_player_overlay(canvas, game, cam);

    // 9. Y-sorted foreground (units, trees, water rocks, towers, buildings, particles)
    foreground::draw_foreground(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 10. Projectiles (fly above everything)
    foreground::draw_projectiles(canvas, game, assets, cam);

    // 11. HP bars, unit markers, and order labels
    foreground::draw_hp_bars(canvas, game, cam);
    foreground::draw_unit_markers(canvas, game, cam);
    foreground::draw_order_labels(canvas, tc, assets, game, cam, dpi_scale);

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

    // 16. Screen overlays (menu, death, result)
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
