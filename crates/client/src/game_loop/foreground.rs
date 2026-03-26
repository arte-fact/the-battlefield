use super::assets::{LoadedTextures, UnitTextureKey};
use crate::renderer::{Canvas2dRenderer, Renderer, TextureId};
use battlefield_core::animation::TurnAnimator;
use battlefield_core::asset_manifest;
use battlefield_core::game::Game;
use battlefield_core::rendering::{DrawBackend, SpriteInfo, SpriteKey};
use battlefield_core::sprite::SpriteSheet;
use battlefield_core::unit::UnitAnim;

/// WASM implementation of [`DrawBackend`] wrapping Canvas2D + loaded textures.
struct WasmDrawBackend<'a> {
    r: &'a Canvas2dRenderer,
    loaded: &'a LoadedTextures,
}

impl WasmDrawBackend<'_> {
    fn texture_id(&self, key: SpriteKey) -> Option<TextureId> {
        match key {
            SpriteKey::Unit {
                faction,
                kind,
                anim,
            } => self
                .loaded
                .unit_textures
                .get(&UnitTextureKey {
                    faction,
                    kind,
                    anim,
                })
                .or_else(|| {
                    self.loaded.unit_textures.get(&UnitTextureKey {
                        faction,
                        kind,
                        anim: UnitAnim::Idle,
                    })
                })
                .copied(),
            SpriteKey::Building(idx) => {
                self.loaded.building_textures.get(idx).map(|&(id, _, _)| id)
            }
            SpriteKey::Tower(idx) => self.loaded.tower_textures.get(idx).copied(),
            SpriteKey::Tree(idx) => self.loaded.tree_textures.get(idx).map(|&(id, _, _)| id),
            SpriteKey::Rock(idx) => self.loaded.rock_textures.get(idx).copied(),
            SpriteKey::Bush(idx) => self.loaded.bush_textures.get(idx).map(|&(id, _, _)| id),
            SpriteKey::WaterRock(idx) => {
                self.loaded
                    .water_rock_textures
                    .get(idx)
                    .map(|&(id, _, _)| id)
            }
            SpriteKey::Particle(idx) => {
                let filename = asset_manifest::PARTICLE_SPECS.get(idx).map(|s| s.2)?;
                self.loaded.particle_textures.get(filename).copied()
            }
            SpriteKey::Arrow => self.loaded.arrow_texture,
            SpriteKey::Sheep(idx) => self.loaded.sheep_textures.get(idx).map(|&(id, _, _)| id),
            SpriteKey::Pawn(idx) => self.loaded.pawn_textures.get(idx).map(|&(id, _, _)| id),
        }
    }
}

impl DrawBackend for WasmDrawBackend<'_> {
    fn draw_sprite(
        &mut self,
        key: SpriteKey,
        frame: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        flip: bool,
        alpha: f64,
    ) {
        let Some(tex_id) = self.texture_id(key) else {
            return;
        };
        let Some(info) = self.r.texture_info(tex_id) else {
            return;
        };
        let sheet = SpriteSheet {
            frame_width: info.frame_width,
            frame_height: info.frame_height,
            frame_count: info.frame_count,
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(frame);

        // Use pre-flipped canvas for trees/water rocks (avoids canvas save/restore)
        if flip {
            let flipped = match key {
                SpriteKey::Tree(idx) => self
                    .loaded
                    .tree_textures_flipped
                    .get(idx)
                    .map(|(c, _, _)| c),
                SpriteKey::WaterRock(idx) => self
                    .loaded
                    .water_rock_textures_flipped
                    .get(idx)
                    .map(|(c, _, _)| c),
                _ => None,
            };
            if let Some(canvas) = flipped {
                if (alpha - 1.0).abs() > 0.001 {
                    self.r.set_alpha(alpha);
                }
                let fw = info.frame_width as f64;
                let fh = info.frame_height as f64;
                let sheet_w = info.frame_count as f64 * fw;
                let flipped_sx = sheet_w - sx - fw;
                let _ =
                    self.r
                        .draw_canvas_region(canvas, flipped_sx, 0.0, fw, fh, x, y, w, h);
                if (alpha - 1.0).abs() > 0.001 {
                    self.r.set_alpha(1.0);
                }
                return;
            }
        }

        let _ = self
            .r
            .draw_sprite(tex_id, sx, sy, sw, sh, x, y, w, h, flip, alpha);
    }

    fn draw_rotated(&mut self, key: SpriteKey, cx: f64, cy: f64, size: f64, angle: f64) {
        let Some(tex_id) = self.texture_id(key) else {
            return;
        };
        let flip = angle.abs() > std::f64::consts::FRAC_PI_2;
        let draw_angle = if flip {
            angle + std::f64::consts::PI
        } else {
            angle
        };
        self.r.save();
        let _ = self.r.translate(cx, cy);
        let _ = self.r.rotate(draw_angle);
        let half = size / 2.0;
        let _ =
            self.r
                .draw_texture(tex_id, 0.0, 0.0, size, size, -half, -half, size, size);
        self.r.restore();
    }

    fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo> {
        let tex_id = self.texture_id(key)?;
        let info = self.r.texture_info(tex_id)?;
        Some(SpriteInfo {
            frame_w: info.frame_width,
            frame_h: info.frame_height,
            frame_count: info.frame_count,
        })
    }
}

/// Draw all Y-sorted foreground entities via the shared rendering pipeline.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_foreground(
    r: &Canvas2dRenderer,
    game: &Game,
    loaded: &LoadedTextures,
    animator: &TurnAnimator,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let mut backend = WasmDrawBackend { r, loaded };

    battlefield_core::rendering::foreground::draw_foreground(
        &mut backend,
        game,
        (min_gx, min_gy, max_gx, max_gy),
        elapsed,
        |u| {
            if animator.is_playing() {
                animator.is_visually_alive(u.id) || u.death_fade > 0.0
            } else {
                u.alive || u.death_fade > 0.0
            }
        },
    );
}
