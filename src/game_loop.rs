use crate::game::{Game, PlayerAction, SwipePreview};
use crate::grid::{self, TileKind, TILE_SIZE};
use crate::input::{Input, SwipeDir};
use crate::renderer::{draw_sprite, load_image, Canvas2d, TextureId, TextureManager};
use crate::sprite::SpriteSheet;
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
    canvas2d: Canvas2d,
    game: Game,
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
        canvas2d,
        game,
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

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {
        // Process everything in a single scope to avoid borrowing conflicts
        {
            let mut state_guard = state.borrow_mut();
            let last_time = state_guard.last_time;
            state_guard.last_time = Some(timestamp);

            let dt = match last_time {
                Some(last) => (timestamp - last) / 1000.0,
                None => 0.0,
            };
            let dt = dt.min(0.1); // cap at 100ms to avoid spiral

            let game = &mut state_guard.game;

            // Process input
            {
                let mut inp = input.borrow_mut();

                // Camera controls
                let (pan_x, pan_y) = inp.camera_pan();
                if pan_x != 0.0 || pan_y != 0.0 {
                    game.camera.pan(pan_x, pan_y, dt as f32);
                }

                let scroll = inp.take_scroll();
                if scroll != 0.0 {
                    game.camera.zoom_by(scroll);
                }

                // Handle mouse clicks
                if let Some((sx, sy)) = inp.take_click() {
                    handle_click(game, sx, sy);
                }

                // Space to end turn
                if inp.is_key_down(" ") {
                    inp.key_up(" ");
                    if game.turn.phase == TurnPhase::PlayerTurn {
                        game.handle_player_action(PlayerAction::EndTurn);
                    }
                }

                // End Turn button (on-screen)
                if inp.take_end_turn() && game.turn.phase == TurnPhase::PlayerTurn {
                    game.handle_player_action(PlayerAction::EndTurn);
                }

                // Touch: pinch-to-zoom
                let pinch = inp.take_pinch_zoom();
                if pinch.abs() > f32::EPSILON {
                    game.camera.zoom_by(pinch);
                }

                // Touch: two-finger pan
                let (pan_tx, pan_ty) = inp.take_touch_pan();
                if pan_tx.abs() > f32::EPSILON || pan_ty.abs() > f32::EPSILON {
                    game.camera.x -= pan_tx / game.camera.zoom;
                    game.camera.y -= pan_ty / game.camera.zoom;
                }

                // Touch: swipe for movement and attack
                if let Some(dir) = inp.take_swipe() {
                    if game.turn.phase == TurnPhase::PlayerTurn {
                        // Attack takes priority over movement
                        if let Some(target_id) =
                            game.find_attack_target_in_direction(dir)
                        {
                            game.handle_player_action(PlayerAction::Attack {
                                target_id,
                            });
                        } else {
                            game.handle_player_action(PlayerAction::MoveDirection { dir });
                        }
                    }
                }

                // Compute live swipe preview
                let preview = if let Some(swipe) = inp.swipe_state() {
                    let dx = swipe.current_x - swipe.start_x;
                    let dy = swipe.current_y - swipe.start_y;
                    SwipeDir::from_delta(dx, dy, 30.0)
                        .filter(|_| game.turn.phase == TurnPhase::PlayerTurn)
                        .map(|dir| game.compute_swipe_preview(dir))
                } else {
                    None
                };
                game.swipe_preview = preview;
            }

            // Run AI and resolution if needed
            if game.turn.phase == TurnPhase::AiTurn {
                game.run_ai_turn();
            }
            if game.turn.phase == TurnPhase::Resolution {
                game.resolve_turn();
            }

            // Update game state
            game.update(dt);
            game.events.clear();

            // Update HUD (hud is Rc<Option<HudElements>>, not a RefCell)
            if let Some(ref hud) = *hud.as_ref() {
                hud.update(game);
            }
        }

        // Render (separate scope to avoid borrowing conflicts)
        {
            // textures_loading is Rc<RefCell<bool>>, so we need to deref the Ref<bool>
            if !*textures_loading.borrow() {
                let loaded = loaded_textures.borrow();
                let mut state_guard = state.borrow_mut();
                if let Err(e) = render_frame(&mut state_guard, &loaded) {
                    log::error!("Render error: {:?}", e);
                }
            }
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn handle_click(game: &mut Game, screen_x: f32, screen_y: f32) {
    use crate::input::SwipeDir;

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

    // Derive direction from player to clicked tile, then use MoveDirection
    if let Some(player) = game.player_unit() {
        let dx = gx as i32 - player.grid_x as i32;
        let dy = gy as i32 - player.grid_y as i32;
        if let Some(dir) = SwipeDir::from_grid_delta(dx, dy) {
            game.handle_player_action(PlayerAction::MoveDirection { dir });
        }
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

    // Touch events
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            let count = touches.length();
            if count >= 1 {
                let t = e.changed_touches().get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                input_clone.borrow_mut().on_touch_start(t.identifier(), cx, cy, count);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchstart", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let touches = e.touches();
            if touches.length() == 1 {
                let t = touches.get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                input_clone.borrow_mut().on_touch_move_single(cx, cy);
            } else if touches.length() >= 2 {
                let t0 = touches.get(0).unwrap();
                let t1 = touches.get(1).unwrap();
                let (x0, y0) = canvas_touch_coords(&canvas_clone, &t0);
                let (x1, y1) = canvas_touch_coords(&canvas_clone, &t1);
                input_clone.borrow_mut().on_touch_move_two_finger(x0, y0, x1, y1);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchmove", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }
    {
        let input_clone = input.clone();
        let canvas_clone = canvas.clone();
        let closure = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
            e.prevent_default();
            let ct = e.changed_touches();
            if ct.length() >= 1 {
                let t = ct.get(0).unwrap();
                let (cx, cy) = canvas_touch_coords(&canvas_clone, &t);
                let remaining = e.touches().length();
                input_clone.borrow_mut().on_touch_end(t.identifier(), cx, cy, remaining);
            }
        }) as Box<dyn FnMut(_)>);
        canvas.add_event_listener_with_callback("touchend", closure.as_ref().unchecked_ref())?;
        closure.forget();
    }

    // End Turn button
    {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        if let Some(btn) = document.get_element_by_id("end-turn-btn") {
            let input_clone = input.clone();
            let closure = Closure::wrap(Box::new(move |_e: web_sys::MouseEvent| {
                input_clone.borrow_mut().end_turn_requested = true;
            }) as Box<dyn FnMut(_)>);
            btn.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())?;
            closure.forget();

            // Also handle touch on the button to avoid 300ms delay
            let input_clone2 = input.clone();
            let closure2 = Closure::wrap(Box::new(move |e: web_sys::TouchEvent| {
                e.prevent_default();
                input_clone2.borrow_mut().end_turn_requested = true;
            }) as Box<dyn FnMut(_)>);
            btn.add_event_listener_with_callback("touchend", closure2.as_ref().unchecked_ref())?;
            closure2.forget();
        }
    }

    // Focus the canvas for keyboard events
    canvas.set_tab_index(0);
    canvas.focus()?;

    Ok(())
}

/// Convert a Touch's client coordinates to canvas-relative coordinates.
fn canvas_touch_coords(canvas: &web_sys::HtmlCanvasElement, touch: &web_sys::Touch) -> (f32, f32) {
    let rect = canvas.get_bounding_client_rect();
    let scale_x = canvas.width() as f64 / rect.width();
    let scale_y = canvas.height() as f64 / rect.height();
    let cx = (touch.client_x() as f64 - rect.left()) * scale_x;
    let cy = (touch.client_y() as f64 - rect.top()) * scale_y;
    (cx as f32, cy as f32)
}

/// Load a texture through `LoopState`, splitting borrows around the await.
async fn load_texture(
    state: &Rc<RefCell<LoopState>>,
    url: &str,
    frame_w: u32,
    frame_h: u32,
    frame_count: u32,
) -> Result<TextureId, JsValue> {
    // Check cache first (short borrow, then release)
    {
        let guard = state.borrow();
        if let Some(id) = guard.texture_manager.get_cached(url) {
            return Ok(id);
        }
    }

    // Load image without holding any borrow on state
    let element = load_image(url).await?;

    log::info!(
        "Loaded sprite sheet: {url} ({}x{}, {frame_count} frames of {frame_w}x{frame_h})",
        element.natural_width(),
        element.natural_height()
    );

    // Register the loaded image (short borrow, then release)
    let id = {
        let mut guard = state.borrow_mut();
        guard
            .texture_manager
            .register(url, element, frame_w, frame_h, frame_count)
    };

    Ok(id)
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

    // Load arrow projectile
    {
        let url = format!("{}/Units/Blue Units/Archer/Arrow.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().arrow_texture = Some(tex_id);
    }

    // Load tilemap texture (576x384, loaded as single frame)
    {
        let url = format!("{}/Terrain/Tileset/Tilemap_color1.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 576, 384, 1).await?;
        loaded.borrow_mut().tilemap_texture = Some(tex_id);
    }

    // Load water background texture (64x64, single tile)
    {
        let url = format!("{}/Terrain/Tileset/Water Background color.png", ASSET_BASE);
        let tex_id = load_texture(state, &url, 64, 64, 1).await?;
        loaded.borrow_mut().water_texture = Some(tex_id);
    }

    Ok(())
}

fn render_frame(state: &mut LoopState, loaded: &LoadedTextures) -> Result<(), JsValue> {
    let ctx = &state.canvas2d.ctx;
    let canvas_w = state.canvas2d.width;
    let canvas_h = state.canvas2d.height;
    let game = &state.game;
    let tm = &state.texture_manager;

    let ts = TILE_SIZE as f64;

    // 1. Clear canvas with dark background
    ctx.set_fill_style_str("#1a1a26");
    ctx.fill_rect(0.0, 0.0, canvas_w, canvas_h);

    // 2. Apply camera transform
    let zoom = game.camera.zoom as f64;
    ctx.save();
    ctx.translate(canvas_w / 2.0, canvas_h / 2.0)?;
    ctx.scale(zoom, zoom)?;
    ctx.translate(-(game.camera.x as f64), -(game.camera.y as f64))?;

    // Visible tile range
    let (vl, vt, vr, vb) = game.camera.visible_rect();
    let min_gx = ((vl / TILE_SIZE).floor() as i32).max(0) as u32;
    let min_gy = ((vt / TILE_SIZE).floor() as i32).max(0) as u32;
    let max_gx = ((vr / TILE_SIZE).ceil() as i32).min(game.grid.width as i32) as u32;
    let max_gy = ((vb / TILE_SIZE).ceil() as i32).min(game.grid.height as i32) as u32;

    // 3. Draw background tiles
    draw_tiles(ctx, game, loaded, tm, min_gx, min_gy, max_gx, max_gy)?;

    // 4. Draw color overlays
    draw_color_overlays(ctx, game, min_gx, min_gy, max_gx, max_gy, ts)?;

    // 5. Draw foreground sprites (units, particles, projectiles)
    draw_foreground(ctx, game, loaded, tm)?;

    // Restore camera transform
    ctx.restore();

    Ok(())
}

fn draw_tiles(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
) -> Result<(), JsValue> {
    let ts = TILE_SIZE as f64;

    // Tilemap tiles (grass, hill, forest, rock)
    if let Some(tilemap_tex_id) = loaded.tilemap_texture {
        if let Some((img, _, _, _)) = tm.get_image(tilemap_tex_id) {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    let tile = game.grid.get(gx, gy);
                    if let Some((col, row)) = tile.tilemap_coords() {
                        let (sx, sy, sw, sh) = grid::tilemap_src_rect(col, row);
                        let dx = (gx as f64) * ts;
                        let dy = (gy as f64) * ts;
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, sx, sy, sw, sh, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
        }
    }

    // Water tiles (separate texture)
    if let Some(water_tex_id) = loaded.water_texture {
        if let Some((img, _, _, _)) = tm.get_image(water_tex_id) {
            for gy in min_gy..max_gy {
                for gx in min_gx..max_gx {
                    if game.grid.get(gx, gy) == TileKind::Water {
                        let dx = (gx as f64) * ts;
                        let dy = (gy as f64) * ts;
                        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            img, 0.0, 0.0, 64.0, 64.0, dx, dy, ts, ts,
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn draw_color_overlays(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    min_gx: u32,
    min_gy: u32,
    max_gx: u32,
    max_gy: u32,
    ts: f64,
) -> Result<(), JsValue> {
    // Forest overlay tint
    for gy in min_gy..max_gy {
        for gx in min_gx..max_gx {
            if game.grid.get(gx, gy) == TileKind::Forest {
                let dx = (gx as f64) * ts;
                let dy = (gy as f64) * ts;
                ctx.set_fill_style_str("rgba(0,38,0,0.4)");
                ctx.fill_rect(dx, dy, ts, ts);
            }
        }
    }

    if let Some(ref preview) = game.swipe_preview {
        // Swipe preview mode: draw preview path instead of move/attack targets
        draw_swipe_preview(ctx, preview, ts);
    } else {
        // Standard mode: draw move and attack target highlights
        for &(mx, my) in &game.move_targets {
            let dx = (mx as f64) * ts + 1.0;
            let dy = (my as f64) * ts + 1.0;
            let size = ts - 2.0;
            ctx.set_fill_style_str("rgba(51,153,255,0.3)");
            ctx.fill_rect(dx, dy, size, size);
        }

        for &target_id in &game.attack_targets {
            if let Some(unit) = game.find_unit(target_id) {
                let dx = (unit.grid_x as f64) * ts + 1.0;
                let dy = (unit.grid_y as f64) * ts + 1.0;
                let size = ts - 2.0;
                ctx.set_fill_style_str("rgba(255,51,51,0.4)");
                ctx.fill_rect(dx, dy, size, size);
            }
        }
    }

    // Highlight player unit's tile
    if let Some(player) = game.player_unit() {
        let dx = (player.grid_x as f64) * ts + 1.0;
        let dy = (player.grid_y as f64) * ts + 1.0;
        let size = ts - 2.0;
        ctx.set_fill_style_str("rgba(255,255,51,0.3)");
        ctx.fill_rect(dx, dy, size, size);
    }

    // HP bars for alive units
    for unit in &game.units {
        if !unit.alive {
            continue;
        }
        let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
        let bar_width = 48.0_f64;
        let bar_height = 6.0_f64;
        let bar_y = (wy as f64) - (TILE_SIZE as f64) * 0.45;
        let bar_x = (wx as f64) - bar_width / 2.0;

        // Background
        ctx.set_global_alpha(0.8);
        ctx.set_fill_style_str("rgb(51,51,51)");
        ctx.fill_rect(bar_x, bar_y - bar_height / 2.0, bar_width, bar_height);

        // Fill
        let hp_ratio = unit.hp as f64 / unit.stats.max_hp as f64;
        let fill_width = bar_width * hp_ratio;
        let fill_color = if hp_ratio > 0.5 {
            "rgb(51,204,51)"
        } else if hp_ratio > 0.25 {
            "rgb(230,179,26)"
        } else {
            "rgb(230,51,26)"
        };
        ctx.set_global_alpha(0.9);
        ctx.set_fill_style_str(fill_color);
        ctx.fill_rect(
            bar_x,
            bar_y - (bar_height - 2.0) / 2.0,
            fill_width,
            bar_height - 2.0,
        );

        ctx.set_global_alpha(1.0);
    }

    Ok(())
}

fn draw_swipe_preview(
    ctx: &web_sys::CanvasRenderingContext2d,
    preview: &SwipePreview,
    ts: f64,
) {
    let path_len = preview.path.len();
    for (i, &(px, py)) in preview.path.iter().enumerate() {
        let dx = (px as f64) * ts + 1.0;
        let dy = (py as f64) * ts + 1.0;
        let size = ts - 2.0;

        // Destination tile is brighter, intermediate tiles are dimmer
        if i == path_len - 1 && preview.attack_target.is_none() {
            ctx.set_fill_style_str("rgba(51,153,255,0.5)");
        } else {
            ctx.set_fill_style_str("rgba(51,153,255,0.2)");
        }
        ctx.fill_rect(dx, dy, size, size);
    }

    // Draw attack target highlight
    if let Some((ax, ay)) = preview.attack_target {
        let dx = (ax as f64) * ts + 1.0;
        let dy = (ay as f64) * ts + 1.0;
        let size = ts - 2.0;
        ctx.set_fill_style_str("rgba(255,51,51,0.5)");
        ctx.fill_rect(dx, dy, size, size);
    }
}

fn draw_foreground(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    loaded: &LoadedTextures,
    tm: &TextureManager,
) -> Result<(), JsValue> {
    // Unit sprites (sorted by Y for proper layering)
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

        if let Some((img, frame_w, frame_h, _)) = tm.get_image(tex_id) {
            let sheet = SpriteSheet {
                frame_width: frame_w,
                frame_height: frame_h,
                frame_count: unit.animation.frame_count,
            };
            let (sx, sy, sw, sh) = sheet.frame_src_rect(unit.animation.current_frame);
            let (wx, wy) = grid::grid_to_world(unit.grid_x, unit.grid_y);
            let sprite_size = unit.kind.frame_size() as f64;

            let opacity = if unit.alive {
                1.0
            } else {
                (unit.death_fade / DEATH_FADE_DURATION).clamp(0.0, 1.0) as f64
            };

            let dx = (wx as f64) - sprite_size / 2.0;
            let dy = (wy as f64) - sprite_size / 2.0;

            draw_sprite(
                ctx,
                img,
                sx,
                sy,
                sw,
                sh,
                dx,
                dy,
                sprite_size,
                sprite_size,
                unit.facing == Facing::Left,
                opacity,
            )?;
        }
    }

    // Particle sprites
    for particle in &game.particles {
        let filename = particle.kind.asset_filename();
        let tex_id = match loaded.particle_textures.get(filename) {
            Some(&id) => id,
            None => continue,
        };

        if let Some((img, frame_w, frame_h, _)) = tm.get_image(tex_id) {
            let sheet = SpriteSheet {
                frame_width: frame_w,
                frame_height: frame_h,
                frame_count: particle.animation.frame_count,
            };
            let (sx, sy, sw, sh) = sheet.frame_src_rect(particle.animation.current_frame);
            let size = particle.kind.frame_size() as f64;
            let dx = (particle.world_x as f64) - size / 2.0;
            let dy = (particle.world_y as f64) - size / 2.0;

            draw_sprite(ctx, img, sx, sy, sw, sh, dx, dy, size, size, false, 1.0)?;
        }
    }

    // Arrow projectiles
    if let Some(&arrow_tex_id) = loaded.arrow_texture.as_ref() {
        if let Some((img, _, _, _)) = tm.get_image(arrow_tex_id) {
            for proj in &game.projectiles {
                let flip = proj.angle.abs() > std::f32::consts::FRAC_PI_2;
                let draw_angle = if flip {
                    (proj.angle as f64) + std::f64::consts::PI
                } else {
                    proj.angle as f64
                };

                ctx.save();
                ctx.translate(proj.current_x as f64, proj.current_y as f64)?;
                ctx.rotate(draw_angle)?;
                // Draw arrow centered at origin after rotation
                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img, 0.0, 0.0, 64.0, 64.0, -32.0, -32.0, 64.0, 64.0,
                )?;
                ctx.restore();
            }
        }
    }

    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}

struct LoopState {
    canvas2d: Canvas2d,
    game: Game,
    texture_manager: TextureManager,
    last_time: Option<f64>,
}
