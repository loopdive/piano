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

@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var s_scene: sampler;

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_extract(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let color = textureSample(t_scene, s_scene, in.uv).rgb;
    let lum = luminance(color);
    let threshold = 0.18;
    let knee = 0.1;
    let softness = clamp(lum - threshold + knee, 0.0, 2.0 * knee);
    let contribution = softness * softness / (4.0 * knee + 0.0001);
    let brightness = max(lum - threshold, contribution);
    let factor = brightness / max(lum, 0.0001);
    return vec4<f32>(color * factor, 1.0);
}
