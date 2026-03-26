use super::LoopState;
use crate::renderer::load_image;
use crate::renderer::TextureId;
use battlefield_core::render_util;
use battlefield_core::unit::{Faction, UnitAnim, UnitKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};

use super::ASSET_BASE;

/// Texture keys for loaded unit animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct UnitTextureKey {
    pub(super) faction: Faction,
    pub(super) kind: UnitKind,
    pub(super) anim: UnitAnim,
}

pub(super) struct LoadedTextures {
    pub(super) unit_textures: HashMap<UnitTextureKey, TextureId>,
    pub(super) particle_textures: HashMap<&'static str, TextureId>,
    pub(super) arrow_texture: Option<TextureId>,
    pub(super) tilemap_texture: Option<TextureId>,
    pub(super) tilemap_texture2: Option<TextureId>,
    pub(super) water_texture: Option<TextureId>,
    pub(super) shadow_texture: Option<TextureId>,
    pub(super) foam_texture: Option<TextureId>,
    /// Tree sprite sheets: (texture_id, frame_w, frame_h) for 4 variants
    pub(super) tree_textures: Vec<(TextureId, u32, u32)>,
    /// Rock decoration textures: single 64x64 sprites for 4 variants
    pub(super) rock_textures: Vec<TextureId>,
    /// Pre-flipped tree canvases: (canvas, frame_w, frame_h) for each variant
    pub(super) tree_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Pre-flipped rock canvases: one per variant
    pub(super) rock_textures_flipped: Vec<web_sys::HtmlCanvasElement>,
    /// Bush sprite sheets: (texture_id, frame_w, frame_h) for 4 variants (128x128 frames)
    pub(super) bush_textures: Vec<(TextureId, u32, u32)>,
    /// Pre-flipped bush canvases: (canvas, frame_w, frame_h) for each variant
    pub(super) bush_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Water rock sprite sheets: (texture_id, frame_w, frame_h) for 4 variants (64x64 frames)
    pub(super) water_rock_textures: Vec<(TextureId, u32, u32)>,
    /// Pre-flipped water rock canvases: (canvas, frame_w, frame_h) for each variant
    pub(super) water_rock_textures_flipped: Vec<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Tower building textures: index 0=Black(neutral), 1=Blue, 2=Red (128x256 each)
    pub(super) tower_textures: Vec<TextureId>,
    /// Base building textures: indexed by kind_index * 2 + faction_index
    /// kind: 0=Barracks, 1=Archery, 2=Monastery, 3=Castle, 4=DefenseTower, 5=House1, 6=House2, 7=House3; faction: 0=Blue, 1=Red
    pub(super) building_textures: Vec<(TextureId, u32, u32)>,
    /// UI 9-slice panel background (SpecialPaper.png, 320x320, 3x3 grid of 106px cells)
    pub(super) ui_special_paper: Option<TextureId>,
    /// UI blue button (BigBlueButton_Regular.png, 320x320)
    pub(super) ui_blue_btn: Option<TextureId>,
    /// UI red button (BigRedButton_Regular.png, 320x320)
    pub(super) ui_red_btn: Option<TextureId>,
    /// Pre-processed gapless 9-slice atlas for SpecialPaper (canvas, width, height)
    pub(super) ui_panel_atlas: Option<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Pre-processed gapless 9-slice atlas for BigBlueButton (canvas, width, height)
    pub(super) ui_blue_btn_atlas: Option<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// Pre-processed gapless 9-slice atlas for BigRedButton (canvas, width, height)
    pub(super) ui_red_btn_atlas: Option<(web_sys::HtmlCanvasElement, u32, u32)>,
    /// UI bar base frame (BigBar_Base.png, 320x64)
    pub(super) ui_bar_base: Option<TextureId>,
    /// UI bar fill (BigBar_Fill.png, 64x64)
    pub(super) ui_bar_fill: Option<TextureId>,
    /// UI ribbon sprite sheet (BigRibbons.png, 448x640, 3 cols x 5 rows of 149x128)
    pub(super) ui_big_ribbons: Option<TextureId>,
    /// Sheep sprite sheets: Idle(6), Move(4), Grass(12) at 128x128
    pub(super) sheep_textures: Vec<(TextureId, u32, u32)>,
    /// Pawn sprite sheets: indexed by faction_offset + sprite_index (5 per faction, 10 total)
    pub(super) pawn_textures: Vec<(TextureId, u32, u32)>,
    /// Unit avatar portraits (256×256 each): 0=Warrior, 1=Lancer, 2=Archer, 3=Monk
    pub(super) avatar_textures: Vec<TextureId>,
}

