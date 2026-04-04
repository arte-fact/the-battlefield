//! Batched sprite quad renderer — accumulates quads and flushes per texture.

use crate::gpu::{GpuContext, SpriteVertex};
use wgpu::util::DeviceExt;

/// Handle identifying a loaded GPU texture.
pub type TextureId = usize;

/// A loaded GPU texture with its bind group.
pub struct GpuTexture {
    pub _texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub bind_group: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
    /// Downscale factor applied on upload (1.0 = original, 0.5 = halved once).
    pub scale: f32,
}

/// Accumulates textured quads and draws them in batches grouped by texture.
pub struct SpriteBatch {
    vertices: Vec<SpriteVertex>,
    indices: Vec<u32>,
    current_texture: Option<TextureId>,
    draw_calls: Vec<BatchDrawCall>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
}

struct BatchDrawCall {
    texture_id: TextureId,
    index_start: u32,
    index_count: u32,
}

impl Default for SpriteBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl SpriteBatch {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(8000),
            indices: Vec::with_capacity(12000),
            current_texture: None,
            draw_calls: Vec::with_capacity(64),
            vertex_buffer: None,
            index_buffer: None,
        }
    }

    /// Reset for a new frame.
    pub fn begin(&mut self) {
        self.vertices.clear();
        self.indices.clear();
        self.draw_calls.clear();
        self.current_texture = None;
    }

    /// Push a textured quad.
    ///
    /// - `src`: (x, y, w, h) in texels within the texture
    /// - `dst`: (x, y, w, h) in screen/world pixels
    /// - `tex_size`: (width, height) of the full texture
    /// - `flip`: horizontal flip
    /// - `color_mod`: RGBA modulation (alpha in .a)
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sprite(
        &mut self,
        texture_id: TextureId,
        src: [f32; 4],
        dst: [f32; 4],
        tex_size: (u32, u32),
        flip: bool,
        color_mod: [f32; 4],
    ) {
        self.draw_sprite_rotated(texture_id, src, dst, tex_size, flip, color_mod, 0.0);
    }

    /// Push a textured quad with rotation (radians around center).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sprite_rotated(
        &mut self,
        texture_id: TextureId,
        src: [f32; 4],
        dst: [f32; 4],
        tex_size: (u32, u32),
        flip: bool,
        color_mod: [f32; 4],
        rotation: f32,
    ) {
        // Record batch boundary if texture changed
        if self.current_texture != Some(texture_id) {
            self.flush_batch();
            self.current_texture = Some(texture_id);
        }

        let [sx, sy, sw, sh] = src;
        let [dx, dy, dw, dh] = dst;
        let tw = tex_size.0 as f32;
        let th = tex_size.1 as f32;

        // UV coordinates
        let mut u0 = sx / tw;
        let mut u1 = (sx + sw) / tw;
        let v0 = sy / th;
        let v1 = (sy + sh) / th;

        if flip {
            std::mem::swap(&mut u0, &mut u1);
        }

        let base = self.vertices.len() as u32;

        if rotation.abs() < 0.001 {
            // No rotation — axis-aligned quad
            let x0 = dx;
            let y0 = dy;
            let x1 = dx + dw;
            let y1 = dy + dh;

            self.vertices.extend_from_slice(&[
                SpriteVertex {
                    position: [x0, y0],
                    uv: [u0, v0],
                    color_mod,
                },
                SpriteVertex {
                    position: [x1, y0],
                    uv: [u1, v0],
                    color_mod,
                },
                SpriteVertex {
                    position: [x1, y1],
                    uv: [u1, v1],
                    color_mod,
                },
                SpriteVertex {
                    position: [x0, y1],
                    uv: [u0, v1],
                    color_mod,
                },
            ]);
        } else {
            // Rotated quad around center
            let cx = dx + dw * 0.5;
            let cy = dy + dh * 0.5;
            let hw = dw * 0.5;
            let hh = dh * 0.5;
            let cos = rotation.cos();
            let sin = rotation.sin();

            let rotate = |lx: f32, ly: f32| -> [f32; 2] {
                [cx + lx * cos - ly * sin, cy + lx * sin + ly * cos]
            };

            self.vertices.extend_from_slice(&[
                SpriteVertex {
                    position: rotate(-hw, -hh),
                    uv: [u0, v0],
                    color_mod,
                },
                SpriteVertex {
                    position: rotate(hw, -hh),
                    uv: [u1, v0],
                    color_mod,
                },
                SpriteVertex {
                    position: rotate(hw, hh),
                    uv: [u1, v1],
                    color_mod,
                },
                SpriteVertex {
                    position: rotate(-hw, hh),
                    uv: [u0, v1],
                    color_mod,
                },
            ]);
        }

        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Record the current batch if any vertices are pending for current texture.
    fn flush_batch(&mut self) {
        if let Some(tex_id) = self.current_texture {
            let idx_end = self.indices.len() as u32;
            let last_end = self
                .draw_calls
                .last()
                .map(|dc| dc.index_start + dc.index_count)
                .unwrap_or(0);
            if idx_end > last_end {
                self.draw_calls.push(BatchDrawCall {
                    texture_id: tex_id,
                    index_start: last_end,
                    index_count: idx_end - last_end,
                });
            }
        }
    }

    /// Finish accumulating and upload buffers to GPU.
    pub fn finish(&mut self, gpu: &GpuContext) {
        self.flush_batch();

        if self.vertices.is_empty() {
            return;
        }

        self.vertex_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("sprite_vb"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        self.index_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("sprite_ib"),
                contents: bytemuck::cast_slice(&self.indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));
    }

    /// Issue all draw calls into the given render pass.
    /// `textures` is the primary texture list (assets). `extra_textures` provides
    /// additional textures (e.g. text cache) for IDs beyond the primary range.
    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        gpu: &'a GpuContext,
        textures: &'a [GpuTexture],
        extra_textures: &'a [GpuTexture],
    ) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(&gpu.sprite_pipeline);
        pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);

        for dc in &self.draw_calls {
            let tex = textures
                .get(dc.texture_id)
                .or_else(|| extra_textures.get(dc.texture_id.wrapping_sub(textures.len())));
            if let Some(tex) = tex {
                pass.set_bind_group(1, &tex.bind_group, &[]);
                pass.draw_indexed(dc.index_start..dc.index_start + dc.index_count, 0, 0..1);
            }
        }
    }

    /// Issue all draw calls using a custom pipeline instead of the default sprite pipeline.
    pub fn render_with_pipeline<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        gpu: &'a GpuContext,
        textures: &'a [GpuTexture],
        extra_textures: &'a [GpuTexture],
        pipeline: &'a wgpu::RenderPipeline,
    ) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);

        for dc in &self.draw_calls {
            let tex = textures
                .get(dc.texture_id)
                .or_else(|| extra_textures.get(dc.texture_id.wrapping_sub(textures.len())));
            if let Some(tex) = tex {
                pass.set_bind_group(1, &tex.bind_group, &[]);
                pass.draw_indexed(dc.index_start..dc.index_start + dc.index_count, 0, 0..1);
            }
        }
    }

    /// Render assuming bind group 0 (camera) is already set by the caller.
    pub fn render_without_camera<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        gpu: &'a GpuContext,
        textures: &'a [GpuTexture],
        extra_textures: &'a [GpuTexture],
    ) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(&gpu.sprite_pipeline);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);

        for dc in &self.draw_calls {
            let tex = textures
                .get(dc.texture_id)
                .or_else(|| extra_textures.get(dc.texture_id.wrapping_sub(textures.len())));
            if let Some(tex) = tex {
                pass.set_bind_group(1, &tex.bind_group, &[]);
                pass.draw_indexed(dc.index_start..dc.index_start + dc.index_count, 0, 0..1);
            }
        }
    }
}
