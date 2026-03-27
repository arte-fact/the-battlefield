//! Asset loading — PNG decode → wgpu textures.
//!
//! Paths and loading order match the SDL crate's assets.rs exactly.

use crate::gpu::GpuContext;
use crate::renderer::sprite_batch::{GpuTexture, TextureId};
use battlefield_core::asset_manifest::{self, ASSET_BASE};
use battlefield_core::particle::ParticleKind;
use battlefield_core::render_util;
use battlefield_core::rendering::{SpriteInfo, SpriteKey};
use battlefield_core::unit::{Faction, UnitAnim, UnitKind};
use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Unit texture key (same as SDL crate)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Hash, Eq, PartialEq)]
struct UnitTexKey {
    faction: Faction,
    kind: UnitKind,
    anim: UnitAnim,
}

// ─────────────────────────────────────────────────────────────────────────────
// Assets
// ─────────────────────────────────────────────────────────────────────────────

pub struct Assets {
    /// All GPU textures (flat vec, indexed by TextureId).
    pub textures: Vec<GpuTexture>,

    /// Unit sprites: key → (TextureId, frame_w, frame_h, frame_count).
    unit_textures: HashMap<UnitTexKey, (TextureId, u32, u32, u32)>,

    /// Particle textures indexed by ParticleKind.
    particle_textures: HashMap<ParticleKind, TextureId>,

    /// Building textures indexed by `asset_manifest::building_tex_index()`.
    building_textures: Vec<Option<(TextureId, u32, u32)>>,

    /// Arrow projectile.
    arrow_texture: Option<TextureId>,

    /// Tower color variants: neutral, blue, red.
    tower_textures: Vec<TextureId>,

    /// Tree variants: (TextureId, frame_w, frame_h, frame_count).
    tree_textures: Vec<(TextureId, u32, u32, u32)>,

    /// Bush variants.
    bush_textures: Vec<(TextureId, u32, u32, u32)>,

    /// Rock variants.
    rock_textures: Vec<TextureId>,

    /// Water rock variants.
    water_rock_textures: Vec<(TextureId, u32, u32, u32)>,

    /// Sheep animation variants.
    sheep_textures: Vec<(TextureId, u32)>,

    /// Pawn textures.
    pawn_textures: Vec<(TextureId, u32, u32, u32)>,

    /// Avatar textures.
    avatar_textures: Vec<TextureId>,

    /// Terrain textures.
    pub tilemap_texture: Option<TextureId>,
    pub tilemap_texture2: Option<TextureId>,
    pub water_texture: Option<TextureId>,
    pub foam_texture: Option<TextureId>,
    pub shadow_texture: Option<TextureId>,

    /// Fog of war (dynamic texture, updated per frame).
    pub fog_texture: Option<TextureId>,
    pub fog_wgpu_texture: Option<wgpu::Texture>,

    /// 1x1 white pixel texture for drawing colored rects in the sprite batch.
    pub white_texture: Option<TextureId>,

    /// UI 9-slice panels.
    pub ui_special_paper: Option<(TextureId, u32, u32)>,
    pub ui_blue_btn: Option<(TextureId, u32, u32)>,
    pub ui_red_btn: Option<(TextureId, u32, u32)>,
    pub ui_wood_table: Option<(TextureId, u32, u32)>,

    /// UI bars (3-slice).
    pub ui_bar_base: Option<(TextureId, u32, u32)>,
    pub ui_bar_fill: Option<TextureId>,

    /// Ribbons.
    pub ui_big_ribbons: Option<TextureId>,
    pub ui_small_ribbons: Option<TextureId>,

    /// Text renderer with glyph cache.
    pub text: crate::renderer::text::TextRenderer,
}