impl LoadedTextures {
    pub(super) fn new() -> Self {
        Self {
            unit_textures: HashMap::new(),
            particle_textures: HashMap::new(),
            arrow_texture: None,
            tilemap_texture: None,
            tilemap_texture2: None,
            water_texture: None,
            shadow_texture: None,
            foam_texture: None,
            tree_textures: Vec::new(),
            rock_textures: Vec::new(),
            tree_textures_flipped: Vec::new(),
            rock_textures_flipped: Vec::new(),
            bush_textures: Vec::new(),
            bush_textures_flipped: Vec::new(),
            water_rock_textures: Vec::new(),
            water_rock_textures_flipped: Vec::new(),
            tower_textures: Vec::new(),
            building_textures: Vec::new(),
            ui_special_paper: None,
            ui_blue_btn: None,
            ui_red_btn: None,
            ui_panel_atlas: None,
            ui_blue_btn_atlas: None,
            ui_red_btn_atlas: None,
            ui_bar_base: None,
            ui_bar_fill: None,
            ui_big_ribbons: None,
            sheep_textures: Vec::new(),
            pawn_textures: Vec::new(),
            avatar_textures: Vec::new(),
        }
    }
}

/// Load a texture through `LoopState`, splitting borrows around the await.
pub(super) async fn load_texture(
    state: &Rc<RefCell<LoopState>>,
    url: &str,
    frame_w: u32,
    frame_h: u32,
    frame_count: u32,
) -> Result<TextureId, JsValue> {
    {
        let guard = state.borrow();
        if let Some(id) = guard.renderer.texture_manager().get_cached(url) {
            return Ok(id);
        }
    }

    let element = load_image(url).await?;

    log::info!(
        "Loaded sprite sheet: {url} ({}x{}, {frame_count} frames of {frame_w}x{frame_h})",
        element.natural_width(),
        element.natural_height()
    );

    let id = {
        let mut guard = state.borrow_mut();
        guard
            .renderer
            .texture_manager_mut()
            .register(url, element, frame_w, frame_h, frame_count)
    };

    Ok(id)
}

/// Create a gapless 9-slice atlas canvas by blitting 9 source cells from the
/// original image into a tightly packed layout. Returns `(canvas, width, height)`.
fn create_9slice_atlas(
    img: &HtmlImageElement,
    cells: &[[f64; 4]; 9],
) -> Result<(HtmlCanvasElement, u32, u32), JsValue> {
    let (aw, ah) = render_util::nine_cell_atlas_size(cells);
    let positions = render_util::nine_cell_atlas_positions(cells);

    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(aw);
    canvas.set_height(ah);

    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    for (i, cell) in cells.iter().enumerate() {
        let (sx, sy, sw, sh) = (cell[0], cell[1], cell[2], cell[3]);
        let (dx, dy) = positions[i];
        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            img, sx, sy, sw, sh, dx, dy, sw, sh,
        )?;
    }

    Ok((canvas, aw, ah))
}

