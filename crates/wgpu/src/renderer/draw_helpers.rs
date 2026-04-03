//! 9-slice panel, 3-slice bar, and ribbon drawing helpers.

use battlefield_core::render_util::{self, NineSlice};

use super::sprite_batch::{SpriteBatch, TextureId};

/// Draw a 9-slice panel with uniform scale applied to border sizes.
/// Source rects sample the full atlas borders; destination borders are scaled.
pub fn draw_panel_scaled(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    atlas_w: u32,
    atlas_h: u32,
    ns: &NineSlice,
    dx: f32,
    dy: f32,
    dw: f32,
    dh: f32,
    scale: f32,
) {
    let aw = atlas_w as f32;
    let ah = atlas_h as f32;
    let (sl, st, sr, sb) = (
        ns.left as f32,
        ns.top as f32,
        ns.right as f32,
        ns.bottom as f32,
    );
    // Destination border sizes (scaled)
    let dl = sl * scale;
    let dt = st * scale;
    let dr = sr * scale;
    let db = sb * scale;
    let cw_src = aw - sl - sr;
    let ch_src = ah - st - sb;
    let dst_cw = (dw - dl - dr).max(0.0);
    let dst_ch = (dh - dt - db).max(0.0);
    let tex_size = (atlas_w, atlas_h);
    let white = [1.0_f32, 1.0, 1.0, 1.0];

    // Row 0: TL, TC, TR
    batch.draw_sprite(tex_id, [0.0, 0.0, sl, st], [dx, dy, dl, dt], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl, 0.0, cw_src, st], [dx + dl, dy, dst_cw, dt], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl + cw_src, 0.0, sr, st], [dx + dl + dst_cw, dy, dr, dt], tex_size, false, white);
    // Row 1: ML, MC, MR
    batch.draw_sprite(tex_id, [0.0, st, sl, ch_src], [dx, dy + dt, dl, dst_ch], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl, st, cw_src, ch_src], [dx + dl, dy + dt, dst_cw, dst_ch], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl + cw_src, st, sr, ch_src], [dx + dl + dst_cw, dy + dt, dr, dst_ch], tex_size, false, white);
    // Row 2: BL, BC, BR
    batch.draw_sprite(tex_id, [0.0, st + ch_src, sl, sb], [dx, dy + dt + dst_ch, dl, db], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl, st + ch_src, cw_src, sb], [dx + dl, dy + dt + dst_ch, dst_cw, db], tex_size, false, white);
    batch.draw_sprite(tex_id, [sl + cw_src, st + ch_src, sr, sb], [dx + dl + dst_cw, dy + dt + dst_ch, dr, db], tex_size, false, white);
}

/// Draw a 9-slice panel using pre-processed gapless atlas.
pub fn draw_panel(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    atlas_w: u32,
    atlas_h: u32,
    ns: &NineSlice,
    dx: f32,
    dy: f32,
    dw: f32,
    dh: f32,
) {
    let aw = atlas_w as f32;
    let ah = atlas_h as f32;
    let (l, t, r, b) = (
        ns.left as f32,
        ns.top as f32,
        ns.right as f32,
        ns.bottom as f32,
    );
    let cw = aw - l - r; // center column width in atlas
    let ch = ah - t - b; // center row height in atlas

    let dst_cw = (dw - l - r).max(0.0); // destination center width
    let dst_ch = (dh - t - b).max(0.0); // destination center height

    let tex_size = (atlas_w, atlas_h);
    let white = [1.0_f32, 1.0, 1.0, 1.0];

    // Row 0: TL, TC, TR
    batch.draw_sprite(
        tex_id,
        [0.0, 0.0, l, t],
        [dx, dy, l, t],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l, 0.0, cw, t],
        [dx + l, dy, dst_cw, t],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l + cw, 0.0, r, t],
        [dx + l + dst_cw, dy, r, t],
        tex_size,
        false,
        white,
    );

    // Row 1: ML, MC, MR
    batch.draw_sprite(
        tex_id,
        [0.0, t, l, ch],
        [dx, dy + t, l, dst_ch],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l, t, cw, ch],
        [dx + l, dy + t, dst_cw, dst_ch],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l + cw, t, r, ch],
        [dx + l + dst_cw, dy + t, r, dst_ch],
        tex_size,
        false,
        white,
    );

    // Row 2: BL, BC, BR
    batch.draw_sprite(
        tex_id,
        [0.0, t + ch, l, b],
        [dx, dy + t + dst_ch, l, b],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l, t + ch, cw, b],
        [dx + l, dy + t + dst_ch, dst_cw, b],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [l + cw, t + ch, r, b],
        [dx + l + dst_cw, dy + t + dst_ch, r, b],
        tex_size,
        false,
        white,
    );
}

