struct FullscreenOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> FullscreenOutput {
    let uv = vec2<f32>(f32(idx >> 1u), f32(idx & 1u)) * 2.0;
    let pos = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    return FullscreenOutput(vec4<f32>(pos, 0.0, 1.0), uv);
}

@group(0) @binding(0) var t_input: texture_2d<f32>;
@group(0) @binding(1) var s_input: sampler;

struct BlurUniforms {
    direction: vec2<f32>,
    _padding: vec2<f32>,
};
@group(1) @binding(0) var<uniform> blur: BlurUniforms;

@fragment
fn fs_blur(in: FullscreenOutput) -> @location(0) vec4<f32> {
    // 9-tap Gaussian blur, manually unrolled (WebGL2 requires constant array indexing)
    let w0 = 0.227027;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.054054;
    let w4 = 0.016216;

    var color = textureSample(t_input, s_input, in.uv).rgb * w0;

    let o1 = blur.direction * 1.0;
    color += textureSample(t_input, s_input, in.uv + o1).rgb * w1;
    color += textureSample(t_input, s_input, in.uv - o1).rgb * w1;

    let o2 = blur.direction * 2.0;
    color += textureSample(t_input, s_input, in.uv + o2).rgb * w2;
    color += textureSample(t_input, s_input, in.uv - o2).rgb * w2;

    let o3 = blur.direction * 3.0;
    color += textureSample(t_input, s_input, in.uv + o3).rgb * w3;
    color += textureSample(t_input, s_input, in.uv - o3).rgb * w3;

    let o4 = blur.direction * 4.0;
    color += textureSample(t_input, s_input, in.uv + o4).rgb * w4;
    color += textureSample(t_input, s_input, in.uv - o4).rgb * w4;

    return vec4<f32>(color, 1.0);
}
