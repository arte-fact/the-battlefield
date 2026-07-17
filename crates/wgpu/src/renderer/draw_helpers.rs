//! Thin blitters over the shared UI assembly in `render_util`.

use battlefield_core::render_util::{self, NineSlice, SrcDst};

use super::sprite_batch::{SpriteBatch, TextureId};

pub fn blit(batch: &mut SpriteBatch, tex_id: TextureId, tex_size: (u32, u32), q: &SrcDst) {
    blit_tinted(batch, tex_id, tex_size, q, [1.0, 1.0, 1.0, 1.0]);
}

pub fn blit_tinted(
    batch: &mut SpriteBatch,
    tex_id: TextureId,
    tex_size: (u32, u32),
    q: &SrcDst,
    color: [f32; 4],
) {
    if q.dw < 0.5 || q.dh < 0.5 {
        return;
    }
    batch.draw_sprite(
        tex_id,
        [q.sx as f32, q.sy as f32, q.sw as f32, q.sh as f32],
        [q.dx as f32, q.dy as f32, q.dw as f32, q.dh as f32],
        tex_size,
        false,
        color,
    );
}

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
    let parts = ns.compute_scaled(
        atlas_w as f64,
        atlas_h as f64,
        dx as f64,
        dy as f64,
        dw as f64,
        dh as f64,
        scale as f64,
    );
    for q in &parts {
        blit(batch, tex_id, (atlas_w, atlas_h), q);
    }
}

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
    draw_panel_scaled(batch, tex_id, atlas_w, atlas_h, ns, dx, dy, dw, dh, 1.0);
}

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
    let parts = render_util::bar_base_quads(
        atlas_w as f64,
        atlas_h as f64,
        dx as f64,
        dy as f64,
        dw as f64,
        dh as f64,
        cap_w as f64,
    );
    for q in &parts {
        blit(batch, tex_id, (atlas_w, atlas_h), q);
    }
}

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
    let parts = render_util::small_ribbon_quads(
        color_row,
        cx as f64,
        cy as f64,
        center_w as f64,
        scale as f64,
    );
    for q in &parts {
        blit(batch, tex_id, (tex_w, tex_h), q);
    }
}

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
    let parts = render_util::big_ribbon_quads(
        color_row,
        dx as f64,
        dy as f64,
        dw as f64,
        dh as f64,
        cap_w as f64,
    );
    for q in &parts {
        blit(batch, tex_id, (tex_w, tex_h), q);
    }
}