impl Assets {
    pub fn load(gpu: &GpuContext) -> Self {
        let mut assets = Self {
            textures: Vec::with_capacity(128),
            unit_textures: HashMap::new(),
            particle_textures: HashMap::new(),
            building_textures: Vec::new(),
            tower_textures: Vec::new(),
            tree_textures: Vec::new(),
            bush_textures: Vec::new(),
            rock_textures: Vec::new(),
            water_rock_textures: Vec::new(),
            arrow_texture: None,
            sheep_textures: Vec::new(),
            pawn_textures: Vec::new(),
            avatar_textures: Vec::new(),
            tilemap_texture: None,
            tilemap_texture2: None,
            water_texture: None,
            foam_texture: None,
            shadow_texture: None,
            fog_texture: None,
            fog_wgpu_texture: None,
            white_texture: None,
            ui_special_paper: None,
            ui_blue_btn: None,
            ui_red_btn: None,
            ui_wood_table: None,
            ui_bar_base: None,
            ui_bar_fill: None,
            ui_big_ribbons: None,
            ui_small_ribbons: None,
            text: crate::renderer::text::TextRenderer::new(0), // base_id set after load
        };

        assets.load_units(gpu);
        assets.load_particles(gpu);
        assets.load_buildings(gpu);
        assets.load_terrain(gpu);
        assets.load_decorations(gpu);
        assets.load_sheep(gpu);
        assets.load_pawns(gpu);
        assets.load_avatars(gpu);
        assets.create_fog_texture(gpu);
        assets.create_white_texture(gpu);
        assets.load_ui(gpu);

        // Text cache textures start after all asset textures
        assets.text = crate::renderer::text::TextRenderer::new(assets.textures.len());

        log::info!("Loaded {} GPU textures", assets.textures.len());
        assets
    }

    /// Look up texture info for a SpriteKey (used by DrawBackend).
    pub fn sprite_lookup(&self, key: SpriteKey) -> Option<(TextureId, u32, u32, u32)> {
        match key {
            SpriteKey::Unit {
                faction,
                kind,
                anim,
            } => self
                .unit_textures
                .get(&UnitTexKey {
                    faction,
                    kind,
                    anim,
                })
                .copied(),
            SpriteKey::Building(idx) => self
                .building_textures
                .get(idx)
                .and_then(|o| o.as_ref())
                .map(|&(id, w, h)| (id, w, h, 1)),
            SpriteKey::Tower(idx) => self.tower_textures.get(idx).map(|&id| {
                let t = &self.textures[id];
                (id, t.width, t.height, 1)
            }),
            SpriteKey::Tree(idx) => self.tree_textures.get(idx).copied(),
            SpriteKey::Rock(idx) => self.rock_textures.get(idx).map(|&id| {
                let t = &self.textures[id];
                (id, t.width, t.height, 1)
            }),
            SpriteKey::Bush(idx) => self.bush_textures.get(idx).copied(),
            SpriteKey::WaterRock(idx) => self.water_rock_textures.get(idx).copied(),
            SpriteKey::Particle(idx) => {
                // Map particle sprite index back to ParticleKind for lookup
                let kind = match idx {
                    0 => ParticleKind::Dust,
                    2 => ParticleKind::ExplosionLarge,
                    3 => ParticleKind::HealEffect,
                    _ => return None,
                };
                self.particle_textures.get(&kind).map(|&id| {
                    let t = &self.textures[id];
                    let fw = t.height;
                    let fc = t.width / fw.max(1);
                    (id, fw, fw, fc)
                })
            }
            SpriteKey::Arrow => self.arrow_texture.map(|id| {
                let t = &self.textures[id];
                (id, t.width, t.height, 1)
            }),
            SpriteKey::Sheep(idx) => self.sheep_textures.get(idx).map(|&(id, fc)| {
                let t = &self.textures[id];
                let fw = t.width / fc.max(1);
                (id, fw, t.height, fc)
            }),
            SpriteKey::Pawn(idx) => self.pawn_textures.get(idx).copied(),
            SpriteKey::Avatar(idx) => self.avatar_textures.get(idx).map(|&id| {
                let t = &self.textures[id];
                (id, t.width, t.height, 1)
            }),
        }
    }

