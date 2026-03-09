use crate::game::{Game, PlayerAction};
use crate::grid::{self, TileKind, TILE_SIZE};
use crate::input::Input;
use crate::renderer::{
    BatchRenderer, ColorInstance, Gpu, SpriteBatch, SpriteInstance, TextureId, TextureManager,
};
use crate::turn::TurnPhase;
use crate::unit::{Facing, Faction, UnitAnim, UnitKind, DEATH_FADE_DURATION};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

const ASSET_BASE: &str = "assets/Tiny Swords (Free Pack)";

/// Texture keys for loaded unit animations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct UnitTextureKey {
    faction: Faction,
    kind: UnitKind,
    anim: UnitAnim,
}

struct LoadedTextures {
    unit_textures: HashMap<UnitTextureKey, TextureId>,
    particle_textures: HashMap<&'static str, TextureId>,
    arrow_texture: Option<TextureId>,
    tilemap_texture: Option<TextureId>,
    water_texture: Option<TextureId>,
}

impl LoadedTextures {
    fn new() -> Self {
        Self {
            unit_textures: HashMap::new(),
            particle_textures: HashMap::new(),
            arrow_texture: None,
            tilemap_texture: None,
            water_texture: None,
        }
    }
}

/// Cached HUD DOM elements to avoid repeated lookups.
struct HudElements {
    turn_display: web_sys::Element,
    phase_display: web_sys::HtmlElement,
    hp_bar_fill: web_sys::HtmlElement,
}

impl HudElements {
    fn from_document(doc: &web_sys::Document) -> Option<Self> {
        let turn_display = doc.get_element_by_id("turn-display")?;
        let phase_display = doc
            .get_element_by_id("phase-display")?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        let hp_bar_fill = doc
            .get_element_by_id("hp-bar-fill")?
            .dyn_into::<web_sys::HtmlElement>()
            .ok()?;
        Some(Self {
            turn_display,
            phase_display,
            hp_bar_fill,
        })
    }

    fn update(&self, game: &Game) {
        // Turn number
        let turn_text = format!("Turn {}", game.turn.turn_number);
        self.turn_display.set_text_content(Some(&turn_text));

        // Phase display
        let (phase_text, phase_color) = match game.turn.phase {
            TurnPhase::PlayerTurn => ("Your Turn", "#4fc3f7"),
            TurnPhase::AiTurn => ("Enemy Turn", "#ef5350"),
            TurnPhase::Resolution => ("Resolving...", "#ffd740"),
        };
        self.phase_display.set_text_content(Some(phase_text));
        let _ = self
            .phase_display
            .style()
            .set_property("color", phase_color);

        // Player HP bar
        if let Some(player) = game.player_unit() {
            let ratio = player.hp as f32 / player.stats.max_hp as f32;
            let pct = format!("{}%", (ratio * 100.0) as u32);
            let _ = self.hp_bar_fill.style().set_property("width", &pct);

            let color = if ratio > 0.5 {
                "#4caf50"
            } else if ratio > 0.25 {
                "#ff9800"
            } else {
                "#f44336"
            };
            let _ = self.hp_bar_fill.style().set_property("background", color);
        } else {
            let _ = self.hp_bar_fill.style().set_property("width", "0%");
        }
    }
}

