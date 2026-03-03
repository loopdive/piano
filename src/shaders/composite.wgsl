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
@group(1) @binding(0) var t_bloom: texture_2d<f32>;
@group(1) @binding(1) var s_bloom: sampler;

@fragment
fn fs_composite(in: FullscreenOutput) -> @location(0) vec4<f32> {
    let scene = textureSample(t_scene, s_scene, in.uv).rgb;
    let bloom = textureSample(t_bloom, s_bloom, in.uv).rgb;
    let bloom_intensity = 2.0;
    let color = scene + bloom * bloom_intensity;
    // Clamp instead of Reinhard to preserve color saturation
    return vec4<f32>(min(color, vec3<f32>(1.0)), 1.0);
}
