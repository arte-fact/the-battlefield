use super::gpu::Gpu;
use crate::sprite::SpriteSheet;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

/// Unique key for loaded textures.
pub type TextureId = u32;

struct LoadedTexture {
    pub bind_group: wgpu::BindGroup,
    pub sprite_sheet: SpriteSheet,
}

pub struct TextureManager {
    textures: HashMap<TextureId, LoadedTexture>,
    next_id: TextureId,
    url_to_id: HashMap<String, TextureId>,
}

impl TextureManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
            next_id: 1,
            url_to_id: HashMap::new(),
        }
    }

    /// Load a sprite sheet from URL and register it. Returns the texture ID.
    pub async fn load(
        &mut self,
        gpu: &Gpu,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        url: &str,
        frame_width: u32,
        frame_height: u32,
        frame_count: u32,
    ) -> Result<TextureId, JsValue> {
        // Return cached if already loaded
        if let Some(&id) = self.url_to_id.get(url) {
            return Ok(id);
        }

        let sprite_sheet =
            SpriteSheet::from_url(url, frame_width, frame_height, frame_count).await?;

        let texture = create_texture(gpu, &sprite_sheet);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_bind_group"),
            layout: texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        let id = self.next_id;
        self.next_id += 1;
        self.textures.insert(
            id,
            LoadedTexture {
                bind_group,
                sprite_sheet,
            },
        );
        self.url_to_id.insert(url.to_string(), id);

        Ok(id)
    }

    pub fn get_bind_group(&self, id: TextureId) -> Option<&wgpu::BindGroup> {
        self.textures.get(&id).map(|t| &t.bind_group)
    }

    pub fn get_sprite_sheet(&self, id: TextureId) -> Option<&SpriteSheet> {
        self.textures.get(&id).map(|t| &t.sprite_sheet)
    }
}

fn create_texture(gpu: &Gpu, sprite_sheet: &SpriteSheet) -> wgpu::Texture {
    let size = wgpu::Extent3d {
        width: sprite_sheet.image_width,
        height: sprite_sheet.image_height,
        depth_or_array_layers: 1,
    };

    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sprite_texture"),
        size,
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
        &sprite_sheet.image_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * sprite_sheet.image_width),
            rows_per_image: Some(sprite_sheet.image_height),
        },
        size,
    );

    texture
}
