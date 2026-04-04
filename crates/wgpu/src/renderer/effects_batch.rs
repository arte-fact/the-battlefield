//! Batched procedural-circle renderer for zone overlays and player aim.

use crate::gpu::{EffectVertex, GpuContext};
use wgpu::util::DeviceExt;

pub struct EffectsBatch {
    vertices: Vec<EffectVertex>,
    indices: Vec<u32>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
}

impl Default for EffectsBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl EffectsBatch {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(64),
            indices: Vec::with_capacity(96),
            vertex_buffer: None,
            index_buffer: None,
        }
    }

    pub fn begin(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Emit a quad covering a circle at (cx, cy) with given radius.
    /// `kind`: 0.0 = zone capture, 1.0 = player aim, 2.0 = order pulse.
    /// `extra`: passed as params.w (e.g. capturing flag for zones).
    pub fn draw_circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        color: [f32; 4],
        time: f32,
        kind: f32,
        extra: f32,
    ) {
        let r = radius * 1.05; // slight overshoot so edge doesn't clip
        let base = self.vertices.len() as u32;
        let params = [time, kind, radius, extra];

        self.vertices.extend_from_slice(&[
            EffectVertex {
                position: [cx - r, cy - r],
                uv: [-1.0, -1.0],
                color,
                params,
            },
            EffectVertex {
                position: [cx + r, cy - r],
                uv: [1.0, -1.0],
                color,
                params,
            },
            EffectVertex {
                position: [cx + r, cy + r],
                uv: [1.0, 1.0],
                color,
                params,
            },
            EffectVertex {
                position: [cx - r, cy + r],
                uv: [-1.0, 1.0],
                color,
                params,
            },
        ]);

        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    pub fn finish(&mut self, gpu: &GpuContext) {
        if self.vertices.is_empty() {
            return;
        }

        self.vertex_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("effects_vb"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        self.index_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("effects_ib"),
                contents: bytemuck::cast_slice(&self.indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, gpu: &'a GpuContext) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(&gpu.effects_pipeline);
        pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
    }
}
