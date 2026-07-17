use battlefield_core::render_util;
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture};
use sdl2::video::Window;

/// Draw a filled circle using horizontal scanlines (midpoint circle algorithm).
pub(super) fn fill_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
    if radius <= 0 {
        return;
    }
    let mut x = radius;
    let mut y = 0i32;
    let mut err = 1 - radius;
    while x >= y {
        let _ = canvas.draw_line((cx - x, cy + y), (cx + x, cy + y));
        let _ = canvas.draw_line((cx - x, cy - y), (cx + x, cy - y));
        let _ = canvas.draw_line((cx - y, cy + x), (cx + y, cy + x));
        let _ = canvas.draw_line((cx - y, cy - x), (cx + y, cy - x));
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

/// Draw a circle outline (midpoint circle algorithm).
pub(super) fn stroke_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
    if radius <= 0 {
        return;
    }
    let mut x = radius;
    let mut y = 0i32;
    let mut err = 1 - radius;
    while x >= y {
        let _ = canvas.draw_point((cx + x, cy + y));
        let _ = canvas.draw_point((cx - x, cy + y));
        let _ = canvas.draw_point((cx + x, cy - y));
        let _ = canvas.draw_point((cx - x, cy - y));
        let _ = canvas.draw_point((cx + y, cy + x));
        let _ = canvas.draw_point((cx - y, cy + x));
        let _ = canvas.draw_point((cx + y, cy - x));
        let _ = canvas.draw_point((cx - y, cy - x));
        y += 1;
        if err < 0 {
            err += 2 * y + 1;
        } else {
            x -= 1;
            err += 2 * (y - x) + 1;
        }
    }
}

/// Draw a 9-slice panel from a pre-processed gapless atlas texture.
pub(super) fn draw_panel(
    canvas: &mut Canvas<Window>,
    tex: &Texture,
    ns: &render_util::NineSlice,
    img_w: f64,
    img_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
) {
    let parts = ns.compute(img_w, img_h, dx, dy, dw, dh);
    for p in &parts {
        if p.dw > 0.5 && p.dh > 0.5 {
            let src = Rect::new(
                p.sx as i32,
                p.sy as i32,
                p.sw.ceil() as u32,
                p.sh.ceil() as u32,
            );
            let dst = Rect::new(
                p.dx as i32,
                p.dy as i32,
                p.dw.ceil() as u32,
                p.dh.ceil() as u32,
            );
            let _ = canvas.copy(tex, src, dst);
        }
    }
}

/// Draw a horizontal 3-part bar from a pre-processed gapless atlas.
pub(super) fn draw_bar_3slice(
    canvas: &mut Canvas<Window>,
    tex: &Texture,
    img_w: f64,
    img_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    cap_w: f64,
) {
    for q in render_util::bar_base_quads(img_w, img_h, dx, dy, dw, dh, cap_w) {
        blit(canvas, tex, &q);
    }
}

pub(super) fn draw_ribbon(
    canvas: &mut Canvas<Window>,
    tex: &Texture,
    color_row: u32,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    cap_w: f64,
) {
    for q in render_util::big_ribbon_quads(color_row, dx, dy, dw, dh, cap_w) {
        blit(canvas, tex, &q);
    }
}

/// Draw a SmallRibbon centered at (cx, cy).
pub(super) fn draw_small_ribbon(
    canvas: &mut Canvas<Window>,
    tex: &Texture,
    color_row: u32,
    cx: f64,
    cy: f64,
    center_w: f64,
    scale: f64,
) {
    for q in render_util::small_ribbon_quads(color_row, cx, cy, center_w, scale) {
        blit(canvas, tex, &q);
    }
}

pub(super) fn blit(canvas: &mut Canvas<Window>, tex: &Texture, q: &render_util::SrcDst) {
    if q.dw < 0.5 || q.dh < 0.5 {
        return;
    }
    let src = Rect::new(
        q.sx as i32,
        q.sy as i32,
        q.sw.ceil() as u32,
        q.sh.ceil() as u32,
    );
    let dst = Rect::new(
        q.dx as i32,
        q.dy as i32,
        q.dw.ceil() as u32,
        q.dh.ceil() as u32,
    );
    let _ = canvas.copy(tex, src, dst);
}
