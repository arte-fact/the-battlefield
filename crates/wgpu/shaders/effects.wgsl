// Procedural circle effects — zone capture areas, player aim, order pulse.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,       // -1..1 from circle center
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,    // [time, kind, radius, _]
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.params = in.params;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let time = in.params.x;
    let kind = in.params.y;
    let radius = in.params.z;

    // Pixel grid fixed in world space: each chunk = 1 world pixel regardless
    // of circle size. Small circles look chunky, large circles look finer.
    let pixel = 1.0 / max(radius, 1.0);
    let uv = floor(in.uv / pixel + 0.5) * pixel;
    let d = length(uv);
    let angle = atan2(uv.y, uv.x);

    if d > 1.05 {
        discard;
    }

    if kind < 0.5 {
        // ── Zone capture circle ──────────────────────────────────────
        let capturing = in.params.w; // 1.0 when capturing/contested, 0.0 otherwise

        // Solid border ring (always visible)
        let border = smoothstep(0.04, 0.0, abs(d - 0.96)) * 0.5;

        // Dashed rings — only during capture, rotating
        let dash1 = step(0.0, sin(angle * 10.0 - time * 4.0));
        let ring1 = smoothstep(0.035, 0.0, abs(d - 0.88)) * 0.4 * dash1 * capturing;

        let dash2 = step(0.0, sin(angle * 6.0 + time * 3.2));
        let ring2 = smoothstep(0.03, 0.0, abs(d - 0.6)) * 0.25 * dash2 * capturing;

        let dash3 = step(0.0, sin(angle * 8.0 - time * 5.0));
        let ring3 = smoothstep(0.025, 0.0, abs(d - 0.35)) * 0.2 * dash3 * capturing;

        // Center glow
        let center = exp(-d * d * 8.0) * 0.1;

        // Soft radial fill
        let fill = smoothstep(1.0, 0.4, d) * 0.06;

        // Rotating sweep (only during capture)
        let sweep = pow(max(cos(angle - time * 1.3), 0.0), 4.0) * 0.08
                  * smoothstep(1.0, 0.1, d) * capturing;

        let a = border + ring1 + ring2 + ring3 + center + fill + sweep;
        return vec4<f32>(in.color.rgb, max(a, 0.0) * in.color.a);

    } else if kind < 1.5 {
        // ── Player aim circle ────────────────────────────────────────
        // Soft center glow
        let glow = exp(-d * d * 5.0) * 0.2;

        // Pulsing ring
        let pulse = 0.6 + 0.4 * sin(time * 3.5);
        let ring = smoothstep(0.055, 0.0, abs(d - 0.78)) * 0.5 * pulse;

        // Dashed outer ring (8 dashes, rotating)
        let dash = step(0.0, sin(angle * 8.0 + time * 2.0));
        let outer = smoothstep(0.1, 0.0, abs(d - 0.95)) * 0.35 * dash;

        let a = glow + ring + outer;
        return vec4<f32>(in.color.rgb, max(a, 0.0) * in.color.a);

    } else {
        // ── Order range pulse ────────────────────────────────────────
        let progress = time; // 0→1 expanding
        let fade = 1.0 - progress;

        // Bold expanding ring
        let ring_pos = progress * 0.95;
        let ring_w = 0.06 + progress * 0.03;
        let ring = smoothstep(ring_w, 0.0, abs(d - ring_pos)) * 0.7 * fade;

        // Dashed ring trailing behind (12 dashes, spinning)
        let dash = step(0.0, sin(angle * 12.0 + progress * 6.0));
        let trail_pos = ring_pos * 0.82;
        let dash_ring = smoothstep(0.035, 0.0, abs(d - trail_pos)) * 0.35 * dash * fade;

        // Faint fill behind ring
        let fill = smoothstep(ring_pos + 0.05, ring_pos - 0.15, d) * 0.1 * fade;

        let a = ring + dash_ring + fill;
        return vec4<f32>(in.color.rgb, max(a, 0.0) * in.color.a);
    }
}
