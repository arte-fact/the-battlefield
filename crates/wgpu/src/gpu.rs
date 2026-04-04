//! wgpu device, surface, pipeline initialization.

use std::sync::Arc;
use wgpu::util::DeviceExt;

// ─────────────────────────────────────────────────────────────────────────────
// Vertex types
// ─────────────────────────────────────────────────────────────────────────────

/// Vertex for textured sprite quads.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SpriteVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color_mod: [f32; 4], // RGBA tint + alpha in .a
}

impl SpriteVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Vertex for colored primitives (rects, circles, lines).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PrimitiveVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

impl PrimitiveVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Vertex for procedural effect circles (zone overlays, player aim).
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct EffectVertex {
    pub position: [f32; 2], // World-space quad corner
    pub uv: [f32; 2],       // -1..1 from circle center
    pub color: [f32; 4],    // RGBA
    pub params: [f32; 4],   // [time, effect_kind, _, _]
}

impl EffectVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
        2 => Float32x4,
        3 => Float32x4,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Camera uniform
// ─────────────────────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    pub view_proj: [f32; 16],
}

impl CameraUniform {
    /// Build an orthographic projection for screen-pixel coordinates.
    /// (0,0) = top-left, (w,h) = bottom-right. No camera transform.
    pub fn screen_ortho(w: f32, h: f32) -> Self {
        #[rustfmt::skip]
        let view_proj = [
            2.0 / w,  0.0,       0.0, 0.0,
            0.0,     -2.0 / h,   0.0, 0.0,
            0.0,      0.0,       1.0, 0.0,
           -1.0,      1.0,       0.0, 1.0,
        ];
        Self { view_proj }
    }

    /// Build an orthographic projection that applies camera transform.
    /// Maps world coordinates to clip space via zoom + offset.
    pub fn world_camera(cam: &battlefield_core::camera::Camera) -> Self {
        let vw = cam.viewport_w;
        let vh = cam.viewport_h;
        let ox = (vw * 0.5 - cam.x * cam.zoom).round();
        let oy = (vh * 0.5 - cam.y * cam.zoom).round();
        let z = cam.zoom;

        // World → screen: sx = wx * zoom + ox, sy = wy * zoom + oy
        // Screen → clip: cx = sx * 2/vw - 1, cy = 1 - sy * 2/vh
        // Combined: cx = wx * (2*z/vw) + (2*ox/vw - 1)
        //           cy = wx * (-2*z/vh) + (1 - 2*oy/vh)
        let sx = 2.0 * z / vw;
        let sy = -2.0 * z / vh;
        let tx = 2.0 * ox / vw - 1.0;
        let ty = 1.0 - 2.0 * oy / vh;

        #[rustfmt::skip]
        let view_proj = [
            sx,  0.0, 0.0, 0.0,
            0.0, sy,  0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            tx,  ty,  0.0, 1.0,
        ];
        Self { view_proj }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU context
// ─────────────────────────────────────────────────────────────────────────────

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub surface_format: wgpu::TextureFormat,

    // Pipelines
    pub sprite_pipeline: wgpu::RenderPipeline,
    pub primitive_pipeline: wgpu::RenderPipeline,
    pub effects_pipeline: wgpu::RenderPipeline,
    pub fog_pipeline: wgpu::RenderPipeline,

    // Camera uniforms (bind group 0)
    pub camera_buffer: wgpu::Buffer,
    pub camera_bind_group: wgpu::BindGroup,
    pub camera_bind_group_layout: wgpu::BindGroupLayout,

    // Second camera for HUD (screen-space) — separate buffer so we can
    // set both before submitting without write_buffer conflicts.
    pub hud_camera_buffer: wgpu::Buffer,
    pub hud_camera_bind_group: wgpu::BindGroup,

    // Texture bind group layout (for sprite pipeline, bind group 1)
    pub texture_bind_group_layout: wgpu::BindGroupLayout,

    // Shared samplers
    pub nearest_sampler: wgpu::Sampler,
    pub linear_sampler: wgpu::Sampler,
}

impl GpuContext {
    /// Create GPU context from a winit window. Blocks on async wgpu init (native).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(window: Arc<winit::window::Window>) -> Self {
        pollster::block_on(Self::new_async(window))
    }

