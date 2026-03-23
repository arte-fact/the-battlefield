use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

/// Unique key for loaded textures.
pub type TextureId = u32;

struct LoadedImage {
    pub element: web_sys::HtmlImageElement,
    pub frame_width: u32,
    pub frame_height: u32,
    pub frame_count: u32,
}

pub struct TextureManager {
    images: HashMap<TextureId, LoadedImage>,
    next_id: TextureId,
    url_to_id: HashMap<String, TextureId>,
}

impl TextureManager {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
            next_id: 1,
            url_to_id: HashMap::new(),
        }
    }

    /// Check if a URL is already cached; return its TextureId if so.
    pub fn get_cached(&self, url: &str) -> Option<TextureId> {
        self.url_to_id.get(url).copied()
    }

    /// Register a pre-loaded image element. Returns the texture ID.
    pub fn register(
        &mut self,
        url: &str,
        element: web_sys::HtmlImageElement,
        frame_width: u32,
        frame_height: u32,
        frame_count: u32,
    ) -> TextureId {
        let id = self.next_id;
        self.next_id += 1;
        self.images.insert(
            id,
            LoadedImage {
                element,
                frame_width,
                frame_height,
                frame_count,
            },
        );
        self.url_to_id.insert(url.to_string(), id);
        id
    }

    /// Get the HtmlImageElement and frame metadata for a texture.
    pub fn get_image(&self, id: TextureId) -> Option<(&web_sys::HtmlImageElement, u32, u32, u32)> {
        self.images.get(&id).map(|img| {
            (
                &img.element,
                img.frame_width,
                img.frame_height,
                img.frame_count,
            )
        })
    }
}

/// Load an image by creating an HtmlImageElement and waiting for onload.
pub async fn load_image(url: &str) -> Result<web_sys::HtmlImageElement, JsValue> {
    let img = web_sys::HtmlImageElement::new()?;

    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve_clone = resolve.clone();
        let onload = Closure::once(Box::new(move || {
            let _ = resolve_clone.call0(&JsValue::NULL);
        }) as Box<dyn FnOnce()>);
        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();

        let onerror = Closure::once(Box::new(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("Image load failed"));
        }) as Box<dyn FnOnce()>);
        img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
    });

    img.set_src(url);
    wasm_bindgen_futures::JsFuture::from(promise).await?;

    Ok(img)
}
