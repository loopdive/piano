struct Globals {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
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
};

@vertex
fn vs_particle(
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
    return out;
}

@fragment
fn fs_particle(in: VertexOutput) -> @location(0) vec4<f32> {
    let p = (in.uv - 0.5) * 2.0;
    let dist = length(p);

    // Bright core
    let core = exp(-dist * 4.0);

    // Star/sparkle cross arms
    let ax = exp(-abs(p.y) * 12.0) * exp(-abs(p.x) * 3.0);
    let ay = exp(-abs(p.x) * 12.0) * exp(-abs(p.y) * 3.0);
    let sparkle = (ax + ay) * 0.4;

    let brightness = core + sparkle;
    return vec4<f32>(in.color.rgb * brightness, 1.0);
}
