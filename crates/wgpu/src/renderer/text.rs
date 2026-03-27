//! Text rendering — rusttype glyph rasterization → GPU texture → sprite batch.
//!
//! Rasterized strings are cached as GPU textures to avoid per-frame uploads.

use crate::gpu::GpuContext;
use crate::renderer::sprite_batch::{GpuTexture, SpriteBatch, TextureId};
use rusttype::{point, Font, Scale};
use std::collections::HashMap;

/// Cached text entry.
struct CachedText {
    tex_id: TextureId,
    width: u32,
    height: u32,
}

/// Key for the text cache.
#[derive(Hash, Eq, PartialEq)]
struct CacheKey {
    text: String,
    size_tenths: u16, // size * 10, to avoid float hashing
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

pub struct TextRenderer {
    font: Font<'static>,
    cache: HashMap<CacheKey, CachedText>,
    /// Textures owned by the text cache (appended to the main texture vec).
    cache_textures: Vec<GpuTexture>,
    /// Offset into the main texture vec where cache textures start.
    cache_base_id: usize,
}

impl TextRenderer {
    pub fn new(cache_base_id: usize) -> Self {
        let font_data = battlefield_assets::get("assets/Uncial.ttf")
            .expect("Font not found in embedded assets");
        let font = Font::try_from_vec(font_data.to_vec()).expect("Failed to parse font");
        Self {
            font,
            cache: HashMap::new(),
            cache_textures: Vec::new(),
            cache_base_id,
        }
    }

    /// Draw text centered at screen position `(cx, cy)`.
    pub fn draw_text_centered(
        &mut self,
        batch: &mut SpriteBatch,
        gpu: &GpuContext,
        text: &str,
        cx: f32,
        cy: f32,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
        if text.is_empty() {
            return;
        }
        let entry = self.get_or_create(gpu, text, size, r, g, b, a);
        let Some(entry) = entry else { return };
        let w = entry.width as f32;
        let h = entry.height as f32;
        let tex_id = entry.tex_id;
        let tex = &self.cache_textures[tex_id - self.cache_base_id];

        batch.draw_sprite(
            tex_id,
            [0.0, 0.0, w, h],
            [cx - w * 0.5, cy - h * 0.5, w, h],
            (tex.width, tex.height),
            false,
            [1.0, 1.0, 1.0, 1.0],
        );
    }

    /// Draw text left-aligned at `(x, y)` (top-left corner).
    pub fn draw_text(
        &mut self,
        batch: &mut SpriteBatch,
        gpu: &GpuContext,
        text: &str,
        x: f32,
        y: f32,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) {
        if text.is_empty() {
            return;
        }
        let entry = self.get_or_create(gpu, text, size, r, g, b, a);
        let Some(entry) = entry else { return };
        let w = entry.width as f32;
        let h = entry.height as f32;
        let tex_id = entry.tex_id;
        let tex = &self.cache_textures[tex_id - self.cache_base_id];

        batch.draw_sprite(
            tex_id,
            [0.0, 0.0, w, h],
            [x, y, w, h],
            (tex.width, tex.height),
            false,
            [1.0, 1.0, 1.0, 1.0],
        );
    }

    /// Measure text dimensions without rendering.
    pub fn measure_text(&self, text: &str, size: f32) -> (u32, u32) {
        let scale = Scale::uniform(size);
        let v = self.font.v_metrics(scale);
        let glyphs: Vec<_> = self
            .font
            .layout(text, scale, point(0.0, v.ascent))
            .collect();
        if glyphs.is_empty() {
            return (0, 0);
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
        ((max_x - min_x) as u32, (v.ascent - v.descent).ceil() as u32)
    }

    /// Get all cache textures for rendering (the sprite batch references these by TextureId).
    pub fn textures(&self) -> &[GpuTexture] {
        &self.cache_textures
    }

    /// Flush cache if it gets too large.
    pub fn maybe_flush(&mut self) {
        if self.cache.len() > 256 {
            self.cache.clear();
            self.cache_textures.clear();
        }
    }

    fn get_or_create(
        &mut self,
        gpu: &GpuContext,
        text: &str,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) -> Option<&CachedText> {
        let key = CacheKey {
            text: text.to_string(),
            size_tenths: (size * 10.0) as u16,
            r,
            g,
            b,
            a,
        };

        if !self.cache.contains_key(&key) {
            let (pixels, w, h) = self.rasterize(text, size, r, g, b, a)?;
            let tex_id = self.cache_base_id + self.cache_textures.len();
            let gpu_tex = upload_text_texture(gpu, &pixels, w, h);
            self.cache_textures.push(gpu_tex);
            self.cache.insert(
                key.clone(),
                CachedText {
                    tex_id,
                    width: w,
                    height: h,
                },
            );
        }

        self.cache.get(&key)
    }

    fn rasterize(
        &self,
        text: &str,
        size: f32,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) -> Option<(Vec<u8>, u32, u32)> {
        let scale = Scale::uniform(size);
        let v = self.font.v_metrics(scale);
        let glyphs: Vec<_> = self
            .font
            .layout(text, scale, point(0.0, v.ascent))
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
        let text_h = (v.ascent - v.descent).ceil() as u32;
        if text_w == 0 || text_h == 0 {
            return None;
        }

        let mut pixels = vec![0u8; (text_w * text_h * 4) as usize];
        for glyph in &glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, coverage| {
                    let px = (bb.min.x - min_x) as u32 + gx;
                    let py = bb.min.y as u32 + gy;
                    if px < text_w && py < text_h {
                        let idx = ((py * text_w + px) * 4) as usize;
                        let alpha = (coverage * a as f32) as u8;
                        pixels[idx] = r;
                        pixels[idx + 1] = g;
                        pixels[idx + 2] = b;
                        pixels[idx + 3] = alpha;
                    }
                });
            }
        }

        Some((pixels, text_w, text_h))
    }
}

fn upload_text_texture(gpu: &GpuContext, pixels: &[u8], w: u32, h: u32) -> GpuTexture {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("text"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
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
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = gpu.create_texture_bind_group(&view, &gpu.nearest_sampler);

    GpuTexture {
        _texture: texture,
        view,
        bind_group,
        width: w,
        height: h,
    }
}

/// Derive Clone + Hash for CacheKey
impl Clone for CacheKey {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            size_tenths: self.size_tenths,
            r: self.r,
            g: self.g,
            b: self.b,
            a: self.a,
        }
    }
}
