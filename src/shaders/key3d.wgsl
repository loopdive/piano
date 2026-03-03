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
    @location(2) pos_width: vec2<f32>,    // x position, key width
    @location(3) height_depth: vec2<f32>, // key height, key depth
    @location(4) press_black: vec2<f32>,  // press amount (0-1), is_black (0 or 1)
    @location(5) color: vec4<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_key(v: VsIn, inst: Instance) -> VsOut {
    let pos_x = inst.pos_width.x;
    let key_w = inst.pos_width.y;
    let key_h = inst.height_depth.x;
    let key_d = inst.height_depth.y;
    let press = inst.press_black.x;

    // Scale unit box [0,1]^3 to key dimensions
    var p = v.position * vec3<f32>(key_w, key_h, key_d);
    var n = v.normal;

    // Press: rotate around back edge (z = key_d) — like a real piano key pivot
    // Negative angle makes front edge dip DOWN in screen space
    let angle = -press * 0.05;
    let c = cos(angle);
    let s = sin(angle);
    let rel_z = p.z - key_d;
    p = vec3<f32>(p.x, p.y * c - rel_z * s, p.y * s + rel_z * c + key_d);
    n = vec3<f32>(n.x, n.y * c - n.z * s, n.y * s + n.z * c);

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
    let depth = z_frac * 0.5 + (1.0 - inst.press_black.y) * 0.4;

    var out: VsOut;
    out.position = vec4<f32>(ndc_x, ndc_y, depth, 1.0);
    out.normal = normalize(n);
    out.color = inst.color;
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

    let color = in.color.rgb * diffuse + vec3<f32>(spec);
    return vec4<f32>(color, in.color.a);
}