    pub fn sprite_info(&self, key: SpriteKey) -> Option<SpriteInfo> {
        self.sprite_lookup(key).map(|(_, fw, fh, fc)| SpriteInfo {
            frame_w: fw,
            frame_h: fh,
            frame_count: fc,
        })
    }

    // ── Accessors for terrain draws ─────────────────────────────────────

    pub fn bush_textures_ref(&self) -> &[(TextureId, u32, u32, u32)] {
        &self.bush_textures
    }
    pub fn bush_count(&self) -> usize {
        self.bush_textures.len()
    }
    pub fn rock_textures_ref(&self) -> &[TextureId] {
        &self.rock_textures
    }
    pub fn rock_count(&self) -> usize {
        self.rock_textures.len()
    }

    // ── Internal loading helpers ────────────────────────────────────────

    fn load_png(&mut self, gpu: &GpuContext, path: &str) -> Option<TextureId> {
        let data = battlefield_assets::get(path)?;
        let id = self.upload_png(gpu, data, path);
        Some(id)
    }

    fn upload_png(&mut self, gpu: &GpuContext, data: &[u8], label: &str) -> TextureId {
        let (rgba, width, height) = decode_png(data);
        self.upload_rgba(gpu, &rgba, width, height, label)
    }

    fn upload_rgba(
        &mut self,
        gpu: &GpuContext,
        rgba: &[u8],
        width: u32,
        height: u32,
        label: &str,
    ) -> TextureId {
        let max_dim = gpu.device.limits().max_texture_dimension_2d;
        if width > max_dim || height > max_dim {
            log::warn!(
                "Texture {label} ({width}x{height}) exceeds max {max_dim}, creating 1x1 placeholder"
            );
            // Create a tiny placeholder texture so the game doesn't crash
            return self.upload_rgba(gpu, &[255, 0, 255, 255], 1, 1, label);
        }
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = gpu.create_texture_bind_group(&view, &gpu.nearest_sampler);

        let id = self.textures.len();
        self.textures.push(GpuTexture {
            _texture: texture,
            view,
            bind_group,
            width,
            height,
        });
        id
    }

    /// Count frames in a horizontal sprite sheet.
    fn count_frames(path: &str, frame_w: u32) -> u32 {
        let Some(data) = battlefield_assets::get(path) else {
            return 1;
        };
        let decoder = png::Decoder::new(std::io::Cursor::new(data));
        let reader = match decoder.read_info() {
            Ok(r) => r,
            Err(_) => return 1,
        };
        reader.info().width / frame_w
    }

    // ── Load units (matches SDL assets.rs lines 234-276) ────────────────

    fn load_units(&mut self, gpu: &GpuContext) {
        let all_anims = [
            UnitAnim::Idle,
            UnitAnim::Run,
            UnitAnim::Attack,
            UnitAnim::Attack2,
        ];

        for &faction in &[Faction::Blue, Faction::Red] {
            let folder = faction.asset_folder();
            for &kind in &[
                UnitKind::Warrior,
                UnitKind::Archer,
                UnitKind::Lancer,
                UnitKind::Monk,
            ] {
                let kind_folder = asset_manifest::unit_kind_folder(kind);
                for &anim in &all_anims {
                    let Some(spec) = asset_manifest::unit_sprite(kind, anim) else {
                        continue;
                    };
                    let path =
                        format!("{ASSET_BASE}/Units/{folder}/{kind_folder}/{}", spec.filename);
                    if let Some(id) = self.load_png(gpu, &path) {
                        self.unit_textures.insert(
                            UnitTexKey {
                                faction,
                                kind,
                                anim,
                            },
                            (id, spec.frame_w, spec.frame_h, spec.frame_count),
                        );
                    } else {
                        log::warn!("Missing texture: {path}");
                    }
                }
                // Arrow texture (from Archer folder)
                if kind == UnitKind::Archer && self.arrow_texture.is_none() {
                    let path = format!("{ASSET_BASE}/Units/{folder}/Archer/Arrow.png");
                    self.arrow_texture = self.load_png(gpu, &path);
                }
            }
        }
    }

