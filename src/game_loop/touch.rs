use crate::input::Input;
use crate::renderer::{Canvas2dRenderer, Renderer};
use wasm_bindgen::prelude::*;

/// Draw a circular touch button with label.
fn draw_touch_button(
    r: &Canvas2dRenderer,
    cx: f64,
    cy: f64,
    radius: f64,
    fill_color: &str,
    label: &str,
    pressed: bool,
    dpr: f64,
) -> Result<(), JsValue> {
    let scale = if pressed { 1.12 } else { 1.0 };
    let btn_r = radius * scale;

    // Dark background for contrast
    r.set_alpha(if pressed { 0.6 } else { 0.35 });
    r.set_fill_color("rgba(0,0,0,0.7)");
    r.begin_path();
    r.arc(cx, cy, btn_r + 2.0 * dpr, 0.0, std::f64::consts::TAU)?;
    r.fill();

    // Colored fill
    r.set_alpha(if pressed { 0.85 } else { 0.55 });
    r.set_fill_color(fill_color);
    r.begin_path();
    r.arc(cx, cy, btn_r, 0.0, std::f64::consts::TAU)?;
    r.fill();

    // Border ring
    r.set_stroke_color(if pressed {
        "rgba(255,255,255,0.8)"
    } else {
        "rgba(255,255,255,0.4)"
    });
    r.set_line_width(2.0 * dpr);
    r.stroke();

    // Label
    let font_size = (radius * 0.5).max(10.0 * dpr);
    r.set_alpha(0.95);
    r.set_fill_color("white");
    r.set_font(&format!("bold {}px monospace", font_size as u32));
    r.set_text_align("center");
    r.set_text_baseline("middle");
    r.fill_text(label, cx, cy);

    r.set_alpha(1.0);
    Ok(())
}

/// Draw touch controls in screen space (virtual joystick + attack + order buttons).
pub(super) fn draw_touch_controls(
    r: &Canvas2dRenderer,
    input: &Input,
    canvas_h: f64,
    dpr: f64,
) -> Result<(), JsValue> {
    if !input.is_touch_device {
        return Ok(());
    }

    // Ghost joystick hint (before first use)
    if !input.has_used_joystick && !input.joystick.active {
        let ghost_x = 100.0 * dpr;
        let ghost_y = canvas_h - 120.0 * dpr;
        r.set_alpha(0.15);
        r.set_fill_color("rgba(255,255,255,0.3)");
        r.begin_path();
        r.arc(
            ghost_x,
            ghost_y,
            input.joystick.max_radius as f64,
            0.0,
            std::f64::consts::TAU,
        )?;
        r.fill();
        r.set_stroke_color("rgba(255,255,255,0.2)");
        r.set_line_width(2.0 * dpr);
        r.stroke();
        r.set_alpha(0.3);
        r.set_fill_color("white");
        r.set_font(&format!("bold {}px monospace", (12.0 * dpr) as u32));
        r.set_text_align("center");
        r.set_text_baseline("middle");
        r.fill_text("MOVE", ghost_x, ghost_y);
        r.set_alpha(1.0);
    }

    // Virtual joystick (when active)
    if input.joystick.active {
        // Base circle
        r.set_alpha(0.25);
        r.set_fill_color("rgba(255,255,255,0.3)");
        r.begin_path();
        r.arc(
            input.joystick.center_x as f64,
            input.joystick.center_y as f64,
            input.joystick.max_radius as f64,
            0.0,
            std::f64::consts::TAU,
        )?;
        r.fill();

        // Stick knob
        r.set_alpha(0.6);
        r.set_fill_color("rgba(255,255,255,0.7)");
        r.begin_path();
        r.arc(
            input.joystick.stick_x as f64,
            input.joystick.stick_y as f64,
            22.0 * dpr,
            0.0,
            std::f64::consts::TAU,
        )?;
        r.fill();
        r.set_stroke_color("rgba(255,255,255,0.5)");
        r.set_line_width(2.0 * dpr);
        r.stroke();
    }

    // Attack button
    draw_touch_button(
        r,
        input.attack_button.center_x as f64,
        input.attack_button.center_y as f64,
        input.attack_button.radius as f64,
        "rgba(220,50,50,0.6)",
        "ATK",
        input.attack_button.pressed,
        dpr,
    )?;

    // Order buttons
    let order_btns: [(&crate::input::ActionButton, &str, &str); 4] = [
        (&input.order_hold_btn, "H", "rgba(200,170,50,0.5)"),
        (&input.order_go_btn, "G", "rgba(50,180,80,0.5)"),
        (&input.order_retreat_btn, "R", "rgba(50,120,200,0.5)"),
        (&input.order_follow_btn, "F", "rgba(160,80,200,0.5)"),
    ];
    for (btn, label, color) in &order_btns {
        draw_touch_button(
            r,
            btn.center_x as f64,
            btn.center_y as f64,
            btn.radius as f64,
            color,
            label,
            btn.pressed,
            dpr,
        )?;
    }

    r.set_alpha(1.0);
    Ok(())
}
