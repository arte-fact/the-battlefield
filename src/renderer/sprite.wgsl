struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct InstanceInput {
    @location(2) world_pos: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) uv_min: vec2<f32>,
    @location(5) uv_max: vec2<f32>,
    @location(6) flip_x: f32,
    @location(7) opacity: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) opacity: f32,
};

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    var out: VertexOutput;

    // Scale the unit quad by instance size, offset by world position
    var local_x = vert.position.x * inst.size.x;
    let local_y = vert.position.y * inst.size.y;

    // Apply horizontal flip
    if (inst.flip_x > 0.5) {
        local_x = -local_x;
    }

    let world = vec4<f32>(
        inst.world_pos.x + local_x,
        inst.world_pos.y + local_y,
        0.0,
        1.0,
    );

    out.clip_position = camera.view_proj * world;

    // Remap tex_coords to the instance's UV rectangle
    var u = mix(inst.uv_min.x, inst.uv_max.x, vert.tex_coords.x);
    let v = mix(inst.uv_min.y, inst.uv_max.y, vert.tex_coords.y);

    // Flip UV horizontally if flipped
    if (inst.flip_x > 0.5) {
        u = inst.uv_max.x - (u - inst.uv_min.x);
    }

    out.tex_coords = vec2<f32>(u, v);
    out.opacity = inst.opacity;

    return out;
}

@group(1) @binding(0)
var t_sprite: texture_2d<f32>;
@group(1) @binding(1)
var s_sprite: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(t_sprite, s_sprite, in.tex_coords);
    color.a = color.a * in.opacity;
    if (color.a < 0.01) {
        discard;
    }
    return color;
}