    // ── Load particles (matches SDL assets.rs lines 278-291) ────────────

    fn load_particles(&mut self, gpu: &GpuContext) {
        for &kind in &[ParticleKind::Dust, ParticleKind::ExplosionLarge] {
            let path = format!("{ASSET_BASE}/Particle FX/{}", kind.asset_filename());
            if let Some(id) = self.load_png(gpu, &path) {
                self.particle_textures.insert(kind, id);
            }
        }
        // Heal effect (from Monk sprite folder)
        let path = format!("{ASSET_BASE}/Units/Blue Units/Monk/Heal_Effect.png");
        if let Some(id) = self.load_png(gpu, &path) {
            self.particle_textures.insert(ParticleKind::HealEffect, id);
        }
    }

    // ── Load buildings (matches SDL assets.rs lines 293-306) ────────────

    fn load_buildings(&mut self, gpu: &GpuContext) {
        let total = asset_manifest::BUILDING_SPECS.len() * 2;
        self.building_textures = (0..total).map(|_| None).collect();

        for (spec_idx, &(sw, sh, filename)) in asset_manifest::BUILDING_SPECS.iter().enumerate() {
            for (faction_idx, faction_folder) in
                asset_manifest::BUILDING_FACTION_FOLDERS.iter().enumerate()
            {
                let path = format!("{ASSET_BASE}/Buildings/{faction_folder}/{filename}");
                if let Some(id) = self.load_png(gpu, &path) {
                    self.building_textures[spec_idx * 2 + faction_idx] = Some((id, sw, sh));
                }
            }
        }

        // Tower textures (neutral/black, blue, red)
        for color_folder in asset_manifest::TOWER_COLOR_FOLDERS {
            let path = format!("{ASSET_BASE}/Buildings/{color_folder}/Tower.png");
            if let Some(id) = self.load_png(gpu, &path) {
                self.tower_textures.push(id);
            }
        }
    }

    // ── Load terrain (matches SDL assets.rs lines 308-324) ──────────────

    fn load_terrain(&mut self, gpu: &GpuContext) {
        self.tilemap_texture =
            self.load_png(gpu, &format!("{ASSET_BASE}/Terrain/Tileset/Tilemap_color1.png"));
        self.tilemap_texture2 =
            self.load_png(gpu, &format!("{ASSET_BASE}/Terrain/Tileset/Tilemap_color2.png"));
        self.water_texture = self.load_png(
            gpu,
            &format!("{ASSET_BASE}/Terrain/Tileset/Water Background color.png"),
        );
        self.foam_texture =
            self.load_png(gpu, &format!("{ASSET_BASE}/Terrain/Tileset/Water Foam.png"));
        self.shadow_texture =
            self.load_png(gpu, &format!("{ASSET_BASE}/Terrain/Tileset/Shadow.png"));
    }

    // ── Load decorations (matches SDL assets.rs lines 326-370) ──────────

