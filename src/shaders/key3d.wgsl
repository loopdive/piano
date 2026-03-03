struct Uniforms {
    screen_size: vec2<f32>,
    keyboard_y: f32,
    keyboard_height: f32,
    max_depth: f32,       // white key depth — defines full keyboard span
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    light_dir: vec4<f32>, // xyz = direction, w = ambient intensity
};
@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
};

struct Instance {
    @location(2) pos_width: vec2<f32>,        // x position, key width
    @location(3) height_depth: vec2<f32>,     // key height, key depth
    @location(4) press_black_light: vec4<f32>, // press, is_black, light, _pad
    @location(5) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) press: f32,
    @location(3) local_uv: vec2<f32>, // position on key surface (0-1 in x and z)
    @location(4) flashlight: f32,     // upcoming note highlight intensity (0-1)
};

@vertex
fn vs_key(v: VsIn, inst: Instance) -> VsOut {
    let pos_x = inst.pos_width.x;
    let key_w = inst.pos_width.y;
    let key_h = inst.height_depth.x;
    let key_d = inst.height_depth.y;
    let press = inst.press_black_light.x;

    // Scale unit box [0,1]^3 to key dimensions
    var p = v.position * vec3<f32>(key_w, key_h, key_d);
    var n = v.normal;

    // Press: rotate around back edge (z = key_d) — real piano key pivot
    let angle = press * -0.06; // ~3.4 degrees downward at full press
    let cos_a = cos(angle);
    let sin_a = sin(angle);
    let dz = p.z - key_d;
    p = vec3<f32>(p.x, p.y * cos_a - dz * sin_a, p.y * sin_a + dz * cos_a + key_d);
    n = vec3<f32>(n.x, n.y * cos_a - n.z * sin_a, n.y * sin_a + n.z * cos_a);

    // Translate to world X position
    p.x = p.x + pos_x;

    // Oblique projection to screen coordinates
    // Front of key (z=0) at top of keyboard area, back (z=max) at bottom
    let z_frac = clamp(p.z / uniforms.max_depth, 0.0, 1.0);
    let screen_y_base = uniforms.keyboard_y + z_frac * uniforms.keyboard_height;
    let screen_y = screen_y_base - p.y * 1.0;

    // Convert to NDC
    let ndc_x = p.x / uniforms.screen_size.x * 2.0 - 1.0;
    let ndc_y = 1.0 - screen_y / uniforms.screen_size.y * 2.0;

    // Depth: black keys always in front of white keys (larger gap prevents z-fighting)
    let depth = z_frac * 0.5 + (1.0 - inst.press_black_light.y) * 0.4;

    var out: VsOut;
    out.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    out.normal = normalize(n);
    out.color = inst.color;
    out.press = press;
    out.local_uv = v.position.xz; // unit box x,z → 0..1 across key surface
    out.flashlight = inst.press_black_light.z;
    return out;
}

@fragment
fn fs_key(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(-uniforms.light_dir.xyz);
    let ambient = uniforms.light_dir.w;

    // Diffuse
    let ndotl = max(dot(n, l), 0.0);
    let diffuse = ambient + (1.0 - ambient) * ndotl;

    // Specular (Blinn-Phong, view from above-front)
    let view = normalize(vec3<f32>(0.0, 0.8, -0.5));
    let h = normalize(l + view);
    let spec = pow(max(dot(n, h), 0.0), 48.0) * 0.2;

    // Spotlight from above: focused cone on the key top surface
    // Only illuminates top face (dot with up), fades from front-center outward
    let spot_dir = vec3<f32>(0.0, 1.0, 0.0);
    let spot_ndotl = max(dot(n, spot_dir), 0.0);
    // Radial falloff from front-center of key (x=0.5, z=0.15) — light hits near front
    let dx = in.local_uv.x - 0.5;
    let dz = in.local_uv.y - 0.15;
    let dist_sq = dx * dx * 4.0 + dz * dz * 1.5; // elliptical: tighter in x, longer in z
    let falloff = exp(-dist_sq * 3.0);
    let spot_intensity = in.press * spot_ndotl * falloff;
    let spotlight = in.color.rgb * vec3<f32>(1.0, 0.97, 0.9) * spot_intensity * 2.5;

    // Flashlight: warm glow on keys about to be pressed
    // Soft gradient concentrated at front of key, top face only
    let flash_top = max(dot(n, vec3<f32>(0.0, 1.0, 0.0)), 0.0);
    let flash_z_fade = exp(-in.local_uv.y * 2.0); // brighter at front, fading toward back
    let flash_color = vec3<f32>(1.0, 0.7, 0.3); // warm orange
    let flashlight = flash_color * in.flashlight * flash_top * flash_z_fade * 1.5;

    let color = in.color.rgb * diffuse + vec3<f32>(spec) + spotlight + flashlight;
    return vec4<f32>(color, in.color.a);
}
