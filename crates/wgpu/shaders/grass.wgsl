// Grass wind shader — pixel-art UV displacement with layered sine waves.
// Grass tips sway with wind while roots stay anchored. All movement snaps
// to whole pixels to preserve the pixel-art aesthetic.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

@group(1) @binding(0) var t_sprite: texture_2d<f32>;
@group(1) @binding(1) var s_sprite: sampler;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color_mod: vec4<f32>,  // .r = elapsed time, .gba unused
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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_dims = vec2<f32>(textureDimensions(t_sprite, 0));
    let pixel = 1.0 / tex_dims;

    let t = in.time;
    let wp = in.world_pos;

    // 3 layered sine waves at different scales for organic wind
    let w1 = sin(t * 1.2 + wp.x * 0.014 + wp.y * 0.007);
    let w2 = sin(t * 2.3 + wp.x * 0.023 - wp.y * 0.011) * 0.4;
    let w3 = sin(t * 0.6 + wp.x * 0.008 + wp.y * 0.019) * 0.25;
    let wind = (w1 + w2 + w3);

    // Height mask: UV.y 0=top 1=bottom in the tile's local source rect.
    // We want the top of the grass (low UV.y) to sway most.
    // fract(uv * tex_dims) gives local position within the tile.
    let local_y = fract(in.uv.y * tex_dims.y / 64.0);  // 64px tiles
    let height = 1.0 - local_y;  // 1 at top, 0 at bottom
    let sway = pow(height, 2.5);  // quadratic+: strong at tips, zero at ground

    // Pixel-snapped horizontal displacement (whole pixels only)
    let shift_px = floor(wind * sway * 1.5 + 0.5);
    let offset = shift_px * pixel.x;

    let displaced_uv = vec2<f32>(in.uv.x + offset, in.uv.y);

    let texel = textureSample(t_sprite, s_sprite, displaced_uv);

    // Discard fully transparent pixels
    if texel.a < 0.01 {
        discard;
    }

    return texel;
}