    fn load_decorations(&mut self, gpu: &GpuContext) {
        // Trees (4 variants, animated)
        for &(fw, fh, fc, filename) in asset_manifest::TREE_SPECS {
            let path = format!("{ASSET_BASE}/Terrain/Resources/Wood/Trees/{filename}");
            if let Some(id) = self.load_png(gpu, &path) {
                self.tree_textures.push((id, fw, fh, fc));
            }
        }

        // Bushes (4 variants, animated 128x128)
        for i in 1..=asset_manifest::BUSH_VARIANTS {
            let path = format!("{ASSET_BASE}/Terrain/Decorations/Bushes/Bushe{i}.png");
            let fc = Self::count_frames(&path, asset_manifest::BUSH_FRAME_SIZE);
            if let Some(id) = self.load_png(gpu, &path) {
                self.bush_textures.push((
                    id,
                    asset_manifest::BUSH_FRAME_SIZE,
                    asset_manifest::BUSH_FRAME_SIZE,
                    fc,
                ));
            }
        }

        // Rocks (4 variants, static)
        for i in 1..=asset_manifest::ROCK_VARIANTS {
            let path = format!("{ASSET_BASE}/Terrain/Decorations/Rocks/Rock{i}.png");
            if let Some(id) = self.load_png(gpu, &path) {
                self.rock_textures.push(id);
            }
        }

        // Water rocks (4 variants, animated 64x64)
        for i in 1..=asset_manifest::WATER_ROCK_VARIANTS {
            let path =
                format!("{ASSET_BASE}/Terrain/Decorations/Rocks in the Water/Water Rocks_0{i}.png");
            let fc = Self::count_frames(&path, asset_manifest::WATER_ROCK_FRAME_SIZE);
            if let Some(id) = self.load_png(gpu, &path) {
                self.water_rock_textures.push((
                    id,
                    asset_manifest::WATER_ROCK_FRAME_SIZE,
                    asset_manifest::WATER_ROCK_FRAME_SIZE,
                    fc,
                ));
            }
        }
    }

    // ── Load sheep (matches SDL assets.rs lines 480-492) ────────────────

    fn load_sheep(&mut self, gpu: &GpuContext) {
        for &(filename, frame_count) in asset_manifest::SHEEP_SPECS {
            let path = format!("{ASSET_BASE}/Terrain/Resources/Meat/Sheep/{filename}");
            if let Some(id) = self.load_png(gpu, &path) {
                self.sheep_textures.push((id, frame_count));
            }
        }
    }

    // ── Load pawns (matches SDL assets.rs lines 494-511) ────────────────

    fn load_pawns(&mut self, gpu: &GpuContext) {
        let pawn_frame_size = battlefield_core::pawn::PAWN_FRAME_SIZE;
        let faction_folders = ["Blue Units", "Red Units"];
        for folder in &faction_folders {
            for &(filename, frame_count) in asset_manifest::PAWN_SPECS {
                let path = format!("{ASSET_BASE}/Units/{folder}/Pawn/{filename}");
                let fc = if frame_count > 0 {
                    frame_count
                } else {
                    Self::count_frames(&path, pawn_frame_size)
                };
                if let Some(id) = self.load_png(gpu, &path) {
                    self.pawn_textures
                        .push((id, pawn_frame_size, pawn_frame_size, fc));
                }
            }
        }
    }

    // ── Load avatars (matches SDL assets.rs lines 539-549) ──────────────

    fn load_avatars(&mut self, gpu: &GpuContext) {
        let avatar_base = format!("{ASSET_BASE}/UI Elements/UI Elements/Human Avatars");
        for filename in asset_manifest::AVATAR_FILES {
            let path = format!("{avatar_base}/{filename}");
            if let Some(id) = self.load_png(gpu, &path) {
                self.avatar_textures.push(id);
            }
        }
    }

    // ── UI textures (9-slice panels, 3-slice bars, ribbons) ────────────