pub(super) async fn load_textures(
    state: &Rc<RefCell<LoopState>>,
    loaded: &Rc<RefCell<LoadedTextures>>,
) -> Result<(), JsValue> {
    let factions = [Faction::Blue, Faction::Red];
    let unit_kinds = [
        (UnitKind::Warrior, "Warrior"),
        (UnitKind::Archer, "Archer"),
        (UnitKind::Lancer, "Lancer"),
        (UnitKind::Monk, "Monk"),
    ];
    let anims = [
        (UnitAnim::Idle, "Idle"),
        (UnitAnim::Run, "Run"),
        (UnitAnim::Attack, "Attack"),
        (UnitAnim::Attack2, "Attack2"),
    ];

    for &faction in &factions {
        for &(kind, kind_name) in &unit_kinds {
            for &(anim, _anim_name) in &anims {
                let maybe = match (kind, anim) {
                    (UnitKind::Warrior, UnitAnim::Idle) => Some(("Warrior_Idle.png", 8)),
                    (UnitKind::Warrior, UnitAnim::Run) => Some(("Warrior_Run.png", 6)),
                    (UnitKind::Warrior, UnitAnim::Attack) => Some(("Warrior_Attack1.png", 4)),
                    (UnitKind::Warrior, UnitAnim::Attack2) => Some(("Warrior_Attack2.png", 4)),
                    (UnitKind::Archer, UnitAnim::Idle) => Some(("Archer_Idle.png", 6)),
                    (UnitKind::Archer, UnitAnim::Run) => Some(("Archer_Run.png", 4)),
                    (UnitKind::Archer, UnitAnim::Attack) => Some(("Archer_Shoot.png", 8)),
                    (UnitKind::Lancer, UnitAnim::Idle) => Some(("Lancer_Idle.png", 12)),
                    (UnitKind::Lancer, UnitAnim::Run) => Some(("Lancer_Run.png", 6)),
                    (UnitKind::Lancer, UnitAnim::Attack) => Some(("Lancer_Right_Attack.png", 3)),
                    (UnitKind::Monk, UnitAnim::Idle) => Some(("Idle.png", 6)),
                    (UnitKind::Monk, UnitAnim::Run) => Some(("Run.png", 4)),
                    (UnitKind::Monk, UnitAnim::Attack) => Some(("Heal.png", 11)),
                    (_, UnitAnim::Attack2) => None, // only Warrior has Attack2
                };
                let Some((filename, frame_count)) = maybe else {
                    continue;
                };

                let frame_size = kind.frame_size();
                let url = format!(
                    "{}/Units/{}/{}/{}",
                    ASSET_BASE,
                    faction.asset_folder(),
                    kind_name,
                    filename
                );

                let tex_id = load_texture(state, &url, frame_size, frame_size, frame_count).await?;

                loaded.borrow_mut().unit_textures.insert(
                    UnitTextureKey {
                        faction,
                        kind,
                        anim,
                    },
                    tex_id,
                );
            }
        }
    }

    // Load particle effects
    let particles = [
        ("Dust_01.png", 64, 8),
        ("Explosion_01.png", 192, 8),
        ("Explosion_02.png", 192, 10),
    ];
    for &(filename, frame_size, frame_count) in &particles {
        let url = format!("{}/Particle FX/{}", ASSET_BASE, filename);
        let tex_id = load_texture(state, &url, frame_size, frame_size, frame_count).await?;
        loaded
            .borrow_mut()
            .particle_textures
            .insert(filename, tex_id);
    }
    // Heal effect (from Monk sprite folder)
    {
        let url = format!("{}/Units/Blue Units/Monk/Heal_Effect.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 192, 192, 11).await?;
        loaded
            .borrow_mut()
            .particle_textures
            .insert("Heal_Effect.png", tex_id);
    }

    // Load arrow projectile
    {
        let url = format!("{}/Units/Blue Units/Archer/Arrow.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().arrow_texture = Some(tex_id);
    }

    // Load tilemap textures
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color1.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 576, 384, 1).await?;
        loaded.borrow_mut().tilemap_texture = Some(tex_id);
    }
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color2.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 576, 384, 1).await?;
        loaded.borrow_mut().tilemap_texture2 = Some(tex_id);
    }

    // Load water background texture
    {
        let url = format!("{}/Terrain/Tileset/Water Background color.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().water_texture = Some(tex_id);
    }

    // Load shadow texture
    {
        let url = format!("{}/Terrain/Tileset/Shadow.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 192, 192, 1).await?;
        loaded.borrow_mut().shadow_texture = Some(tex_id);
    }

    // Load water foam texture
    {
        let url = format!("{}/Terrain/Tileset/Water Foam.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 192, 192, 16).await?;
        loaded.borrow_mut().foam_texture = Some(tex_id);
    }

    // Load tree sprites (4 variants)
    // Tree1/2: 1536x256, 8 frames of 192x256
    // Tree3/4: 1536x192, 8 frames of 192x192
    let trees = [
        ("Tree1.png", 192u32, 256u32, 8u32),
        ("Tree2.png", 192, 256, 8),
        ("Tree3.png", 192, 192, 8),
        ("Tree4.png", 192, 192, 8),
    ];
    for &(filename, fw, fh, frame_count) in &trees {
        let url = format!("{}/Terrain/Resources/Wood/Trees/{}", ASSET_BASE, filename);
        let tex_id = load_texture(state, &url, fw, fh, frame_count).await?;
        loaded.borrow_mut().tree_textures.push((tex_id, fw, fh));
    }

    // Load rock decoration sprites (4 variants, 64x64 each)
    for i in 1..=4 {
        let url = format!("{}/Terrain/Decorations/Rocks/Rock{}.png", ASSET_BASE, i);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().rock_textures.push(tex_id);
    }

    // Load bush sprites (4 variants, 1024x128, 8 frames of 128x128)
    for i in 1..=4 {
        let url = format!("{}/Terrain/Decorations/Bushes/Bushe{}.png", ASSET_BASE, i);
        let tex_id = load_texture(state, &url, 128, 128, 8).await?;
        loaded.borrow_mut().bush_textures.push((tex_id, 128, 128));
    }

    // Load water rock sprites (4 variants, 1024x64, 16 frames of 64x64)
    for i in 1..=4 {
        let url = format!(
            "{}/Terrain/Decorations/Rocks in the Water/Water Rocks_0{}.png",
            ASSET_BASE, i
        );
        let tex_id = load_texture(state, &url, 64, 64, 16).await?;
        loaded
            .borrow_mut()
            .water_rock_textures
            .push((tex_id, 64, 64));
    }

    // Load tower building sprites (3 color variants: Black=neutral, Blue, Red)
    {
        let tower_colors = ["Black Buildings", "Blue Buildings", "Red Buildings"];
        for color_folder in &tower_colors {
            let url = format!("{}/Buildings/{}/Tower.png", ASSET_BASE, color_folder);
            let tex_id = load_texture(state, &url, 128, 256, 1).await?;
            loaded.borrow_mut().tower_textures.push(tex_id);
        }
    }

    // Load base building sprites from shared manifest
    {
        use battlefield_core::asset_manifest::{BUILDING_FACTION_FOLDERS, BUILDING_SPECS};
        for &(sw, sh, filename) in BUILDING_SPECS {
            for faction_folder in BUILDING_FACTION_FOLDERS {
                let url = format!("{}/Buildings/{}/{}", ASSET_BASE, faction_folder, filename);
                let tex_id = load_texture(state, &url, sw, sh, 1).await?;
                loaded.borrow_mut().building_textures.push((tex_id, sw, sh));
            }
        }
    }

    // UI textures (9-slice panels, buttons, bars, ribbons)
    let ui_base = format!("{ASSET_BASE}/UI Elements/UI Elements");
    {
        let url = format!("{ui_base}/Papers/SpecialPaper.png");
        loaded.borrow_mut().ui_special_paper = load_texture(state, &url, 106, 106, 9).await.ok();
    }
    {
        let url = format!("{ui_base}/Buttons/BigBlueButton_Regular.png");
        loaded.borrow_mut().ui_blue_btn = load_texture(state, &url, 106, 106, 9).await.ok();
    }
    {
        let url = format!("{ui_base}/Buttons/BigRedButton_Regular.png");
        loaded.borrow_mut().ui_red_btn = load_texture(state, &url, 106, 106, 9).await.ok();
    }
    {
        let url = format!("{ui_base}/Bars/BigBar_Base.png");
        loaded.borrow_mut().ui_bar_base = load_texture(state, &url, 106, 64, 3).await.ok();
    }
    {
        let url = format!("{ui_base}/Bars/BigBar_Fill.png");
        loaded.borrow_mut().ui_bar_fill = load_texture(state, &url, 64, 64, 1).await.ok();
    }
    {
        let url = format!("{ui_base}/Ribbons/BigRibbons.png");
        loaded.borrow_mut().ui_big_ribbons = load_texture(state, &url, 149, 128, 15).await.ok();
    }

    // Sheep sprites (Idle=6, Move=4, Grass=12 frames at 128x128)
    {
        let sheep_specs: &[(&str, u32)] = &[
            ("Sheep_Idle.png", 6),
            ("Sheep_Move.png", 4),
            ("Sheep_Grass.png", 12),
        ];
        for &(filename, frame_count) in sheep_specs {
            let url = format!("{}/Terrain/Resources/Meat/Sheep/{}", ASSET_BASE, filename);
            if let Ok(tex_id) = load_texture(state, &url, 128, 128, frame_count).await {
                loaded.borrow_mut().sheep_textures.push((tex_id, 128, 128));
            }
        }
    }

    // Pawn sprites (5 animations × 2 factions = 10 textures, 192×192 frames)
    {
        use battlefield_core::asset_manifest::PAWN_SPECS;
        let faction_folders = ["Blue Units", "Red Units"];
        for folder in &faction_folders {
            for &(filename, frame_count) in PAWN_SPECS {
                let url = format!("{}/Units/{}/Pawn/{}", ASSET_BASE, folder, filename);
                if let Ok(tex_id) = load_texture(state, &url, 192, 192, frame_count).await {
                    loaded
                        .borrow_mut()
                        .pawn_textures
                        .push((tex_id, 192, 192));
                }
            }
        }
    }

    // Unit avatar portraits (256×256 each)
    {
        use battlefield_core::asset_manifest::AVATAR_FILES;
        let avatar_base = format!("{ASSET_BASE}/UI Elements/UI Elements/Human Avatars");
        for filename in AVATAR_FILES {
            let url = format!("{avatar_base}/{filename}");
            if let Ok(tex_id) = load_texture(state, &url, 256, 256, 1).await {
                loaded.borrow_mut().avatar_textures.push(tex_id);
            }
        }
    }

    // Build gapless 9-slice atlases from the loaded UI images
    {
        let guard = state.borrow();
        let tm = guard.renderer.texture_manager();
        let loaded_ref = loaded.borrow();

        let panel_atlas = loaded_ref.ui_special_paper.and_then(|tex_id| {
            let (img, _, _, _) = tm.get_image(tex_id)?;
            create_9slice_atlas(img, &render_util::SPECIAL_PAPER_CELLS).ok()
        });
        let blue_atlas = loaded_ref.ui_blue_btn.and_then(|tex_id| {
            let (img, _, _, _) = tm.get_image(tex_id)?;
            create_9slice_atlas(img, &render_util::BUTTON_CELLS).ok()
        });
        let red_atlas = loaded_ref.ui_red_btn.and_then(|tex_id| {
            let (img, _, _, _) = tm.get_image(tex_id)?;
            create_9slice_atlas(img, &render_util::BUTTON_CELLS).ok()
        });

        drop(loaded_ref);
        drop(guard);

        let mut loaded_mut = loaded.borrow_mut();
        loaded_mut.ui_panel_atlas = panel_atlas;
        loaded_mut.ui_blue_btn_atlas = blue_atlas;
        loaded_mut.ui_red_btn_atlas = red_atlas;
    }

    // Pre-flip tree and rock textures at load time (eliminates per-frame save/translate/scale/restore)
    {
        let document = web_sys::window().unwrap().document().unwrap();
        let guard = state.borrow();
        let tm = guard.renderer.texture_manager();
        let loaded_ref = loaded.borrow();

        let mut tree_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.tree_textures {
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                    );
                tree_flipped.push((c, fw, fh));
            }
        }

        let mut rock_flipped = Vec::new();
        for &tex_id in &loaded_ref.rock_textures {
            if let Some((img, _, _, _)) = tm.get_image(tex_id) {
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(64);
                c.set_height(64);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(64.0, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img, 0.0, 0.0, 64.0, 64.0, 0.0, 0.0, 64.0, 64.0,
                    );
                rock_flipped.push(c);
            }
        }

        let mut bush_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.bush_textures {
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                    );
                bush_flipped.push((c, fw, fh));
            }
        }

        let mut water_rock_flipped = Vec::new();
        for &(tex_id, fw, fh) in &loaded_ref.water_rock_textures {
            if let Some((img, _, _, frame_count)) = tm.get_image(tex_id) {
                let sheet_w = fw * frame_count;
                let c = document
                    .create_element("canvas")
                    .unwrap()
                    .dyn_into::<web_sys::HtmlCanvasElement>()
                    .unwrap();
                c.set_width(sheet_w);
                c.set_height(fh);
                let fctx = c
                    .get_context("2d")
                    .unwrap()
                    .unwrap()
                    .dyn_into::<web_sys::CanvasRenderingContext2d>()
                    .unwrap();
                let _ = fctx.translate(sheet_w as f64, 0.0);
                let _ = fctx.scale(-1.0, 1.0);
                let _ = fctx
                    .draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                        img,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                        0.0,
                        0.0,
                        sheet_w as f64,
                        fh as f64,
                    );
                water_rock_flipped.push((c, fw, fh));
            }
        }

        drop(loaded_ref);
        drop(guard);

        let mut loaded_mut = loaded.borrow_mut();
        loaded_mut.tree_textures_flipped = tree_flipped;
        loaded_mut.rock_textures_flipped = rock_flipped;
        loaded_mut.bush_textures_flipped = bush_flipped;
        loaded_mut.water_rock_textures_flipped = water_rock_flipped;
    }

    Ok(())
}
