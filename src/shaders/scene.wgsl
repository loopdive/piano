struct Globals {
    screen_size: vec2<f32>,
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
    return out;
}

@fragment
fn fs_quad(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
