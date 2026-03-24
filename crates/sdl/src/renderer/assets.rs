use battlefield_core::building::BuildingKind;
use battlefield_core::particle::ParticleKind;
use battlefield_core::render_util;
use battlefield_core::unit::{Faction, UnitAnim, UnitKind};
use sdl2::pixels::PixelFormatEnum;
use sdl2::render::{BlendMode, Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::video::WindowContext;
use std::collections::HashMap;

use super::text::TextRenderer;
use super::UnitTexKey;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// All loaded textures for rendering.
pub struct Assets<'a> {
    pub(super) unit_textures: HashMap<UnitTexKey, (Texture<'a>, u32, u32, u32)>,
    pub(super) particle_textures: HashMap<ParticleKind, Texture<'a>>,
    pub(super) building_textures: HashMap<(Faction, BuildingKind), (Texture<'a>, u32, u32)>,
    pub(super) arrow_texture: Option<Texture<'a>>,
    // Terrain
    pub(super) tilemap_texture: Option<Texture<'a>>,
    pub(super) tilemap_texture2: Option<Texture<'a>>,
    pub(super) water_texture: Option<Texture<'a>>,
    pub(super) foam_texture: Option<Texture<'a>>,
    pub(super) shadow_texture: Option<Texture<'a>>,
    // Decorations
    pub(super) tree_textures: Vec<(Texture<'a>, u32, u32, u32)>,
    pub(super) bush_textures: Vec<(Texture<'a>, u32, u32, u32)>,
    pub(super) rock_textures: Vec<Texture<'a>>,
    pub(super) water_rock_textures: Vec<(Texture<'a>, u32, u32, u32)>,
    // Buildings
    pub(super) tower_textures: Vec<Texture<'a>>,
    // UI 9-slice panels
    pub(super) ui_special_paper: Option<(Texture<'a>, u32, u32)>,
    pub(super) ui_blue_btn: Option<(Texture<'a>, u32, u32)>,
    pub(super) ui_red_btn: Option<(Texture<'a>, u32, u32)>,
    // Bars
    pub(super) ui_bar_base: Option<(Texture<'a>, u32, u32)>,
    pub(super) ui_bar_fill: Option<Texture<'a>>,
    // Ribbons
    pub(super) ui_big_ribbons: Option<Texture<'a>>,
    pub(super) ui_small_ribbons: Option<Texture<'a>>,
    // Swords
    pub(super) _ui_swords: Option<Texture<'a>>,
    // Wood table frame for minimap
    pub(super) ui_wood_table: Option<(Texture<'a>, u32, u32)>,
    // Fog
    pub fog_texture: Option<Texture<'a>>,
    // Text rendering
    pub text: TextRenderer,
}

/// Load a PNG file into an SDL2 texture using the `png` crate.
pub(super) fn load_png_texture<'a>(
    tc: &'a TextureCreator<WindowContext>,
    path: &str,
) -> Option<Texture<'a>> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let width = info.width;
    let height = info.height;

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let pixels = info.buffer_size() / 3;
            let mut rgba = Vec::with_capacity(pixels * 4);
            for i in 0..pixels {
                rgba.push(buf[i * 3]);
                rgba.push(buf[i * 3 + 1]);
                rgba.push(buf[i * 3 + 2]);
                rgba.push(255);
            }
            rgba
        }
        _ => return None,
    };

    let mut pixel_data = rgba;
    let surface = Surface::from_data(
        &mut pixel_data,
        width,
        height,
        width * 4,
        PixelFormatEnum::ABGR8888,
    )
    .ok()?;
    let mut tex = tc.create_texture_from_surface(&surface).ok()?;
    tex.set_blend_mode(BlendMode::Blend);
    Some(tex)
}

/// Load a PNG and rearrange its 9 source cells into a gapless atlas texture.
fn load_9slice_atlas<'a>(
    tc: &'a TextureCreator<WindowContext>,
    path: &str,
    cells: &[[f64; 4]; 9],
) -> Option<(Texture<'a>, u32, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let src_w = info.width as usize;
    let bpp: usize = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return None,
    };

    let (aw, ah) = render_util::nine_cell_atlas_size(cells);
    let positions = render_util::nine_cell_atlas_positions(cells);
    let mut atlas = vec![0u8; (aw as usize) * (ah as usize) * 4];

    for (i, cell) in cells.iter().enumerate() {
        let (sx, sy, sw, sh) = (
            cell[0] as usize,
            cell[1] as usize,
            cell[2] as usize,
            cell[3] as usize,
        );
        let (dx, dy) = (positions[i].0 as usize, positions[i].1 as usize);
        for row in 0..sh {
            for col in 0..sw {
                let src_idx = ((sy + row) * src_w + (sx + col)) * bpp;
                let dst_idx = ((dy + row) * aw as usize + (dx + col)) * 4;
                if src_idx + bpp <= buf.len() && dst_idx + 4 <= atlas.len() {
                    atlas[dst_idx] = buf[src_idx];
                    atlas[dst_idx + 1] = buf[src_idx + 1];
                    atlas[dst_idx + 2] = buf[src_idx + 2];
                    atlas[dst_idx + 3] = if bpp == 4 { buf[src_idx + 3] } else { 255 };
                }
            }
        }
    }

    let surface = Surface::from_data(&mut atlas, aw, ah, aw * 4, PixelFormatEnum::ABGR8888).ok()?;
    let mut tex = tc.create_texture_from_surface(&surface).ok()?;
    tex.set_blend_mode(BlendMode::Blend);
    Some((tex, aw, ah))
}

/// Pre-process a 3-part bar sprite into a gapless horizontal atlas.
fn load_3slice_atlas<'a>(
    tc: &'a TextureCreator<WindowContext>,
    path: &str,
    cells: &[[f64; 4]; 3],
) -> Option<(Texture<'a>, u32, u32)> {
    let file = std::fs::File::open(path).ok()?;
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let src_w = info.width as usize;
    let bpp: usize = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        _ => return None,
    };

    let aw = (cells[0][2] + cells[1][2] + cells[2][2]).ceil() as u32;
    let ah = cells[0][3].max(cells[1][3]).max(cells[2][3]).ceil() as u32;
    let mut atlas = vec![0u8; (aw as usize) * (ah as usize) * 4];

    let mut dx_offset = 0usize;
    for cell in cells {
        let (sx, sy, sw, sh) = (
            cell[0] as usize,
            cell[1] as usize,
            cell[2] as usize,
            cell[3] as usize,
        );
        for row in 0..sh {
            for col in 0..sw {
                let src_idx = ((sy + row) * src_w + (sx + col)) * bpp;
                let dst_idx = (row * aw as usize + (dx_offset + col)) * 4;
                if src_idx + bpp <= buf.len() && dst_idx + 4 <= atlas.len() {
                    atlas[dst_idx] = buf[src_idx];
                    atlas[dst_idx + 1] = buf[src_idx + 1];
                    atlas[dst_idx + 2] = buf[src_idx + 2];
                    atlas[dst_idx + 3] = if bpp == 4 { buf[src_idx + 3] } else { 255 };
                }
            }
        }
        dx_offset += sw;
    }

    let surface = Surface::from_data(&mut atlas, aw, ah, aw * 4, PixelFormatEnum::ABGR8888).ok()?;
    let mut tex = tc.create_texture_from_surface(&surface).ok()?;
    tex.set_blend_mode(BlendMode::Blend);
    Some((tex, aw, ah))
}

/// Count frames in a horizontal sprite sheet given total width and frame width.
fn count_frames(path: &str, frame_w: u32) -> u32 {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return 1,
    };
    let decoder = png::Decoder::new(file);
    let reader = match decoder.read_info() {
        Ok(r) => r,
        Err(_) => return 1,
    };
    let info = reader.info();
    info.width / frame_w
}

impl<'a> Assets<'a> {
    pub fn load(tc: &'a TextureCreator<WindowContext>) -> Self {
        let mut unit_textures = HashMap::new();
        let mut particle_textures = HashMap::new();
        let mut building_textures = HashMap::new();
        let mut arrow_texture = None;

        // Load unit sprites
        for &faction in &[Faction::Blue, Faction::Red] {
            let folder = faction.asset_folder();
            for &kind in &[
                UnitKind::Warrior,
                UnitKind::Archer,
                UnitKind::Lancer,
                UnitKind::Monk,
            ] {
                let frame_size = kind.frame_size();
                let anims: &[(UnitAnim, &str, u32)] = match kind {
                    UnitKind::Warrior => &[
                        (UnitAnim::Idle, "Warrior_Idle.png", kind.idle_frames()),
                        (UnitAnim::Run, "Warrior_Run.png", kind.run_frames()),
                        (
                            UnitAnim::Attack,
                            "Warrior_Attack1.png",
                            kind.attack_frames(),
                        ),
                    ],
                    UnitKind::Archer => &[
                        (UnitAnim::Idle, "Archer_Idle.png", kind.idle_frames()),
                        (UnitAnim::Run, "Archer_Run.png", kind.run_frames()),
                        (UnitAnim::Attack, "Archer_Shoot.png", kind.attack_frames()),
                    ],
                    UnitKind::Lancer => &[
                        (UnitAnim::Idle, "Lancer_Idle.png", kind.idle_frames()),
                        (UnitAnim::Run, "Lancer_Run.png", kind.run_frames()),
                        (
                            UnitAnim::Attack,
                            "Lancer_Right_Attack.png",
                            kind.attack_frames(),
                        ),
                    ],
                    UnitKind::Monk => &[
                        (UnitAnim::Idle, "Idle.png", kind.idle_frames()),
                        (UnitAnim::Run, "Run.png", kind.run_frames()),
                        (UnitAnim::Attack, "Heal.png", kind.attack_frames()),
                    ],
                };

                let kind_folder = match kind {
                    UnitKind::Warrior => "Warrior",
                    UnitKind::Archer => "Archer",
                    UnitKind::Lancer => "Lancer",
                    UnitKind::Monk => "Monk",
                };

                for &(anim, filename, frames) in anims {
                    let path = format!("{ASSET_BASE}/Units/{folder}/{kind_folder}/{filename}");
                    if let Some(tex) = load_png_texture(tc, &path) {
                        unit_textures.insert(
                            UnitTexKey {
                                faction,
                                kind,
                                anim,
                            },
                            (tex, frame_size, frame_size, frames),
                        );
                    } else {
                        log::warn!("Missing texture: {path}");
                    }
                }

                if kind == UnitKind::Archer && arrow_texture.is_none() {
                    let path = format!("{ASSET_BASE}/Units/{folder}/Archer/Arrow.png");
                    arrow_texture = load_png_texture(tc, &path);
                }
            }
        }

        // Load particle textures
        for &kind in &[ParticleKind::Dust, ParticleKind::ExplosionLarge] {
            let path = format!("{ASSET_BASE}/Particle FX/{}", kind.asset_filename());
            if let Some(tex) = load_png_texture(tc, &path) {
                particle_textures.insert(kind, tex);
            }
        }

        // Load building textures
        for &faction in &[Faction::Blue, Faction::Red] {
            let color = match faction {
                Faction::Blue => "Blue",
                Faction::Red => "Red",
            };
            for &bkind in &[
                BuildingKind::Barracks,
                BuildingKind::Archery,
                BuildingKind::Monastery,
                BuildingKind::Castle,
                BuildingKind::DefenseTower,
                BuildingKind::House,
            ] {
                let (sw, sh) = bkind.sprite_size();
                let path = format!(
                    "{ASSET_BASE}/Buildings/{color} Buildings/{}",
                    bkind.asset_filename()
                );
                if let Some(tex) = load_png_texture(tc, &path) {
                    building_textures.insert((faction, bkind), (tex, sw, sh));
                }
            }
        }

        // Terrain textures
        let tilemap_texture = load_png_texture(
            tc,
            &format!("{ASSET_BASE}/Terrain/Tileset/Tilemap_color1.png"),
        );
        let tilemap_texture2 = load_png_texture(
            tc,
            &format!("{ASSET_BASE}/Terrain/Tileset/Tilemap_color2.png"),
        );
        let water_texture = load_png_texture(
            tc,
            &format!("{ASSET_BASE}/Terrain/Tileset/Water Background color.png"),
        );
        let foam_texture =
            load_png_texture(tc, &format!("{ASSET_BASE}/Terrain/Tileset/Water Foam.png"));
        let shadow_texture =
            load_png_texture(tc, &format!("{ASSET_BASE}/Terrain/Tileset/Shadow.png"));

        // Tree sprites (4 variants, animated)
        let tree_specs: &[(&str, u32, u32)] = &[
            ("Tree1.png", 192, 256),
            ("Tree2.png", 192, 256),
            ("Tree3.png", 192, 192),
            ("Tree4.png", 192, 192),
        ];
        let mut tree_textures = Vec::new();
        for &(filename, fw, fh) in tree_specs {
            let path = format!("{ASSET_BASE}/Terrain/Resources/Wood/Trees/{filename}");
            let fc = count_frames(&path, fw);
            if let Some(tex) = load_png_texture(tc, &path) {
                tree_textures.push((tex, fw, fh, fc));
            }
        }

        // Bush sprites (4 variants, animated 128x128 frames)
        let mut bush_textures = Vec::new();
        for i in 1..=4 {
            let path = format!("{ASSET_BASE}/Terrain/Decorations/Bushes/Bushe{i}.png");
            let fc = count_frames(&path, 128);
            if let Some(tex) = load_png_texture(tc, &path) {
                bush_textures.push((tex, 128u32, 128u32, fc));
            }
        }

        // Rock sprites (4 variants, single 64x64)
        let mut rock_textures = Vec::new();
        for i in 1..=4 {
            let path = format!("{ASSET_BASE}/Terrain/Decorations/Rocks/Rock{i}.png");
            if let Some(tex) = load_png_texture(tc, &path) {
                rock_textures.push(tex);
            }
        }

        // Water rock sprites (4 variants, animated 64x64 frames)
        let mut water_rock_textures = Vec::new();
        for i in 1..=4 {
            let path =
                format!("{ASSET_BASE}/Terrain/Decorations/Rocks in the Water/Water Rocks_0{i}.png");
            let fc = count_frames(&path, 64);
            if let Some(tex) = load_png_texture(tc, &path) {
                water_rock_textures.push((tex, 64u32, 64u32, fc));
            }
        }

        // Tower textures (neutral, blue, red)
        let mut tower_textures = Vec::new();
        for color_folder in &["Black Buildings", "Blue Buildings", "Red Buildings"] {
            let path = format!("{ASSET_BASE}/Buildings/{color_folder}/Tower.png");
            if let Some(tex) = load_png_texture(tc, &path) {
                tower_textures.push(tex);
            }
        }

        // UI 9-slice panels (pre-processed gapless atlases)
        let ui_base = format!("{ASSET_BASE}/UI Elements/UI Elements");
        let ui_special_paper = load_9slice_atlas(
            tc,
            &format!("{ui_base}/Papers/SpecialPaper.png"),
            &render_util::SPECIAL_PAPER_CELLS,
        );
        let ui_blue_btn = load_9slice_atlas(
            tc,
            &format!("{ui_base}/Buttons/BigBlueButton_Regular.png"),
            &render_util::BUTTON_CELLS,
        );
        let ui_red_btn = load_9slice_atlas(
            tc,
            &format!("{ui_base}/Buttons/BigRedButton_Regular.png"),
            &render_util::BUTTON_CELLS,
        );

        // Bars (3-part horizontal, pre-processed gapless atlas)
        let ui_bar_base = {
            let cells: [[f64; 4]; 3] = [
                [
                    render_util::BAR_LEFT.0,
                    render_util::BAR_LEFT.1,
                    render_util::BAR_LEFT.2,
                    render_util::BAR_LEFT.3,
                ],
                [
                    render_util::BAR_CENTER.0,
                    render_util::BAR_CENTER.1,
                    render_util::BAR_CENTER.2,
                    render_util::BAR_CENTER.3,
                ],
                [
                    render_util::BAR_RIGHT.0,
                    render_util::BAR_RIGHT.1,
                    render_util::BAR_RIGHT.2,
                    render_util::BAR_RIGHT.3,
                ],
            ];
            load_3slice_atlas(tc, &format!("{ui_base}/Bars/BigBar_Base.png"), &cells)
        };

        // Bar fill — convert to greyscale so set_color_mod can tint to any color
        let ui_bar_fill = {
            let path = format!("{ui_base}/Bars/BigBar_Fill.png");
            let file = std::fs::File::open(&path).ok();
            file.and_then(|f| {
                let decoder = png::Decoder::new(f);
                let mut reader = decoder.read_info().ok()?;
                let mut buf = vec![0u8; reader.output_buffer_size()];
                let info = reader.next_frame(&mut buf).ok()?;
                let w = info.width;
                let h = info.height;
                let bpp: usize = if info.color_type == png::ColorType::Rgba {
                    4
                } else {
                    3
                };
                let mut grey = vec![0u8; (w * h * 4) as usize];
                for i in 0..(w * h) as usize {
                    let si = i * bpp;
                    let lum =
                        ((buf[si] as u16 * 77 + buf[si + 1] as u16 * 150 + buf[si + 2] as u16 * 29)
                            >> 8) as u8;
                    let boosted = 128 + (lum as u16 * 127 / 255) as u8;
                    let di = i * 4;
                    grey[di] = boosted;
                    grey[di + 1] = boosted;
                    grey[di + 2] = boosted;
                    grey[di + 3] = if bpp == 4 {
                        buf[si + 3]
                    } else if lum > 10 {
                        255
                    } else {
                        0
                    };
                }
                let surface =
                    Surface::from_data(&mut grey, w, h, w * 4, PixelFormatEnum::ABGR8888).ok()?;
                let mut tex = tc.create_texture_from_surface(&surface).ok()?;
                tex.set_blend_mode(BlendMode::Blend);
                Some(tex)
            })
        };

        // Ribbons
        let ui_big_ribbons = load_png_texture(tc, &format!("{ui_base}/Ribbons/BigRibbons.png"));
        let ui_small_ribbons = load_png_texture(tc, &format!("{ui_base}/Ribbons/SmallRibbons.png"));

        // Swords
        let ui_swords = load_png_texture(tc, &format!("{ui_base}/Swords/Swords.png"));

        // Wood table frame for minimap
        let ui_wood_table = load_9slice_atlas(
            tc,
            &format!("{ui_base}/Wood Table/WoodTable.png"),
            &render_util::WOOD_TABLE_CELLS,
        );

        Self {
            unit_textures,
            particle_textures,
            building_textures,
            arrow_texture,
            tilemap_texture,
            tilemap_texture2,
            water_texture,
            foam_texture,
            shadow_texture,
            tree_textures,
            bush_textures,
            rock_textures,
            water_rock_textures,
            tower_textures,
            ui_special_paper,
            ui_blue_btn,
            ui_red_btn,
            ui_bar_base,
            ui_bar_fill,
            ui_big_ribbons,
            ui_small_ribbons,
            _ui_swords: ui_swords,
            ui_wood_table,
            fog_texture: {
                use battlefield_core::grid::GRID_SIZE;
                sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");
                let tex = tc
                    .create_texture_streaming(Some(PixelFormatEnum::ABGR8888), GRID_SIZE, GRID_SIZE)
                    .ok()
                    .map(|mut t| {
                        t.set_blend_mode(BlendMode::Blend);
                        t
                    });
                sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
                tex
            },
            text: TextRenderer::new(),
        }
    }
}
