use crate::renderer::{Canvas2dRenderer, Renderer};
use wasm_bindgen::prelude::*;

/// Which screen the game is currently showing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GameScreen {
    MainMenu,
    Playing,
    PlayerDeath,
    GameWon,
    GameLost,
}

/// Action triggered by clicking an overlay button.
#[derive(Clone, Copy)]
pub(super) enum OverlayAction {
    Play,
    Retry,
    NewGame,
}

/// A clickable button on an overlay screen.
pub(super) struct OverlayButton {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) action: OverlayAction,
}

/// Draw a rounded-rect overlay button and register it for hit testing.
pub(super) fn draw_overlay_button(
    r: &Canvas2dRenderer,
    label: &str,
    cx: f64,
    cy: f64,
    dpr: f64,
    fill_color: &str,
    action: OverlayAction,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let btn_w = 200.0 * dpr;
    let btn_h = 50.0 * dpr;
    let btn_x = cx - btn_w / 2.0;
    let btn_y = cy - btn_h / 2.0;
    let radius = 10.0 * dpr;

    // Button background
    r.set_fill_color(fill_color);
    r.begin_path();
    // Rounded rect using arc_to
    r.move_to(btn_x + radius, btn_y);
    r.arc_to(btn_x + btn_w, btn_y, btn_x + btn_w, btn_y + btn_h, radius)?;
    r.arc_to(btn_x + btn_w, btn_y + btn_h, btn_x, btn_y + btn_h, radius)?;
    r.arc_to(btn_x, btn_y + btn_h, btn_x, btn_y, radius)?;
    r.arc_to(btn_x, btn_y, btn_x + btn_w, btn_y, radius)?;
    r.close_path();
    r.fill();

    // Border
    r.set_stroke_color("rgba(255, 255, 255, 0.4)");
    r.set_line_width(2.0 * dpr);
    r.stroke();

    // Label
    let font_size = 20.0 * dpr;
    r.set_font(&format!("bold {font_size}px sans-serif"));
    r.set_fill_color("white");
    r.set_text_align("center");
    r.set_text_baseline("middle");
    r.fill_text(label, cx, cy);

    buttons.push(OverlayButton {
        x: btn_x,
        y: btn_y,
        w: btn_w,
        h: btn_h,
        action,
    });

    Ok(())
}

/// Draw the main menu screen.
pub(super) fn draw_main_menu(
    r: &Canvas2dRenderer,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Dark overlay
    r.set_fill_color("rgba(0, 0, 0, 0.75)");
    r.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 52.0 * dpr;
    r.set_font(&format!("bold {title_font}px sans-serif"));
    r.set_text_align("center");
    r.set_text_baseline("middle");

    // Shadow
    r.set_fill_color("rgba(0, 0, 0, 0.7)");
    r.fill_text(
        "THE BATTLEFIELD",
        cx + 3.0 * dpr,
        cy - 80.0 * dpr + 3.0 * dpr,
    );

    // Gold title
    r.set_fill_color("#ffd700");
    r.fill_text("THE BATTLEFIELD", cx, cy - 80.0 * dpr);

    // Play button
    draw_overlay_button(
        r,
        "PLAY",
        cx,
        cy + 20.0 * dpr,
        dpr,
        "rgba(70, 150, 70, 0.85)",
        OverlayAction::Play,
        buttons,
    )?;

    // Controls hint
    let hint_font = 13.0 * dpr;
    r.set_font(&format!("{hint_font}px monospace"));
    r.set_fill_color("rgba(255, 255, 255, 0.5)");
    r.fill_text(
        "WASD move \u{2022} SPACE attack \u{2022} H/G/R/F orders",
        cx,
        cy + 90.0 * dpr,
    );
    r.fill_text("Enter / Space to start", cx, cy + 110.0 * dpr);

    Ok(())
}

