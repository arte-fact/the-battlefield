use super::LoopState;
use crate::building::BuildingKind;
use crate::renderer::load_image;
use crate::renderer::TextureId;
use crate::unit::{Faction, UnitAnim, UnitKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

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
    /// kind: 0=Barracks, 1=Archery, 2=Monastery; faction: 0=Blue, 1=Red
    pub(super) building_textures: Vec<(TextureId, u32, u32)>,
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
    ];

    for &faction in &factions {
        for &(kind, kind_name) in &unit_kinds {
            for &(anim, _anim_name) in &anims {
                let (filename, frame_count) = match (kind, anim) {
                    (UnitKind::Warrior, UnitAnim::Idle) => ("Warrior_Idle.png", 8),
                    (UnitKind::Warrior, UnitAnim::Run) => ("Warrior_Run.png", 6),
                    (UnitKind::Warrior, UnitAnim::Attack) => ("Warrior_Attack1.png", 4),
                    (UnitKind::Archer, UnitAnim::Idle) => ("Archer_Idle.png", 6),
                    (UnitKind::Archer, UnitAnim::Run) => ("Archer_Run.png", 4),
                    (UnitKind::Archer, UnitAnim::Attack) => ("Archer_Shoot.png", 8),
                    (UnitKind::Lancer, UnitAnim::Idle) => ("Lancer_Idle.png", 12),
                    (UnitKind::Lancer, UnitAnim::Run) => ("Lancer_Run.png", 6),
                    (UnitKind::Lancer, UnitAnim::Attack) => ("Lancer_Right_Attack.png", 3),
                    (UnitKind::Monk, UnitAnim::Idle) => ("Idle.png", 6),
                    (UnitKind::Monk, UnitAnim::Run) => ("Run.png", 4),
                    (UnitKind::Monk, UnitAnim::Attack) => ("Heal.png", 11),
                };

                let frame_size = kind.frame_size();
                let url = format!(
                    "{}/Units/{}/{}/{}",
                    ASSET_BASE,
                    faction.asset_folder(),
                    kind_name,
                    filename
                );

                let tex_id =
                    load_texture(state, &url, frame_size, frame_size, frame_count).await?;

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

    // Load base production building sprites (3 kinds x 2 factions)
    {
        let kinds = [
            (BuildingKind::Barracks, 192u32, 256u32),
            (BuildingKind::Archery, 192, 256),
            (BuildingKind::Monastery, 192, 320),
        ];
        let faction_folders = ["Blue Buildings", "Red Buildings"];
        for &(kind, sw, sh) in &kinds {
            for faction_folder in &faction_folders {
                let url = format!(
                    "{}/Buildings/{}/{}",
                    ASSET_BASE,
                    faction_folder,
                    kind.asset_filename()
                );
                let tex_id = load_texture(state, &url, sw, sh, 1).await?;
                loaded
                    .borrow_mut()
                    .building_textures
                    .push((tex_id, sw, sh));
            }
        }
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
