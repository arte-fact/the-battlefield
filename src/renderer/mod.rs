mod canvas2d;
mod texture_manager;
mod traits;

pub use canvas2d::Canvas2dRenderer;
pub use texture_manager::{load_image, TextureId};
pub use traits::Renderer;

// Re-exported for downstream access via Canvas2dRenderer::texture_manager().
#[allow(unused_imports)]
pub use texture_manager::TextureManager;
#[allow(unused_imports)]
pub use traits::TextureInfo;
