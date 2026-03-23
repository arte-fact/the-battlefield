#![allow(clippy::too_many_arguments)]

use battlefield_core::autotile;
use battlefield_core::building::BuildingKind;
use battlefield_core::camera::Camera;
use battlefield_core::game::{Game, ATTACK_CONE_HALF_ANGLE, ORDER_FLASH_DURATION};
use battlefield_core::grid::{self, Decoration, TileKind, TILE_SIZE};
use battlefield_core::particle::ParticleKind;
use battlefield_core::render_util;
use battlefield_core::sprite::SpriteSheet;
use battlefield_core::unit::{Facing, Faction, UnitAnim, UnitKind};
use battlefield_core::zone::{ZoneState, VICTORY_HOLD_TIME};
use rusttype::{point, Font, Scale};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{BlendMode, Canvas, Texture, TextureCreator};
use sdl2::surface::Surface;
use sdl2::video::{Window, WindowContext};
use std::collections::HashMap;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

pub use battlefield_core::ui::GameScreen;

/// A clickable button region returned by the renderer for hit-testing.
pub struct ClickableButton {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub action: battlefield_core::ui::ButtonAction,
}

impl ClickableButton {
    /// Returns true if the given point is inside this button's rectangle.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        let px = px as f64;
        let py = py as f64;
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Texture keys
// ───────────────────────────────────────────────────────────────────────────

#[derive(Hash, Eq, PartialEq)]
struct UnitTexKey {
    faction: Faction,
    kind: UnitKind,
    anim: UnitAnim,
}

// ───────────────────────────────────────────────────────────────────────────
// Asset loading
// ───────────────────────────────────────────────────────────────────────────

// ───────────────────────────────────────────────────────────────────────────
// Text rendering (rusttype, pure Rust TTF)
// ───────────────────────────────────────────────────────────────────────────

pub struct TextRenderer {
    font: Font<'static>,
}

impl TextRenderer {
    fn new() -> Self {
        let font_data = std::fs::read("assets/Uncial.ttf").expect("Failed to load font");
        let font = Font::try_from_vec(font_data).expect("Failed to parse font");
        Self { font }
    }

    /// Draw text centered at `(cx, cy)` with given color and size.
    fn draw_text_centered(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        text: &str,
        cx: i32,
        cy: i32,
        size: f32,
        color: Color,
    ) {
        let (text_w, text_h, mut pixels) = match self.rasterize(text, size, color) {
            Some(result) => result,
            None => return,
        };
        let dx = cx - text_w as i32 / 2;
        let dy = cy - text_h as i32 / 2;
        blit_pixels(canvas, tc, &mut pixels, text_w, text_h, dx, dy);
    }

    /// Draw text left-aligned at `(x, y)` (top-left corner).
    #[allow(dead_code)]
    fn draw_text(
        &self,
        canvas: &mut Canvas<Window>,
        tc: &TextureCreator<WindowContext>,
        text: &str,
        x: i32,
        y: i32,
        size: f32,
        color: Color,
    ) {
        let (text_w, text_h, mut pixels) = match self.rasterize(text, size, color) {
            Some(result) => result,
            None => return,
        };
        blit_pixels(canvas, tc, &mut pixels, text_w, text_h, x, y);
    }

    /// Rasterize text into an RGBA pixel buffer, returning `(width, height, pixels)`.
    fn rasterize(&self, text: &str, size: f32, color: Color) -> Option<(u32, u32, Vec<u8>)> {
        let scale = Scale::uniform(size);
        let v_metrics = self.font.v_metrics(scale);
        let glyphs: Vec<_> = self
            .font
            .layout(text, scale, point(0.0, v_metrics.ascent))
            .collect();

        if glyphs.is_empty() {
            return None;
        }

        let min_x = glyphs
            .first()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.min.x)
            .unwrap_or(0);
        let max_x = glyphs
            .last()
            .and_then(|g| g.pixel_bounding_box())
            .map(|bb| bb.max.x)
            .unwrap_or(0);
        let text_w = (max_x - min_x) as u32;
        let text_h = (v_metrics.ascent - v_metrics.descent).ceil() as u32;

        if text_w == 0 || text_h == 0 {
            return None;
        }

        let mut pixels = vec![0u8; (text_w * text_h * 4) as usize];
        for glyph in &glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px = (bb.min.x - min_x) as u32 + gx;
                    let py = bb.min.y as u32 + gy;
                    if px < text_w && py < text_h {
                        let idx = ((py * text_w + px) * 4) as usize;
                        let alpha = (v * color.a as f32) as u8;
                        pixels[idx] = color.r;
                        pixels[idx + 1] = color.g;
                        pixels[idx + 2] = color.b;
                        pixels[idx + 3] = alpha;
                    }
                });
            }
        }

        Some((text_w, text_h, pixels))
    }
}

/// Create an SDL surface from RGBA pixel data, convert to texture, and blit to canvas.
fn blit_pixels(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    pixels: &mut [u8],
    w: u32,
    h: u32,
    x: i32,
    y: i32,
) {
    let surface = match Surface::from_data(pixels, w, h, w * 4, PixelFormatEnum::ABGR8888) {
        Ok(s) => s,
        Err(_) => return,
    };
    let tex = match tc.create_texture_from_surface(&surface) {
        Ok(t) => t,
        Err(_) => return,
    };
    let _ = canvas.copy(&tex, None, Rect::new(x, y, w, h));
}

