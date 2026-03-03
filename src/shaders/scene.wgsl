struct Globals {
    screen_size: vec2<f32>,
    note_mode: f32,
    _padding: f32,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct InstanceInput {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) size_px: vec2<f32>,
};

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);
    let pixel_pos = instance.pos + vec2<f32>(x, y) * instance.size;
    let ndc = vec2<f32>(
        pixel_pos.x / globals.screen_size.x * 2.0 - 1.0,
        1.0 - pixel_pos.y / globals.screen_size.y * 2.0,
    );
    var out: VertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.color = instance.color;
    out.uv = vec2<f32>(x, y);
    out.size_px = instance.size;
    return out;
}

@fragment
fn fs_quad(in: VertexOutput) -> @location(0) vec4<f32> {
    let is_note = globals.note_mode > 0.5;

    // Rounded rectangle SDF — generous radius for notes, minimal for keyboard
    let radius = select(
        min(2.0, min(in.size_px.x, in.size_px.y) * 0.08),
        min(8.0, min(in.size_px.x, in.size_px.y) * 0.4),
        is_note
    );
    let half_size = in.size_px * 0.5;
    let p = (in.uv - 0.5) * in.size_px;
    let q = abs(p) - half_size + vec2<f32>(radius, radius);
    let d = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;

    // Anti-aliased edge
    let edge_alpha = 1.0 - smoothstep(-1.0, 0.5, d);

    // Bright border only on notes
    let border = smoothstep(-2.5, -1.5, d) * (1.0 - smoothstep(-1.5, -0.5, d));
    let border_boost = select(1.0, 1.0 + border * 0.6, is_note);

    // Notes: onset (bottom, uv.y=1) bright → release (top, uv.y=0) dark
    // Keyboard: top-lit spotlight
    let gradient = select(
        1.0 + (0.5 - in.uv.y) * 0.3,                // keyboard: top-lit
        0.7 + in.uv.y * 0.5,                          // notes: 0.7 at top (release) → 1.2 at bottom (onset)
        is_note
    );

    let col = in.color.rgb * gradient * border_boost;
    return vec4<f32>(col, in.color.a * edge_alpha);
}