/// Draw the "YOU DIED" screen with red tint.
pub(super) fn draw_death_screen(
    r: &Canvas2dRenderer,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Red-tinted overlay
    r.set_fill_color("rgba(80, 0, 0, 0.6)");
    r.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 56.0 * dpr;
    r.set_font(&format!("bold {title_font}px sans-serif"));
    r.set_text_align("center");
    r.set_text_baseline("middle");

    r.set_fill_color("rgba(0, 0, 0, 0.7)");
    r.fill_text("YOU DIED", cx + 3.0 * dpr, cy - 70.0 * dpr + 3.0 * dpr);

    r.set_fill_color("#cc2222");
    r.fill_text("YOU DIED", cx, cy - 70.0 * dpr);

    // Buttons
    let btn_y = cy + 20.0 * dpr;
    let gap = 120.0 * dpr;
    draw_overlay_button(
        r,
        "RETRY",
        cx - gap / 2.0,
        btn_y,
        dpr,
        "rgba(180, 80, 40, 0.85)",
        OverlayAction::Retry,
        buttons,
    )?;
    draw_overlay_button(
        r,
        "NEW GAME",
        cx + gap / 2.0,
        btn_y,
        dpr,
        "rgba(60, 120, 180, 0.85)",
        OverlayAction::NewGame,
        buttons,
    )?;

    // Hint
    let hint_font = 12.0 * dpr;
    r.set_font(&format!("{hint_font}px monospace"));
    r.set_fill_color("rgba(255, 255, 255, 0.4)");
    r.fill_text(
        "Enter = Retry \u{2022} Space = New Game",
        cx,
        btn_y + 45.0 * dpr,
    );

    Ok(())
}

/// Draw the victory / defeat result screen.
pub(super) fn draw_result_screen(
    r: &Canvas2dRenderer,
    canvas_w: f64,
    canvas_h: f64,
    dpr: f64,
    is_victory: bool,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    // Overlay tint
    if is_victory {
        r.set_fill_color("rgba(0, 30, 60, 0.6)");
    } else {
        r.set_fill_color("rgba(40, 0, 0, 0.6)");
    }
    r.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // Title
    let title_font = 52.0 * dpr;
    r.set_font(&format!("bold {title_font}px sans-serif"));
    r.set_text_align("center");
    r.set_text_baseline("middle");

    let (title, color) = if is_victory {
        ("VICTORY", "#4ea8ff")
    } else {
        ("DEFEAT", "#ff5555")
    };

    r.set_fill_color("rgba(0, 0, 0, 0.7)");
    r.fill_text(title, cx + 3.0 * dpr, cy - 80.0 * dpr + 3.0 * dpr);
    r.set_fill_color(color);
    r.fill_text(title, cx, cy - 80.0 * dpr);

    // Subtitle
    let sub_font = 18.0 * dpr;
    r.set_font(&format!("{sub_font}px sans-serif"));
    r.set_fill_color("rgba(255, 255, 255, 0.7)");
    let subtitle = if is_victory {
        "All capture zones held for 2 minutes"
    } else {
        "The enemy holds all capture zones"
    };
    r.fill_text(subtitle, cx, cy - 40.0 * dpr);

    // Buttons
    let btn_y = cy + 30.0 * dpr;
    let gap = 120.0 * dpr;
    let retry_label = if is_victory { "REPLAY" } else { "RETRY" };
    draw_overlay_button(
        r,
        retry_label,
        cx - gap / 2.0,
        btn_y,
        dpr,
        "rgba(180, 130, 40, 0.85)",
        OverlayAction::Retry,
        buttons,
    )?;
    draw_overlay_button(
        r,
        "NEW GAME",
        cx + gap / 2.0,
        btn_y,
        dpr,
        "rgba(60, 120, 180, 0.85)",
        OverlayAction::NewGame,
        buttons,
    )?;

    // Hint
    let hint_font = 12.0 * dpr;
    r.set_font(&format!("{hint_font}px monospace"));
    r.set_fill_color("rgba(255, 255, 255, 0.4)");
    r.fill_text(
        &format!("Enter = {} \u{2022} Space = New Game", retry_label),
        cx,
        btn_y + 45.0 * dpr,
    );

    Ok(())
}
