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
    text_renderer: &TextRenderer,
    dpi_scale: f64,
) {
    if !input.is_touch_device {
        return;
    }

    let dpr = dpi_scale;

    // Ghost joystick hint (before first use)
    if !input.has_used_joystick && !input.joystick.active {
        let ghost_x = (100.0 * dpr) as i32;
        let ghost_y =
            canvas.output_size().map(|(_, h)| h).unwrap_or(640) as i32 - (120.0 * dpr) as i32;
        let radius = input.joystick.max_radius as i32;

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
    if input.joystick.active {
        let cx = input.joystick.center_x as i32;
        let cy = input.joystick.center_y as i32;
        let base_r = input.joystick.max_radius as i32;

        // Base circle
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 64));
        fill_circle(canvas, cx, cy, base_r);

        // Stick knob
        let knob_r = (22.0 * dpr) as i32;
        let kx = input.joystick.stick_x as i32;
        let ky = input.joystick.stick_y as i32;
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 153));
        fill_circle(canvas, kx, ky, knob_r);
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 128));
        stroke_circle(canvas, kx, ky, knob_r);
    }

    // Attack button
    draw_touch_button(
        canvas,
        tc,
        text_renderer,
        input.attack_button.center_x as i32,
        input.attack_button.center_y as i32,
        input.attack_button.radius as i32,
        Color::RGBA(220, 50, 50, 153),
        "ATK",
        input.attack_button.pressed,
        dpr,
    );

    // Order buttons (Follow, Charge, Defend — no separate Recruit)
    let order_btns: [(&crate::input::ActionButton, &str, Color); 3] = [
        (&input.order_follow_btn, "F", Color::RGBA(160, 80, 200, 128)),
        (&input.order_charge_btn, "C", Color::RGBA(220, 50, 50, 128)),
        (&input.order_defend_btn, "V", Color::RGBA(50, 120, 200, 128)),
    ];
    for (btn, label, color) in &order_btns {
        draw_touch_button(
            canvas,
            tc,
            text_renderer,
            btn.center_x as i32,
            btn.center_y as i32,
            btn.radius as i32,
            *color,
            label,
            btn.pressed,
            dpr,
        );
    }
}
