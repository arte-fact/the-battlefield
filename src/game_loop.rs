use crate::renderer::{Gpu, SpriteRenderer};
use crate::sprite::{AnimationState, SpriteSheet};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

const ANIMATION_FPS: f64 = 10.0;

pub fn run(
    gpu: Gpu,
    sprite_renderer: SpriteRenderer,
    sprite_sheet: SpriteSheet,
) -> Result<(), JsValue> {
    let state = Rc::new(RefCell::new(GameState {
        gpu,
        sprite_renderer,
        sprite_sheet,
        animation: AnimationState::new(8, ANIMATION_FPS),
        last_time: None,
    }));

    let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();

    let state_clone = state.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move |timestamp: f64| {
        let mut s = state_clone.borrow_mut();
        let dt = match s.last_time {
            Some(last) => (timestamp - last) / 1000.0,
            None => 0.0,
        };
        s.last_time = Some(timestamp);

        s.animation.update(dt);

        if let Err(e) = s
            .sprite_renderer
            .render(&s.gpu, &s.animation, &s.sprite_sheet)
        {
            log::error!("Render error: {:?}", e);
        }

        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut(f64)>));

    request_animation_frame(g.borrow().as_ref().unwrap());
    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .expect("no window")
        .request_animation_frame(f.as_ref().unchecked_ref())
        .expect("should register `requestAnimationFrame`");
}

struct GameState {
    gpu: Gpu,
    sprite_renderer: SpriteRenderer,
    sprite_sheet: SpriteSheet,
    animation: AnimationState,
    last_time: Option<f64>,
}
