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
    // 9-tap Gaussian (sigma ~ 4.0)
    let weights = array<f32, 5>(0.227027, 0.1945946, 0.1216216, 0.054054, 0.016216);

    var color = textureSample(t_input, s_input, in.uv).rgb * weights[0];

    for (var i = 1; i < 5; i++) {
        let offset = blur.direction * f32(i);
        color += textureSample(t_input, s_input, in.uv + offset).rgb * weights[i];
        color += textureSample(t_input, s_input, in.uv - offset).rgb * weights[i];
    }

    return vec4<f32>(color, 1.0);
}
