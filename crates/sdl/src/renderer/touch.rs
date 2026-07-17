use battlefield_core::unit::OrderRequest;

use crate::input::InputState;
use sdl2::pixels::Color;
use sdl2::render::{Canvas, TextureCreator};
use sdl2::video::{Window, WindowContext};

use super::draw_helpers::{fill_circle, stroke_circle};
use super::text::TextRenderer;

/// Draw a circular touch button with label.
fn draw_touch_button(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    text_renderer: &TextRenderer,
    cx: i32,
    cy: i32,
    radius: i32,
    color: Color,
    label: &str,
    pressed: bool,
    dpr: f64,
) {
    let scale = if pressed { 1.12 } else { 1.0 };
    let btn_r = (radius as f64 * scale) as i32;

    let bg_alpha: u8 = if pressed { 153 } else { 89 };
    let fill_alpha: u8 = if pressed { 217 } else { 140 };
    let border_alpha: u8 = if pressed { 204 } else { 102 };

    // Dark background
    canvas.set_draw_color(Color::RGBA(0, 0, 0, bg_alpha));
    fill_circle(canvas, cx, cy, btn_r + (2.0 * dpr) as i32);

    // Colored fill
    canvas.set_draw_color(Color::RGBA(color.r, color.g, color.b, fill_alpha));
    fill_circle(canvas, cx, cy, btn_r);

    // Border ring
    canvas.set_draw_color(Color::RGBA(255, 255, 255, border_alpha));
    stroke_circle(canvas, cx, cy, btn_r);

    // Label
    let font_size = ((radius as f64 * 0.5).max(10.0 * dpr)) as f32;
    text_renderer.draw_text_centered(
        canvas,
        tc,
        label,
        cx,
        cy,
        font_size,
        Color::RGBA(255, 255, 255, 242),
    );
}

/// Draw touch controls in screen space (virtual joystick + buttons).
/// Only renders when touch has been detected.
pub(super) fn draw_touch_controls(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    input: &InputState,
    assets: &crate::renderer::Assets,
    dpi_scale: f64,
) {
    let text_renderer = &assets.text;
    let touch = &input.touch;
    if !touch.is_touch_device {
        return;
    }

    let dpr = dpi_scale;

    // Ghost joystick hint (before first use)
    if !touch.has_used_joystick && !touch.joystick.active {
        let ghost_x = (100.0 * dpr) as i32;
        let ghost_y =
            canvas.output_size().map(|(_, h)| h).unwrap_or(640) as i32 - (120.0 * dpr) as i32;
        let radius = touch.joystick.max_radius as i32;

        canvas.set_draw_color(Color::RGBA(255, 255, 255, 38));
        fill_circle(canvas, ghost_x, ghost_y, radius);
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 51));
        stroke_circle(canvas, ghost_x, ghost_y, radius);

        let font_size = (12.0 * dpr) as f32;
        text_renderer.draw_text_centered(
            canvas,
            tc,
            "MOVE",
            ghost_x,
            ghost_y,
            font_size,
            Color::RGBA(255, 255, 255, 77),
        );
    }

    // Virtual joystick (when active)
    if touch.joystick.active {
        let cx = touch.joystick.center_x as i32;
        let cy = touch.joystick.center_y as i32;
        let base_r = touch.joystick.max_radius as i32;

        // Base circle
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 64));
        fill_circle(canvas, cx, cy, base_r);

        // Stick knob
        let knob_r = (22.0 * dpr) as i32;
        let kx = touch.joystick.stick_x as i32;
        let ky = touch.joystick.stick_y as i32;
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 153));
        fill_circle(canvas, kx, ky, knob_r);
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 128));
        stroke_circle(canvas, kx, ky, knob_r);
    }

    // Attack button
    draw_round_asset_button(
        canvas,
        tc,
        text_renderer,
        assets,
        touch.attack.center_x as f64,
        touch.attack.center_y as f64,
        touch.attack.radius as f64,
        touch.attack.pressed,
        true,
        0,
        "ATK",
        dpr,
    );

    let order_btns = [
        (&touch.charge, OrderRequest::Charge, 2usize),
        (&touch.defend, OrderRequest::Defend, 1),
        (&touch.dismiss, OrderRequest::Dismiss, 3),
    ];
    for (btn, req, icon) in order_btns {
        draw_round_asset_button(
            canvas,
            tc,
            text_renderer,
            assets,
            btn.center_x as f64,
            btn.center_y as f64,
            btn.radius as f64,
            btn.pressed,
            false,
            icon,
            req.short_label(),
            dpr,
        );
        if req == OrderRequest::Dismiss && btn.pressed {
            let frac = touch.dismiss_hold_frac();
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 115));
            fill_circle(
                canvas,
                btn.center_x as i32,
                btn.center_y as i32,
                (btn.radius * frac * 0.4) as i32,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_round_asset_button(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    text_renderer: &TextRenderer,
    assets: &crate::renderer::Assets,
    cx: f64,
    cy: f64,
    radius: f64,
    pressed: bool,
    red: bool,
    icon: usize,
    fallback_label: &str,
    dpr: f64,
) {
    let base = match (red, pressed) {
        (true, false) => assets.ui_round_red.as_ref(),
        (true, true) => assets
            .ui_round_red_pressed
            .as_ref()
            .or(assets.ui_round_red.as_ref()),
        (false, false) => assets.ui_round_blue.as_ref(),
        (false, true) => assets
            .ui_round_blue_pressed
            .as_ref()
            .or(assets.ui_round_blue.as_ref()),
    };
    let (base_q, icon_q) =
        battlefield_core::render_util::round_button_quads(cx, cy, radius, pressed);
    if let Some(tex) = base {
        super::draw_helpers::blit(canvas, tex, &base_q);
        if let Some(icon_tex) = assets.ui_icons[icon].as_ref() {
            super::draw_helpers::blit(canvas, icon_tex, &icon_q);
        }
        return;
    }

    draw_touch_button(
        canvas,
        tc,
        text_renderer,
        cx as i32,
        cy as i32,
        radius as i32,
        if red {
            Color::RGBA(220, 50, 50, 153)
        } else {
            Color::RGBA(50, 90, 150, 128)
        },
        fallback_label,
        pressed,
        dpr,
    );
}
