use super::assets::LoadedTextures;
use crate::renderer::{Canvas2dRenderer, Renderer, TextureId};
use battlefield_core::render_util::{NineSlice, NINE_SLICE_BUTTON, NINE_SLICE_SPECIAL_PAPER};
use battlefield_core::ui::{ButtonAction, ButtonStyle, ScreenLayout};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

pub(super) use battlefield_core::ui::GameScreen;

/// Action triggered by clicking an overlay button (maps directly to core ButtonAction).
#[derive(Clone, Copy)]
pub(super) enum OverlayAction {
    Play,
    Retry,
    NewGame,
}

impl From<ButtonAction> for OverlayAction {
    fn from(action: ButtonAction) -> Self {
        match action {
            ButtonAction::Play => OverlayAction::Play,
            ButtonAction::Retry => OverlayAction::Retry,
            ButtonAction::NewGame => OverlayAction::NewGame,
        }
    }
}

/// A clickable button on an overlay screen.
pub(super) struct OverlayButton {
    pub(super) x: f64,
    pub(super) y: f64,
    pub(super) w: f64,
    pub(super) h: f64,
    pub(super) action: OverlayAction,
}

// ---------------------------------------------------------------------------
// 9-slice drawing helpers
// ---------------------------------------------------------------------------

/// Draw a 9-slice panel from a pre-processed gapless atlas canvas.
///
/// Uses `NineSlice::compute()` to split the atlas into 9 source-to-dest draw
/// commands. Corners keep their source pixel size; edges and center stretch.
pub(super) fn draw_9slice_panel(
    r: &Canvas2dRenderer,
    atlas: &HtmlCanvasElement,
    ns: &NineSlice,
    img_w: f64,
    img_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
) -> Result<(), JsValue> {
    let parts = ns.compute(img_w, img_h, dx, dy, dw, dh);
    for p in &parts {
        if p.dw > 0.5 && p.dh > 0.5 {
            r.draw_canvas_region(atlas, p.sx, p.sy, p.sw, p.sh, p.dx, p.dy, p.dw, p.dh)?;
        }
    }
    Ok(())
}