/// All loaded textures for rendering.
pub struct Assets<'a> {
    unit_textures: HashMap<UnitTexKey, (Texture<'a>, u32, u32, u32)>,
    particle_textures: HashMap<ParticleKind, Texture<'a>>,
    building_textures: HashMap<(Faction, BuildingKind), (Texture<'a>, u32, u32)>,
    arrow_texture: Option<Texture<'a>>,
    // Terrain
    tilemap_texture: Option<Texture<'a>>,
    tilemap_texture2: Option<Texture<'a>>,
    water_texture: Option<Texture<'a>>,
    foam_texture: Option<Texture<'a>>,
    shadow_texture: Option<Texture<'a>>,
    // Decorations
    tree_textures: Vec<(Texture<'a>, u32, u32, u32)>, // tex, frame_w, frame_h, frame_count
    bush_textures: Vec<(Texture<'a>, u32, u32, u32)>,
    rock_textures: Vec<Texture<'a>>,
    water_rock_textures: Vec<(Texture<'a>, u32, u32, u32)>,
    // Buildings
    tower_textures: Vec<Texture<'a>>, // 0=Black/neutral, 1=Blue, 2=Red
    // UI 9-slice panels (pre-processed gapless atlas: texture, width, height)
    ui_special_paper: Option<(Texture<'a>, u32, u32)>,
    ui_blue_btn: Option<(Texture<'a>, u32, u32)>,
    ui_red_btn: Option<(Texture<'a>, u32, u32)>,
    // Bars (3-part horizontal, pre-processed gapless atlas: texture, width, height)
    ui_bar_base: Option<(Texture<'a>, u32, u32)>,
    // Bar fill (greyscale-converted so set_color_mod can tint to any color)
    ui_bar_fill: Option<Texture<'a>>,
    // Ribbons
    ui_big_ribbons: Option<Texture<'a>>,
    _ui_small_ribbons: Option<Texture<'a>>,
    // Swords
    _ui_swords: Option<Texture<'a>>,
    // Wood table frame for minimap
    ui_wood_table: Option<(Texture<'a>, u32, u32)>,
    // Fog (streaming texture, created lazily)
    pub fog_texture: Option<Texture<'a>>,
    // Text rendering
    pub text: TextRenderer,
}

/// Load a PNG file into an SDL2 texture using the `png` crate.
fn load_png_texture<'a>(tc: &'a TextureCreator<WindowContext>, path: &str) -> Option<Texture<'a>> {
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
///
/// The original sprite may have transparent gaps between cells. This function
/// extracts each cell defined in `cells` and packs them contiguously so that
/// the standard `NineSlice::compute()` algorithm works seamlessly.
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
/// Each cell is [sx, sy, sw, sh]. The atlas is left+center+right packed tightly.
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
                // Convert to greyscale RGBA: preserve luminance shading, keep alpha.
                // set_color_mod multiplies RGB, so greyscale base → tinted result.
                let mut grey = vec![0u8; (w * h * 4) as usize];
                for i in 0..(w * h) as usize {
                    let si = i * bpp;
                    let lum =
                        ((buf[si] as u16 * 77 + buf[si + 1] as u16 * 150 + buf[si + 2] as u16 * 29)
                            >> 8) as u8;
                    // Boost luminance toward white so color_mod gives vivid colors
                    // (raw lum would be dark; map 0..max → 128..255 for brightness)
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
            _ui_small_ribbons: ui_small_ribbons,
            _ui_swords: ui_swords,
            ui_wood_table,
            fog_texture: {
                use battlefield_core::grid::GRID_SIZE;
                // Linear filtering for fog only (smooth bilinear interpolation)
                sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "1");
                let tex = tc
                    .create_texture_streaming(Some(PixelFormatEnum::ABGR8888), GRID_SIZE, GRID_SIZE)
                    .ok()
                    .map(|mut t| {
                        t.set_blend_mode(BlendMode::Blend);
                        t
                    });
                // Restore nearest-neighbor for all other textures
                sdl2::hint::set("SDL_RENDER_SCALE_QUALITY", "0");
                tex
            },
            text: TextRenderer::new(),
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Coordinate helpers
// ───────────────────────────────────────────────────────────────────────────

/// Convert world coordinates to screen pixel coordinates.
/// The camera offset is snapped to integer pixels so all elements in the same
/// frame use a consistent sub-pixel shift — preventing jitter between layers.
fn world_to_screen(wx: f32, wy: f32, cam: &Camera) -> (i32, i32) {
    // Snap camera offset to integer (computed once per frame in practice,
    // but recalculated here for simplicity — the rounding is deterministic)
    let offset_x = (cam.viewport_w * 0.5 - cam.x * cam.zoom).round();
    let offset_y = (cam.viewport_h * 0.5 - cam.y * cam.zoom).round();
    let sx = (wx * cam.zoom + offset_x) as i32;
    let sy = (wy * cam.zoom + offset_y) as i32;
    (sx, sy)
}

/// Convert f64 tilemap source rect to SDL Rect.
fn src_rect(sx: f64, sy: f64, sw: f64, sh: f64) -> Rect {
    Rect::new(sx as i32, sy as i32, sw as u32, sh as u32)
}

/// Draw a filled circle using horizontal scanlines (midpoint circle algorithm).
fn fill_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
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
fn stroke_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
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

// ───────────────────────────────────────────────────────────────────────────
// 9-slice / 3-slice sprite helpers
// ───────────────────────────────────────────────────────────────────────────

/// Draw a 9-slice panel from a pre-processed gapless atlas texture.
///
/// Uses `NineSlice::compute()` to split the atlas into 9 source-to-dest draw
/// commands. Corners keep their source pixel size; edges and center stretch.
fn draw_panel(
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
/// `cap_w`: width of left/right caps in source pixels.
/// `img_w`, `img_h`: atlas dimensions.
fn draw_bar_3slice(
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

    // Source boundaries in atlas (gapless: left | center | right)
    let sl = cap_w; // source left cap width
    let sr = cap_w; // source right cap width
    let sc = img_w - sl - sr; // source center width

    // Dest boundaries snapped to integer pixels
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
/// `color_row` selects the ribbon color (Blue=0, Red=1, Yellow=2, Purple=3, Black=4).
/// Uses exact pixel boundaries from render_util constants.
fn draw_ribbon(
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

    // Left end (floor pos, ceil size to prevent gaps)
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

// ───────────────────────────────────────────────────────────────────────────
// Y-sorted drawable enum
// ───────────────────────────────────────────────────────────────────────────

enum Drawable {
    Unit(usize),
    Tree(u32, u32),
    WaterRock(u32, u32),
    Tower(u8),
    BaseBuilding(usize),
    Particle(usize),
}

// ───────────────────────────────────────────────────────────────────────────
// Main render entry point
// ───────────────────────────────────────────────────────────────────────────

pub fn render_frame(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    game: &Game,
    assets: &mut Assets,
    screen: GameScreen,
    elapsed: f64,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
) -> Vec<ClickableButton> {
    let ts = TILE_SIZE * game.camera.zoom;
    let cam = &game.camera;
    let (min_gx, min_gy, max_gx, max_gy) =
        render_util::visible_tile_range(cam, game.grid.width, game.grid.height);

    // 1. Clear
    canvas.set_draw_color(Color::RGB(26, 26, 38));
    canvas.clear();

    // 2. Water background (under everything)
    draw_water(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );
    //
    // 3. Foam animation
    draw_foam(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 4. Terrain (autotiled ground, roads, elevation)
    draw_terrain(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 5. Zone overlays (in world space)
    draw_zones(canvas, game, cam, ts);

    // 6. Bushes (ground level, behind units)
    draw_bushes(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 7. Rocks (ground level, behind units)
    draw_rocks(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 8. Player aim cone overlay
    draw_player_overlay(canvas, game, cam);

    // 9. Y-sorted foreground (units, trees, water rocks, towers, buildings, particles)
    draw_foreground(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy, elapsed,
    );

    // 10. Projectiles (fly above everything)
    draw_projectiles(canvas, game, assets, cam);

    // 11. HP bars and order labels
    draw_hp_bars(canvas, game, cam);
    draw_order_labels(canvas, tc, assets, game, cam);

    // 12. Fog of war
    draw_fog(
        canvas, game, assets, cam, ts, min_gx, min_gy, max_gx, max_gy,
    );

    // 13. Screen-space HUD
    draw_hud(canvas, game, assets);

    // 14. Victory progress bar
    draw_victory_progress(canvas, tc, assets, game);

    // 15. Minimap
    draw_minimap(canvas, game, assets);

    // 16. Screen overlays (menu, death, result)
    let buttons = draw_screen_overlay(
        canvas,
        tc,
        assets,
        screen,
        mouse_x,
        mouse_y,
        focused_button,
        gamepad_connected,
    );

    canvas.present();

    buttons
}

// ───────────────────────────────────────────────────────────────────────────
// Water background
// ───────────────────────────────────────────────────────────────────────────

fn draw_water(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let tsi = ts.ceil() as u32;
    let water_tex = match assets.water_texture.as_ref() {
        Some(t) => t,
        None => {
            // Fallback: flat blue for water tiles
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if !game.grid.get(gx, gy).is_land() {
                        let wx = gx as f32 * TILE_SIZE;
                        let wy = gy as f32 * TILE_SIZE;
                        let (sx, sy) = world_to_screen(wx, wy, cam);
                        canvas.set_draw_color(Color::RGB(48, 96, 160));
                        let _ = canvas.fill_rect(Rect::new(sx, sy, tsi, tsi));
                    }
                }
            }
            return;
        }
    };

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let is_water = !game.grid.get(gx, gy).is_land();
            let has_foam = game
                .water_adjacency
                .get((gy * game.grid.width + gx) as usize)
                .copied()
                .unwrap_or(false);
            if !is_water && !has_foam {
                continue;
            }
            let wx = gx as f32 * TILE_SIZE;
            let wy = gy as f32 * TILE_SIZE;
            let (sx, sy) = world_to_screen(wx, wy, cam);
            let src = Rect::new(0, 0, 64, 64);
            let dst = Rect::new(sx, sy, tsi, tsi);
            let _ = canvas.copy(water_tex, src, dst);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Terrain (autotiled ground, roads, elevation)
// ───────────────────────────────────────────────────────────────────────────

fn draw_terrain(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let tsi = ts.ceil() as u32;
    let w = game.grid.width;
    let h = game.grid.height;

    // Road sand fill (extends 1 tile into neighbors)
    canvas.set_draw_color(Color::RGB(196, 162, 101));
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Road {
                continue;
            }
            let wx = gx as f32 * TILE_SIZE;
            let wy = gy as f32 * TILE_SIZE;
            let (sx, sy) = world_to_screen(wx, wy, cam);
            let _ = canvas.fill_rect(Rect::new(sx, sy, tsi, tsi));

            // Extend into non-road neighbors
            if gx > 0 && game.grid.get(gx - 1, gy) != TileKind::Road {
                let (nx, ny) = world_to_screen((gx - 1) as f32 * TILE_SIZE, wy, cam);
                let _ = canvas.fill_rect(Rect::new(nx, ny, tsi, tsi));
            }
            if gx + 1 < w && game.grid.get(gx + 1, gy) != TileKind::Road {
                let (nx, ny) = world_to_screen((gx + 1) as f32 * TILE_SIZE, wy, cam);
                let _ = canvas.fill_rect(Rect::new(nx, ny, tsi, tsi));
            }
            if gy > 0 && game.grid.get(gx, gy - 1) != TileKind::Road {
                let (nx, ny) = world_to_screen(wx, (gy - 1) as f32 * TILE_SIZE, cam);
                let _ = canvas.fill_rect(Rect::new(nx, ny, tsi, tsi));
            }
            if gy + 1 < h && game.grid.get(gx, gy + 1) != TileKind::Road {
                let (nx, ny) = world_to_screen(wx, (gy + 1) as f32 * TILE_SIZE, cam);
                let _ = canvas.fill_rect(Rect::new(nx, ny, tsi, tsi));
            }
        }
    }

    // Flat ground (autotiled)
    if let Some(ref tilemap_tex) = assets.tilemap_texture {
        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                let tile = game.grid.get(gx, gy);
                if !tile.is_land() || tile == TileKind::Road {
                    continue;
                }
                let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
                let wx = gx as f32 * TILE_SIZE;
                let wy = gy as f32 * TILE_SIZE;
                let (sx, sy) = world_to_screen(wx, wy, cam);
                let src = src_rect(tsx, tsy, tsw, tsh);
                let dst = Rect::new(sx, sy, tsi, tsi);

                let flip_h = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
                let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);
            }
        }
    }

    // Road surface: draw autotiled grass texture then tint with sand overlay
    if let Some(ref tilemap_tex) = assets.tilemap_texture {
        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                if game.grid.get(gx, gy) != TileKind::Road {
                    continue;
                }
                let (col, row) = autotile::flat_ground_src(&game.grid, gx, gy);
                let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
                let wx = gx as f32 * TILE_SIZE;
                let wy = gy as f32 * TILE_SIZE;
                let (sx, sy) = world_to_screen(wx, wy, cam);
                let src = src_rect(tsx, tsy, tsw, tsh);
                let dst = Rect::new(sx, sy, tsi, tsi);

                let flip_h = col == 1 && row == 1 && render_util::tile_flip(gx, gy);
                let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);

                // Sand tint overlay
                canvas.set_draw_color(Color::RGBA(212, 176, 112, 140));
                let _ = canvas.fill_rect(dst);
            }
        }
    }

    // Elevation — extend scan 1 tile above visible range because cliffs/shadows
    // at gy+1 from an elevated tile at gy would be culled otherwise
    let elev_min_gy = min_gy.saturating_sub(1);
    for level in 2..=2u8 {
        // Shadow below elevated edges
        if let Some(ref mut shadow_tex) = assets.shadow_texture {
            shadow_tex.set_alpha_mod(128);
            for gy in elev_min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if game.grid.elevation(gx, gy) < level {
                        continue;
                    }
                    if gy + 1 < h && game.grid.elevation(gx, gy + 1) < level {
                        let shadow_world = 192.0_f32;
                        let shadow_draw = shadow_world * cam.zoom;
                        let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                        let center_wy = (gy + 1) as f32 * TILE_SIZE + TILE_SIZE * 0.5;
                        let (scx, scy) = world_to_screen(center_wx, center_wy, cam);
                        let half = (shadow_draw * 0.5) as i32;
                        let dst = Rect::new(
                            scx - half,
                            scy - half,
                            shadow_draw as u32,
                            shadow_draw as u32,
                        );
                        let src = Rect::new(0, 0, 192, 192);
                        let _ = canvas.copy(shadow_tex, src, dst);
                    }
                }
            }
            shadow_tex.set_alpha_mod(255);
        }

        // Elevated surface
        let elev_tex = if level == 2 {
            assets.tilemap_texture2.as_ref()
        } else {
            assets.tilemap_texture.as_ref()
        };
        if let Some(tilemap_tex) = elev_tex {
            for gy in elev_min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if game.grid.elevation(gx, gy) < level {
                        continue;
                    }
                    let (col, row) = autotile::elevated_top_src(&game.grid, gx, gy, level);
                    let (tsx, tsy, tsw, tsh) = grid::tilemap_src_rect(col, row);
                    let wx = gx as f32 * TILE_SIZE;
                    let wy = gy as f32 * TILE_SIZE;
                    let (sx, sy) = world_to_screen(wx, wy, cam);
                    let src = src_rect(tsx, tsy, tsw, tsh);
                    let dst = Rect::new(sx, sy, tsi, tsi);

                    let flip_h = col == 6 && row == 1 && render_util::tile_flip(gx, gy);
                    let _ = canvas.copy_ex(tilemap_tex, src, dst, 0.0, None, flip_h, false);

                    // Cliff face
                    if let Some((ccol, crow)) = autotile::cliff_src(&game.grid, gx, gy, level) {
                        let (csx, csy, csw, csh) = grid::tilemap_src_rect(ccol, crow);
                        let cliff_wy = (gy + 1) as f32 * TILE_SIZE;
                        let (_, cliff_sy) = world_to_screen(wx, cliff_wy, cam);
                        let cliff_src = src_rect(csx, csy, csw, csh);
                        let cliff_dst = Rect::new(sx, cliff_sy, tsi, tsi);
                        let cliff_flip = render_util::tile_flip(gx, gy.wrapping_add(1000));
                        let _ = canvas.copy_ex(
                            tilemap_tex,
                            cliff_src,
                            cliff_dst,
                            0.0,
                            None,
                            cliff_flip,
                            false,
                        );
                    }
                }
            }
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Foam animation
// ───────────────────────────────────────────────────────────────────────────

fn draw_foam(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let foam_tex = match assets.foam_texture.as_ref() {
        Some(t) => t,
        None => return,
    };
    let foam_sprite_size = 192.0_f32;
    let foam_draw = foam_sprite_size * cam.zoom;

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            let idx = (gy * game.grid.width + gx) as usize;
            if !game.water_adjacency.get(idx).copied().unwrap_or(false) {
                continue;
            }
            let frame = match render_util::foam_frame(elapsed, gx, gy) {
                Some(f) => f,
                None => continue, // wind calm — skip foam
            };
            let foam_sx = frame as i32 * foam_sprite_size as i32;
            let src = Rect::new(foam_sx, 0, foam_sprite_size as u32, foam_sprite_size as u32);

            let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let center_wy = gy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let (scx, scy) = world_to_screen(center_wx, center_wy, cam);
            let half = (foam_draw * 0.5) as i32;
            let dst = Rect::new(scx - half, scy - half, foam_draw as u32, foam_draw as u32);
            let _ = canvas.copy(foam_tex, src, dst);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Bushes (ground level)
// ───────────────────────────────────────────────────────────────────────────

fn draw_bushes(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    _ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    if assets.bush_textures.is_empty() {
        return;
    }

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.decoration(gx, gy) != Some(Decoration::Bush) {
                continue;
            }
            let variant_idx =
                render_util::variant_index(gx, gy, assets.bush_textures.len(), 41, 23);
            let (ref tex, fw, fh, frame_count) = assets.bush_textures[variant_idx];

            let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15);
            let sx = frame * fw;

            // Draw at native sprite size × zoom (centered on tile)
            let draw_w = (fw as f32 * cam.zoom) as u32;
            let draw_h = (fh as f32 * cam.zoom) as u32;
            let center_wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let center_wy = gy as f32 * TILE_SIZE + TILE_SIZE * 0.5;
            let (scx, scy) = world_to_screen(center_wx, center_wy, cam);

            let src = Rect::new(sx as i32, 0, fw, fh);
            let dst = Rect::new(
                scx - draw_w as i32 / 2,
                scy - draw_h as i32 / 2,
                draw_w,
                draw_h,
            );
            let flip_h = render_util::tile_flip(gx, gy);
            let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Rocks (ground level)
// ───────────────────────────────────────────────────────────────────────────

fn draw_rocks(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    if assets.rock_textures.is_empty() {
        return;
    }
    let tsi = ts.ceil() as u32;

    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) != TileKind::Rock {
                continue;
            }
            let variant_idx =
                render_util::variant_index(gx, gy, assets.rock_textures.len(), 13, 29);
            let tex = &assets.rock_textures[variant_idx];

            let wx = gx as f32 * TILE_SIZE;
            let wy = gy as f32 * TILE_SIZE;
            let (screen_x, screen_y) = world_to_screen(wx, wy, cam);

            let src = Rect::new(0, 0, 64, 64);
            let dst = Rect::new(screen_x, screen_y, tsi, tsi);
            let flip_h = render_util::tile_flip(gx, gy);
            let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Zone overlays (world space)
// ───────────────────────────────────────────────────────────────────────────

fn draw_zones(canvas: &mut Canvas<Window>, game: &Game, cam: &Camera, ts: f32) {
    canvas.set_blend_mode(BlendMode::Blend);
    for zone in &game.zone_manager.zones {
        let (sx, sy) = world_to_screen(zone.center_wx, zone.center_wy, cam);
        let radius = (zone.radius as f32 * ts) as i32;

        let (fr, fg, fb, fa) = render_util::zone_fill_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(fr, fg, fb, fa));
        fill_circle(canvas, sx, sy, radius);

        let (br, bg, bb, ba) = render_util::zone_border_rgba(zone.state);
        canvas.set_draw_color(Color::RGBA(br, bg, bb, ba));
        stroke_circle(canvas, sx, sy, radius);

        // Progress bar above zone
        let bar_w = radius;
        let bar_h = 4_i32;
        let bar_x = sx - bar_w / 2;
        let bar_y = sy - radius - 8;
        canvas.set_draw_color(Color::RGBA(0, 0, 0, 100));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        let progress = zone.progress as f64;
        if progress > 0.01 {
            canvas.set_draw_color(Color::RGBA(60, 120, 255, 200));
            let fill_w = ((bar_w as f64) * 0.5 * progress) as u32;
            let _ = canvas.fill_rect(Rect::new(bar_x + bar_w / 2, bar_y, fill_w, bar_h as u32));
        } else if progress < -0.01 {
            canvas.set_draw_color(Color::RGBA(255, 60, 60, 200));
            let fill_w = ((bar_w as f64) * 0.5 * (-progress)) as u32;
            let _ = canvas.fill_rect(Rect::new(
                bar_x + bar_w / 2 - fill_w as i32,
                bar_y,
                fill_w,
                bar_h as u32,
            ));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Player overlay (aim cone, highlight circle)
// ───────────────────────────────────────────────────────────────────────────

fn draw_player_overlay(canvas: &mut Canvas<Window>, game: &Game, cam: &Camera) {
    let player = match game.player_unit() {
        Some(p) => p,
        None => return,
    };

    let (px, py) = world_to_screen(player.x, player.y, cam);

    // Yellow circle under player
    let radius = (24.0 * cam.zoom) as i32;
    canvas.set_draw_color(Color::RGBA(255, 255, 51, 50));
    draw_filled_circle(canvas, px, py, radius);

    // Aim direction wedge (approximated as a triangle)
    let aim = game.player_aim_dir;
    let half = ATTACK_CONE_HALF_ANGLE;
    let wedge_radius = 40.0 * cam.zoom;

    let x0 = px as f32;
    let y0 = py as f32;
    let x1 = x0 + wedge_radius * (aim - half).cos();
    let y1 = y0 + wedge_radius * (aim - half).sin();
    let x2 = x0 + wedge_radius * (aim + half).cos();
    let y2 = y0 + wedge_radius * (aim + half).sin();

    // Draw filled triangle approximation for aim cone
    canvas.set_draw_color(Color::RGBA(255, 255, 100, 30));
    draw_filled_triangle(
        canvas, x0 as i32, y0 as i32, x1 as i32, y1 as i32, x2 as i32, y2 as i32,
    );

    // Border lines
    canvas.set_draw_color(Color::RGBA(255, 255, 100, 90));
    let _ = canvas.draw_line((px, py), (x1 as i32, y1 as i32));
    let _ = canvas.draw_line((px, py), (x2 as i32, y2 as i32));
    let _ = canvas.draw_line((x1 as i32, y1 as i32), (x2 as i32, y2 as i32));
}

/// Draw a filled circle using horizontal line segments.
fn draw_filled_circle(canvas: &mut Canvas<Window>, cx: i32, cy: i32, radius: i32) {
    for dy in -radius..=radius {
        let dx = ((radius * radius - dy * dy) as f32).sqrt() as i32;
        let _ = canvas.draw_line((cx - dx, cy + dy), (cx + dx, cy + dy));
    }
}

/// Draw a filled triangle using scanline.
fn draw_filled_triangle(
    canvas: &mut Canvas<Window>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
) {
    let mut verts = [(x0, y0), (x1, y1), (x2, y2)];
    verts.sort_by_key(|v| v.1);
    let (ax, ay) = verts[0];
    let (bx, by) = verts[1];
    let (cx, cy) = verts[2];

    if ay == cy {
        return;
    }

    for y in ay..=cy {
        let min_x;
        let max_x;

        // Interpolate x along edges
        let x_ac = ax + (cx - ax) * (y - ay) / (cy - ay);

        if y <= by {
            if by == ay {
                min_x = ax.min(bx);
                max_x = ax.max(bx);
            } else {
                let x_ab = ax + (bx - ax) * (y - ay) / (by - ay);
                min_x = x_ac.min(x_ab);
                max_x = x_ac.max(x_ab);
            }
        } else if cy == by {
            min_x = bx.min(cx);
            max_x = bx.max(cx);
        } else {
            let x_bc = bx + (cx - bx) * (y - by) / (cy - by);
            min_x = x_ac.min(x_bc);
            max_x = x_ac.max(x_bc);
        }

        let _ = canvas.draw_line((min_x, y), (max_x, y));
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Y-sorted foreground
// ───────────────────────────────────────────────────────────────────────────

fn draw_foreground(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    elapsed: f64,
) {
    let ts_f64 = TILE_SIZE as f64;
    let player_pos = game.player_unit().map(|u| (u.x as f64, u.y as f64));

    let mut drawables: Vec<(f64, Drawable)> = Vec::new();

    // Units
    for (i, u) in game.units.iter().enumerate() {
        if !u.alive && u.death_fade <= 0.0 {
            continue;
        }
        let (gx, gy) = u.grid_cell();
        if !render_util::is_visible_to_player(u.faction, gx, gy, &game.visible, game.grid.width) {
            continue;
        }
        drawables.push((u.y as f64 + ts_f64 * 0.5, Drawable::Unit(i)));
    }

    // Trees and water rocks — trees are up to 4 tiles tall (bottom-anchored),
    // so a tree rooted below the viewport can have its canopy visible.
    // Extend scan range downward (max_gy) to catch those roots.
    let tree_max_gy = (max_gy + 4).min(game.grid.height);
    for gy in min_gy..tree_max_gy {
        for gx in min_gx..max_gx {
            let tile = game.grid.get(gx, gy);
            let foot_y = ((gy + 1) as f64) * ts_f64;
            if tile == TileKind::Forest && !assets.tree_textures.is_empty() {
                drawables.push((foot_y, Drawable::Tree(gx, gy)));
            }
            if game.grid.decoration(gx, gy) == Some(Decoration::WaterRock)
                && !assets.water_rock_textures.is_empty()
            {
                drawables.push((foot_y, Drawable::WaterRock(gx, gy)));
            }
        }
    }

    // Tower buildings at zone centers
    for (i, zone) in game.zone_manager.zones.iter().enumerate() {
        let foot_y = (zone.center_gy as f64 + 1.0) * ts_f64;
        drawables.push((foot_y, Drawable::Tower(i as u8)));
    }

    // Production buildings
    for (i, b) in game.buildings.iter().enumerate() {
        let foot_y = (b.grid_y as f64 + 1.0) * ts_f64;
        drawables.push((foot_y, Drawable::BaseBuilding(i)));
    }

    // Particles
    for (i, p) in game.particles.iter().enumerate() {
        if !p.finished {
            drawables.push((p.world_y as f64 + ts_f64 * 0.5, Drawable::Particle(i)));
        }
    }

    drawables.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    for (_, drawable) in &drawables {
        match drawable {
            Drawable::Unit(idx) => {
                draw_unit(canvas, game, assets, cam, ts, *idx, elapsed);
            }
            Drawable::Tree(gx, gy) => {
                draw_tree(canvas, assets, cam, ts, *gx, *gy, elapsed, player_pos);
            }
            Drawable::WaterRock(gx, gy) => {
                draw_water_rock(canvas, assets, cam, ts, *gx, *gy, elapsed);
            }
            Drawable::Tower(zone_idx) => {
                draw_tower(canvas, game, assets, cam, ts, *zone_idx);
            }
            Drawable::BaseBuilding(idx) => {
                draw_base_building(canvas, game, assets, cam, ts, *idx);
            }
            Drawable::Particle(idx) => {
                draw_particle(canvas, game, assets, cam, ts, *idx);
            }
        }
    }
}

fn draw_unit(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
    elapsed: f64,
) {
    let unit = &game.units[idx];
    let key = UnitTexKey {
        faction: unit.faction,
        kind: unit.kind,
        anim: unit.current_anim,
    };

    if let Some((tex, fw, _fh, _frames)) = assets.unit_textures.get_mut(&key) {
        let fw_val = *fw;
        let frame_count = unit.animation.frame_count;
        let sheet = SpriteSheet {
            frame_width: fw_val,
            frame_height: fw_val,
            frame_count,
        };

        // Archer idle uses wind wave pattern
        let anim_frame = if unit.kind == UnitKind::Archer && unit.current_anim == UnitAnim::Idle {
            let (gx, gy) = unit.grid_cell();
            render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15)
        } else {
            unit.animation.current_frame
        };

        let (sx, sy, sw, sh) = sheet.frame_src_rect(anim_frame);
        let draw_size = ts * (fw_val as f32 / TILE_SIZE);
        let (screen_x, screen_y) = world_to_screen(unit.x, unit.y, cam);
        let half = (draw_size / 2.0) as i32;

        let dst = Rect::new(
            screen_x - half,
            screen_y - half,
            draw_size as u32,
            draw_size as u32,
        );
        let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);

        let opacity = render_util::unit_opacity(unit.alive, unit.death_fade, unit.hit_flash);
        let alpha = (opacity * 255.0) as u8;
        tex.set_alpha_mod(alpha);

        let flip = unit.facing == Facing::Left;
        let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip, false);

        tex.set_alpha_mod(255);
    } else {
        // Fallback: colored square
        let (screen_x, screen_y) = world_to_screen(unit.x, unit.y, cam);
        let color = match unit.faction {
            Faction::Blue => Color::RGB(60, 120, 255),
            Faction::Red => Color::RGB(255, 60, 60),
        };
        canvas.set_draw_color(color);
        let size = (ts * 0.6) as u32;
        let half = size as i32 / 2;
        let _ = canvas.fill_rect(Rect::new(screen_x - half, screen_y - half, size, size));
    }
}

fn draw_tree(
    canvas: &mut Canvas<Window>,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    gx: u32,
    gy: u32,
    elapsed: f64,
    player_pos: Option<(f64, f64)>,
) {
    let variant_idx = render_util::variant_index(gx, gy, assets.tree_textures.len(), 31, 17);
    let (ref mut tex, fw, fh, frame_count) = assets.tree_textures[variant_idx];
    let ts_f64 = TILE_SIZE as f64;

    let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.15);
    let sx = frame * fw;

    let draw_w = ts * 3.0;
    let draw_h = draw_w * (fh as f32 / fw as f32);
    let wx = gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    let wy = gy as f32 * TILE_SIZE + TILE_SIZE;
    let (screen_cx, screen_by) = world_to_screen(wx, wy, cam);
    let dst_x = screen_cx - (draw_w * 0.5) as i32;
    let dst_y = screen_by - draw_h as i32;
    let dst = Rect::new(dst_x, dst_y, draw_w as u32, draw_h as u32);
    let src = Rect::new(sx as i32, 0, fw, fh);

    // Fade trees near player — use visual center (canopy), not root tile
    let tree_cx = gx as f64 * ts_f64 + ts_f64 * 0.5;
    let tree_cy = gy as f64 * ts_f64 - ts_f64 * 1.0; // canopy center ~2 tiles above root
    let alpha_f = render_util::tree_alpha(tree_cx, tree_cy, player_pos, ts_f64);
    let alpha = (alpha_f * 255.0) as u8;
    tex.set_alpha_mod(alpha);

    let flip_h = render_util::tile_flip(gx, gy);
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);

    tex.set_alpha_mod(255);
}

fn draw_water_rock(
    canvas: &mut Canvas<Window>,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    gx: u32,
    gy: u32,
    elapsed: f64,
) {
    let variant_idx = render_util::variant_index(gx, gy, assets.water_rock_textures.len(), 37, 19);
    let (ref tex, fw, fh, frame_count) = assets.water_rock_textures[variant_idx];

    let frame = render_util::compute_wave_frame(elapsed, gx, gy, frame_count, 0.2);
    let sx = frame * fw;

    let tsi = ts.ceil() as u32;
    let wx = gx as f32 * TILE_SIZE;
    let wy = gy as f32 * TILE_SIZE;
    let (screen_x, screen_y) = world_to_screen(wx, wy, cam);

    let src = Rect::new(sx as i32, 0, fw, fh);
    let dst = Rect::new(screen_x, screen_y, tsi, tsi);
    let flip_h = render_util::tile_flip(gx, gy);
    let _ = canvas.copy_ex(tex, src, dst, 0.0, None, flip_h, false);
}

fn draw_tower(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    zone_idx: u8,
) {
    if assets.tower_textures.is_empty() {
        return;
    }
    let zone = &game.zone_manager.zones[zone_idx as usize];

    let color_idx = match zone.state {
        ZoneState::Controlled(Faction::Blue) | ZoneState::Capturing(Faction::Blue) => 1,
        ZoneState::Controlled(Faction::Red) | ZoneState::Capturing(Faction::Red) => 2,
        _ => 0,
    };
    if color_idx >= assets.tower_textures.len() {
        return;
    }

    let tex = &mut assets.tower_textures[color_idx];
    let draw_w = ts * 2.0;
    let draw_h = ts * 4.0;
    let wx = zone.center_gx as f32 * TILE_SIZE + TILE_SIZE * 0.5;
    let wy = zone.center_gy as f32 * TILE_SIZE + TILE_SIZE;
    let (scx, sby) = world_to_screen(wx, wy, cam);
    let dst = Rect::new(
        scx - (draw_w * 0.5) as i32,
        sby - draw_h as i32,
        draw_w as u32,
        draw_h as u32,
    );

    // Pulse opacity during capture
    let alpha = match zone.state {
        ZoneState::Capturing(_) => {
            ((zone.progress.abs() as f64 * 0.5 + 0.5).clamp(0.5, 1.0) * 255.0) as u8
        }
        _ => 255,
    };
    tex.set_alpha_mod(alpha);

    let src = Rect::new(0, 0, 128, 256);
    let _ = canvas.copy(tex, src, dst);

    tex.set_alpha_mod(255);
}

fn draw_base_building(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
) {
    let building = &game.buildings[idx];
    let key = (building.faction, building.kind);
    if let Some((ref tex, sw, sh)) = assets.building_textures.get(&key) {
        let draw_w = ts * 3.0;
        let draw_h = draw_w * (*sh as f32 / *sw as f32);
        let wx = building.grid_x as f32 * TILE_SIZE + TILE_SIZE * 0.5;
        let wy = building.grid_y as f32 * TILE_SIZE + TILE_SIZE;
        let (scx, sby) = world_to_screen(wx, wy, cam);
        let dst = Rect::new(
            scx - (draw_w * 0.5) as i32,
            sby - draw_h as i32,
            draw_w as u32,
            draw_h as u32,
        );
        let _ = canvas.copy(tex, None, dst);
    }
}

fn draw_particle(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    idx: usize,
) {
    let p = &game.particles[idx];
    if p.finished {
        return;
    }
    if let Some(tex) = assets.particle_textures.get(&p.kind) {
        let fs = p.kind.frame_size();
        let sheet = SpriteSheet {
            frame_width: fs,
            frame_height: fs,
            frame_count: p.kind.frame_count(),
        };
        let (sx, sy, sw, sh) = sheet.frame_src_rect(p.animation.current_frame);
        let draw_size = ts * (fs as f32 / TILE_SIZE);
        let (screen_x, screen_y) = world_to_screen(p.world_x, p.world_y, cam);
        let half = (draw_size / 2.0) as i32;

        let dst = Rect::new(
            screen_x - half,
            screen_y - half,
            draw_size as u32,
            draw_size as u32,
        );
        let src = Rect::new(sx as i32, sy as i32, sw as u32, sh as u32);
        let _ = canvas.copy(tex, src, dst);
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Projectiles
// ───────────────────────────────────────────────────────────────────────────

fn draw_projectiles(canvas: &mut Canvas<Window>, game: &Game, assets: &mut Assets, cam: &Camera) {
    let zoom = cam.zoom;
    for proj in &game.projectiles {
        if proj.finished {
            continue;
        }
        let (sx, sy) = world_to_screen(proj.current_x, proj.current_y, cam);

        if let Some(ref tex) = assets.arrow_texture {
            // Arrow sprite is 64x64 — draw at native size × zoom
            let arrow_size = (64.0 * zoom) as u32;
            let half = arrow_size as i32 / 2;
            let dst = Rect::new(sx - half, sy - half, arrow_size, arrow_size);
            let angle_deg = proj.angle.to_degrees() as f64;
            let _ = canvas.copy_ex(tex, None, dst, angle_deg, None, false, false);
        } else {
            let w = (8.0 * zoom) as u32;
            let h = (4.0 * zoom) as u32;
            canvas.set_draw_color(Color::RGB(200, 180, 120));
            let _ = canvas.fill_rect(Rect::new(sx - w as i32 / 2, sy - h as i32 / 2, w, h));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// HP bars
// ───────────────────────────────────────────────────────────────────────────

fn draw_hp_bars(canvas: &mut Canvas<Window>, game: &Game, cam: &Camera) {
    canvas.set_blend_mode(BlendMode::Blend);
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = unit.grid_cell();
        if !render_util::is_visible_to_player(unit.faction, gx, gy, &game.visible, game.grid.width)
        {
            continue;
        }

        let zoom = game.camera.zoom;
        let (sx, sy) = world_to_screen(unit.x, unit.y, cam);
        let bar_w = (36.0 * zoom) as i32;
        let bar_h = (4.0 * zoom).max(2.0) as i32;
        let bar_y = sy - (TILE_SIZE * zoom * 0.7) as i32;
        let bar_x = sx - bar_w / 2;

        canvas.set_draw_color(Color::RGBA(40, 40, 40, 200));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, bar_w as u32, bar_h as u32));

        let ratio = unit.hp as f32 / unit.stats.max_hp as f32;
        let fill_w = (bar_w as f32 * ratio) as u32;
        let (hr, hg, hb) = render_util::hp_bar_color(ratio as f64);
        canvas.set_draw_color(Color::RGB(hr, hg, hb));
        let _ = canvas.fill_rect(Rect::new(bar_x, bar_y, fill_w, bar_h as u32));
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Order labels (colored rect placeholders since no TTF)
// ───────────────────────────────────────────────────────────────────────────

fn draw_order_labels(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    game: &Game,
    cam: &Camera,
) {
    canvas.set_blend_mode(BlendMode::Blend);
    for unit in &game.units {
        if !unit.alive || unit.order_flash <= 0.0 {
            continue;
        }
        let label = match render_util::order_label(unit.order.as_ref()) {
            Some(l) => l,
            None => continue,
        };

        let alpha = ((unit.order_flash / ORDER_FLASH_DURATION) * 255.0) as u8;
        let (sx, sy) = world_to_screen(unit.x, unit.y, cam);
        let label_y = sy - (TILE_SIZE * game.camera.zoom) as i32;

        let font_size = 14.0 * game.camera.zoom;
        assets.text.draw_text_centered(
            canvas,
            tc,
            label,
            sx,
            label_y - (6.0 * game.camera.zoom) as i32,
            font_size,
            Color::RGBA(255, 215, 0, alpha),
        );
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Fog of war
// ───────────────────────────────────────────────────────────────────────────

fn draw_fog(
    canvas: &mut Canvas<Window>,
    game: &Game,
    assets: &mut Assets,
    cam: &Camera,
    ts: f32,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) {
    let w = game.grid.width;

    // Rebuild fog pixel data with smooth per-tile alpha
    let pixels = render_util::build_fog_pixels(&game.visible, w, game.grid.height);

    if let Some(ref mut tex) = assets.fog_texture {
        // Upload pixel data to the streaming texture
        let pitch = (w * 4) as usize;
        let _ = tex.update(None, &pixels, pitch);

        // Draw the visible portion stretched over the world area.
        // Source rect: tile coords (1px = 1 tile). Dest rect: screen coords.
        // SDL bilinear-scales the 1px-per-tile texture up, producing smooth fog edges.
        let src_w = (max_gx - min_gx).max(1);
        let src_h = (max_gy - min_gy).max(1);
        let src = Rect::new(min_gx as i32, min_gy as i32, src_w, src_h);

        let (sx, sy) = world_to_screen(min_gx as f32 * TILE_SIZE, min_gy as f32 * TILE_SIZE, cam);
        let dst_w = (src_w as f32 * ts) as u32;
        let dst_h = (src_h as f32 * ts) as u32;
        let dst = Rect::new(sx, sy, dst_w, dst_h);

        let _ = canvas.copy(tex, src, dst);
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Screen-space HUD
// ───────────────────────────────────────────────────────────────────────────

fn draw_hud(canvas: &mut Canvas<Window>, game: &Game, assets: &mut Assets) {
    let (w, _h) = canvas.output_size().unwrap_or((960, 640));

    // Player HP bar at top-left
    if let Some(player) = game.player_unit() {
        let bar_x = 10.0_f64;
        let bar_y = 6.0_f64;
        let bar_w = 200.0_f64;
        let bar_h = 46.0_f64;

        canvas.set_blend_mode(BlendMode::Blend);

        // 1. Bar base frame first (opaque wooden bar)
        if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
            draw_bar_3slice(
                canvas, tex, bw as f64, bh as f64, bar_x, bar_y, bar_w, bar_h, 24.0,
            );
        }

        // 2. HP fill ON TOP of bar, inside the inner area between border ornaments
        let ratio = player.hp as f64 / player.stats.max_hp as f64;
        let fill_left = 10.0_f64;
        let fill_right = 10.0_f64;
        let fill_top = 12.0_f64;
        let fill_bottom = 12.0_f64;
        let inner_w = bar_w - fill_left - fill_right;
        let fill_w = (inner_w * ratio).max(0.0);
        let fill_h = (bar_h - fill_top - fill_bottom).max(1.0);
        if fill_w > 0.0 {
            let (hr, hg, hb) = render_util::hp_bar_color(ratio);
            if let Some(ref mut fill_tex) = assets.ui_bar_fill {
                fill_tex.set_color_mod(hr, hg, hb);
                let _ = canvas.copy(
                    fill_tex,
                    Rect::new(0, 20, 64, 24), // opaque strip rows 20-43
                    Rect::new(
                        (bar_x + fill_left) as i32,
                        (bar_y + fill_top) as i32,
                        fill_w as u32,
                        fill_h as u32,
                    ),
                );
                fill_tex.set_color_mod(255, 255, 255);
            } else {
                canvas.set_draw_color(Color::RGB(hr, hg, hb));
                let _ = canvas.fill_rect(Rect::new(
                    (bar_x + fill_left) as i32,
                    (bar_y + fill_top) as i32,
                    fill_w as u32,
                    fill_h as u32,
                ));
            }
        }

        // Fallback if no bar texture
        if assets.ui_bar_base.is_none() {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 120));
            let _ = canvas.draw_rect(Rect::new(
                bar_x as i32,
                bar_y as i32,
                bar_w as u32,
                bar_h as u32,
            ));
        }
    }

    // Zone control pips at top-center
    let zone_count = game.zone_manager.zones.len() as u32;
    if zone_count > 0 {
        let pip_size = 14_u32;
        let gap = 4_u32;
        let total_w = zone_count * pip_size + (zone_count - 1) * gap;
        let start_x = (w / 2 - total_w / 2) as i32;
        let pip_y = 10_i32;

        for (i, zone) in game.zone_manager.zones.iter().enumerate() {
            let px = start_x + (i as u32 * (pip_size + gap)) as i32;
            canvas.set_draw_color(Color::RGBA(40, 40, 40, 180));
            let _ = canvas.fill_rect(Rect::new(px, pip_y, pip_size, pip_size));

            let (r, g, b) = render_util::zone_pip_rgb(zone.state);
            let fill_h = (pip_size as f32 * zone.progress.abs()) as u32;
            if fill_h > 0 {
                canvas.set_draw_color(Color::RGBA(r, g, b, 200));
                let _ = canvas.fill_rect(Rect::new(
                    px,
                    pip_y + (pip_size - fill_h) as i32,
                    pip_size,
                    fill_h,
                ));
            }

            canvas.set_draw_color(Color::RGBA(255, 255, 255, 80));
            let _ = canvas.draw_rect(Rect::new(px, pip_y, pip_size, pip_size));
        }
    }
}

// ───────────────────────────────────────────────────────────────────────────
// Victory progress bar
// ───────────────────────────────────────────────────────────────────────────

fn draw_victory_progress(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &mut Assets,
    game: &Game,
) {
    let progress = game.zone_manager.victory_progress();
    if progress < f32::EPSILON || game.winner.is_some() {
        return;
    }
    let faction = match game.zone_manager.victory_candidate {
        Some(f) => f,
        None => return,
    };

    let (w, _h) = canvas.output_size().unwrap_or((960, 640));
    let cx = w as f64 / 2.0;
    let bar_w = 300.0_f64;
    let bar_h = 46.0_f64;
    let bar_x = cx - bar_w / 2.0;
    let bar_y = 46.0_f64;

    canvas.set_blend_mode(BlendMode::Blend);

    // 1. Bar base frame first (opaque wooden bar)
    if let Some((ref tex, bw, bh)) = assets.ui_bar_base {
        draw_bar_3slice(
            canvas, tex, bw as f64, bh as f64, bar_x, bar_y, bar_w, bar_h, 24.0,
        );
    }

    // 2. Fill ON TOP inside the inner area
    let fill_left = 22.0_f64;
    let fill_right = 22.0_f64;
    let fill_top = 9.0_f64;
    let fill_bottom = 4.0_f64;
    let inner_w = bar_w - fill_left - fill_right;
    let fill_w = (inner_w * progress as f64).max(0.0);
    let fill_h = (bar_h - fill_top - fill_bottom).max(1.0);
    if fill_w > 0.0 {
        let (fr, fg, fb) = match faction {
            Faction::Blue => (70u8, 130u8, 230u8),
            Faction::Red => (220, 60, 60),
        };
        if let Some(ref mut fill_tex) = assets.ui_bar_fill {
            fill_tex.set_color_mod(fr, fg, fb);
            let _ = canvas.copy(
                fill_tex,
                Rect::new(0, 20, 64, 24),
                Rect::new(
                    (bar_x + fill_left) as i32,
                    (bar_y + fill_top) as i32,
                    fill_w as u32,
                    fill_h as u32,
                ),
            );
            fill_tex.set_color_mod(255, 255, 255);
        } else {
            canvas.set_draw_color(Color::RGB(fr, fg, fb));
            let _ = canvas.fill_rect(Rect::new(
                (bar_x + fill_left) as i32,
                (bar_y + fill_top) as i32,
                fill_w as u32,
                fill_h as u32,
            ));
        }
    }

    // Fallback if no bar texture
    if assets.ui_bar_base.is_none() {
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 100));
        let _ = canvas.draw_rect(Rect::new(
            bar_x as i32,
            bar_y as i32,
            bar_w as u32,
            bar_h as u32,
        ));
    }

    let remaining = ((1.0 - progress) * VICTORY_HOLD_TIME) as u32;
    let faction_name = if faction == Faction::Blue {
        "Blue"
    } else {
        "Red"
    };
    let msg = format!(
        "{} holds all zones — Victory in {}s",
        faction_name, remaining
    );
    assets.text.draw_text_centered(
        canvas,
        tc,
        &msg,
        cx as i32,
        (bar_y - 16.0) as i32,
        14.0,
        Color::RGBA(255, 255, 255, 180),
    );
}

// ───────────────────────────────────────────────────────────────────────────
// Minimap
// ───────────────────────────────────────────────────────────────────────────

fn draw_minimap(canvas: &mut Canvas<Window>, game: &Game, assets: &Assets) {
    let (canvas_w, canvas_h) = canvas.output_size().unwrap_or((960, 640));
    let mm_size = 140_u32;
    let mm_margin = 8_i32;
    let frame_pad = 30_i32; // space for wood table frame around minimap
    let mm_x = canvas_w as i32 - mm_margin + 18 - frame_pad - mm_size as i32;
    let mm_y = canvas_h as i32 - mm_margin + 25 - frame_pad - mm_size as i32;

    let grid_w = game.grid.width;
    let grid_h = game.grid.height;
    let scale_x = mm_size as f32 / grid_w as f32;
    let scale_y = mm_size as f32 / grid_h as f32;

    // Wood table frame (behind minimap content)
    if let Some((ref tex, tw, th)) = assets.ui_wood_table {
        draw_panel(
            canvas,
            tex,
            &render_util::NINE_SLICE_WOOD_TABLE,
            tw as f64,
            th as f64,
            (mm_x + 10 - frame_pad) as f64,
            (mm_y + 8 - frame_pad) as f64,
            (mm_size as i32 + frame_pad * 2 - 20) as f64,
            (mm_size as i32 + frame_pad * 2) as f64,
        );
    }

    // Dark background for map content
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 180));
    let _ = canvas.fill_rect(Rect::new(mm_x, mm_y, mm_size, mm_size));

    // Terrain dots (sample every 2nd tile for performance)
    let step = 2_u32;
    let rect_w = (scale_x * step as f32).ceil() as u32;
    let rect_h = (scale_y * step as f32).ceil() as u32;
    let mut gy = 0_u32;
    while gy < grid_h {
        let mut gx = 0_u32;
        while gx < grid_w {
            let (r, g, b) = match game.grid.get(gx, gy) {
                TileKind::Water => (30, 60, 120),
                TileKind::Forest => (30, 80, 30),
                TileKind::Rock => (90, 85, 75),
                TileKind::Road => (160, 140, 100),
                TileKind::Grass => {
                    if game.grid.elevation(gx, gy) >= 2 {
                        (100, 95, 70)
                    } else if game.grid.decoration(gx, gy) == Some(Decoration::Bush) {
                        (55, 100, 45)
                    } else {
                        (70, 110, 50)
                    }
                }
            };
            let rx = mm_x + (gx as f32 * scale_x) as i32;
            let ry = mm_y + (gy as f32 * scale_y) as i32;
            canvas.set_draw_color(Color::RGB(r, g, b));
            let _ = canvas.fill_rect(Rect::new(rx, ry, rect_w.max(1), rect_h.max(1)));
            gx += step;
        }
        gy += step;
    }

    // Fog overlay on minimap
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 140));
    let mut gy = 0_u32;
    while gy < grid_h {
        let mut gx = 0_u32;
        while gx < grid_w {
            let idx = (gy * grid_w + gx) as usize;
            if idx < game.visible.len() && !game.visible[idx] {
                let rx = mm_x + (gx as f32 * scale_x) as i32;
                let ry = mm_y + (gy as f32 * scale_y) as i32;
                let _ = canvas.fill_rect(Rect::new(rx, ry, rect_w.max(1), rect_h.max(1)));
            }
            gx += step;
        }
        gy += step;
    }

    // Zone circles (colored rects on minimap)
    for zone in &game.zone_manager.zones {
        let zx = mm_x + (zone.center_gx as f32 * scale_x) as i32;
        let zy = mm_y + (zone.center_gy as f32 * scale_y) as i32;
        let zr = ((zone.radius as f32 * scale_x) as i32).max(2);

        let (r, g, b) = render_util::zone_pip_rgb(zone.state);
        canvas.set_draw_color(Color::RGBA(r, g, b, 200));
        fill_circle(canvas, zx, zy, zr);
    }

    // Unit dots
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (gx, gy) = grid::world_to_grid(unit.x, unit.y);
        // Hide enemies in fog
        if unit.faction != Faction::Blue {
            let idx = (gy as u32 * grid_w + gx as u32) as usize;
            if idx >= game.visible.len() || !game.visible[idx] {
                continue;
            }
        }
        let ux = mm_x + (gx as f32 * scale_x) as i32;
        let uy = mm_y + (gy as f32 * scale_y) as i32;
        let ur = 1_u32.max((scale_x * 0.8) as u32);

        let color = match unit.faction {
            Faction::Blue => Color::RGB(74, 158, 255),
            Faction::Red => Color::RGB(255, 74, 74),
        };
        canvas.set_draw_color(color);
        let _ = canvas.fill_rect(Rect::new(ux - ur as i32, uy - ur as i32, ur * 2, ur * 2));
    }

    // Camera viewport rectangle
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let world_size = grid_w as f32 * TILE_SIZE;
    let vx = mm_x as f32 + (vl / world_size) * mm_size as f32;
    let vy = mm_y as f32 + (vt / world_size) * mm_size as f32;
    let vw = ((vr - vl) / world_size) * mm_size as f32;
    let vh = ((vb - vt) / world_size) * mm_size as f32;

    canvas.set_draw_color(Color::RGBA(255, 255, 255, 200));
    let _ = canvas.draw_rect(Rect::new(
        (vx.max(mm_x as f32)) as i32,
        (vy.max(mm_y as f32)) as i32,
        (vw.min(mm_size as f32 - (vx - mm_x as f32).max(0.0))) as u32,
        (vh.min(mm_size as f32 - (vy - mm_y as f32).max(0.0))) as u32,
    ));
}

// ───────────────────────────────────────────────────────────────────────────
// Screen overlays (menu, death, result)
// ───────────────────────────────────────────────────────────────────────────

fn draw_screen_overlay(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    screen: GameScreen,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
) -> Vec<ClickableButton> {
    let (w, h) = canvas.output_size().unwrap_or((960, 640));
    canvas.set_blend_mode(BlendMode::Blend);

    let layout = match screen {
        GameScreen::Playing => return Vec::new(),
        GameScreen::MainMenu => battlefield_core::ui::main_menu_layout(),
        GameScreen::PlayerDeath => battlefield_core::ui::death_layout(),
        GameScreen::GameWon => battlefield_core::ui::result_layout(true),
        GameScreen::GameLost => battlefield_core::ui::result_layout(false),
    };

    draw_layout_overlay(
        canvas,
        tc,
        assets,
        w,
        h,
        &layout,
        mouse_x,
        mouse_y,
        focused_button,
        gamepad_connected,
    )
}

/// Render a `ScreenLayout` using SDL2 primitives and 9-slice sprites.
/// Returns clickable button regions for hit-testing.
fn draw_layout_overlay(
    canvas: &mut Canvas<Window>,
    tc: &TextureCreator<WindowContext>,
    assets: &Assets,
    w: u32,
    h: u32,
    layout: &battlefield_core::ui::ScreenLayout,
    mouse_x: i32,
    mouse_y: i32,
    focused_button: usize,
    gamepad_connected: bool,
) -> Vec<ClickableButton> {
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;

    // 1. Full-screen tinted overlay
    let (or, og, ob, oa) = layout.overlay;
    canvas.set_draw_color(Color::RGBA(or, og, ob, oa));
    let _ = canvas.fill_rect(Rect::new(0, 0, w, h));

    // 2. Panel background (9-slice)
    let panel_y = if let Some((pw, ph)) = layout.panel_size {
        let px = cx - pw / 2.0;
        let py = cy - ph / 2.0;
        if let Some((ref tex, aw, ah)) = assets.ui_special_paper {
            draw_panel(
                canvas,
                tex,
                &render_util::NINE_SLICE_SPECIAL_PAPER,
                aw as f64,
                ah as f64,
                px,
                py,
                pw,
                ph,
            );
        }
        py
    } else {
        cy
    };

    // 3. Ribbon behind title
    if let Some((color_row, ribbon_offset_y, ribbon_w, ribbon_h)) = layout.title_ribbon {
        let ribbon_x = cx - ribbon_w / 2.0;
        let ribbon_y = panel_y + ribbon_offset_y;
        if let Some(ref tex) = assets.ui_big_ribbons {
            draw_ribbon(
                canvas, tex, color_row, ribbon_x, ribbon_y, ribbon_w, ribbon_h, ribbon_h,
            );
        }
    }

    // 4. Title text
    if let Some(ref title) = layout.title {
        let tx = (cx + title.offset_x) as i32;
        let ty = (cy + title.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &title.text,
            tx,
            ty,
            title.size as f32,
            Color::RGBA(title.r, title.g, title.b, title.a),
        );
    }

    // 5. Subtitle text
    if let Some(ref sub) = layout.subtitle {
        let sx = (cx + sub.offset_x) as i32;
        let sy = (cy + sub.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &sub.text,
            sx,
            sy,
            sub.size as f32,
            Color::RGBA(sub.r, sub.g, sub.b, sub.a),
        );
    }

    // 6. Buttons
    let mut clickable_buttons = Vec::new();
    for (i, btn) in layout.buttons.iter().enumerate() {
        let bx = cx + btn.offset_x;
        let by = cy + btn.offset_y;
        let btn_x = bx - btn.w / 2.0;
        let btn_y = by - btn.h / 2.0;

        let is_focused = gamepad_connected && i == focused_button;
        let mouse_hovering = mouse_x as f64 >= btn_x
            && mouse_x as f64 <= btn_x + btn.w
            && mouse_y as f64 >= btn_y
            && mouse_y as f64 <= btn_y + btn.h;
        let hovering = mouse_hovering || is_focused;

        let btn_atlas = match btn.style {
            battlefield_core::ui::ButtonStyle::Blue => assets.ui_blue_btn.as_ref(),
            battlefield_core::ui::ButtonStyle::Red => assets.ui_red_btn.as_ref(),
        };

        if let Some((tex, aw, ah)) = btn_atlas {
            draw_panel(
                canvas,
                tex,
                &render_util::NINE_SLICE_BUTTON,
                *aw as f64,
                *ah as f64,
                btn_x,
                btn_y,
                btn.w,
                btn.h,
            );
        }

        // Hover highlight
        if hovering {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 40));
            let _ = canvas.fill_rect(Rect::new(
                btn_x as i32,
                btn_y as i32,
                btn.w as u32,
                btn.h as u32,
            ));
        }

        assets.text.draw_text_centered(
            canvas,
            tc,
            btn.label,
            bx as i32,
            by as i32,
            20.0,
            Color::RGB(255, 255, 255),
        );

        // Gamepad hint label (small text below button label)
        if gamepad_connected && is_focused {
            assets.text.draw_text_centered(
                canvas,
                tc,
                "(A)",
                bx as i32,
                (by + btn.h / 2.0 - 6.0) as i32,
                12.0,
                Color::RGBA(255, 255, 255, 180),
            );
        }

        clickable_buttons.push(ClickableButton {
            x: btn_x,
            y: btn_y,
            w: btn.w,
            h: btn.h,
            action: btn.action,
        });
    }

    // 7. Hint texts
    for hint in &layout.hints {
        let hx = (cx + hint.offset_x) as i32;
        let hy = (cy + hint.offset_y) as i32;
        assets.text.draw_text_centered(
            canvas,
            tc,
            &hint.text,
            hx,
            hy,
            hint.size as f32,
            Color::RGBA(hint.r, hint.g, hint.b, hint.a),
        );
    }

    clickable_buttons
}