/// Draw a 3-slice horizontal bar from pre-processed atlas.
pub fn draw_bar_3slice(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    atlas_w: u32,
    atlas_h: u32,
    dx: f32,
    dy: f32,
    dw: f32,
    dh: f32,
    cap_w: f32,
) {
    let aw = atlas_w as f32;
    let ah = atlas_h as f32;
    let tex_size = (atlas_w, atlas_h);
    let white = [1.0_f32, 1.0, 1.0, 1.0];

    // Left cap, center stretch, right cap
    let center_w = aw - cap_w * 2.0;
    let dst_center = (dw - cap_w * 2.0).max(0.0);

    batch.draw_sprite(
        tex_id,
        [0.0, 0.0, cap_w, ah],
        [dx, dy, cap_w, dh],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [cap_w, 0.0, center_w, ah],
        [dx + cap_w, dy, dst_center, dh],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [cap_w + center_w, 0.0, cap_w, ah],
        [dx + cap_w + dst_center, dy, cap_w, dh],
        tex_size,
        false,
        white,
    );
}

/// Draw a small ribbon (3-part: left end, center stretch, right end).
/// `color_row` selects which row in SmallRibbons.png (1=Blue, 3=Red, 5=Yellow, 7=Purple, 9=Black).
pub fn draw_small_ribbon(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    tex_w: u32,
    tex_h: u32,
    color_row: u32,
    cx: f32,
    cy: f32,
    center_w: f32,
    scale: f32,
) {
    let row_y = color_row as f32 * render_util::SMALL_RIBBON_CELL_H as f32;
    let (lx, ly, lw, lh) = (
        render_util::SMALL_RIBBON_LEFT.0 as f32,
        row_y + render_util::SMALL_RIBBON_LEFT.1 as f32,
        render_util::SMALL_RIBBON_LEFT.2 as f32,
        render_util::SMALL_RIBBON_LEFT.3 as f32,
    );
    let (mx, my, mw, mh) = (
        render_util::SMALL_RIBBON_CENTER.0 as f32,
        row_y + render_util::SMALL_RIBBON_CENTER.1 as f32,
        render_util::SMALL_RIBBON_CENTER.2 as f32,
        render_util::SMALL_RIBBON_CENTER.3 as f32,
    );
    let (rx, ry, rw, rh) = (
        render_util::SMALL_RIBBON_RIGHT.0 as f32,
        row_y + render_util::SMALL_RIBBON_RIGHT.1 as f32,
        render_util::SMALL_RIBBON_RIGHT.2 as f32,
        render_util::SMALL_RIBBON_RIGHT.3 as f32,
    );

    let draw_h = lh * scale;
    let draw_lw = lw * scale;
    let draw_rw = rw * scale;
    let draw_cw = center_w;
    let total_w = draw_lw + draw_cw + draw_rw;

    let start_x = cx - total_w * 0.5;
    let start_y = cy - draw_h * 0.5;
    let tex_size = (tex_w, tex_h);
    let white = [1.0_f32, 1.0, 1.0, 1.0];

    batch.draw_sprite(
        tex_id,
        [lx, ly, lw, lh],
        [start_x, start_y, draw_lw, draw_h],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [mx, my, mw, mh],
        [start_x + draw_lw, start_y, draw_cw, draw_h],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [rx, ry, rw, rh],
        [start_x + draw_lw + draw_cw, start_y, draw_rw, draw_h],
        tex_size,
        false,
        white,
    );
}

/// Draw a big ribbon (3-part: left cap, center stretch, right cap).
/// `color_row` selects which row in BigRibbons.png (Blue=0, Red=1, Yellow=2, Purple=3, Black=4).
pub fn draw_ribbon(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    tex_w: u32,
    tex_h: u32,
    color_row: u32,
    dx: f32,
    dy: f32,
    dw: f32,
    dh: f32,
    cap_w: f32,
) {
    let row_y = color_row as f32 * render_util::RIBBON_CELL_H as f32;
    let (lx, ly, lw, lh) = (
        render_util::RIBBON_LEFT.0 as f32,
        row_y + render_util::RIBBON_LEFT.1 as f32,
        render_util::RIBBON_LEFT.2 as f32,
        render_util::RIBBON_LEFT.3 as f32,
    );
    let (mx, my, mw, mh) = (
        render_util::RIBBON_CENTER.0 as f32,
        row_y + render_util::RIBBON_CENTER.1 as f32,
        render_util::RIBBON_CENTER.2 as f32,
        render_util::RIBBON_CENTER.3 as f32,
    );
    let (rx, ry, rw, rh) = (
        render_util::RIBBON_RIGHT.0 as f32,
        row_y + render_util::RIBBON_RIGHT.1 as f32,
        render_util::RIBBON_RIGHT.2 as f32,
        render_util::RIBBON_RIGHT.3 as f32,
    );

    let tex_size = (tex_w, tex_h);
    let white = [1.0_f32, 1.0, 1.0, 1.0];

    let draw_cap = cap_w.min(dw * 0.5);
    let draw_center = (dw - draw_cap * 2.0).max(0.0);

    batch.draw_sprite(
        tex_id,
        [lx, ly, lw, lh],
        [dx, dy, draw_cap, dh],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [mx, my, mw, mh],
        [dx + draw_cap, dy, draw_center, dh],
        tex_size,
        false,
        white,
    );
    batch.draw_sprite(
        tex_id,
        [rx, ry, rw, rh],
        [dx + draw_cap + draw_center, dy, draw_cap, dh],
        tex_size,
        false,
        white,
    );
}
