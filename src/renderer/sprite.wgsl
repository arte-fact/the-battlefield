struct FrameUV {
    uv: vec4<f32>, // x: u_start, y: v_start, z: u_end, w: v_end
};

@group(1) @binding(0)
var<uniform> frame: FrameUV;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    // Remap tex_coords [0,1] to the frame's UV range
    out.tex_coords = vec2<f32>(
        mix(frame.uv.x, frame.uv.z, in.tex_coords.x),
        mix(frame.uv.y, frame.uv.w, in.tex_coords.y),
    );
    return out;
}

@group(0) @binding(0)
var t_sprite: texture_2d<f32>;
@group(0) @binding(1)
var s_sprite: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_sprite, s_sprite, in.tex_coords);
}
