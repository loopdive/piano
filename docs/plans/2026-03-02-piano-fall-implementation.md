# Piano Fall Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a wgpu+WebAssembly web app that renders falling piano notes with bloom/glow effects and piano sample audio.

**Architecture:** Pure wgpu 0.19 with 5-pass rendering (scene → bright extract → H-blur → V-blur → composite). Instanced quad rendering for notes/keyboard/particles. WebAudio API via web-sys for sample playback. All 2D — pixel-space coordinates converted to NDC in vertex shader.

**Tech Stack:** Rust, wgpu 0.19, winit 0.29, wasm-bindgen, web-sys, bytemuck, wasm-pack

---

## Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `src/lib.rs`
- Create: `build.sh`
- Create: `web/index.html`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "piano-fall"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[[bin]]
name = "piano-fall"
path = "src/main.rs"

[dependencies]
cfg-if = "1"
winit = { version = "0.29", features = ["rwh_05"] }
env_logger = "0.10"
log = "0.4"
wgpu = "0.19"
pollster = "0.3"
bytemuck = { version = "1", features = ["derive"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
console_error_panic_hook = "0.1.6"
console_log = "1.0"
wgpu = { version = "0.19", features = ["webgl"] }
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = { version = "0.3", features = [
    "Document", "Window", "Element", "HtmlCanvasElement",
    "Performance",
    "AudioContext", "AudioBuffer", "AudioBufferSourceNode",
    "AudioDestinationNode", "AudioNode", "AudioParam",
    "BaseAudioContext",
    "Request", "RequestInit", "RequestMode", "Response",
] }
```

**Step 2: Create src/main.rs**

```rust
use piano_fall::run;

fn main() {
    pollster::block_on(run());
}
```

**Step 3: Create src/lib.rs — minimal wgpu init + event loop**

Adapt from the existing `wgpu/rust/src/lib.rs` sandbox. Key changes:
- Window title: "Piano Fall"
- Canvas size: 1200x800 (wasm)
- Clear color: black `(0.0, 0.0, 0.0, 1.0)`
- Canvas element ID: `"piano-fall"`

```rust
use std::iter;
use winit::{
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: &'a Window,
}

impl<'a> State<'a> {
    async fn new(window: &'a Window) -> State<'a> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance.create_surface(window).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: if cfg!(target_arch = "wasm32") {
                        wgpu::Limits::downlevel_webgl2_defaults()
                    } else {
                        wgpu::Limits::default()
                    },
                },
                None,
            )
            .await
            .unwrap();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            desired_maximum_frame_latency: 2,
            view_formats: vec![],
        };

        Self { surface, device, queue, config, size, window }
    }

    fn window(&self) -> &Window { &self.window }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    fn input(&mut self, _event: &WindowEvent) -> bool { false }
    fn update(&mut self) {}

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") },
        );
        {
            let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
        }
        self.queue.submit(iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub async fn run() {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Info).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new()
        .with_title("Piano Fall")
        .build(&event_loop)
        .unwrap();

    #[cfg(target_arch = "wasm32")]
    {
        use winit::dpi::PhysicalSize;
        use winit::platform::web::WindowExtWebSys;
        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| {
                let dst = doc.get_element_by_id("piano-fall")?;
                let canvas = web_sys::Element::from(window.canvas()?);
                dst.append_child(&canvas).ok()?;
                Some(())
            })
            .expect("Couldn't append canvas to document body.");
        let _ = window.request_inner_size(PhysicalSize::new(1200, 800));
    }

    let mut state = State::new(&window).await;
    let mut surface_configured = false;

    event_loop
        .run(move |event, control_flow| match event {
            Event::WindowEvent { ref event, window_id }
                if window_id == state.window().id() =>
            {
                if !state.input(event) {
                    match event {
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            event: KeyEvent {
                                state: ElementState::Pressed,
                                physical_key: PhysicalKey::Code(KeyCode::Escape),
                                ..
                            },
                            ..
                        } => control_flow.exit(),
                        WindowEvent::Resized(physical_size) => {
                            surface_configured = true;
                            state.resize(*physical_size);
                        }
                        WindowEvent::RedrawRequested => {
                            state.window().request_redraw();
                            if !surface_configured { return; }
                            state.update();
                            match state.render() {
                                Ok(_) => {}
                                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                                    state.resize(state.size)
                                }
                                Err(wgpu::SurfaceError::OutOfMemory) => control_flow.exit(),
                                Err(wgpu::SurfaceError::Timeout) => {
                                    log::warn!("Surface timeout")
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        })
        .unwrap();
}
```

**Step 4: Create build.sh**

```bash
#!/bin/bash
set -e
wasm-pack build --target web --out-dir web/pkg
echo "Build complete. Serve the web/ directory, e.g.:"
echo "  cd web && python3 -m http.server 8080"
```

**Step 5: Create web/index.html**

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>Piano Fall</title>
    <style>
        body { margin: 0; background: #000; display: flex; justify-content: center; align-items: center; height: 100vh; }
        #piano-fall { width: 1200px; height: 800px; }
    </style>
</head>
<body>
    <div id="piano-fall"></div>
    <script type="module">
        import init from './pkg/piano_fall.js';
        init();
    </script>
</body>
</html>
```

**Step 6: Verify native build compiles**

Run: `cargo build`
Expected: Compiles without errors

**Step 7: Verify wasm build compiles**

Run: `bash build.sh`
Expected: wasm-pack produces `web/pkg/` directory

**Step 8: Commit**

```bash
git add -A
git commit -m "feat: scaffold piano-fall project with wgpu + wasm setup"
```

---

## Task 2: Keyboard Layout Module

**Files:**
- Create: `src/keyboard.rs`
- Modify: `src/lib.rs` (add `mod keyboard;`)

This is pure math — fully testable without GPU.

**Step 1: Write failing tests for keyboard layout**

Create `src/keyboard.rs`:

```rust
/// Piano keyboard layout calculations for 88 keys (A0 to C8).
/// Pitch 0 = A0, Pitch 87 = C8.

/// Returns true if the given pitch (0-87) is a black key.
pub fn is_black_key(pitch: u8) -> bool {
    // Pitch 0 = A0. Note name within octave: A=0, A#=1, B=2, C=3, C#=4, D=5, D#=6, E=7, F=8, F#=9, G=10, G#=11
    // Black keys: A#(1), C#(4), D#(6), F#(9), G#(11)
    let note = (pitch + 9) % 12; // shift so C=0: C=0, C#=1, D=2, D#=3, E=4, F=5, F#=6, G=7, G#=8, A=9, A#=10, B=11
    matches!(note, 1 | 3 | 6 | 8 | 10)
}

/// Returns the x-position (left edge) and width of a key in pixels,
/// given the total keyboard width and the pitch (0-87).
pub fn key_rect(pitch: u8, total_width: f32) -> (f32, f32) {
    // Count total white keys: 52
    let white_key_width = total_width / 52.0;
    let black_key_width = white_key_width * 0.6;

    if is_black_key(pitch) {
        // Black key is centered between its two adjacent white keys
        let white_index = white_key_index_before(pitch);
        let x = (white_index as f32 + 1.0) * white_key_width - black_key_width / 2.0;
        (x, black_key_width)
    } else {
        let white_index = count_white_keys_before(pitch);
        let x = white_index as f32 * white_key_width;
        (x, white_key_width)
    }
}

/// Count how many white keys are at indices < this pitch's white key position.
fn count_white_keys_before(pitch: u8) -> u32 {
    (0..pitch).filter(|&p| !is_black_key(p)).count() as u32
}

/// For a black key, return the white-key-index of the white key just below it.
fn white_key_index_before(pitch: u8) -> u32 {
    // Find the nearest white key below this black key
    let mut p = pitch - 1;
    while is_black_key(p) {
        p -= 1;
    }
    count_white_keys_before(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_black_key() {
        // A0 (pitch 0) = white
        assert!(!is_black_key(0));
        // A#0 (pitch 1) = black
        assert!(is_black_key(1));
        // B0 (pitch 2) = white
        assert!(!is_black_key(2));
        // C1 (pitch 3) = white
        assert!(!is_black_key(3));
        // C#1 (pitch 4) = black
        assert!(is_black_key(4));
        // D1 (pitch 5) = white
        assert!(!is_black_key(5));
        // C8 (pitch 87) = white
        assert!(!is_black_key(87));
    }

    #[test]
    fn test_white_key_count() {
        let white_count = (0..88).filter(|&p| !is_black_key(p)).count();
        assert_eq!(white_count, 52);
    }

    #[test]
    fn test_black_key_count() {
        let black_count = (0..88).filter(|&p| is_black_key(p)).count();
        assert_eq!(black_count, 36);
    }

    #[test]
    fn test_key_rect_first_key() {
        let (x, w) = key_rect(0, 1040.0); // 1040 / 52 = 20px per white key
        assert!((x - 0.0).abs() < 0.01);
        assert!((w - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_key_rect_black_key() {
        let (x, w) = key_rect(1, 1040.0); // A#0, black key
        let white_w = 1040.0 / 52.0; // 20
        let black_w = white_w * 0.6; // 12
        // A#0 is between A0 (white idx 0) and B0 (white idx 1)
        let expected_x = 1.0 * white_w - black_w / 2.0; // 20 - 6 = 14
        assert!((x - expected_x).abs() < 0.01);
        assert!((w - black_w).abs() < 0.01);
    }

    #[test]
    fn test_key_rect_last_key() {
        let (x, w) = key_rect(87, 1040.0); // C8, last white key
        let white_w = 1040.0 / 52.0;
        let expected_x = 51.0 * white_w; // last white key
        assert!((x - expected_x).abs() < 0.01);
        assert!((w - white_w).abs() < 0.01);
    }
}
```

**Step 2: Add module to lib.rs**

Add at top of `src/lib.rs`:
```rust
pub mod keyboard;
```

**Step 3: Run tests to verify they pass**

Run: `cargo test -- keyboard`
Expected: All 5 tests pass

**Step 4: Commit**

```bash
git add src/keyboard.rs src/lib.rs
git commit -m "feat: add keyboard layout module with 88-key position calculations"
```

---

## Task 3: Note Data Model + Demo Song

**Files:**
- Create: `src/note.rs`
- Modify: `src/lib.rs` (add `mod note;`)

**Step 1: Create note data model with demo song**

Create `src/note.rs`:

```rust
/// A single note in the song.
#[derive(Clone, Debug)]
pub struct Note {
    /// Piano key index: 0 = A0, 87 = C8
    pub pitch: u8,
    /// Start time in seconds from song beginning
    pub start_time: f32,
    /// Duration in seconds
    pub duration: f32,
    /// Velocity 0.0-1.0 (affects brightness)
    pub velocity: f32,
}

/// A collection of notes forming a song.
pub struct Song {
    pub notes: Vec<Note>,
    pub bpm: f32,
}

/// Helper: convert a MIDI note number (60 = C4) to our pitch (0-87).
/// MIDI 21 = A0 (pitch 0), MIDI 108 = C8 (pitch 87).
pub fn midi_to_pitch(midi_note: u8) -> u8 {
    midi_note.saturating_sub(21)
}

/// Create a demo song: C major scale up/down + some chords.
pub fn demo_song() -> Song {
    let bpm = 120.0;
    let beat = 60.0 / bpm; // 0.5 seconds per beat
    let mut notes = Vec::new();

    // C major scale ascending: C4 D4 E4 F4 G4 A4 B4 C5
    let scale_up = [60, 62, 64, 65, 67, 69, 71, 72];
    for (i, &midi) in scale_up.iter().enumerate() {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.8,
        });
    }

    // C major scale descending: B4 A4 G4 F4 E4 D4 C4
    let scale_down = [71, 69, 67, 65, 64, 62, 60];
    let offset = 8.0 * beat;
    for (i, &midi) in scale_down.iter().enumerate() {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: offset + i as f32 * beat,
            duration: beat * 0.9,
            velocity: 0.8,
        });
    }

    // Chords section
    let chord_offset = 16.0 * beat;
    // C major chord (C4-E4-G4)
    for &midi in &[60, 64, 67] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // F major chord (F4-A4-C5)
    for &midi in &[65, 69, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 2.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // G major chord (G4-B4-D5)
    for &midi in &[67, 71, 74] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 4.0,
            duration: beat * 2.0,
            velocity: 0.9,
        });
    }
    // C major chord (C4-E4-G4-C5) — final
    for &midi in &[60, 64, 67, 72] {
        notes.push(Note {
            pitch: midi_to_pitch(midi),
            start_time: chord_offset + beat * 6.0,
            duration: beat * 4.0,
            velocity: 1.0,
        });
    }

    Song { notes, bpm }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_to_pitch() {
        assert_eq!(midi_to_pitch(21), 0);   // A0
        assert_eq!(midi_to_pitch(60), 39);  // C4
        assert_eq!(midi_to_pitch(108), 87); // C8
    }

    #[test]
    fn test_demo_song_has_notes() {
        let song = demo_song();
        assert!(!song.notes.is_empty());
        assert!(song.notes.len() > 20);
    }

    #[test]
    fn test_demo_song_pitches_in_range() {
        let song = demo_song();
        for note in &song.notes {
            assert!(note.pitch <= 87, "Pitch {} out of range", note.pitch);
        }
    }

    #[test]
    fn test_demo_song_times_positive() {
        let song = demo_song();
        for note in &song.notes {
            assert!(note.start_time >= 0.0);
            assert!(note.duration > 0.0);
        }
    }
}
```

**Step 2: Add module to lib.rs**

Add at top of `src/lib.rs`:
```rust
pub mod note;
```

**Step 3: Run tests**

Run: `cargo test -- note`
Expected: All 4 tests pass

**Step 4: Commit**

```bash
git add src/note.rs src/lib.rs
git commit -m "feat: add note data model and demo song"
```

---

## Task 4: Scene Rendering — Instanced Quads

**Files:**
- Create: `src/renderer/mod.rs`
- Create: `src/renderer/quad.rs`
- Create: `src/shaders/scene.wgsl`
- Modify: `src/lib.rs` (add renderer module, integrate into State)

This task sets up the core instanced quad rendering pipeline used by notes and keyboard.

**Step 1: Create the WGSL scene shader**

Create `src/shaders/scene.wgsl`:

```wgsl
// Globals uniform: screen dimensions
struct Globals {
    screen_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

// Per-instance data
struct InstanceInput {
    @location(0) pos: vec2<f32>,    // top-left in pixels
    @location(1) size: vec2<f32>,   // width, height in pixels
    @location(2) color: vec4<f32>,  // RGBA
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Unit quad vertices: 0,1,2,3 → two triangles via index buffer
// Vertex index encodes position within the unit quad
@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Unit quad corners: (0,0), (1,0), (0,1), (1,1)
    let x = f32(vertex_index & 1u);
    let y = f32((vertex_index >> 1u) & 1u);

    // Scale and offset in pixel space
    let pixel_pos = instance.pos + vec2<f32>(x, y) * instance.size;

    // Convert pixel coords to NDC: x: [0, width] → [-1, 1], y: [0, height] → [1, -1]
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

// Particle fragment shader: soft circular gradient
@fragment
fn fs_particle(in: VertexOutput) -> @location(0) vec4<f32> {
    // Not used yet — placeholder for Task 12
    return in.color;
}
```

**Step 2: Create the quad renderer module**

Create `src/renderer/mod.rs`:
```rust
pub mod quad;
```

Create `src/renderer/quad.rs`:

```rust
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Per-instance data for a quad. Sent to GPU each frame.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QuadInstance {
    pub pos: [f32; 2],   // top-left corner in pixels
    pub size: [f32; 2],  // width, height in pixels
    pub color: [f32; 4], // RGBA (values > 1.0 for HDR bloom contribution)
}

impl QuadInstance {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2, // pos
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2, // size
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4, // color
                },
            ],
        }
    }
}

/// Globals uniform buffer data.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Globals {
    pub screen_size: [f32; 2],
    pub _padding: [f32; 2], // uniform buffers must be 16-byte aligned
}

/// Manages the quad rendering pipeline.
pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
}

impl QuadRenderer {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
        });

        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Buffer"),
            contents: bytemuck::cast_slice(&[Globals {
                screen_size: [1200.0, 800.0],
                _padding: [0.0; 2],
            }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Globals Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[&globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[QuadInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_quad",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // Index buffer for a quad: two triangles from 4 vertices
        let indices: [u16; 6] = [0, 1, 2, 2, 1, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
            index_buffer,
        }
    }

    /// Update screen size uniform. Call on resize.
    pub fn update_globals(&self, queue: &wgpu::Queue, width: f32, height: f32) {
        queue.write_buffer(
            &self.globals_buffer,
            0,
            bytemuck::cast_slice(&[Globals {
                screen_size: [width, height],
                _padding: [0.0; 2],
            }]),
        );
    }

    /// Draw a batch of quad instances in the given render pass.
    pub fn draw<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instance_buffer: &'a wgpu::Buffer,
        instance_count: u32,
    ) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..instance_count);
    }
}
```

**Step 3: Integrate into State — render a test rectangle**

Modify `src/lib.rs`:
- Add `mod renderer;` at the top
- Add `QuadRenderer` and a test instance buffer to `State`
- In `State::new()`, create the `QuadRenderer` and a test `QuadInstance` buffer
- In `State::render()`, draw the test quad

Add to State struct:
```rust
quad_renderer: renderer::quad::QuadRenderer,
test_instance_buffer: wgpu::Buffer,
```

In `State::new()` after creating `config`:
```rust
let quad_renderer = renderer::quad::QuadRenderer::new(&device, surface_format);

let test_instances = [renderer::quad::QuadInstance {
    pos: [100.0, 100.0],
    size: [200.0, 50.0],
    color: [0.0, 0.5, 1.0, 1.0], // blue
}];
let test_instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Test Instance Buffer"),
    contents: bytemuck::cast_slice(&test_instances),
    usage: wgpu::BufferUsages::VERTEX,
});
```

In `State::render()`, replace the empty render pass with:
```rust
{
    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Render Pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        occlusion_query_set: None,
        timestamp_writes: None,
    });
    self.quad_renderer.draw(&mut render_pass, &self.test_instance_buffer, 1);
}
```

**Step 4: Build and verify native**

Run: `cargo run`
Expected: Window shows a blue rectangle (200x50px) at position (100,100) on black background

**Step 5: Build and verify wasm**

Run: `bash build.sh && cd web && python3 -m http.server 8080`
Open: http://localhost:8080
Expected: Same blue rectangle in browser

**Step 6: Commit**

```bash
git add src/renderer/ src/shaders/ src/lib.rs
git commit -m "feat: add instanced quad rendering pipeline"
```

---

## Task 5: Render the Piano Keyboard

**Files:**
- Modify: `src/lib.rs` (replace test quad with keyboard)

**Step 1: Generate keyboard instances in State::new()**

Remove `test_instance_buffer`. Add keyboard rendering logic. In `State::new()`:

```rust
use crate::keyboard;
use crate::renderer::quad::QuadInstance;

let keyboard_height = size.height as f32 * 0.2;
let keyboard_y = size.height as f32 - keyboard_height;
let keyboard_width = size.width as f32;

let mut keyboard_instances = Vec::new();

// White keys first (drawn underneath)
for pitch in 0..88u8 {
    if !keyboard::is_black_key(pitch) {
        let (x, w) = keyboard::key_rect(pitch, keyboard_width);
        keyboard_instances.push(QuadInstance {
            pos: [x, keyboard_y],
            size: [w - 1.0, keyboard_height], // -1px gap between keys
            color: [0.9, 0.9, 0.9, 1.0],     // light gray
        });
    }
}

// Black keys on top
for pitch in 0..88u8 {
    if keyboard::is_black_key(pitch) {
        let (x, w) = keyboard::key_rect(pitch, keyboard_width);
        keyboard_instances.push(QuadInstance {
            pos: [x, keyboard_y],
            size: [w, keyboard_height * 0.65], // black keys are shorter
            color: [0.15, 0.15, 0.15, 1.0],   // dark gray
        });
    }
}

let keyboard_instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Keyboard Instance Buffer"),
    contents: bytemuck::cast_slice(&keyboard_instances),
    usage: wgpu::BufferUsages::VERTEX,
});
let keyboard_instance_count = keyboard_instances.len() as u32;
```

Store `keyboard_instance_buffer` and `keyboard_instance_count` in State.

**Step 2: Draw keyboard in render()**

```rust
self.quad_renderer.draw(&mut render_pass, &self.keyboard_instance_buffer, self.keyboard_instance_count);
```

**Step 3: Build and verify**

Run: `cargo run`
Expected: 88-key piano keyboard at the bottom of the window (white keys with dark gaps, black keys overlaid)

**Step 4: Commit**

```bash
git add src/lib.rs
git commit -m "feat: render 88-key piano keyboard"
```

---

## Task 6: Falling Notes with Animation

**Files:**
- Modify: `src/lib.rs` (add time tracking, note rendering)

**Step 1: Add time tracking to State**

Add fields to State:
```rust
start_time: f64,
current_time: f64,
song: note::Song,
note_instance_buffer: wgpu::Buffer,
```

Initialize `start_time` in `State::new()`:
```rust
#[cfg(target_arch = "wasm32")]
let start_time = {
    let perf = web_sys::window().unwrap().performance().unwrap();
    perf.now() / 1000.0  // ms → seconds
};
#[cfg(not(target_arch = "wasm32"))]
let start_time = 0.0; // use std::time::Instant for native if needed

let song = note::demo_song();

// Pre-allocate note instance buffer (max ~200 visible notes)
let note_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("Note Instance Buffer"),
    size: (std::mem::size_of::<QuadInstance>() * 200) as u64,
    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
});
```

**Step 2: Update note positions each frame in update()**

```rust
fn update(&mut self) {
    // Get current time
    #[cfg(target_arch = "wasm32")]
    {
        let perf = web_sys::window().unwrap().performance().unwrap();
        self.current_time = perf.now() / 1000.0 - self.start_time;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // For native, increment by a fixed dt or use std::time
        self.current_time += 1.0 / 60.0;
    }

    let screen_w = self.size.width as f32;
    let screen_h = self.size.height as f32;
    let keyboard_height = screen_h * 0.2;
    let fall_area_height = screen_h - keyboard_height;
    let keyboard_y = fall_area_height;

    // Scroll speed: pixels per second
    let scroll_speed = 200.0;
    // How many seconds of notes are visible on screen
    let visible_duration = fall_area_height / scroll_speed;

    let t = self.current_time as f32;
    let mut instances = Vec::new();

    for n in &self.song.notes {
        // Note's bottom edge y-position (keyboard line = keyboard_y when note starts playing)
        let note_bottom_y = keyboard_y - (n.start_time + n.duration - t) * scroll_speed;
        let note_top_y = note_bottom_y - n.duration * scroll_speed;

        // Skip if completely off screen
        if note_bottom_y < 0.0 || note_top_y > keyboard_y {
            continue;
        }

        let (key_x, key_w) = keyboard::key_rect(n.pitch, screen_w);

        // Color: gradient by pitch (deep blue → cyan)
        let pitch_ratio = n.pitch as f32 / 87.0;
        let r = 0.1 + pitch_ratio * 0.2;
        let g = 0.3 + pitch_ratio * 0.5;
        let b = 0.8 + pitch_ratio * 0.2;
        let brightness = n.velocity;

        instances.push(QuadInstance {
            pos: [key_x, note_top_y.max(0.0)],
            size: [key_w, (note_bottom_y - note_top_y.max(0.0)).min(keyboard_y - note_top_y.max(0.0))],
            color: [r * brightness, g * brightness, b * brightness, 1.0],
        });
    }

    // Upload instances to GPU
    if !instances.is_empty() {
        self.queue.write_buffer(
            &self.note_instance_buffer,
            0,
            bytemuck::cast_slice(&instances),
        );
    }
    self.note_instance_count = instances.len() as u32;
}
```

Add `note_instance_count: u32` to State.

**Step 3: Draw notes before keyboard in render()**

```rust
// Draw notes first (behind keyboard)
if self.note_instance_count > 0 {
    self.quad_renderer.draw(&mut render_pass, &self.note_instance_buffer, self.note_instance_count);
}
// Draw keyboard on top
self.quad_renderer.draw(&mut render_pass, &self.keyboard_instance_buffer, self.keyboard_instance_count);
```

**Step 4: Build and verify**

Run: `cargo run`
Expected: Blue notes fall from top, hit the keyboard, and continue scrolling past. Keyboard stays at the bottom. Notes are different widths for black/white keys.

**Step 5: Commit**

```bash
git add src/lib.rs
git commit -m "feat: add falling note animation with demo song"
```

---

## Task 7: Active Key Highlighting

**Files:**
- Modify: `src/lib.rs` (rebuild keyboard instances each frame based on active notes)

**Step 1: Make keyboard dynamic — rebuild each frame**

Move keyboard instance generation into `update()`. For each key, check if any note is currently "hitting" the keyboard line. If so, make that key glow (brighter color).

In `update()`, after generating note instances:

```rust
// Determine active keys (notes currently touching keyboard line)
let mut active_keys = [false; 88];
for n in &self.song.notes {
    let note_start_in_screen = t >= n.start_time;
    let note_end_in_screen = t < n.start_time + n.duration;
    if note_start_in_screen && note_end_in_screen {
        active_keys[n.pitch as usize] = true;
    }
}

// Rebuild keyboard instances
let mut kb_instances = Vec::new();
// White keys
for pitch in 0..88u8 {
    if !keyboard::is_black_key(pitch) {
        let (x, w) = keyboard::key_rect(pitch, screen_w);
        let color = if active_keys[pitch as usize] {
            [0.3, 0.6, 1.0, 1.0] // glowing blue
        } else {
            [0.9, 0.9, 0.9, 1.0] // normal white
        };
        kb_instances.push(QuadInstance {
            pos: [x, keyboard_y],
            size: [w - 1.0, keyboard_height],
            color,
        });
    }
}
// Black keys
for pitch in 0..88u8 {
    if keyboard::is_black_key(pitch) {
        let (x, w) = keyboard::key_rect(pitch, screen_w);
        let color = if active_keys[pitch as usize] {
            [0.2, 0.4, 0.9, 1.0] // glowing blue
        } else {
            [0.15, 0.15, 0.15, 1.0] // normal dark
        };
        kb_instances.push(QuadInstance {
            pos: [x, keyboard_y],
            size: [w, keyboard_height * 0.65],
            color,
        });
    }
}

self.queue.write_buffer(
    &self.keyboard_instance_buffer,
    0,
    bytemuck::cast_slice(&kb_instances),
);
self.keyboard_instance_count = kb_instances.len() as u32;
```

Change `keyboard_instance_buffer` to `COPY_DST` usage in `State::new()`.

**Step 2: Build and verify**

Run: `cargo run`
Expected: Keys light up blue when notes hit them, return to normal when note passes.

**Step 3: Commit**

```bash
git add src/lib.rs
git commit -m "feat: highlight active piano keys when notes hit"
```

---

## Task 8: Offscreen Rendering + Bloom Extract

**Files:**
- Create: `src/renderer/bloom.rs`
- Create: `src/shaders/bloom_extract.wgsl`
- Create: `src/shaders/fullscreen.wgsl`
- Modify: `src/renderer/mod.rs`
- Modify: `src/lib.rs`

This is the first post-processing pass. We render the scene to an offscreen texture, then extract bright pixels.

**Step 1: Create fullscreen vertex shader**

Create `src/shaders/fullscreen.wgsl` — this will be included in all post-process shaders:

Not possible to include in wgpu 0.19. Instead, each post-process shader contains its own fullscreen vertex function. The vertex shader is:

```wgsl
struct FullscreenOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) idx: u32) -> FullscreenOutput {
    // Fullscreen triangle covering [-1,1] clip space
    let uv = vec2<f32>(f32(idx >> 1u), f32(idx & 1u)) * 2.0;
    let pos = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    return FullscreenOutput(vec4<f32>(pos, 0.0, 1.0), uv);
}
```

**Step 2: Create bloom_extract.wgsl**

Create `src/shaders/bloom_extract.wgsl`:

```wgsl
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
    // Soft threshold at 0.5
    let threshold = 0.5;
    let knee = 0.2;
    let softness = clamp(lum - threshold + knee, 0.0, 2.0 * knee);
    let contribution = softness * softness / (4.0 * knee + 0.0001);
    let brightness = max(lum - threshold, contribution);
    let factor = brightness / max(lum, 0.0001);
    return vec4<f32>(color * factor, 1.0);
}
```

**Step 3: Create bloom renderer module**

Create `src/renderer/bloom.rs`:

```rust
/// Bloom post-processing pipeline.
/// Creates offscreen textures and manages the multi-pass bloom effect.

pub struct BloomRenderer {
    // Textures
    scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    bloom_texture: wgpu::Texture,
    bloom_view: wgpu::TextureView,
    sampler: wgpu::Sampler,

    // Pipelines
    extract_pipeline: wgpu::RenderPipeline,
    extract_bind_group: wgpu::BindGroup,

    // Bind group layout (shared by blur + composite)
    texture_bind_group_layout: wgpu::BindGroupLayout,
}

impl BloomRenderer {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let offscreen_format = wgpu::TextureFormat::Rgba8Unorm;

        // Create textures
        let scene_texture = Self::create_texture(device, width, height, offscreen_format, "Scene");
        let scene_view = scene_texture.create_view(&Default::default());
        let bloom_texture = Self::create_texture(device, width, height, offscreen_format, "Bloom");
        let bloom_view = bloom_texture.create_view(&Default::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Shared bind group layout for texture + sampler
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Texture BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        // Extract pipeline
        let extract_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Bloom Extract Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/bloom_extract.wgsl").into(),
            ),
        });

        let extract_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Extract Pipeline Layout"),
                bind_group_layouts: &[&texture_bind_group_layout],
                push_constant_ranges: &[],
            });

        let extract_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Bloom Extract Pipeline"),
            layout: Some(&extract_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &extract_shader,
                entry_point: "vs_fullscreen",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &extract_shader,
                entry_point: "fs_extract",
                targets: &[Some(wgpu::ColorTargetState {
                    format: offscreen_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let extract_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Extract Bind Group"),
            layout: &texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Self {
            scene_texture,
            scene_view,
            bloom_texture,
            bloom_view,
            sampler,
            extract_pipeline,
            extract_bind_group,
            texture_bind_group_layout,
        }
    }

    fn create_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        label: &str,
    ) -> wgpu::Texture {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Returns the scene texture view — render your scene into this.
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }

    /// Run the bright-extract pass: scene_texture → bloom_texture.
    pub fn extract_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Extract Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bloom_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.extract_pipeline);
        pass.set_bind_group(0, &self.extract_bind_group, &[]);
        pass.draw(0..3, 0..1); // fullscreen triangle
    }

    /// Getter for texture_bind_group_layout (used by blur + composite).
    pub fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bind_group_layout
    }

    /// Resize textures on window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let format = wgpu::TextureFormat::Rgba8Unorm;
        self.scene_texture = Self::create_texture(device, width, height, format, "Scene");
        self.scene_view = self.scene_texture.create_view(&Default::default());
        self.bloom_texture = Self::create_texture(device, width, height, format, "Bloom");
        self.bloom_view = self.bloom_texture.create_view(&Default::default());

        self.extract_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Extract Bind Group"),
            layout: &self.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.scene_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
    }
}
```

**Step 4: Update mod.rs**

Add to `src/renderer/mod.rs`:
```rust
pub mod bloom;
```

**Step 5: Integrate bloom into State — render scene to offscreen texture**

In `State::new()`, create BloomRenderer. In `State::render()`:
1. Render scene (notes + keyboard) into `bloom.scene_view()` instead of swapchain
2. Run `bloom.extract_pass()`
3. For now, just blit the scene texture to screen (composite comes later)

This requires a temporary "passthrough" pipeline that samples scene_texture and outputs to swapchain. We'll replace this with the full composite in Task 11.

**Step 6: Build and verify**

Run: `cargo run`
Expected: Same visual as before (scene rendered via offscreen texture). If you temporarily render the bloom texture instead, you should see only the bright note/key pixels.

**Step 7: Commit**

```bash
git add src/renderer/bloom.rs src/shaders/bloom_extract.wgsl src/renderer/mod.rs src/lib.rs
git commit -m "feat: add offscreen scene rendering and bloom bright-extract pass"
```

---

## Task 9: Gaussian Blur Passes

**Files:**
- Create: `src/shaders/blur.wgsl`
- Modify: `src/renderer/bloom.rs` (add blur pipelines and textures)

**Step 1: Create blur.wgsl**

Create `src/shaders/blur.wgsl`:

```wgsl
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
    direction: vec2<f32>,  // (1/w, 0) for H, (0, 1/h) for V
    _padding: vec2<f32>,
};
@group(1) @binding(0) var<uniform> blur: BlurUniforms;

// 9-tap Gaussian blur
@fragment
fn fs_blur(in: FullscreenOutput) -> @location(0) vec4<f32> {
    // Gaussian weights for sigma ≈ 4.0
    let offsets = array<f32, 4>(1.0, 2.0, 3.0, 4.0);
    let weights = array<f32, 4>(0.2270, 0.1945, 0.1216, 0.0541);

    var color = textureSample(t_input, s_input, in.uv).rgb * 0.2270; // center weight

    for (var i = 0; i < 4; i++) {
        let offset = blur.direction * offsets[i];
        let w = weights[i];
        color += textureSample(t_input, s_input, in.uv + offset).rgb * w;
        color += textureSample(t_input, s_input, in.uv - offset).rgb * w;
    }

    return vec4<f32>(color, 1.0);
}
```

**Step 2: Add blur infrastructure to BloomRenderer**

Add to `BloomRenderer`:
- Two additional textures: `blur_h_texture` (for horizontal blur output) and reuse `bloom_texture` for final blur output
- `blur_pipeline` (shared for H and V)
- `blur_h_uniform_buffer`, `blur_v_uniform_buffer`
- `blur_h_bind_groups`, `blur_v_bind_groups`
- Methods: `blur_h_pass()`, `blur_v_pass()`

The blur uniform for horizontal: `direction: [1.0 / width, 0.0]`
The blur uniform for vertical: `direction: [0.0, 1.0 / height]`

Pipeline flow:
1. Extract: scene → bloom_texture (bright pixels)
2. Blur H: bloom_texture → blur_h_texture
3. Blur V: blur_h_texture → bloom_texture (final blurred bloom)

**Step 3: Build and verify**

Run: `cargo run`
Expected: Bloom texture now contains a soft blurred glow version of the bright pixels.

**Step 4: Commit**

```bash
git add src/shaders/blur.wgsl src/renderer/bloom.rs
git commit -m "feat: add horizontal and vertical Gaussian blur passes"
```

---

## Task 10: Composite Pass — Final Bloom

**Files:**
- Create: `src/shaders/composite.wgsl`
- Modify: `src/renderer/bloom.rs` (add composite pipeline)
- Modify: `src/lib.rs` (use composite pass for final output)

**Step 1: Create composite.wgsl**

Create `src/shaders/composite.wgsl`:

```wgsl
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

    // Additive bloom blending
    let bloom_intensity = 1.5;
    let color = scene + bloom * bloom_intensity;

    // Simple Reinhard tone mapping
    let mapped = color / (color + vec3<f32>(1.0));

    return vec4<f32>(mapped, 1.0);
}
```

**Step 2: Add composite pipeline to BloomRenderer**

Add `composite_pipeline`, `composite_scene_bind_group`, `composite_bloom_bind_group`.

Add method `composite_pass()` that renders to the swapchain view, sampling both scene_texture and bloom_texture.

**Step 3: Wire up in State::render()**

Final render flow:
```rust
fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
    let output = self.surface.get_current_texture()?;
    let screen_view = output.texture.create_view(&Default::default());
    let mut encoder = self.device.create_command_encoder(&Default::default());

    // Pass 1: Scene → offscreen texture
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: self.bloom.scene_view(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        if self.note_instance_count > 0 {
            self.quad_renderer.draw(&mut pass, &self.note_instance_buffer, self.note_instance_count);
        }
        self.quad_renderer.draw(&mut pass, &self.keyboard_instance_buffer, self.keyboard_instance_count);
    }

    // Pass 2: Bright extract
    self.bloom.extract_pass(&mut encoder);

    // Pass 3: Horizontal blur
    self.bloom.blur_h_pass(&mut encoder);

    // Pass 4: Vertical blur
    self.bloom.blur_v_pass(&mut encoder);

    // Pass 5: Composite → screen
    self.bloom.composite_pass(&mut encoder, &screen_view);

    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    Ok(())
}
```

**Step 4: Build and verify**

Run: `cargo run`
Expected: Notes and active keys now have a visible glow/bloom effect! The background stays dark, notes glow blue/cyan.

**Step 5: Build and verify wasm**

Run: `bash build.sh` + open in browser
Expected: Same bloom effect in the browser

**Step 6: Commit**

```bash
git add src/shaders/composite.wgsl src/renderer/bloom.rs src/lib.rs
git commit -m "feat: add composite pass — bloom rendering complete"
```

---

## Task 11: Particle System

**Files:**
- Create: `src/renderer/particles.rs`
- Create: `src/shaders/particle.wgsl`
- Modify: `src/renderer/mod.rs`
- Modify: `src/lib.rs`

**Step 1: Create particle.wgsl**

Create `src/shaders/particle.wgsl`:

```wgsl
struct Globals {
    screen_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> globals: Globals;

struct ParticleInstance {
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
    instance: ParticleInstance,
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
    // Soft circle: distance from center
    let center = vec2<f32>(0.5, 0.5);
    let dist = length(in.uv - center) * 2.0;
    let alpha = 1.0 - smoothstep(0.0, 1.0, dist);
    return vec4<f32>(in.color.rgb * alpha, 1.0);
}
```

**Step 2: Create particle renderer**

Create `src/renderer/particles.rs`:

```rust
use bytemuck::{Pod, Zeroable};
use super::quad::QuadInstance; // Reuse instance struct

/// Single particle in the system.
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,     // remaining life in seconds
    pub max_life: f32,
    pub size: f32,
    pub color: [f32; 3],
}

/// Manages particle spawning, updating, and rendering.
pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pipeline: wgpu::RenderPipeline,
    index_buffer: wgpu::Buffer,
}

impl ParticleSystem {
    pub fn new(
        device: &wgpu::Device,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/particle.wgsl").into()),
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Pipeline Layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_particle",
                buffers: &[QuadInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_particle",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    // Additive blending: particle brightens what's behind it
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let indices: [u16; 6] = [0, 1, 2, 2, 1, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            particles: Vec::new(),
            pipeline,
            index_buffer,
        }
    }

    /// Spawn particles at the given position with the given color.
    pub fn spawn(&mut self, x: f32, y: f32, color: [f32; 3], count: usize) {
        use std::f32::consts::PI;
        for i in 0..count {
            let angle = (i as f32 / count as f32) * 2.0 * PI + (self.particles.len() as f32 * 0.1);
            let speed = 30.0 + (i as f32 * 7.0) % 50.0;
            self.particles.push(Particle {
                x,
                y,
                vx: angle.cos() * speed * 0.5,
                vy: -angle.sin().abs() * speed, // mostly upward
                life: 0.5 + (i as f32 * 0.03) % 0.5,
                max_life: 0.8,
                size: 4.0 + (i as f32 * 2.0) % 6.0,
                color,
            });
        }
    }

    /// Update all particles. Call once per frame with delta time.
    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 20.0 * dt; // slight gravity
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Generate instances for rendering.
    pub fn instances(&self) -> Vec<QuadInstance> {
        self.particles
            .iter()
            .map(|p| {
                let alpha = (p.life / p.max_life).clamp(0.0, 1.0);
                let s = p.size * alpha;
                QuadInstance {
                    pos: [p.x - s / 2.0, p.y - s / 2.0],
                    size: [s, s],
                    color: [
                        p.color[0] * alpha,
                        p.color[1] * alpha,
                        p.color[2] * alpha,
                        1.0,
                    ],
                }
            })
            .collect()
    }
}
```

Note: `QuadInstance::desc()` needs to be made `pub`. Update `src/renderer/quad.rs` to make the `desc()` method public.

**Step 3: Integrate particles into State**

In `update()`:
- For each active note, spawn 1-2 particles at the contact point per frame
- Call `particle_system.update(dt)`

In `render()`:
- After drawing notes and keyboard in the scene pass, draw particles with the particle pipeline (additive blending)

**Step 4: Build and verify**

Run: `cargo run`
Expected: Small glowing particles emit from note-keyboard contact points, drift upward, and fade out. The bloom effect makes them glow.

**Step 5: Commit**

```bash
git add src/renderer/particles.rs src/shaders/particle.wgsl src/renderer/mod.rs src/lib.rs
git commit -m "feat: add particle system with bloom-enhanced glow trails"
```

---

## Task 12: Audio — Piano Sample Playback

**Files:**
- Create: `src/audio.rs`
- Modify: `src/lib.rs`
- Create: `web/assets/samples/` directory with sample files

**Step 1: Create audio module (wasm32-only)**

Create `src/audio.rs`:

```rust
//! WebAudio-based piano sample playback.
//! Only compiled for wasm32 target.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioBuffer, AudioBufferSourceNode, Response};

/// Manages audio playback with pre-loaded piano samples.
pub struct AudioPlayer {
    ctx: AudioContext,
    /// One sample per octave (C1 through C7 = 7 samples)
    samples: Vec<Option<AudioBuffer>>,
}

impl AudioPlayer {
    /// Create a new AudioPlayer. Call `load_samples()` after construction.
    pub fn new() -> Result<Self, JsValue> {
        let ctx = AudioContext::new()?;
        Ok(Self {
            ctx,
            samples: vec![None; 7], // C1..C7
        })
    }

    /// Resume AudioContext (required after user gesture).
    pub async fn resume(&self) -> Result<(), JsValue> {
        JsFuture::from(self.ctx.resume()?).await?;
        Ok(())
    }

    /// Load a single sample from a URL into the given octave slot.
    pub async fn load_sample(&mut self, octave_index: usize, url: &str) -> Result<(), JsValue> {
        let window = web_sys::window().unwrap();

        let mut opts = web_sys::RequestInit::new();
        opts.method("GET");
        opts.mode(web_sys::RequestMode::SameOrigin);

        let request = web_sys::Request::new_with_str_and_init(url, &opts)?;
        let resp: Response = JsFuture::from(window.fetch_with_request(&request))
            .await?
            .dyn_into()?;

        let array_buffer: js_sys::ArrayBuffer = JsFuture::from(resp.array_buffer()?)
            .await?
            .dyn_into()?;

        let audio_buffer: AudioBuffer = JsFuture::from(self.ctx.decode_audio_data(&array_buffer)?)
            .await?
            .dyn_into()?;

        if octave_index < self.samples.len() {
            self.samples[octave_index] = Some(audio_buffer);
        }
        Ok(())
    }

    /// Play a note at the given pitch (0-87).
    /// Pitch-shifts the nearest octave sample.
    pub fn play_note(&self, pitch: u8, velocity: f32) -> Result<(), JsValue> {
        // Pitch 0 = A0 (MIDI 21), pitch 87 = C8 (MIDI 108)
        // Our samples are at C1(pitch 3), C2(15), C3(27), C4(39), C5(51), C6(63), C7(75)
        let sample_pitches: [u8; 7] = [3, 15, 27, 39, 51, 63, 75];

        // Find nearest sample
        let mut best_idx = 0;
        let mut best_dist = 128i16;
        for (i, &sp) in sample_pitches.iter().enumerate() {
            let dist = (pitch as i16 - sp as i16).abs();
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }

        if let Some(buffer) = &self.samples[best_idx] {
            let source: AudioBufferSourceNode = self.ctx.create_buffer_source()?;
            source.set_buffer(Some(buffer));

            // Pitch shift: each semitone = 2^(1/12) ratio
            let semitone_diff = pitch as f32 - sample_pitches[best_idx] as f32;
            let rate = 2.0_f32.powf(semitone_diff / 12.0);
            source.playback_rate().set_value(rate);

            // Volume via gain (simplified: just use playback)
            source.connect_with_audio_node(&self.ctx.destination())?;
            source.start()?;
        }

        Ok(())
    }
}
```

**Step 2: Add module to lib.rs**

```rust
#[cfg(target_arch = "wasm32")]
mod audio;
```

**Step 3: Integrate audio into the update loop**

In `update()`, when a note first becomes active (crosses the keyboard line), call `audio_player.play_note()`. Track which notes have already been triggered to avoid replaying.

Add to State:
```rust
#[cfg(target_arch = "wasm32")]
audio_player: Option<audio::AudioPlayer>,
triggered_notes: Vec<bool>, // one per song note, true if already triggered
```

In `update()`:
```rust
for (i, n) in self.song.notes.iter().enumerate() {
    let is_active = t >= n.start_time && t < n.start_time + n.duration;
    if is_active && !self.triggered_notes[i] {
        self.triggered_notes[i] = true;
        #[cfg(target_arch = "wasm32")]
        if let Some(ref player) = self.audio_player {
            let _ = player.play_note(n.pitch, n.velocity);
        }
    }
}
```

**Step 4: Add sample files**

Create `web/assets/samples/` directory. Download or create 7 piano samples:
- `c1.mp3`, `c2.mp3`, `c3.mp3`, `c4.mp3`, `c5.mp3`, `c6.mp3`, `c7.mp3`

Sources for free piano samples:
- https://freesound.org (search "piano C4", CC0 license)
- Or generate with a synthesizer tool

**Step 5: Load samples on startup**

In `State::new()` (wasm32 only):
```rust
#[cfg(target_arch = "wasm32")]
{
    let mut player = audio::AudioPlayer::new().ok();
    // Sample loading happens async — do it after first user interaction
    // For now, samples must be loaded via a user-triggered event
}
```

Note: Browsers require a user gesture before AudioContext can play. Add a "Click to start" overlay in `web/index.html` that calls `audio_player.resume()` and loads samples.

**Step 6: Build and verify in browser**

Run: `bash build.sh` + open in browser
Expected: Click to start → notes fall → piano sounds play when notes hit keyboard

**Step 7: Commit**

```bash
git add src/audio.rs src/lib.rs web/
git commit -m "feat: add WebAudio piano sample playback"
```

---

## Task 13: Polish and Song Loop

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/note.rs`

**Step 1: Loop the demo song**

In `update()`, when `current_time` exceeds the last note's end time + 2 seconds, reset:
```rust
let song_duration = self.song.notes.iter()
    .map(|n| n.start_time + n.duration)
    .fold(0.0_f32, f32::max);

if t > song_duration + 2.0 {
    self.start_time = /* current wall time */;
    self.triggered_notes.fill(false);
}
```

**Step 2: Handle window resize properly**

In `resize()`:
- Recreate bloom textures via `self.bloom.resize()`
- Update globals via `self.quad_renderer.update_globals()`
- Rebuild keyboard instances (they depend on screen width)

**Step 3: Build and verify complete app**

Run: `bash build.sh` + open in browser
Expected:
- Notes fall with blue/cyan glow
- Keyboard keys light up
- Particles at contact points
- Bloom/glow post-processing
- Piano sample sounds
- Song loops

**Step 4: Final commit**

```bash
git add -A
git commit -m "feat: polish — song loop, resize handling, complete piano fall app"
```

---

## Summary

| Task | What | Key Files |
|------|------|-----------|
| 1 | Project scaffold | Cargo.toml, lib.rs, build.sh, index.html |
| 2 | Keyboard layout (pure math + tests) | keyboard.rs |
| 3 | Note model + demo song (tests) | note.rs |
| 4 | Instanced quad renderer | renderer/quad.rs, scene.wgsl |
| 5 | Render keyboard | lib.rs |
| 6 | Falling notes + animation | lib.rs |
| 7 | Active key highlighting | lib.rs |
| 8 | Offscreen rendering + bloom extract | renderer/bloom.rs, bloom_extract.wgsl |
| 9 | Gaussian blur passes | blur.wgsl, renderer/bloom.rs |
| 10 | Composite pass (bloom complete) | composite.wgsl, renderer/bloom.rs |
| 11 | Particle system | renderer/particles.rs, particle.wgsl |
| 12 | Audio (WebAudio samples) | audio.rs |
| 13 | Polish + song loop | lib.rs |