    fn load_ui(&mut self, gpu: &GpuContext) {
        let ui_base = format!("{ASSET_BASE}/UI Elements/UI Elements");

        // 9-slice panels
        self.ui_special_paper = self.load_9slice_atlas(
            gpu,
            &format!("{ui_base}/Papers/SpecialPaper.png"),
            &render_util::SPECIAL_PAPER_CELLS,
        );
        self.ui_blue_btn = self.load_9slice_atlas(
            gpu,
            &format!("{ui_base}/Buttons/BigBlueButton_Regular.png"),
            &render_util::BUTTON_CELLS,
        );
        self.ui_red_btn = self.load_9slice_atlas(
            gpu,
            &format!("{ui_base}/Buttons/BigRedButton_Regular.png"),
            &render_util::BUTTON_CELLS,
        );
        self.ui_wood_table = self.load_9slice_atlas(
            gpu,
            &format!("{ui_base}/Wood Table/WoodTable.png"),
            &render_util::WOOD_TABLE_CELLS,
        );

        // 3-slice bars
        let bar_cells: [[f64; 4]; 3] = [
            [render_util::BAR_LEFT.0, render_util::BAR_LEFT.1, render_util::BAR_LEFT.2, render_util::BAR_LEFT.3],
            [render_util::BAR_CENTER.0, render_util::BAR_CENTER.1, render_util::BAR_CENTER.2, render_util::BAR_CENTER.3],
            [render_util::BAR_RIGHT.0, render_util::BAR_RIGHT.1, render_util::BAR_RIGHT.2, render_util::BAR_RIGHT.3],
        ];
        self.ui_bar_base = self.load_3slice_atlas(
            gpu,
            &format!("{ui_base}/Bars/BigBar_Base.png"),
            &bar_cells,
        );

        // Bar fill — load and convert to greyscale for color tinting
        self.ui_bar_fill = self.load_bar_fill(gpu, &format!("{ui_base}/Bars/BigBar_Fill.png"));

        // Ribbons
        self.ui_big_ribbons = self.load_png(gpu, &format!("{ui_base}/Ribbons/BigRibbons.png"));
        self.ui_small_ribbons = self.load_png(gpu, &format!("{ui_base}/Ribbons/SmallRibbons.png"));
    }

    fn load_9slice_atlas(
        &mut self, gpu: &GpuContext, path: &str, cells: &[[f64; 4]; 9],
    ) -> Option<(TextureId, u32, u32)> {
        let data = battlefield_assets::get(path)?;
        let (src_pixels, src_w, _src_h) = decode_png(data);
        let (aw, ah) = render_util::nine_cell_atlas_size(cells);
        let positions = render_util::nine_cell_atlas_positions(cells);
        let mut atlas = vec![0u8; (aw as usize) * (ah as usize) * 4];

        for (i, cell) in cells.iter().enumerate() {
            let (sx, sy, sw, sh) = (cell[0] as usize, cell[1] as usize, cell[2] as usize, cell[3] as usize);
            let (dx, dy) = (positions[i].0 as usize, positions[i].1 as usize);
            for row in 0..sh {
                for col in 0..sw {
                    let si = ((sy + row) * src_w as usize + (sx + col)) * 4;
                    let di = ((dy + row) * aw as usize + (dx + col)) * 4;
                    if si + 4 <= src_pixels.len() && di + 4 <= atlas.len() {
                        atlas[di..di + 4].copy_from_slice(&src_pixels[si..si + 4]);
                    }
                }
            }
        }

        let id = self.upload_rgba(gpu, &atlas, aw, ah, path);
        Some((id, aw, ah))
    }

    fn load_3slice_atlas(
        &mut self, gpu: &GpuContext, path: &str, cells: &[[f64; 4]; 3],
    ) -> Option<(TextureId, u32, u32)> {
        let data = battlefield_assets::get(path)?;
        let (src_pixels, src_w, _src_h) = decode_png(data);
        let aw = (cells[0][2] + cells[1][2] + cells[2][2]).ceil() as u32;
        let ah = cells[0][3].max(cells[1][3]).max(cells[2][3]).ceil() as u32;
        let mut atlas = vec![0u8; (aw as usize) * (ah as usize) * 4];

        let mut dx_offset = 0usize;
        for cell in cells {
            let (sx, sy, sw, sh) = (cell[0] as usize, cell[1] as usize, cell[2] as usize, cell[3] as usize);
            for row in 0..sh {
                for col in 0..sw {
                    let si = ((sy + row) * src_w as usize + (sx + col)) * 4;
                    let di = (row * aw as usize + (dx_offset + col)) * 4;
                    if si + 4 <= src_pixels.len() && di + 4 <= atlas.len() {
                        atlas[di..di + 4].copy_from_slice(&src_pixels[si..si + 4]);
                    }
                }
            }
            dx_offset += sw;
        }

        let id = self.upload_rgba(gpu, &atlas, aw, ah, path);
        Some((id, aw, ah))
    }