/// Draw a horizontal 3-part ribbon from the BigRibbons sprite sheet.
fn draw_ribbon(
    r: &Canvas2dRenderer,
    tex: TextureId,
    color_row: u32,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    cap_w: f64,
) -> Result<(), JsValue> {
    use battlefield_core::render_util::{RIBBON_CELL_H, RIBBON_CENTER, RIBBON_LEFT, RIBBON_RIGHT};
    let cap = cap_w.min(dw / 2.0);
    let mid_w = (dw - cap * 2.0).max(0.0);
    let row_y = color_row as f64 * RIBBON_CELL_H;

    let (lsx, lsy, lsw, lsh) = RIBBON_LEFT;
    let (csx, csy, csw, csh) = RIBBON_CENTER;
    let (rsx, rsy, rsw, rsh) = RIBBON_RIGHT;

    // Left end
    r.draw_texture(tex, lsx, row_y + lsy, lsw, lsh, dx, dy, cap, dh)?;
    // Center (stretch)
    if mid_w > 0.0 {
        r.draw_texture(tex, csx, row_y + csy, csw, csh, dx + cap, dy, mid_w, dh)?;
    }
    // Right end
    r.draw_texture(
        tex,
        rsx,
        row_y + rsy,
        rsw,
        rsh,
        dx + cap + mid_w,
        dy,
        cap,
        dh,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Generic screen overlay renderer
// ---------------------------------------------------------------------------

/// Render any `ScreenLayout` using the wasm Canvas2D renderer.
///
/// When UI textures are loaded, draws sprite-based 9-slice panels, ribbons,
/// and buttons. Falls back to plain colored rectangles otherwise.
#[allow(clippy::too_many_arguments)]
fn draw_screen_overlay(
    r: &Canvas2dRenderer,
    layout: &ScreenLayout,
    loaded: &LoadedTextures,
    canvas_w: f64,
    canvas_h: f64,
    _dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let cx = canvas_w / 2.0;
    let cy = canvas_h / 2.0;

    // 1. Full-screen tinted overlay
    let (or, og, ob, oa) = layout.overlay;
    r.set_fill_color(&format!("rgba({or},{og},{ob},{:.2})", oa as f64 / 255.0));
    r.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    // 2. Panel background (9-slice or fallback rect)
    let (_panel_x, panel_y) = if let Some((pw, ph)) = layout.panel_size {
        let px = cx - pw / 2.0;
        let py = cy - ph / 2.0;
        if let Some((ref atlas, aw, ah)) = loaded.ui_panel_atlas {
            draw_9slice_panel(
                r,
                atlas,
                &NINE_SLICE_SPECIAL_PAPER,
                aw as f64,
                ah as f64,
                px,
                py,
                pw,
                ph,
            )?;
        } else {
            r.set_fill_color("rgba(30,25,20,0.85)");
            r.fill_rect(px, py, pw, ph);
        }
        (px, py)
    } else {
        (cx, cy)
    };

    // 3. Title ribbon
    if let Some((color_row, ribbon_offset_y, ribbon_w, ribbon_h)) = layout.title_ribbon {
        let ribbon_x = cx - ribbon_w / 2.0;
        let ribbon_y = panel_y + ribbon_offset_y;
        if let Some(tex) = loaded.ui_big_ribbons {
            draw_ribbon(
                r, tex, color_row, ribbon_x, ribbon_y, ribbon_w, ribbon_h, 72.0,
            )?;
        }
    }

    // 4. Title text
    if let Some(ref title) = layout.title {
        let tx = cx + title.offset_x;
        let ty = cy + title.offset_y;
        let font_size = title.size;
        let prefix = if title.bold { "bold " } else { "" };
        r.set_font(&format!("{prefix}{font_size}px sans-serif"));
        r.set_text_align("center");
        r.set_text_baseline("middle");

        if title.shadow {
            r.set_fill_color("rgba(0,0,0,0.7)");
            r.fill_text(&title.text, tx + 3.0, ty + 3.0);
        }

        r.set_fill_color(&format!(
            "rgba({},{},{},{:.2})",
            title.r,
            title.g,
            title.b,
            title.a as f64 / 255.0
        ));
        r.fill_text(&title.text, tx, ty);
    }

    // 5. Subtitle text
    if let Some(ref sub) = layout.subtitle {
        let sx = cx + sub.offset_x;
        let sy = cy + sub.offset_y;
        let font_size = sub.size;
        let prefix = if sub.bold { "bold " } else { "" };
        r.set_font(&format!("{prefix}{font_size}px sans-serif"));
        r.set_text_align("center");
        r.set_text_baseline("middle");
        r.set_fill_color(&format!(
            "rgba({},{},{},{:.2})",
            sub.r,
            sub.g,
            sub.b,
            sub.a as f64 / 255.0
        ));
        r.fill_text(&sub.text, sx, sy);
    }

    // 6. Buttons
    for btn in &layout.buttons {
        let bx = cx + btn.offset_x;
        let by = cy + btn.offset_y;
        let bw = btn.w;
        let bh = btn.h;
        let btn_x = bx - bw / 2.0;
        let btn_y = by - bh / 2.0;

        // Choose atlas based on button style
        let btn_atlas = match btn.style {
            ButtonStyle::Blue => loaded.ui_blue_btn_atlas.as_ref(),
            ButtonStyle::Red => loaded.ui_red_btn_atlas.as_ref(),
        };

        if let Some((atlas, aw, ah)) = btn_atlas {
            draw_9slice_panel(
                r,
                atlas,
                &NINE_SLICE_BUTTON,
                *aw as f64,
                *ah as f64,
                btn_x,
                btn_y,
                bw,
                bh,
            )?;
        } else {
            // Fallback: rounded-rect button
            let fill = match btn.style {
                ButtonStyle::Blue => "rgba(60,120,180,0.85)",
                ButtonStyle::Red => "rgba(180,80,40,0.85)",
            };
            let radius = 10.0;
            r.set_fill_color(fill);
            r.begin_path();
            r.round_rect(btn_x, btn_y, bw, bh, radius)?;
            r.fill();
            r.set_stroke_color("rgba(255,255,255,0.4)");
            r.set_line_width(2.0);
            r.stroke();
        }

        // Button label
        let label_size = 20.0;
        r.set_font(&format!("bold {label_size}px sans-serif"));
        r.set_fill_color("white");
        r.set_text_align("center");
        r.set_text_baseline("middle");
        r.fill_text(btn.label, bx, by);

        // Register for hit-testing
        buttons.push(OverlayButton {
            x: btn_x,
            y: btn_y,
            w: bw,
            h: bh,
            action: btn.action.into(),
        });
    }

    // 7. Hint texts
    for hint in &layout.hints {
        let hx = cx + hint.offset_x;
        let hy = cy + hint.offset_y;
        let font_size = hint.size;
        let prefix = if hint.bold { "bold " } else { "" };
        r.set_font(&format!("{prefix}{font_size}px monospace"));
        r.set_fill_color(&format!(
            "rgba({},{},{},{:.2})",
            hint.r,
            hint.g,
            hint.b,
            hint.a as f64 / 255.0
        ));
        r.set_text_align("center");
        r.set_text_baseline("middle");
        r.fill_text(&hint.text, hx, hy);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Public screen drawing functions
// ---------------------------------------------------------------------------

/// Draw the main menu screen.
pub(super) fn draw_main_menu(
    r: &Canvas2dRenderer,
    loaded: &LoadedTextures,
    canvas_w: f64,
    canvas_h: f64,
    _dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let layout = battlefield_core::ui::main_menu_layout();
    draw_screen_overlay(r, &layout, loaded, canvas_w, canvas_h, _dpr, buttons)
}

/// Draw the "YOU DIED" screen with red tint.
pub(super) fn draw_death_screen(
    r: &Canvas2dRenderer,
    loaded: &LoadedTextures,
    canvas_w: f64,
    canvas_h: f64,
    _dpr: f64,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let layout = battlefield_core::ui::death_layout();
    draw_screen_overlay(r, &layout, loaded, canvas_w, canvas_h, _dpr, buttons)
}

/// Draw the victory / defeat result screen.
pub(super) fn draw_result_screen(
    r: &Canvas2dRenderer,
    loaded: &LoadedTextures,
    canvas_w: f64,
    canvas_h: f64,
    _dpr: f64,
    is_victory: bool,
    buttons: &mut Vec<OverlayButton>,
) -> Result<(), JsValue> {
    let layout = battlefield_core::ui::result_layout(is_victory);
    draw_screen_overlay(r, &layout, loaded, canvas_w, canvas_h, _dpr, buttons)
}
