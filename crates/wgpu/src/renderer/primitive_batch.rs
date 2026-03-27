//! Batched primitive renderer — filled rects, circles, lines.

use crate::gpu::{GpuContext, PrimitiveVertex};
use wgpu::util::DeviceExt;

/// Accumulates colored primitives and renders them in one draw call.
pub struct PrimitiveBatch {
    vertices: Vec<PrimitiveVertex>,
    indices: Vec<u32>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
}

impl Default for PrimitiveBatch {
    fn default() -> Self {
        Self::new()
    }
}

impl PrimitiveBatch {
    pub fn new() -> Self {
        Self {
            vertices: Vec::with_capacity(4000),
            indices: Vec::with_capacity(6000),
            vertex_buffer: None,
            index_buffer: None,
        }
    }

    pub fn begin(&mut self) {
        self.vertices.clear();
        self.indices.clear();
    }

    /// Draw a filled rectangle.
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[
            PrimitiveVertex {
                position: [x, y],
                color,
            },
            PrimitiveVertex {
                position: [x + w, y],
                color,
            },
            PrimitiveVertex {
                position: [x + w, y + h],
                color,
            },
            PrimitiveVertex {
                position: [x, y + h],
                color,
            },
        ]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Draw a filled circle (tessellated into a triangle fan).
    pub fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let segments = ((radius * 0.5).ceil() as u32).clamp(12, 64);
        let base = self.vertices.len() as u32;

        // Center vertex
        self.vertices.push(PrimitiveVertex {
            position: [cx, cy],
            color,
        });

        for i in 0..=segments {
            let angle = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
            self.vertices.push(PrimitiveVertex {
                position: [cx + radius * angle.cos(), cy + radius * angle.sin()],
                color,
            });
        }

        for i in 0..segments {
            self.indices
                .extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
        }
    }

    /// Draw a circle outline (approximated with thin triangles).
    pub fn stroke_circle(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        thickness: f32,
        color: [f32; 4],
    ) {
        let segments = ((radius * 0.5).ceil() as u32).clamp(12, 64);
        let inner = radius - thickness * 0.5;
        let outer = radius + thickness * 0.5;
        let base = self.vertices.len() as u32;

        for i in 0..=segments {
            let angle = 2.0 * std::f32::consts::PI * i as f32 / segments as f32;
            let cos = angle.cos();
            let sin = angle.sin();
            self.vertices.push(PrimitiveVertex {
                position: [cx + inner * cos, cy + inner * sin],
                color,
            });
            self.vertices.push(PrimitiveVertex {
                position: [cx + outer * cos, cy + outer * sin],
                color,
            });
        }

        for i in 0..segments {
            let i0 = base + i * 2;
            self.indices
                .extend_from_slice(&[i0, i0 + 1, i0 + 3, i0, i0 + 3, i0 + 2]);
        }
    }

    /// Draw a line (as a thin rectangle).
    pub fn draw_line(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        thickness: f32,
        color: [f32; 4],
    ) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 0.001 {
            return;
        }
        let nx = -dy / len * thickness * 0.5;
        let ny = dx / len * thickness * 0.5;

        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&[
            PrimitiveVertex {
                position: [x0 + nx, y0 + ny],
                color,
            },
            PrimitiveVertex {
                position: [x0 - nx, y0 - ny],
                color,
            },
            PrimitiveVertex {
                position: [x1 - nx, y1 - ny],
                color,
            },
            PrimitiveVertex {
                position: [x1 + nx, y1 + ny],
                color,
            },
        ]);
        self.indices
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Upload buffers to GPU.
    pub fn finish(&mut self, gpu: &GpuContext) {
        if self.vertices.is_empty() {
            return;
        }

        self.vertex_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("prim_vb"),
                contents: bytemuck::cast_slice(&self.vertices),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        self.index_buffer = Some(gpu.device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("prim_ib"),
                contents: bytemuck::cast_slice(&self.indices),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));
    }

    /// Issue draw call (sets pipeline + camera bind group 0).
    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, gpu: &'a GpuContext) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(&gpu.primitive_pipeline);
        pass.set_bind_group(0, &gpu.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
    }

    /// Issue draw call assuming bind group 0 is already set by the caller.
    pub fn render_without_bind_group<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        gpu: &'a GpuContext,
    ) {
        let (Some(vb), Some(ib)) = (&self.vertex_buffer, &self.index_buffer) else {
            return;
        };

        pass.set_pipeline(&gpu.primitive_pipeline);
        pass.set_vertex_buffer(0, vb.slice(..));
        pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.indices.len() as u32, 0, 0..1);
    }
}