    fn load_bar_fill(&mut self, gpu: &GpuContext, path: &str) -> Option<TextureId> {
        let data = battlefield_assets::get(path)?;
        let (src, w, h) = decode_png(data);
        // Convert to boosted greyscale for color_mod tinting
        let mut grey = vec![0u8; (w * h * 4) as usize];
        for i in 0..(w * h) as usize {
            let si = i * 4;
            let lum = ((src[si] as u16 * 77 + src[si + 1] as u16 * 150 + src[si + 2] as u16 * 29) >> 8) as u8;
            let boosted = 128 + (lum as u16 * 127 / 255) as u8;
            let di = i * 4;
            grey[di] = boosted;
            grey[di + 1] = boosted;
            grey[di + 2] = boosted;
            grey[di + 3] = if src.len() > si + 3 { src[si + 3] } else if lum > 10 { 255 } else { 0 };
        }
        Some(self.upload_rgba(gpu, &grey, w, h, path))
    }

    // ── Fog of war texture ──────────────────────────────────────────────

    fn create_fog_texture(&mut self, gpu: &GpuContext) {
        let size = battlefield_core::grid::GRID_SIZE;
        // Use linear (non-sRGB) format for fog — the pixel data is raw RGBA
        // with alpha encoding fog darkness. sRGB would distort the values.
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fog"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Fog uses linear sampler for smooth edges
        let bind_group = gpu.create_texture_bind_group(&view, &gpu.linear_sampler);

        let id = self.textures.len();
        self.textures.push(GpuTexture {
            _texture: texture.clone(),
            view,
            bind_group,
            width: size,
            height: size,
        });
        self.fog_texture = Some(id);
        self.fog_wgpu_texture = Some(texture);
    }

    fn create_white_texture(&mut self, gpu: &GpuContext) {
        let id = self.upload_rgba(gpu, &[255, 255, 255, 255], 1, 1, "white_1x1");
        self.white_texture = Some(id);
    }

    /// Update fog of war texture pixels.
    pub fn update_fog(&self, gpu: &GpuContext, pixels: &[u8], size: u32) {
        if let Some(tex) = &self.fog_wgpu_texture {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * size),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PNG decoding
// ─────────────────────────────────────────────────────────────────────────────

fn decode_png(data: &[u8]) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder.read_info().expect("PNG read_info");
    let info = reader.info();
    let width = info.width;
    let height = info.height;
    let color_type = info.color_type;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let frame = reader.next_frame(&mut buf).expect("PNG next_frame");
    let raw = &buf[..frame.buffer_size()];

    match color_type {
        png::ColorType::Rgba => (raw.to_vec(), width, height),
        png::ColorType::Rgb => {
            let pixel_count = (width * height) as usize;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in raw.chunks_exact(3) {
                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
            }
            (rgba, width, height)
        }
        png::ColorType::GrayscaleAlpha => {
            let pixel_count = (width * height) as usize;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for chunk in raw.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            (rgba, width, height)
        }
        png::ColorType::Grayscale => {
            let pixel_count = (width * height) as usize;
            let mut rgba = Vec::with_capacity(pixel_count * 4);
            for &v in raw {
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
            (rgba, width, height)
        }
        _ => {
            log::warn!(
                "Unsupported PNG color type: {:?}, creating empty texture",
                color_type
            );
            let pixel_count = (width * height) as usize;
            (vec![0u8; pixel_count * 4], width, height)
        }
    }
}
