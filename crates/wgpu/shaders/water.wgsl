// Pixel-art water shader — panning + subtraction caustics with discrete stepping.
// Two procedural noise layers pan in different directions and are combined via
// min() to create animated caustic light patterns. All movement is quantized
// to pixel steps via floor() for a crisp pixel-art look.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color_mod: vec4<f32>,  // .r = elapsed time
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) world_pos: vec2<f32>,
    @location(2) time: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.world_pos = in.position;
    out.time = in.color_mod.r;
    return out;
}

// Simple hash for procedural noise (no texture needed)
fn hash(p: vec2<f32>) -> f32 {
    let k = vec2<f32>(127.1, 311.7);
    let n = dot(p, k);
    return fract(sin(n) * 43758.5453);
}

// Value noise with pixel-grid snapping
fn pixel_noise(p: vec2<f32>, scale: f32) -> f32 {
    let sp = floor(p * scale) / scale;
    return hash(sp);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = in.time;

    // Work in tile-pixel space (64 pixels per tile)
    let pixel_pos = in.world_pos;

    // Pixel grid size — matches sprite texel density (1 texel = 1 world unit)
    let grid = 1.0;

    // Quantize position to pixel grid
    let qp = floor(pixel_pos / grid);

    // Two noise layers panning in different directions with discrete time steps
    let step_speed = 3.0;
    let time_step = floor(t * step_speed) / step_speed;  // discrete time stepping

    // Layer 1: drifts diagonally down-right
    let pan1 = vec2<f32>(time_step * 0.7, time_step * 0.4);
    let n1 = hash(qp + pan1);

    // Layer 2: drifts diagonally up-left at different speed
    let pan2 = vec2<f32>(-time_step * 0.5, time_step * 0.6);
    let n2 = hash(qp * 1.3 + pan2 + vec2<f32>(73.0, 19.0));

    // Combine via min() for caustic erosion pattern
    let caustic = min(n1, n2);

    // Threshold into 3 discrete brightness bands for pixel-art look
    var brightness: f32;
    if caustic < 0.25 {
        brightness = 0.0;   // dark caustic
    } else if caustic < 0.5 {
        brightness = 0.3;   // mid
    } else {
        brightness = 0.7;   // bright highlight
    }

    // Base water colors (dark blue to teal)
    let deep    = vec3<f32>(0.11, 0.18, 0.36);   // dark blue
    let mid     = vec3<f32>(0.15, 0.28, 0.45);   // medium blue
    let light   = vec3<f32>(0.22, 0.42, 0.55);   // teal highlight

    let water_color = mix(deep, mix(mid, light, brightness), 0.5 + brightness * 0.5);

    // Subtle slow undulation for depth variation
    let depth_wave = sin(qp.x * 0.3 + qp.y * 0.2 + t * 0.4) * 0.5 + 0.5;
    let final_color = mix(water_color, water_color * 1.15, depth_wave * 0.15);

    // Sample the base texture and blend with caustic effect
    let base = textureSample(t_sprite, s_sprite, in.uv);
    let blended = mix(vec4<f32>(final_color, 1.0), base, 0.3);

    return blended;
}
