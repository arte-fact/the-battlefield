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
    _img_h: f64,
    dx: f64,
    dy: f64,
    dw: f64,
    dh: f64,
    cap_w: f64,
) {
    let cap = cap_w.min(dw / 2.0);
    let sl = cap_w;
    let sr = cap_w;
    let sc = img_w - sl - sr;

    let x0 = dx.round();
    let x1 = (dx + cap).round();
    let x2 = (dx + dw - cap).round();
    let x3 = (dx + dw).round();
    let y0 = dy.round();
    let y3 = (dy + dh).round();

    // Left cap
    let _ = canvas.copy(
        tex,
        Rect::new(0, 0, sl as u32, dh.ceil() as u32),
        Rect::new(x0 as i32, y0 as i32, (x1 - x0) as u32, (y3 - y0) as u32),
    );
    // Center stretch
    let mid = x2 - x1;
    if mid > 0.0 {
        let _ = canvas.copy(
            tex,
            Rect::new(sl as i32, 0, sc.ceil() as u32, dh.ceil() as u32),
            Rect::new(x1 as i32, y0 as i32, mid as u32, (y3 - y0) as u32),
        );
    }
    // Right cap
    let _ = canvas.copy(
        tex,
        Rect::new((sl + sc) as i32, 0, sr as u32, dh.ceil() as u32),
        Rect::new(x2 as i32, y0 as i32, (x3 - x2) as u32, (y3 - y0) as u32),
    );
}

/// Draw a horizontal 3-part ribbon from a ribbon sprite sheet.
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
    let cap = cap_w.min(dw / 2.0);
    let mid_w = (dw - cap * 2.0).max(0.0);
    let row_y = color_row as f64 * render_util::RIBBON_CELL_H;

    let (lsx, lsy, lsw, lsh) = render_util::RIBBON_LEFT;
    let (csx, csy, csw, csh) = render_util::RIBBON_CENTER;
    let (rsx, rsy, rsw, rsh) = render_util::RIBBON_RIGHT;

    // Left end
    let _ = canvas.copy(
        tex,
        Rect::new(
            lsx as i32,
            (row_y + lsy) as i32,
            lsw.ceil() as u32,
            lsh.ceil() as u32,
        ),
        Rect::new(
            dx.floor() as i32,
            dy.floor() as i32,
            cap.ceil() as u32,
            dh.ceil() as u32,
        ),
    );
    // Center (stretch)
    if mid_w > 0.0 {
        let _ = canvas.copy(
            tex,
            Rect::new(
                csx as i32,
                (row_y + csy) as i32,
                csw.ceil() as u32,
                csh.ceil() as u32,
            ),
            Rect::new(
                (dx + cap).floor() as i32,
                dy.floor() as i32,
                mid_w.ceil() as u32,
                dh.ceil() as u32,
            ),
        );
    }
    // Right end
    let _ = canvas.copy(
        tex,
        Rect::new(
            rsx as i32,
            (row_y + rsy) as i32,
            rsw.ceil() as u32,
            rsh.ceil() as u32,
        ),
        Rect::new(
            (dx + cap + mid_w).floor() as i32,
            dy.floor() as i32,
            cap.ceil() as u32,
            dh.ceil() as u32,
        ),
    );
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
    let row_y = color_row as f64 * render_util::SMALL_RIBBON_CELL_H;

    let (lsx, lsy, lsw, lsh) = render_util::SMALL_RIBBON_LEFT;
    let (csx, csy, csw, csh) = render_util::SMALL_RIBBON_CENTER;
    let (rsx, rsy, rsw, rsh) = render_util::SMALL_RIBBON_RIGHT;

    let cap_lw = (lsw * scale).ceil() as u32;
    let cap_rw = (rsw * scale).ceil() as u32;
    let h = (lsh * scale).ceil() as u32;
    let cw = center_w.ceil().max(0.0) as u32;

    let total_w = cap_lw + cw + cap_rw;
    let dx = (cx - total_w as f64 / 2.0).floor() as i32;
    let dy = (cy - h as f64 / 2.0).floor() as i32;

    // Left end
    let _ = canvas.copy(
        tex,
        Rect::new(
            lsx as i32,
            (row_y + lsy) as i32,
            lsw.ceil() as u32,
            lsh.ceil() as u32,
        ),
        Rect::new(dx, dy, cap_lw, h),
    );
    // Center — stretched horizontally
    if cw > 0 {
        let _ = canvas.copy(
            tex,
            Rect::new(
                csx as i32,
                (row_y + csy) as i32,
                csw.ceil() as u32,
                csh.ceil() as u32,
            ),
            Rect::new(dx + cap_lw as i32, dy, cw, h),
        );
    }
    // Right end
    let _ = canvas.copy(
        tex,
        Rect::new(
            rsx as i32,
            (row_y + rsy) as i32,
            rsw.ceil() as u32,
            rsh.ceil() as u32,
        ),
        Rect::new(dx + cap_lw as i32 + cw as i32, dy, cap_rw, h),
    );
}