pub fn run(
    gpu: Gpu,
    game: Game,
    batch_renderer: BatchRenderer,
    texture_manager: TextureManager,
    canvas: &web_sys::HtmlCanvasElement,
) -> Result<(), JsValue> {
    let input = Rc::new(RefCell::new(Input::new()));
    let loaded_textures = Rc::new(RefCell::new(LoadedTextures::new()));
    let textures_loading = Rc::new(RefCell::new(false));

    // Cache HUD DOM elements
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let hud = HudElements::from_document(&document);
    let hud = Rc::new(hud);

    // Set up input event listeners
    setup_input_listeners(canvas, &input)?;

    let state = Rc::new(RefCell::new(LoopState {
        gpu,
        game,
        batch_renderer,
        texture_manager,
        last_time: None,
    }));

    // Start async texture loading
    {
        let state_clone = state.clone();
        let loaded_clone = loaded_textures.clone();
        let loading_flag = textures_loading.clone();
        *loading_flag.borrow_mut() = true;
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = load_textures(&state_clone, &loaded_clone).await {
                log::error!("Failed to load textures: {:?}", e);
            }
            *loading_flag.borrow_mut() = false;
            log::info!("All textures loaded");
        });
    }

    // Game loop
    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let state_clone = state.clone();
    let input_clone = input.clone();
    let loaded_clone = loaded_textures.clone();
    let loading_clone = textures_loading.clone();
    let hud_clone = hud.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {
        let mut s = state_clone.borrow_mut();
        let dt = match s.last_time {
            Some(last) => (timestamp - last) / 1000.0,
            None => 0.0,
        };
        s.last_time = Some(timestamp);

        let dt = dt.min(0.1); // cap at 100ms to avoid spiral

        // Process input
        {
            let mut inp = input_clone.borrow_mut();

            // Camera controls
            let (pan_x, pan_y) = inp.camera_pan();
            if pan_x != 0.0 || pan_y != 0.0 {
                s.game.camera.pan(pan_x, pan_y, dt as f32);
            }

            let scroll = inp.take_scroll();
            if scroll != 0.0 {
                s.game.camera.zoom_by(scroll);
            }

            // Handle mouse clicks
            if let Some((sx, sy)) = inp.take_click() {
                handle_click(&mut s.game, sx, sy);
            }

            // Space to end turn
            if inp.is_key_down(" ") {
                inp.key_up(" ");
                if s.game.turn.phase == TurnPhase::PlayerTurn {
                    s.game.handle_player_action(PlayerAction::EndTurn);
                }
            }
        }

        // Run AI and resolution if needed
        if s.game.turn.phase == TurnPhase::AiTurn {
            s.game.run_ai_turn();
        }
        if s.game.turn.phase == TurnPhase::Resolution {
            s.game.resolve_turn();
        }

        // Update game state
        s.game.update(dt);
        s.game.events.clear();

        // Update HUD
        if let Some(ref hud) = *hud_clone {
            hud.update(&s.game);
        }

        // Render
        if !*loading_clone.borrow() {
            let loaded = loaded_clone.borrow();
            if let Err(e) = render_frame(&mut s, &loaded) {
                log::error!("Render error: {:?}", e);
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn handle_click(game: &mut Game, screen_x: f32, screen_y: f32) {
    if game.turn.phase != TurnPhase::PlayerTurn {
        return;
    }

    let (wx, wy) = game.camera.screen_to_world(screen_x, screen_y);
    let (gx, gy) = grid::world_to_grid(wx, wy);

    if !game.grid.in_bounds(gx, gy) {
        return;
    }

    let gx = gx as u32;
    let gy = gy as u32;

    // Check if clicking on an enemy (attack)
    if let Some(enemy_id) = game
        .unit_at(gx, gy)
        .filter(|u| {
            game.player_unit()
                .map_or(false, |p| u.faction != p.faction && u.alive)
        })
        .map(|u| u.id)
    {
        if game.attack_targets.contains(&enemy_id) {
            game.handle_player_action(PlayerAction::Attack {
                target_id: enemy_id,
            });
            return;
        }
    }

    // Check if clicking on a valid move target
    if game.move_targets.contains(&(gx, gy)) {
        game.handle_player_action(PlayerAction::Move {
            target_x: gx,
            target_y: gy,
        });
    }
}

fn setup_input_listeners(
    canvas: &web_sys::HtmlCanvasElement,
    input: &Rc<RefCell<Input>>,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("no window")?;

    // Keyboard
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            input_clone.borrow_mut().key_down(e.key());
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::KeyboardEvent| {
            input_clone.borrow_mut().key_up(&e.key());
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keyup", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Mouse click
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
            let mut inp = input_clone.borrow_mut();
            inp.mouse_x = e.offset_x() as f32;
            inp.mouse_y = e.offset_y() as f32;
            inp.mouse_clicked = true;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Mouse wheel (zoom)
    {
        let input_clone = input.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::WheelEvent| {
            e.prevent_default();
            let delta = -e.delta_y().signum() as f32;
            input_clone.borrow_mut().scroll_delta += delta;
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("wheel", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // Focus the canvas for keyboard events
    canvas.set_tab_index(0);
    canvas.focus()?;

    Ok(())
}

async fn load_textures(
    state: &Rc<RefCell<LoopState>>,
    loaded: &Rc<RefCell<LoadedTextures>>,
) -> Result<(), JsValue> {
    // Load unit sprites for the demo factions (Blue and Red)
    let factions = [Faction::Blue, Faction::Red];
    let unit_kinds = [(UnitKind::Warrior, "Warrior"), (UnitKind::Archer, "Archer")];
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
                    _ => continue,
                };

                let frame_size = kind.frame_size();
                let url = format!(
                    "{}/Units/{}/{}/{}",
                    ASSET_BASE,
                    faction.asset_folder(),
                    kind_name,
                    filename
                );

                let tex_id = {
                    let mut s = state.borrow_mut();
                    s.texture_manager
                        .load(
                            &s.gpu,
                            &s.batch_renderer.texture_bind_group_layout,
                            &s.batch_renderer.sampler,
                            &url,
                            frame_size,
                            frame_size,
                            frame_count,
                        )
                        .await?
                };

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
        let tex_id = {
            let mut s = state.borrow_mut();
            s.texture_manager
                .load(
                    &s.gpu,
                    &s.batch_renderer.texture_bind_group_layout,
                    &s.batch_renderer.sampler,
                    &url,
                    frame_size,
                    frame_size,
                    frame_count,
                )
                .await?
        };
        loaded
            .borrow_mut()
            .particle_textures
            .insert(filename, tex_id);
    }

    // Load arrow projectile
    {
        let url = format!("{}/Units/Blue Units/Archer/Arrow.png", ASSET_BASE);
        let tex_id = {
            let mut s = state.borrow_mut();
            s.texture_manager
                .load(
                    &s.gpu,
                    &s.batch_renderer.texture_bind_group_layout,
                    &s.batch_renderer.sampler,
                    &url,
                    64,
                    64,
                    1,
                )
                .await?
        };
        loaded.borrow_mut().arrow_texture = Some(tex_id);
    }

    // Load tilemap texture (576x384, loaded as single frame)
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color1.png", ASSET_BASE);
        let tex_id = {
            let mut s = state.borrow_mut();
            s.texture_manager
                .load(
                    &s.gpu,
                    &s.batch_renderer.texture_bind_group_layout,
                    &s.batch_renderer.sampler,
                    &url,
                    576,
                    384,
                    1,
                )
                .await?
        };
        loaded.borrow_mut().tilemap_texture = Some(tex_id);
    }

    // Load water background texture (64x64, single tile)
    {
        let url = format!("{}/Terrain/Tileset/Water Background color.png", ASSET_BASE);
        let tex_id = {
            let mut s = state.borrow_mut();
            s.texture_manager
                .load(
                    &s.gpu,
                    &s.batch_renderer.texture_bind_group_layout,
                    &s.batch_renderer.sampler,
                    &url,
                    64,
                    64,
                    1,
                )
                .await?
        };
        loaded.borrow_mut().water_texture = Some(tex_id);
    }

    Ok(())
}

fn render_frame(state: &mut LoopState, loaded: &LoadedTextures) -> Result<(), JsValue> {
    let game = &state.game;
    let gpu = &state.gpu;

    // Update camera uniform
    let view_proj = game.camera.view_proj_matrix();
    state.batch_renderer.update_camera(gpu, &view_proj);

    // Visible tile range
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(game.grid.width as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(game.grid.height as i32) as u32;

    // === Background sprite batches (tilemap tiles) ===
    let mut bg_sprite_batches: Vec<SpriteBatch> = Vec::new();
    build_tile_sprites(
        game,
        loaded,
        min_gx,
        min_gy,
        max_gx,
        max_gy,
        &mut bg_sprite_batches,
    );

    // === Color instances (highlights, HP bars) ===
    let mut color_instances: Vec<ColorInstance> = Vec::new();

    // Forest overlay tint (darker green over the grass tile)
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) == TileKind::Forest {
                let (wx, wy) = grid::grid_to_world(gx, gy);
                color_instances.push(ColorInstance {
                    world_pos: [wx, wy],
                    size: [TILE_SIZE, TILE_SIZE],
                    color: [0.0, 0.15, 0.0, 0.4],
                });
            }
        }
    }

    // Move target highlights
    for &(mx, my) in &game.move_targets {
        let (wx, wy) = grid::grid_to_world(mx, my);
        color_instances.push(ColorInstance {
            world_pos: [wx, wy],
            size: [TILE_SIZE - 2.0, TILE_SIZE - 2.0],
            color: [0.2, 0.6, 1.0, 0.3],
        });
    }

    // Attack target highlights
    for &target_id in &game.attack_targets {
        if let Some(unit) = game.find_unit(target_id) {
            let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
            color_instances.push(ColorInstance {
                world_pos: [wx, wy],
                size: [TILE_SIZE - 2.0, TILE_SIZE - 2.0],
                color: [1.0, 0.2, 0.2, 0.4],
            });
        }
    }

    // Highlight player unit's tile
    if let Some(player) = game.player_unit() {
        let (wx, wy) = grid::grid_to_world(player.grid_x, player.grid_y);
        color_instances.push(ColorInstance {
            world_pos: [wx, wy],
            size: [TILE_SIZE - 2.0, TILE_SIZE - 2.0],
            color: [1.0, 1.0, 0.2, 0.3],
        });
    }

    // HP bars for alive units
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
        let bar_width = 48.0;
        let bar_height = 6.0;
        let bar_y = wy - TILE_SIZE * 0.45;

        // Background
        color_instances.push(ColorInstance {
            world_pos: [wx, bar_y],
            size: [bar_width, bar_height],
            color: [0.2, 0.2, 0.2, 0.8],
        });

        // Fill
        let hp_ratio = unit.hp as f32 / unit.stats.max_hp as f32;
        let fill_width = bar_width * hp_ratio;
        let fill_offset = (bar_width - fill_width) * -0.5;
        let fill_color = if hp_ratio > 0.5 {
            [0.2, 0.8, 0.2, 0.9]
        } else if hp_ratio > 0.25 {
            [0.9, 0.7, 0.1, 0.9]
        } else {
            [0.9, 0.2, 0.1, 0.9]
        };
        color_instances.push(ColorInstance {
            world_pos: [wx + fill_offset, bar_y],
            size: [fill_width, bar_height - 2.0],
            color: fill_color,
        });
    }

    // === Foreground sprite batches (units, particles, projectiles) ===
    let mut fg_sprite_batches: Vec<SpriteBatch> = Vec::new();

    // Group instances by texture
    let mut texture_instances: HashMap<TextureId, Vec<SpriteInstance>> = HashMap::new();

    // Unit sprites (sorted by Y for proper layering)
    // Include alive units AND dying units (death_fade > 0)
    let mut unit_indices: Vec<usize> = game
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.alive || u.death_fade > 0.0)
        .map(|(i, _)| i)
        .collect();
    unit_indices.sort_by(|&a, &b| {
        game.units[a]
            .grid_y
            .cmp(&game.units[b].grid_y)
            .then(game.units[a].grid_x.cmp(&game.units[b].grid_x))
    });

    for &idx in &unit_indices {
        let unit = &game.units[idx];
        let key = UnitTextureKey {
            faction: unit.faction,
            kind: unit.kind,
            anim: unit.current_anim,
        };

        let tex_id = match loaded.unit_textures.get(&key) {
            Some(&id) => id,
            None => {
                // Fallback to idle
                let fallback_key = UnitTextureKey {
                    faction: unit.faction,
                    kind: unit.kind,
                    anim: UnitAnim::Idle,
                };
                match loaded.unit_textures.get(&fallback_key) {
                    Some(&id) => id,
                    None => continue,
                }
            }
        };

        let sheet = match state.texture_manager.get_sprite_sheet(tex_id) {
            Some(s) => s,
            None => continue,
        };

        let uv = sheet.frame_uv(unit.animation.current_frame);
        let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
        let sprite_size = unit.kind.frame_size() as f32;

        // Compute opacity: alive = 1.0, dying = fade based on remaining time
        let opacity = if unit.alive {
            1.0
        } else {
            (unit.death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0)
        };

        texture_instances
            .entry(tex_id)
            .or_default()
            .push(SpriteInstance {
                world_pos: [wx, wy],
                size: [sprite_size, sprite_size],
                uv_min: [uv[0], uv[1]],
                uv_max: [uv[2], uv[3]],
                flip_x: if unit.facing == Facing::Left {
                    1.0
                } else {
                    0.0
                },
                opacity,
            });
    }

    // Particle sprites
    for particle in &game.particles {
        let filename = particle.kind.asset_filename();
        let tex_id = match loaded.particle_textures.get(filename) {
            Some(&id) => id,
            None => continue,
        };

        let sheet = match state.texture_manager.get_sprite_sheet(tex_id) {
            Some(s) => s,
            None => continue,
        };

        let uv = sheet.frame_uv(particle.animation.current_frame);
        let size = particle.kind.frame_size() as f32;

        texture_instances
            .entry(tex_id)
            .or_default()
            .push(SpriteInstance {
                world_pos: [particle.world_x, particle.world_y],
                size: [size, size],
                uv_min: [uv[0], uv[1]],
                uv_max: [uv[2], uv[3]],
                flip_x: 0.0,
                opacity: 1.0,
            });
    }

    // Arrow projectiles
    if let Some(&arrow_tex_id) = loaded.arrow_texture.as_ref() {
        for proj in &game.projectiles {
            texture_instances
                .entry(arrow_tex_id)
                .or_default()
                .push(SpriteInstance {
                    world_pos: [proj.current_x, proj.current_y],
                    size: [64.0, 64.0],
                    uv_min: [0.0, 0.0],
                    uv_max: [1.0, 1.0],
                    flip_x: if proj.angle.abs() > std::f32::consts::FRAC_PI_2 {
                        1.0
                    } else {
                        0.0
                    },
                    opacity: 1.0,
                });
        }
    }

    // Convert to batches
    for (tex_id, instances) in texture_instances {
        fg_sprite_batches.push(SpriteBatch {
            texture_id: tex_id,
            instances,
        });
    }

    state.batch_renderer.render(
        gpu,
        &bg_sprite_batches,
        &color_instances,
        &fg_sprite_batches,
        &state.texture_manager,
    )
}