    /// Async GPU init (used directly on web, via pollster on native).
    pub async fn new_async(window: Arc<winit::window::Window>) -> Self {
        let size = window.inner_size();

        #[cfg(target_arch = "wasm32")]
        let backends = wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let backends = wgpu::Backends::all();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });

        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter");

        log::info!("GPU adapter: {}", adapter.get_info().name);

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("battlefield"),
                    required_features: wgpu::Features::empty(),
                    required_limits: {
                        #[cfg(target_arch = "wasm32")]
                        let mut limits = wgpu::Limits::downlevel_webgl2_defaults();
                        #[cfg(not(target_arch = "wasm32"))]
                        let mut limits = wgpu::Limits::default();
                        // Clamp to what the adapter actually supports
                        let adapter_limits = adapter.limits();
                        limits.max_color_attachments = limits
                            .max_color_attachments
                            .min(adapter_limits.max_color_attachments);
                        limits
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .expect("request device");

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // ── Samplers ─────────────────────────────────────────────────────
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("nearest"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Camera bind group layout (group 0, shared) ──────────────────
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl"),
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

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera_buf"),
            contents: bytemuck::cast_slice(&[CameraUniform::screen_ortho(
                size.width.max(1) as f32,
                size.height.max(1) as f32,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // HUD camera (separate buffer so both can be written before submit)
        let hud_camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hud_camera_buf"),
            contents: bytemuck::cast_slice(&[CameraUniform::screen_ortho(
                size.width.max(1) as f32,
                size.height.max(1) as f32,
            )]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let hud_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hud_camera_bg"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: hud_camera_buffer.as_entire_binding(),
            }],
        });

        // ── Texture bind group layout (group 1, sprite pipeline) ────────
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("texture_bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
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

        // ── Sprite pipeline ─────────────────────────────────────────────
        let sprite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sprite_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/sprite.wgsl").into()),
        });

        let sprite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("sprite_pl"),
                bind_group_layouts: &[&camera_bind_group_layout, &texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        let blend_alpha = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };

        let sprite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprite_rp"),
            layout: Some(&sprite_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &sprite_shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &sprite_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(blend_alpha),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Primitive pipeline ──────────────────────────────────────────
        let prim_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("primitive_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/primitive.wgsl").into()),
        });

        let prim_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("prim_pl"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let primitive_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("primitive_rp"),
            layout: Some(&prim_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &prim_shader,
                entry_point: Some("vs_main"),
                buffers: &[PrimitiveVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &prim_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(blend_alpha),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Effects pipeline (procedural circles) ──────────────────────
        let effects_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("effects_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/effects.wgsl").into()),
        });

        let effects_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("effects_pl"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let effects_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("effects_rp"),
            layout: Some(&effects_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &effects_shader,
                entry_point: Some("vs_main"),
                buffers: &[EffectVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &effects_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(blend_alpha),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        // ── Fog pipeline (GPU-side fog-of-war computation) ──────────────
        let fog_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fog_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/fog.wgsl").into()),
        });

        let fog_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fog_rp"),
            layout: Some(&sprite_pipeline_layout), // same layout: camera + texture
            vertex: wgpu::VertexState {
                module: &fog_shader,
                entry_point: Some("vs_main"),
                buffers: &[SpriteVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &fog_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(blend_alpha),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            device,
            queue,
            surface,
            surface_config,
            surface_format,
            sprite_pipeline,
            primitive_pipeline,
            effects_pipeline,
            fog_pipeline,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            hud_camera_buffer,
            hud_camera_bind_group,
            texture_bind_group_layout,
            nearest_sampler,
            linear_sampler,
        }
    }

    /// Reconfigure surface after a window resize.
    /// If either dimension exceeds the GPU max texture size, both are scaled
    /// down proportionally so the aspect ratio is preserved.  Without this,
    /// clamping only the oversized axis causes visible stretching on mobile.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            let max = self.device.limits().max_texture_dimension_2d;
            let scale = 1.0_f32
                .min(max as f32 / width as f32)
                .min(max as f32 / height as f32);
            self.surface_config.width = ((width as f32 * scale).round() as u32).max(1);
            self.surface_config.height = ((height as f32 * scale).round() as u32).max(1);
            self.surface.configure(&self.device, &self.surface_config);
        }
    }

    /// Upload a new camera matrix to the GPU.
    pub fn set_camera(&self, uniform: &CameraUniform) {
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[*uniform]));
    }

    /// Upload the HUD (screen-space) camera matrix.
    pub fn set_hud_camera(&self, uniform: &CameraUniform) {
        self.queue.write_buffer(
            &self.hud_camera_buffer,
            0,
            bytemuck::cast_slice(&[*uniform]),
        );
    }

    /// Create a bind group for a texture + sampler pair.
    pub fn create_texture_bind_group(
        &self,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tex_bg"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}
