// Fog-of-war shader: samples a 1-channel visibility texture with bilinear
// filtering and computes smooth fog with soft edge gradients on the GPU.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

@group(1) @binding(0) var t_vis: texture_2d<f32>;
@group(1) @binding(1) var s_vis: sampler;           // Linear sampler for smooth interpolation

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color_mod: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(t_vis, 0));
    let tx = 1.0 / dims;

    // Center visibility — bilinear filtering gives smooth 0..1 gradients
    let vis = textureSample(t_vis, s_vis, in.uv).r;

    // Weighted neighbor average with larger kernel for smoother edges.
    // Sample in a cross+diagonal pattern with distance-based weights.
    let d1 = 1.0;   // cardinal distance
    let d2 = 1.4;   // diagonal distance
    let w1 = 1.0;   // cardinal weight
    let w2 = 0.7;   // diagonal weight (1/sqrt(2) ≈ 0.707)

    var nb = 0.0;
    // Cardinals
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>(  0.0, -tx.y)).r * w1;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>(-tx.x,   0.0)).r * w1;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>( tx.x,   0.0)).r * w1;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>(  0.0,  tx.y)).r * w1;
    // Diagonals
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>(-tx.x, -tx.y)).r * w2;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>( tx.x, -tx.y)).r * w2;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>(-tx.x,  tx.y)).r * w2;
    nb += textureSample(t_vis, s_vis, in.uv + vec2<f32>( tx.x,  tx.y)).r * w2;

    let total_weight = 4.0 * w1 + 4.0 * w2;  // 6.8
    let avg = nb / total_weight;

    // Blend between center and neighborhood for smooth transitions
    let smooth_vis = mix(avg, vis, 0.5);

    // Fog alpha: smoothstep from fully visible (0 alpha) to fully hidden
    let fog_dark = 140.0 / 255.0;   // max fog darkness for hidden tiles
    let fog_min  =  38.0 / 255.0;   // minimum fog for tiles near visible area
    let edge_dim =  50.0 / 255.0;   // subtle darkening at visible edges

    // Smooth transition: visible → edge → fog
    let alpha = mix(fog_dark, 0.0, smoothstep(0.0, 1.0, smooth_vis))
              + edge_dim * smoothstep(0.7, 1.0, smooth_vis) * (1.0 - smooth_vis) * 4.0;

    // Ensure minimum fog in hidden areas
    let final_alpha = select(max(alpha, fog_min), alpha, smooth_vis > 0.4);

    return vec4<f32>(0.0, 0.0, 0.0, final_alpha);
}