/// Build sprite instances for the visible tilemap region.
fn build_tile_sprites(
    game: &Game,
    loaded: &LoadedTextures,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    batches: &mut Vec<SpriteBatch>,
) {
    // Tilemap tiles (grass, hill, forest, rock)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        let mut tile_instances: Vec<SpriteInstance> = Vec::new();

        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                let tile = game.grid.get(gx, gy);
                if let Some((col, row)) = tile.tilemap_coords() {
                    let (uv_min, uv_max) = grid::tilemap_uv(col, row);
                    let (wx, wy) = grid::grid_to_world(gx, gy);
                    tile_instances.push(SpriteInstance {
                        world_pos: [wx, wy],
                        size: [TILE_SIZE, TILE_SIZE],
                        uv_min,
                        uv_max,
                        flip_x: 0.0,
                        opacity: 1.0,
                    });
                }
            }
        }

        if !tile_instances.is_empty() {
            batches.push(SpriteBatch {
                texture_id: tilemap_tex_id,
                instances: tile_instances,
            });
        }
    }

    // Water tiles (separate texture)
    if let Some(water_tex_id) = loaded.water_texture {
        let mut water_instances: Vec<SpriteInstance> = Vec::new();

        for gy in min_gy..max_gy {
            for gx in min_gx..max_gx {
                if game.grid.get(gx, gy) == TileKind::Water {
                    let (wx, wy) = grid::grid_to_world(gx, gy);
                    water_instances.push(SpriteInstance {
                        world_pos: [wx, wy],
                        size: [TILE_SIZE, TILE_SIZE],
                        uv_min: [0.0, 0.0],
                        uv_max: [1.0, 1.0],
                        flip_x: 0.0,
                        opacity: 1.0,
                    });
                }
            }
        }

        if !water_instances.is_empty() {
            batches.push(SpriteBatch {
                texture_id: water_tex_id,
                instances: water_instances,
            });
        }
    }
}

fn request_animation_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}

struct LoopState {
    gpu: Gpu,
    game: Game,
    batch_renderer: BatchRenderer,
    texture_manager: TextureManager,
    last_time: Option<f64>,
}
