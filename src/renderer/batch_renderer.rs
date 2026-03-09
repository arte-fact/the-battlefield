use super::gpu::Gpu;
use super::texture_manager::TextureId;
use super::vertex::{ColorInstance, SpriteInstance, Vertex};
use wasm_bindgen::prelude::*;

const MAX_INSTANCES: usize = 8192;

/// A draw command: render N instances with a given texture.
pub struct SpriteBatch {
    pub texture_id: TextureId,
    pub instances: Vec<SpriteInstance>,
}

pub struct BatchRenderer {
    // Sprite (textured) pipeline
    sprite_pipeline: wgpu::RenderPipeline,
    // Color (solid) pipeline
    color_pipeline: wgpu::RenderPipeline,
    // Shared quad vertex/index buffers
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    // Camera uniform
    camera_bind_group_layout: wgpu::BindGroupLayout,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    // Texture bind group layout (shared with TextureManager)
    pub texture_bind_group_layout: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
    // Dynamic instance buffer
    sprite_instance_buffer: wgpu::Buffer,
    color_instance_buffer: wgpu::Buffer,
}

impl BatchRenderer {
    pub fn new(gpu: &Gpu) -> Result<Self, JsValue> {
        // Unit quad: centered at origin, extends -0.5 to +0.5
        let vertices: [Vertex; 4] = [
            Vertex {
                position: [-0.5, -0.5],
                tex_coords: [0.0, 1.0],
            },
            Vertex {
                position: [0.5, -0.5],
                tex_coords: [1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5],
                tex_coords: [1.0, 0.0],
            },
            Vertex {
                position: [-0.5, 0.5],
                tex_coords: [0.0, 0.0],
            },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];

        use wgpu::util::DeviceExt;
        let vertex_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("quad_vertex_buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let index_buffer = gpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("quad_index_buffer"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });

        // Camera uniform
        let camera_bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("camera_bind_group_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let camera_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform_buffer"),
            size: 64, // mat4x4<f32> = 16 floats = 64 bytes
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bind_group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Texture bind group layout (for sprite pipeline)
        let texture_bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("texture_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                multisampled: false,
                                view_dimension: wgpu::TextureViewDimension::D2,
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Sprite pipeline
        let sprite_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("sprite_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("sprite.wgsl").into()),
            });

        let sprite_pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("sprite_pipeline_layout"),
                    bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
                    push_constant_ranges: &[],
                });

        let sprite_pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sprite_pipeline"),
                layout: Some(&sprite_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &sprite_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc(), SpriteInstance::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &sprite_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Color pipeline
        let color_shader = gpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("color_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("color.wgsl").into()),
            });

        let color_pipeline_layout =
            gpu.device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("color_pipeline_layout"),
                    bind_group_layouts: &[&camera_bind_group_layout],
                    push_constant_ranges: &[],
                });

        let color_pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("color_pipeline"),
                layout: Some(&color_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &color_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Vertex::desc(), ColorInstance::desc()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &color_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_config.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Instance buffers
        let sprite_instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sprite_instance_buffer"),
            size: (MAX_INSTANCES * std::mem::size_of::<SpriteInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let color_instance_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("color_instance_buffer"),
            size: (MAX_INSTANCES * std::mem::size_of::<ColorInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            sprite_pipeline,
            color_pipeline,
            vertex_buffer,
            index_buffer,
            camera_bind_group_layout,
            camera_buffer,
            camera_bind_group,
            texture_bind_group_layout,
            sampler,
            sprite_instance_buffer,
            color_instance_buffer,
        })
    }

    pub fn update_camera(&self, gpu: &Gpu, view_proj: &[f32; 16]) {
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(view_proj));
    }

    pub fn render(
        &self,
        gpu: &Gpu,
        bg_sprite_batches: &[SpriteBatch],
        color_instances: &[ColorInstance],
        fg_sprite_batches: &[SpriteBatch],
        texture_manager: &super::TextureManager,
    ) -> Result<(), JsValue> {
        let output = gpu
            .surface
            .get_current_texture()
            .map_err(|e| JsValue::from_str(&format!("Failed to get surface texture: {e}")))?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // 1. Background sprites (tilemap tiles)
            self.draw_sprite_batches(gpu, &mut render_pass, bg_sprite_batches, texture_manager);

            // 2. Colored instances (highlights, HP bars)
            if !color_instances.is_empty() {
                let count = color_instances.len().min(MAX_INSTANCES);
                gpu.queue.write_buffer(
                    &self.color_instance_buffer,
                    0,
                    bytemuck::cast_slice(&color_instances[..count]),
                );

                render_pass.set_pipeline(&self.color_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                render_pass.set_vertex_buffer(1, self.color_instance_buffer.slice(..));
                render_pass
                    .set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                render_pass.draw_indexed(0..6, 0, 0..count as u32);
            }

            // 3. Foreground sprites (units, particles, projectiles)
            self.draw_sprite_batches(gpu, &mut render_pass, fg_sprite_batches, texture_manager);
        }

        gpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn draw_sprite_batches<'a>(
        &'a self,
        gpu: &Gpu,
        render_pass: &mut wgpu::RenderPass<'a>,
        batches: &[SpriteBatch],
        texture_manager: &'a super::TextureManager,
    ) {
        for batch in batches {
            if batch.instances.is_empty() {
                continue;
            }
            let bind_group = match texture_manager.get_bind_group(batch.texture_id) {
                Some(bg) => bg,
                None => continue,
            };

            let count = batch.instances.len().min(MAX_INSTANCES);
            gpu.queue.write_buffer(
                &self.sprite_instance_buffer,
                0,
                bytemuck::cast_slice(&batch.instances[..count]),
            );

            render_pass.set_pipeline(&self.sprite_pipeline);
            render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
            render_pass.set_bind_group(1, bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.sprite_instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..6, 0, 0..count as u32);
        }
    }
}
