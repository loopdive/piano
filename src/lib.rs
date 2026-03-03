#[cfg(target_arch = "wasm32")]
mod audio;

pub mod keyboard;
pub mod note;
pub mod renderer;

use std::iter;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use winit::{
    application::ApplicationHandler,
    event::*,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use renderer::keys::KeyInstance3D;
use renderer::quad::{QuadInstance, LabelInstance};

#[derive(Clone, Copy, PartialEq)]
enum DragAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq)]
enum NoteTheme {
    Rainbow,
    Ice,
}

fn default_theme() -> NoteTheme {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(search) = win.location().search() {
                if search.contains("theme=ice") {
                    return NoteTheme::Ice;
                }
            }
        }
    }
    NoteTheme::Rainbow
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h6 = h * 6.0;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r1, g1, b1) = if h6 < 1.0 {
        (c, x, 0.0)
    } else if h6 < 2.0 {
        (x, c, 0.0)
    } else if h6 < 3.0 {
        (0.0, c, x)
    } else if h6 < 4.0 {
        (0.0, x, c)
    } else if h6 < 5.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c * 0.5;
    (r1 + m, g1 + m, b1 + m)
}

fn note_color(pitch: u8, velocity: f32, theme: NoteTheme) -> [f32; 4] {
    match theme {
        NoteTheme::Rainbow => {
            let note = (pitch + 9) % 12;
            let hue = note as f32 / 12.0;
            let dim = 0.4 + velocity * 0.6;
            let (r, g, b) = hsl_to_rgb(hue, 0.7, 0.40 * dim);
            [r, g, b, 1.0]
        }
        NoteTheme::Ice => {
            let pr = pitch as f32 / 87.0;
            let dim = 0.35 + velocity * 0.65;
            let r = (0.08 + pr * 0.25) * dim;
            let g = (0.45 + pr * 0.35) * dim;
            let b = (0.95 + pr * 0.05) * dim;
            [r, g, b, 1.0]
        }
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// Global pending MIDI data — set by JS, consumed by the render loop
static PENDING_MIDI: Mutex<Option<Vec<u8>>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = window, js_name = _setSongTitle)]
    fn set_song_title(title: &str);
}
/// External code (e.g. load_midi) sets this to wake the render loop
static REDRAW_FLAG: AtomicBool = AtomicBool::new(false);

/// Check if URL has ?keyboard=quad (WASM only, defaults to false = use 3D keys)
fn use_quad_keyboard() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(win) = web_sys::window() {
            if let Ok(search) = win.location().search() {
                return search.contains("keyboard=quad");
            }
        }
    }
    false
}

/// Load a MIDI file from JavaScript. The bytes will be parsed on the next frame.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn load_midi(data: &[u8]) {
    *PENDING_MIDI.lock().unwrap() = Some(data.to_vec());
    REDRAW_FLAG.store(true, Ordering::Relaxed);
    log::info!("MIDI file queued ({} bytes)", data.len());
}

// -- GPU state --

struct State {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: Arc<Window>,
    quad_renderer: renderer::quad::QuadRenderer,
    screen_quad_renderer: renderer::quad::QuadRenderer,
    bloom: renderer::bloom::BloomRenderer,
    particle_system: renderer::particles::ParticleSystem,
    key_renderer: renderer::keys::KeyRenderer,
    use_3d_keys: bool,
    keyboard_instance_buffer: wgpu::Buffer,
    keyboard_instance_count: u32,
    overlay_instance_buffer: wgpu::Buffer,
    overlay_instance_count: u32,
    key_instances_3d: Vec<KeyInstance3D>,
    #[allow(dead_code)]
    start_time: f64,
    current_time: f64,
    last_wall_time: f64,
    song: note::Song,
    note_instance_buffer: wgpu::Buffer,
    note_instance_count: u32,
    label_instance_buffer: wgpu::Buffer,
    label_instance_count: u32,
    #[cfg(target_arch = "wasm32")]
    audio_player: Option<audio::AudioPlayer>,
    triggered_notes: Vec<bool>,
    key_press_state: [f32; 88],
    surface_configured: bool,
    paused: bool,
    audio_unlocked: bool,
    waiting_for_samples: bool,
    rewind_target: Option<f64>,
    // Horizontal scroll
    h_offset: f32,
    h_velocity: f64,
    // Drag state
    cursor_x: f64,
    cursor_y: f64,
    drag_active: bool,
    drag_start_x: f64,
    drag_start_y: f64,
    drag_prev_x: f64,
    drag_prev_y: f64,
    drag_axis: Option<DragAxis>,
    scroll_velocity: f64,
    was_paused_before_drag: bool,
    theme: NoteTheme,
    // Zoom
    keyboard_zoom: f32,
    // Multi-touch pinch tracking
    touches: std::collections::HashMap<u64, (f64, f64)>,
    pinch_base_dist: f64,
    pinch_base_zoom: f32,
    // Modifier key state (for Ctrl+scroll zoom)
    modifiers: winit::event::Modifiers,
}

impl State {
    async fn new(window: Arc<Window>) -> State {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();
        let backend = adapter.get_info().backend;
        log::info!("Using backend: {:?}", backend);
        let required_limits = match backend {
            wgpu::Backend::BrowserWebGpu => wgpu::Limits::default(),
            wgpu::Backend::Gl => wgpu::Limits::downlevel_webgl2_defaults(),
            _ => wgpu::Limits::default(),
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits,
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
                experimental_features: Default::default(),
            })
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

        // QuadRenderer for offscreen (notes/particles → bloom source)
        let quad_renderer = renderer::quad::QuadRenderer::new(
            &device,
            &queue,
            renderer::bloom::BloomRenderer::offscreen_format(),
        );

        // QuadRenderer for screen (keyboard BG → swapchain, drawn after composite)
        let screen_quad_renderer = renderer::quad::QuadRenderer::new(
            &device,
            &queue,
            surface_format,
        );

        let bloom = renderer::bloom::BloomRenderer::new(
            &device,
            size.width,
            size.height,
            surface_format,
        );

        let particle_system = renderer::particles::ParticleSystem::new(
            &device,
            quad_renderer.globals_bind_group_layout(),
            renderer::bloom::BloomRenderer::offscreen_format(),
        );

        // KeyRenderer targets swapchain format (drawn after composite, on top of bloom)
        let key_renderer = renderer::keys::KeyRenderer::new(
            &device,
            surface_format,
            size.width,
            size.height,
        );
        let use_3d_keys = !use_quad_keyboard();

        let overlay_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Overlay Instances"),
            size: (16 * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let keyboard_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Keyboard Instances"),
            size: (400 * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let start_time = 0.0;
        let song = note::default_song();
        #[cfg(target_arch = "wasm32")]
        set_song_title(&song.title);
        let h_offset = Self::auto_center_offset(&song, size.width as f32);
        let triggered_notes = vec![false; song.notes.len()];

        #[cfg(target_arch = "wasm32")]
        let audio_player = audio::AudioPlayer::new().ok();
        let note_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Note Instances"),
            size: (2000 * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let label_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Label Instances"),
            size: (2000 * std::mem::size_of::<LabelInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            surface, device, queue, config, size, window,
            quad_renderer, screen_quad_renderer, bloom, particle_system,
            key_renderer, use_3d_keys,
            keyboard_instance_buffer, keyboard_instance_count: 0,
            overlay_instance_buffer, overlay_instance_count: 0,
            key_instances_3d: Vec::new(),
            start_time, current_time: 0.0, last_wall_time: 0.0, song, note_instance_buffer,
            note_instance_count: 0, label_instance_buffer, label_instance_count: 0,
            #[cfg(target_arch = "wasm32")]
            audio_player,
            triggered_notes,
            key_press_state: [0.0; 88],
            surface_configured: false,
            paused: true,
            audio_unlocked: false,
            waiting_for_samples: false,
            rewind_target: None,
            h_offset,
            h_velocity: 0.0,
            cursor_x: 0.0,
            cursor_y: 0.0,
            drag_active: false,
            drag_start_x: 0.0,
            drag_start_y: 0.0,
            drag_prev_x: 0.0,
            drag_prev_y: 0.0,
            drag_axis: None,
            scroll_velocity: 0.0,
            was_paused_before_drag: false,
            theme: default_theme(),
            keyboard_zoom: 1.0,
            touches: std::collections::HashMap::new(),
            pinch_base_dist: 0.0,
            pinch_base_zoom: 1.0,
            modifiers: winit::event::Modifiers::default(),
        }
    }

    /// Check if the viewport has changed (iOS PWA orientation changes may not fire Resized events).
    #[cfg(target_arch = "wasm32")]
    fn check_viewport_resize(&mut self) {
        let win = web_sys::window().unwrap();
        let dpr = win.device_pixel_ratio();
        let css_w = win.inner_width().unwrap().as_f64().unwrap();
        let css_h = win.inner_height().unwrap().as_f64().unwrap();
        let pw = (css_w * dpr) as u32;
        let ph = (css_h * dpr) as u32;
        if pw != self.size.width || ph != self.size.height {
            self.resize(winit::dpi::PhysicalSize::new(pw, ph));
        }
    }

    /// Compute the horizontal offset to center the song's note range on screen.
    fn auto_center_offset(song: &note::Song, screen_w: f32) -> f32 {
        if song.notes.is_empty() { return 0.0; }
        let min_pitch = song.notes.iter().map(|n| n.pitch).min().unwrap();
        let max_pitch = song.notes.iter().map(|n| n.pitch).max().unwrap();
        // Only center if notes don't span the full keyboard
        if min_pitch <= keyboard::VISIBLE_START + 5 && max_pitch >= keyboard::VISIBLE_END - 5 {
            return 0.0;
        }
        let (x_min, _) = keyboard::key_rect(min_pitch, screen_w);
        let (x_max, w_max) = keyboard::key_rect(max_pitch, screen_w);
        let note_center = (x_min + x_max + w_max) / 2.0;
        screen_w / 2.0 - note_center
    }

    fn clamp_h_offset(&self, offset: f32) -> f32 {
        let screen_w = self.size.width as f32;
        let max_offset = screen_w * (self.keyboard_zoom - 1.0) * 0.5 + screen_w * 0.3;
        offset.clamp(-max_offset, max_offset)
    }

    #[allow(unused_variables)]
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        // On WASM, recalculate canvas buffer size from CSS viewport * DPR
        #[cfg(target_arch = "wasm32")]
        let new_size = {
            use winit::platform::web::WindowExtWebSys;
            let canvas = self.window.canvas().unwrap();
            let win = web_sys::window().unwrap();
            let dpr = win.device_pixel_ratio();
            let css_w = win.inner_width().unwrap().as_f64().unwrap();
            let css_h = win.inner_height().unwrap().as_f64().unwrap();
            let pw = (css_w * dpr) as u32;
            let ph = (css_h * dpr) as u32;
            canvas.set_width(pw);
            canvas.set_height(ph);
            winit::dpi::PhysicalSize::new(pw, ph)
        };
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.bloom
                .resize(&self.device, new_size.width, new_size.height);
            self.key_renderer
                .resize(&self.device, new_size.width, new_size.height);
            self.quad_renderer.update_globals(
                &self.queue,
                new_size.width as f32,
                new_size.height as f32,
            );
            self.screen_quad_renderer.update_globals(
                &self.queue,
                new_size.width as f32,
                new_size.height as f32,
            );
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            // Track cursor position for mouse drag
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_x = position.x;
                self.cursor_y = position.y;
                return self.drag_active;
            }
            // Mouse drag start/end
            WindowEvent::MouseInput {
                state: elem_state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                match elem_state {
                    ElementState::Pressed => {
                        if !self.audio_unlocked {
                            self.audio_unlocked = true;
                            #[cfg(target_arch = "wasm32")]
                            if let Some(ref player) = self.audio_player {
                                let _ = player.resume();
                            }
                            self.waiting_for_samples = true;
                        }
                        self.drag_active = true;
                        self.drag_start_x = self.cursor_x;
                        self.drag_start_y = self.cursor_y;
                        self.drag_prev_x = self.cursor_x;
                        self.drag_prev_y = self.cursor_y;
                        self.drag_axis = None;
                        self.scroll_velocity = 0.0;
                        self.h_velocity = 0.0;
                        self.was_paused_before_drag = self.paused;
                        self.rewind_target = None;
                        return true;
                    }
                    ElementState::Released => {
                        self.drag_active = false;
                        return true;
                    }
                }
            }
            // Touch: single-finger drag + multi-finger pinch-to-zoom
            WindowEvent::Touch(touch) => {
                match touch.phase {
                    winit::event::TouchPhase::Started => {
                        if !self.audio_unlocked {
                            self.audio_unlocked = true;
                            #[cfg(target_arch = "wasm32")]
                            if let Some(ref player) = self.audio_player {
                                let _ = player.resume();
                            }
                            self.waiting_for_samples = true;
                        }
                        self.touches.insert(touch.id, (touch.location.x, touch.location.y));
                        if self.touches.len() >= 2 {
                            // Start pinch: record baseline distance and zoom
                            let pts: Vec<(f64, f64)> = self.touches.values().copied().collect();
                            let dx = pts[0].0 - pts[1].0;
                            let dy = pts[0].1 - pts[1].1;
                            self.pinch_base_dist = (dx * dx + dy * dy).sqrt().max(1.0);
                            self.pinch_base_zoom = self.keyboard_zoom;
                            self.drag_active = false; // suppress single-finger drag during pinch
                        } else {
                            // Single touch — start drag
                            self.cursor_x = touch.location.x;
                            self.cursor_y = touch.location.y;
                            self.drag_active = true;
                            self.drag_start_x = self.cursor_x;
                            self.drag_start_y = self.cursor_y;
                            self.drag_prev_x = self.cursor_x;
                            self.drag_prev_y = self.cursor_y;
                            self.drag_axis = None;
                            self.scroll_velocity = 0.0;
                            self.h_velocity = 0.0;
                            self.was_paused_before_drag = self.paused;
                            self.rewind_target = None;
                        }
                        return true;
                    }
                    winit::event::TouchPhase::Moved => {
                        self.touches.insert(touch.id, (touch.location.x, touch.location.y));
                        if self.touches.len() >= 2 {
                            // Pinch zoom
                            let pts: Vec<(f64, f64)> = self.touches.values().copied().collect();
                            let dx = pts[0].0 - pts[1].0;
                            let dy = pts[0].1 - pts[1].1;
                            let new_dist = (dx * dx + dy * dy).sqrt().max(1.0);
                            let old_zoom = self.keyboard_zoom;
                            self.keyboard_zoom = (self.pinch_base_zoom * (new_dist / self.pinch_base_dist) as f32)
                                .clamp(1.0, 5.0);
                            // Keep pinch midpoint visually stable
                            let mid_x = ((pts[0].0 + pts[1].0) / 2.0) as f32;
                            if old_zoom > 0.0 {
                                self.h_offset = mid_x - (mid_x - self.h_offset) * (self.keyboard_zoom / old_zoom);
                                self.h_offset = self.clamp_h_offset(self.h_offset);
                            }
                        } else {
                            // Single-touch move (drag)
                            self.cursor_x = touch.location.x;
                            self.cursor_y = touch.location.y;
                        }
                        return true;
                    }
                    winit::event::TouchPhase::Ended
                    | winit::event::TouchPhase::Cancelled => {
                        self.touches.remove(&touch.id);
                        if self.touches.len() < 2 {
                            // End pinch; if one finger remains, restart drag from it
                            if let Some((&_id, &(x, y))) = self.touches.iter().next() {
                                self.cursor_x = x;
                                self.cursor_y = y;
                                self.drag_active = true;
                                self.drag_start_x = x;
                                self.drag_start_y = y;
                                self.drag_prev_x = x;
                                self.drag_prev_y = y;
                                self.drag_axis = None;
                                self.scroll_velocity = 0.0;
                                self.h_velocity = 0.0;
                            }
                        }
                        if self.touches.is_empty() {
                            self.drag_active = false;
                        }
                        return true;
                    }
                }
            }
            // Track modifier keys for Ctrl+scroll zoom
            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = *new_modifiers;
                return false;
            }
            // macOS trackpad pinch gesture
            WindowEvent::PinchGesture { delta, .. } => {
                let old_zoom = self.keyboard_zoom;
                self.keyboard_zoom = (self.keyboard_zoom * (1.0 + *delta as f32)).clamp(1.0, 5.0);
                // Keep screen center visually stable
                let center_x = self.size.width as f32 / 2.0;
                self.h_offset = center_x - (center_x - self.h_offset) * (self.keyboard_zoom / old_zoom);
                self.h_offset = self.clamp_h_offset(self.h_offset);
                return true;
            }
            // Mouse wheel: Ctrl held = zoom, otherwise scroll time
            WindowEvent::MouseWheel { delta, .. } => {
                let pixels = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y as f64 * 60.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y,
                };
                if self.modifiers.state().control_key() {
                    // Ctrl+scroll: zoom in/out centered on cursor
                    let zoom_delta = pixels as f32 / 500.0;
                    let old_zoom = self.keyboard_zoom;
                    self.keyboard_zoom = (self.keyboard_zoom * (1.0 + zoom_delta)).clamp(1.0, 5.0);
                    let anchor_x = self.cursor_x as f32;
                    self.h_offset = anchor_x - (anchor_x - self.h_offset) * (self.keyboard_zoom / old_zoom);
                    self.h_offset = self.clamp_h_offset(self.h_offset);
                    return true;
                }
                // Positive pixels = scroll up = rewind (go back in time)
                self.current_time -= pixels / 400.0;
                self.current_time = self.current_time.max(0.0);
                if pixels > 0.0 {
                    // Rewinding — reset triggers for future notes
                    let ct = self.current_time as f32;
                    for (i, n) in self.song.notes.iter().enumerate() {
                        if n.start_time > ct { self.triggered_notes[i] = false; }
                    }
                }
                return true;
            }
            // Keyboard shortcuts
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    state: ElementState::Pressed,
                    physical_key: PhysicalKey::Code(code),
                    ..
                },
                ..
            } => {
                match code {
                    KeyCode::Space => {
                        if !self.audio_unlocked {
                            self.audio_unlocked = true;
                            #[cfg(target_arch = "wasm32")]
                            if let Some(ref player) = self.audio_player {
                                let _ = player.resume();
                            }
                            self.waiting_for_samples = true;
                        } else {
                            self.paused = !self.paused;
                        }
                        return true;
                    }
                    KeyCode::Backspace => {
                        self.rewind_target = Some(0.0);
                        self.paused = false;
                        return true;
                    }
                    KeyCode::KeyT => {
                        self.theme = match self.theme {
                            NoteTheme::Rainbow => NoteTheme::Ice,
                            NoteTheme::Ice => NoteTheme::Rainbow,
                        };
                        return true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        false
    }

    fn update(&mut self) {
        // Check for pending MIDI data
        if let Some(midi_data) = PENDING_MIDI.lock().unwrap().take() {
            match note::parse_midi(&midi_data) {
                Ok(song) => {
                    log::info!("Loaded MIDI: {} notes, {:.0} BPM, title={}", song.notes.len(), song.bpm, song.title);
                    #[cfg(target_arch = "wasm32")]
                    set_song_title(&song.title);
                    self.h_offset = Self::auto_center_offset(&song, self.size.width as f32 * self.keyboard_zoom);
                    self.h_velocity = 0.0;
                    self.triggered_notes = vec![false; song.notes.len()];
                    self.song = song;
                    self.current_time = 0.0;
                    self.paused = false;
                    self.audio_unlocked = true;
                    self.rewind_target = None;
                    self.particle_system.particles.clear();
                }
                Err(e) => log::error!("Failed to parse MIDI: {e}"),
            }
        }

        // Time tracking with pause & rewind support
        let wall_now;
        #[cfg(target_arch = "wasm32")]
        {
            let perf = web_sys::window().unwrap().performance().unwrap();
            wall_now = perf.now() / 1000.0;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            wall_now = self.last_wall_time + 1.0 / 60.0;
        }
        let wall_dt = if self.last_wall_time > 0.0 { wall_now - self.last_wall_time } else { 0.0 };
        self.last_wall_time = wall_now;

        let scroll_speed = 400.0_f64;

        // Wait for samples to finish loading before starting playback
        #[cfg(target_arch = "wasm32")]
        if self.waiting_for_samples {
            if let Some(ref player) = self.audio_player {
                if player.samples_ready() {
                    self.waiting_for_samples = false;
                    self.paused = false;
                }
            }
        }

        // Time control priority: drag > momentum > rewind > normal playback
        if self.drag_active {
            // Detect dominant axis after threshold movement
            if self.drag_axis.is_none() {
                let dx = (self.cursor_x - self.drag_start_x).abs();
                let dy = (self.cursor_y - self.drag_start_y).abs();
                let threshold = 8.0;
                if dx > threshold || dy > threshold {
                    self.drag_axis = Some(if dx > dy {
                        DragAxis::Horizontal
                    } else {
                        DragAxis::Vertical
                    });
                }
            }

            match self.drag_axis {
                Some(DragAxis::Vertical) | None => {
                    // Vertical drag: scrub through time (existing behavior)
                    let delta_y = self.cursor_y - self.drag_prev_y;
                    self.current_time += delta_y / scroll_speed;
                    self.current_time = self.current_time.max(0.0);
                    if wall_dt > 0.0 {
                        let instant_vel = delta_y / wall_dt;
                        self.scroll_velocity = self.scroll_velocity * 0.7 + instant_vel * 0.3;
                    }
                    if delta_y < 0.0 {
                        let ct = self.current_time as f32;
                        for (i, n) in self.song.notes.iter().enumerate() {
                            if n.start_time > ct { self.triggered_notes[i] = false; }
                        }
                    }
                }
                Some(DragAxis::Horizontal) => {
                    // Horizontal drag: scroll keyboard — playback continues
                    let delta_x = self.cursor_x - self.drag_prev_x;
                    self.h_offset += delta_x as f32;
                    self.h_offset = self.clamp_h_offset(self.h_offset);
                    if wall_dt > 0.0 {
                        let instant_vel = delta_x / wall_dt;
                        self.h_velocity = self.h_velocity * 0.7 + instant_vel * 0.3;
                    }
                    if !self.paused {
                        self.current_time += wall_dt;
                    }
                }
            }
            self.drag_prev_x = self.cursor_x;
            self.drag_prev_y = self.cursor_y;
        } else if self.scroll_velocity.abs() > 1.0 {
            // Vertical momentum scrolling after drag release
            let friction = 8.0;
            self.scroll_velocity *= (-friction * wall_dt).exp();
            let delta_y = self.scroll_velocity * wall_dt;
            self.current_time += delta_y / scroll_speed;
            self.current_time = self.current_time.max(0.0);
            if delta_y < 0.0 {
                let ct = self.current_time as f32;
                for (i, n) in self.song.notes.iter().enumerate() {
                    if n.start_time > ct { self.triggered_notes[i] = false; }
                }
            }
            if self.scroll_velocity.abs() <= 1.0 {
                self.scroll_velocity = 0.0;
                self.paused = self.was_paused_before_drag;
            }
        } else if self.h_velocity.abs() > 1.0 {
            // Horizontal momentum scrolling after drag release — playback continues
            let friction = 8.0;
            self.h_velocity *= (-friction * wall_dt).exp();
            self.h_offset += (self.h_velocity * wall_dt) as f32;
            self.h_offset = self.clamp_h_offset(self.h_offset);
            if !self.paused {
                self.current_time += wall_dt;
            }
            if self.h_velocity.abs() <= 1.0 {
                self.h_velocity = 0.0;
            }
        } else if let Some(target) = self.rewind_target {
            let rewind_speed = 8.0;
            let step = wall_dt * rewind_speed * (self.current_time.max(1.0));
            let prev_time = self.current_time as f32;
            self.current_time -= step;
            if self.current_time <= target {
                self.current_time = target;
                self.rewind_target = None;
                self.particle_system.particles.clear();
            }
            // Reset triggers for notes whose start_time we just rewound past
            // so they fire again when reached during forward playback or rewind
            let ct = self.current_time as f32;
            for (i, n) in self.song.notes.iter().enumerate() {
                if n.start_time >= ct && n.start_time < prev_time {
                    self.triggered_notes[i] = false;
                }
            }
        } else if !self.paused {
            self.current_time += wall_dt;
        }

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let zoomed_width = screen_w * self.keyboard_zoom;
        let bottom_margin = screen_h * 0.03;
        // Derive keyboard height from white key width to maintain aspect ratio
        let white_key_width = zoomed_width / 49.0;
        let keyboard_height = white_key_width * 6.0;
        let keyboard_y = screen_h - keyboard_height - bottom_margin;
        let scroll_speed_f = scroll_speed as f32 * self.keyboard_zoom;
        let t = self.current_time as f32;
        let ho = self.h_offset; // horizontal offset for all key positions

        let mut note_instances = Vec::new();
        let mut label_instances: Vec<LabelInstance> = Vec::new();
        let note_clip_y_grid = keyboard_y - keyboard_height * 0.16;
        // Grid: vertical line at center of every key lane
        // White keys: C is brightest white, descending through the octave toward note-blue
        // Black keys: half brightness, stay white
        for pitch in keyboard::VISIBLE_START..=keyboard::VISIBLE_END {
            let (x, w) = keyboard::key_rect(pitch, zoomed_width);
            let x = x + ho;
            let semitone = (pitch + 9) % 12; // C=0, C#=1, ..., B=11
            let is_black = keyboard::is_black_key(pitch);
            let color = if is_black {
                [1.0, 1.0, 1.0, 0.12]
            } else {
                match self.theme {
                    NoteTheme::Ice => {
                        // Map white keys within octave: C=0, D=1, E=2, F=3, G=4, A=5, B=6
                        let white_pos = match semitone {
                            0 => 0, 2 => 1, 4 => 2, 5 => 3, 7 => 4, 9 => 5, 11 => 6, _ => 0,
                        };
                        let frac = white_pos as f32 / 6.0;
                        let r = 1.0 * (1.0 - frac) + 0.3 * frac;
                        let g = 1.0 * (1.0 - frac) + 0.7 * frac;
                        let b = 1.0;
                        let alpha = 0.44 * (1.0 - frac) + 0.18 * frac;
                        [r, g, b, alpha]
                    }
                    NoteTheme::Rainbow => {
                        let hue = semitone as f32 / 12.0;
                        let (r, g, b) = hsl_to_rgb(hue, 0.25, 0.55);
                        [r, g, b, 0.15]
                    }
                }
            };
            note_instances.push(QuadInstance {
                pos: [x + w * 0.5, 0.0],
                size: [1.0, note_clip_y_grid],
                color,
            });
        }

        // Grid: horizontal lines at measure and beat boundaries (scroll with notes)
        // Both arrays are sorted by time. Use a pointer for measure matching.
        let grid_color_beat = [1.0, 1.0, 1.0, 0.18];
        let grid_color_measure = [1.0, 1.0, 1.0, 0.30];
        let mut meas_ptr = 0;
        for &beat_time in &self.song.beats {
            let y = keyboard_y - (beat_time - t) * scroll_speed_f;
            if y < 0.0 { break; } // sorted: all remaining are off-screen above
            if y > note_clip_y_grid { continue; } // below the note area
            // Advance measure pointer
            while meas_ptr < self.song.measures.len()
                && self.song.measures[meas_ptr] < beat_time - 0.001 {
                meas_ptr += 1;
            }
            let is_measure = meas_ptr < self.song.measures.len()
                && (self.song.measures[meas_ptr] - beat_time).abs() < 0.001;
            // Snap to pixel grid to prevent flicker; 2px tall so a full pixel is always covered
            let y_snapped = y.round();
            note_instances.push(QuadInstance {
                pos: [0.0, y_snapped],
                size: [screen_w, 2.0],
                color: if is_measure { grid_color_measure } else { grid_color_beat },
            });
        }

        for n in &self.song.notes {
            // Skip notes outside visible keyboard range
            if !keyboard::is_visible(n.pitch) { continue; }

            // Note bottom reaches keyboard_y when t == n.start_time
            let note_bottom_y = keyboard_y - (n.start_time - t) * scroll_speed_f;
            let note_height = n.duration * scroll_speed_f;
            let note_top_y = note_bottom_y - note_height;

            // Skip if off screen
            let note_clip_y = keyboard_y - keyboard_height * 0.16;
            if note_bottom_y < 0.0 || note_top_y > note_clip_y {
                continue;
            }

            let (key_x, key_w) = keyboard::key_rect(n.pitch, zoomed_width);
            let key_x = key_x + ho;

            // Add vertical gap between consecutive notes
            let note_gap = 4.0;
            let note_top_y = note_top_y + note_gap * 0.5;
            let note_bottom_y = note_bottom_y - note_gap * 0.5;

            // Clamp to fall area (stop notes above keyboard so they don't bleed into 3D keys)
            let note_clip_y = keyboard_y - keyboard_height * 0.16;
            let visible_top = note_top_y.max(0.0);
            let visible_bottom = note_bottom_y.min(note_clip_y);
            if visible_bottom <= visible_top {
                continue;
            }

            let color = note_color(n.pitch, n.velocity, self.theme);

            let inset = 2.0;
            note_instances.push(QuadInstance {
                pos: [key_x + inset, visible_top],
                size: [key_w - inset * 2.0, visible_bottom - visible_top],
                color,
            });

            // Label at onset (bottom) of note strip
            let label_size = (key_w - 4.0).min(42.0).max(6.0);
            let label_y = visible_bottom - label_size;
            if label_y >= visible_top {
                let pc = keyboard::pitch_class(n.pitch);
                label_instances.push(LabelInstance {
                    pos: [key_x + (key_w - label_size) * 0.5, label_y],
                    size: [label_size, label_size],
                    color: [0.0, 0.0, 0.0, 0.85],
                    glyph_uv: [pc as f32 / 12.0, 0.0],
                    glyph_size: [1.0 / 12.0, 1.0],
                });
            }
        }

        // Upward-fading hazy glow at the front panel top (where notes disappear)
        // Only shown in Ice theme — Rainbow uses a clean edge
        if self.theme == NoteTheme::Ice {
            let glow_base = keyboard_y - keyboard_height * 0.16;
            let glow_layers: &[(f32, f32, [f32; 4])] = &[
                (0.0,  2.0,  [0.20, 0.50, 1.0, 0.9]),  // bright core line
                (2.0,  8.0,  [0.12, 0.35, 0.85, 0.4]),  // near glow
                (10.0, 18.0, [0.08, 0.25, 0.65, 0.15]), // mid haze
                (28.0, 30.0, [0.04, 0.15, 0.45, 0.06]), // outer fade
            ];
            for &(offset, h, color) in glow_layers {
                note_instances.push(QuadInstance {
                    pos: [0.0, glow_base - offset - h],
                    size: [screen_w, h],
                    color,
                });
            }
        }

        if !note_instances.is_empty() {
            self.queue.write_buffer(
                &self.note_instance_buffer,
                0,
                bytemuck::cast_slice(&note_instances),
            );
        }
        self.note_instance_count = note_instances.len() as u32;

        if !label_instances.is_empty() {
            self.queue.write_buffer(
                &self.label_instance_buffer,
                0,
                bytemuck::cast_slice(&label_instances),
            );
        }
        self.label_instance_count = label_instances.len() as u32;

        // Determine active keys
        let mut active_keys = [false; 88];
        for n in &self.song.notes {
            if t >= n.start_time && t < n.start_time + n.duration {
                active_keys[n.pitch as usize] = true;
            }
        }

        // Key press animation: instant press-down, fast linear release
        let dt = wall_dt as f32;
        for i in 0..88 {
            if active_keys[i] {
                self.key_press_state[i] = 1.0; // snap down instantly
            } else {
                // Linear release: fully up in ~60ms
                self.key_press_state[i] = (self.key_press_state[i] - dt * 16.0).max(0.0);
            }
        }

        // Trigger audio for notes hitting the keyboard (only when audio is unlocked)
        let is_scrolling = self.drag_active || self.scroll_velocity.abs() > 1.0;
        for (i, n) in self.song.notes.iter().enumerate() {
            let is_active = t >= n.start_time && t < n.start_time + n.duration;
            if is_active && !self.triggered_notes[i] {
                self.triggered_notes[i] = true;
                #[cfg(target_arch = "wasm32")]
                if self.audio_unlocked {
                    if let Some(ref player) = self.audio_player {
                        if is_scrolling {
                            let _ = player.play_note(n.pitch, n.velocity * 0.5, n.duration.min(0.15));
                        } else {
                            let _ = player.play_note(n.pitch, n.velocity, n.duration);
                        }
                    }
                }
            }
        }

        // Spawn particles for active notes touching the keyboard
        let dt = 1.0 / 60.0;
        for n in &self.song.notes {
            if !keyboard::is_visible(n.pitch) { continue; }
            if t >= n.start_time && t < n.start_time + n.duration {
                let (key_x, key_w) = keyboard::key_rect(n.pitch, zoomed_width);
                let spawn_x = key_x + ho + key_w / 2.0;
                let spawn_y = keyboard_y;

                let color = match self.theme {
                    NoteTheme::Rainbow => {
                        let note = (n.pitch + 9) % 12;
                        let hue = note as f32 / 12.0;
                        let (r, g, b) = hsl_to_rgb(hue, 0.75, 0.50);
                        [r, g, b]
                    }
                    NoteTheme::Ice => {
                        let pr = n.pitch as f32 / 87.0;
                        [0.3 + pr * 0.4, 0.6 + pr * 0.3, 1.0]
                    }
                };

                // Spawn 2-3 particles per frame per active note
                self.particle_system.spawn(spawn_x, spawn_y, color, 3);
            }
        }
        self.particle_system.update(dt);

        // Loop the song: when all notes have passed, restart
        let song_duration = self.song.notes.iter()
            .map(|n| n.start_time + n.duration)
            .fold(0.0_f32, f32::max);

        if t > song_duration + 2.0 {
            self.current_time = 0.0;
            self.triggered_notes.fill(false);
            self.particle_system.particles.clear();
        }

        // Compute flashlight: highlight keys with upcoming notes
        let lookahead = 0.8_f32; // seconds to look ahead
        let mut key_flash = [0.0_f32; 88];
        for n in &self.song.notes {
            let dt_until = n.start_time - t;
            if dt_until > 0.0 && dt_until < lookahead {
                // Intensity: 0 at lookahead distance, 1 at arrival
                let intensity = 1.0 - dt_until / lookahead;
                let idx = n.pitch as usize;
                if idx < 88 {
                    key_flash[idx] = key_flash[idx].max(intensity);
                }
            }
        }

        // Rebuild keyboard instances
        if self.use_3d_keys {
            // 3D key instances
            let mut keys_3d = Vec::new();
            let gap = 1.5_f32;
            let key_depth_px = keyboard_height * 0.95;
            let bk_depth_px = keyboard_height * 0.60;

            for pitch in keyboard::VISIBLE_START..=keyboard::VISIBLE_END {
                let (x, w) = keyboard::key_rect(pitch, zoomed_width);
                let x = x + ho;
                let is_black = keyboard::is_black_key(pitch);
                let press = self.key_press_state[pitch as usize];
                let flash = key_flash[pitch as usize];

                let p = press;
                if is_black {
                    keys_3d.push(KeyInstance3D {
                        pos_x: x,
                        key_width: w,
                        key_height: keyboard_height * 0.12,
                        key_depth: bk_depth_px,
                        press,
                        is_black: 1.0,
                        light: flash,
                        _pad_inst: 0.0,
                        color: [0.05, 0.05, 0.07, 1.0],
                    });
                } else {
                    // Encode black key neighbors: 1=left, 2=right, 3=both
                    let has_left = pitch > 0 && keyboard::is_black_key(pitch - 1);
                    let has_right = pitch < 87 && keyboard::is_black_key(pitch + 1);
                    let neighbors = has_left as u8 as f32 + has_right as u8 as f32 * 2.0;
                    keys_3d.push(KeyInstance3D {
                        pos_x: x + gap * 0.5,
                        key_width: w - gap,
                        key_height: keyboard_height * 0.06,
                        key_depth: key_depth_px,
                        press,
                        is_black: 0.0,
                        light: flash,
                        _pad_inst: neighbors,
                        color: [
                            0.92 + p * 0.08,
                            0.90 + p * 0.07,
                            0.87 + p * 0.05,
                            1.0,
                        ],
                    });
                }
            }
            self.key_instances_3d = keys_3d;

            // Dark background quad behind keyboard area
            // Front panel just above black key height (0.12) for a piano-back look
            let front_panel_h = keyboard_height * 0.16;
            let bg_margin = front_panel_h + 4.0;
            let mut kb_instances = Vec::new();
            // Main dark background
            kb_instances.push(QuadInstance {
                pos: [0.0, keyboard_y - bg_margin],
                size: [screen_w, keyboard_height + bottom_margin + bg_margin],
                color: [0.02, 0.02, 0.03, 1.0],
            });
            // Front panel strip — dark lip above the keys so notes vanish behind it
            kb_instances.push(QuadInstance {
                pos: [0.0, keyboard_y - front_panel_h],
                size: [screen_w, front_panel_h],
                color: [0.06, 0.06, 0.07, 1.0],
            });
            // Neon glow at the top edge of the front panel (Ice theme only)
            let glow_top = keyboard_y - front_panel_h;
            if self.theme == NoteTheme::Ice {
                let glow_total_h = 144.0_f32;
                let glow_steps = 24_u32;
                let step_h = glow_total_h / glow_steps as f32;
                for s in 0..glow_steps {
                    let frac = s as f32 / (glow_steps - 1) as f32;
                    let alpha = 0.02 + frac * frac * 0.35;
                    let r = 0.15 + frac * 0.8;
                    let g = 0.4 + frac * 0.55;
                    let y = glow_top - glow_total_h + s as f32 * step_h;
                    kb_instances.push(QuadInstance {
                        pos: [0.0, y],
                        size: [screen_w, step_h + 1.0],
                        color: [r.min(1.0), g.min(1.0), 1.0, alpha],
                    });
                }
            }
            // Core line at top edge of front panel
            kb_instances.push(QuadInstance {
                pos: [0.0, glow_top],
                size: [screen_w, 2.0],
                color: if self.theme == NoteTheme::Ice {
                    [0.95, 0.97, 1.0, 1.0]
                } else {
                    [0.15, 0.15, 0.17, 1.0]
                },
            });
            self.queue.write_buffer(
                &self.keyboard_instance_buffer,
                0,
                bytemuck::cast_slice(&kb_instances),
            );
            self.keyboard_instance_count = kb_instances.len() as u32;
        } else {
            // Quad-based keyboard (fallback via ?keyboard=quad)
            let mut kb_instances = Vec::new();
            let gap = 1.5_f32;
            let bk_height = keyboard_height * 0.62;
            let spot_h = keyboard_height * 0.15;

            kb_instances.push(QuadInstance {
                pos: [0.0, keyboard_y],
                size: [screen_w, keyboard_height + bottom_margin],
                color: [0.02, 0.02, 0.03, 1.0],
            });

            for pitch in keyboard::VISIBLE_START..=keyboard::VISIBLE_END {
                if !keyboard::is_black_key(pitch) {
                    let (x, w) = keyboard::key_rect(pitch, zoomed_width);
                    let x = x + ho;
                    let active = active_keys[pitch as usize];
                    let kx = x + gap * 0.5;
                    let kw = w - gap;

                    if active {
                        let press_depth = 5.0;
                        let taper = 1.5;
                        kb_instances.push(QuadInstance {
                            pos: [kx, keyboard_y], size: [kw, press_depth],
                            color: [0.08, 0.07, 0.06, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx + taper, keyboard_y + press_depth],
                            size: [kw - taper * 2.0, keyboard_height - press_depth],
                            color: [0.80, 0.78, 0.75, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx + taper, keyboard_y + press_depth],
                            size: [kw - taper * 2.0, spot_h],
                            color: [0.88, 0.86, 0.83, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx, keyboard_y + press_depth],
                            size: [taper, keyboard_height - press_depth],
                            color: [0.25, 0.24, 0.22, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx + kw - taper, keyboard_y + press_depth],
                            size: [taper, keyboard_height - press_depth],
                            color: [0.35, 0.34, 0.32, 1.0],
                        });
                    } else {
                        kb_instances.push(QuadInstance {
                            pos: [kx, keyboard_y], size: [kw, keyboard_height],
                            color: [0.72, 0.70, 0.67, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx, keyboard_y], size: [kw, spot_h],
                            color: [0.92, 0.90, 0.87, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [kx, keyboard_y + keyboard_height - spot_h],
                            size: [kw, spot_h],
                            color: [0.45, 0.43, 0.40, 1.0],
                        });
                    }
                }
            }

            for pitch in keyboard::VISIBLE_START..=keyboard::VISIBLE_END {
                if keyboard::is_black_key(pitch) {
                    let (x, w) = keyboard::key_rect(pitch, zoomed_width);
                    let x = x + ho;
                    let active = active_keys[pitch as usize];
                    let bk_spot = bk_height * 0.18;

                    if active {
                        let press_depth = 3.0;
                        let taper = 1.0;
                        kb_instances.push(QuadInstance {
                            pos: [x, keyboard_y], size: [w, press_depth],
                            color: [0.01, 0.01, 0.02, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x + taper, keyboard_y + press_depth],
                            size: [w - taper * 2.0, bk_height - press_depth],
                            color: [0.10, 0.10, 0.13, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x + taper, keyboard_y + press_depth],
                            size: [w - taper * 2.0, bk_spot],
                            color: [0.22, 0.22, 0.26, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x, keyboard_y + press_depth],
                            size: [taper, bk_height - press_depth],
                            color: [0.02, 0.02, 0.03, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x + w - taper, keyboard_y + press_depth],
                            size: [taper, bk_height - press_depth],
                            color: [0.03, 0.03, 0.04, 1.0],
                        });
                    } else {
                        kb_instances.push(QuadInstance {
                            pos: [x, keyboard_y], size: [w, bk_height],
                            color: [0.05, 0.05, 0.07, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x, keyboard_y], size: [w, bk_spot],
                            color: [0.14, 0.14, 0.18, 1.0],
                        });
                        kb_instances.push(QuadInstance {
                            pos: [x + 1.0, keyboard_y + bk_height - 3.0],
                            size: [w - 2.0, 3.0],
                            color: [0.02, 0.02, 0.03, 1.0],
                        });
                    }
                }
            }

            self.queue.write_buffer(
                &self.keyboard_instance_buffer,
                0,
                bytemuck::cast_slice(&kb_instances),
            );
            self.keyboard_instance_count = kb_instances.len() as u32;
        }

        // Bottom blende: dark strip drawn AFTER 3D keys to hide pressed key edges
        {
            let panel_h = keyboard_height * 0.10;
            let panel_y = keyboard_y + keyboard_height + panel_h * 0.15;
            let mut overlay = Vec::new();
            overlay.push(QuadInstance {
                pos: [0.0, panel_y],
                size: [screen_w, panel_h + bottom_margin],
                color: [0.04, 0.04, 0.05, 1.0],
            });
            // Subtle highlight line at top edge of bottom blende
            overlay.push(QuadInstance {
                pos: [0.0, panel_y],
                size: [screen_w, 1.0],
                color: [0.12, 0.12, 0.14, 1.0],
            });
            self.queue.write_buffer(
                &self.overlay_instance_buffer,
                0,
                bytemuck::cast_slice(&overlay),
            );
            self.overlay_instance_count = overlay.len() as u32;
        }
    }

    /// Returns true if the scene has ongoing activity requiring another frame.
    fn needs_animation(&self) -> bool {
        if !self.paused { return true; }
        if self.waiting_for_samples { return true; }
        if self.drag_active { return true; }
        if self.touches.len() >= 2 { return true; }
        if self.scroll_velocity.abs() > 0.5 { return true; }
        if self.h_velocity.abs() > 0.5 { return true; }
        if self.rewind_target.is_some() { return true; }
        if !self.particle_system.particles.is_empty() { return true; }
        // Keys still transitioning (not yet settled at 0 or 1)
        self.key_press_state.iter().any(|&v| v > 0.002 && v < 0.998)
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let screen_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        self.quad_renderer.update_globals(
            &self.queue,
            self.size.width as f32,
            self.size.height as f32,
        );
        self.screen_quad_renderer.update_globals(
            &self.queue,
            self.size.width as f32,
            self.size.height as f32,
        );

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            },
        );

        // Pass 1: Notes + particles -> offscreen texture (bloom source)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Notes Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.bloom.scene_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            if self.note_instance_count > 0 {
                self.quad_renderer.draw_notes(
                    &mut pass,
                    &self.note_instance_buffer,
                    self.note_instance_count,
                );
            }
            if self.label_instance_count > 0 {
                self.quad_renderer.draw_labels(
                    &mut pass,
                    &self.label_instance_buffer,
                    self.label_instance_count,
                );
            }
            self.particle_system.draw(
                &mut pass,
                self.quad_renderer.globals_bind_group_notes(),
                &self.queue,
            );
        }

        // Pass 2: Bloom extract
        self.bloom.extract_pass(&mut encoder);
        // Pass 3-4: Gaussian blur (H then V)
        self.bloom.blur_h_pass(&mut encoder);
        self.bloom.blur_v_pass(&mut encoder);

        // Pass 5: Composite (scene + bloom) -> swapchain
        self.bloom.composite_pass(&mut encoder, &screen_view);

        // Pass 6: Keyboard BG drawn ON TOP of composited screen (no bloom bleed)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Keyboard BG Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &screen_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            self.screen_quad_renderer.draw(
                &mut pass,
                &self.keyboard_instance_buffer,
                self.keyboard_instance_count,
            );
        }

        // Pass 7: 3D keys on top of screen (has its own depth buffer)
        if self.use_3d_keys && !self.key_instances_3d.is_empty() {
            let screen_h = self.size.height as f32;
            let bottom_margin = screen_h * 0.03;
            let white_key_width = self.size.width as f32 * self.keyboard_zoom / 49.0;
            let keyboard_height = white_key_width * 6.0;
            let keyboard_y = screen_h - keyboard_height - bottom_margin;
            let max_depth = keyboard_height * 0.95;
            self.key_renderer.update_uniforms(
                &self.queue,
                self.size.width as f32,
                screen_h,
                keyboard_y,
                keyboard_height,
                max_depth,
            );
            self.key_renderer.draw(
                &mut encoder,
                &screen_view,
                &self.key_instances_3d,
                &self.queue,
            );
        }

        // Pass 8: Bottom blende overlay — drawn on top of 3D keys
        if self.overlay_instance_count > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Bottom Blende Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &screen_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
                multiview_mask: None,
            });
            self.screen_quad_renderer.draw(
                &mut pass,
                &self.overlay_instance_buffer,
                self.overlay_instance_count,
            );
        }

        self.queue.submit(iter::once(encoder.finish()));
        output.present();
        Ok(())
    }
}

// -- Application shell (winit 0.30 ApplicationHandler) --

struct App {
    state: Option<State>,
    #[cfg(target_arch = "wasm32")]
    proxy: Option<winit::event_loop::EventLoopProxy<State>>,
}

impl ApplicationHandler<State> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() { return; }

        #[allow(unused_mut)]
        let mut attrs = Window::default_attributes().with_title("Piano Fall");

        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;

            let win = web_sys::window().unwrap();
            let doc = win.document().unwrap();
            let canvas = doc
                .get_element_by_id("canvas")
                .unwrap()
                .unchecked_into::<web_sys::HtmlCanvasElement>();
            // Set canvas buffer to match viewport size * devicePixelRatio for sharp rendering
            let dpr = win.device_pixel_ratio();
            let vw = (win.inner_width().unwrap().as_f64().unwrap() * dpr) as u32;
            let vh = (win.inner_height().unwrap().as_f64().unwrap() * dpr) as u32;
            canvas.set_width(vw);
            canvas.set_height(vh);
            canvas.style().set_property("width", "100%").unwrap();
            canvas.style().set_property("height", "100%").unwrap();
            attrs = attrs.with_canvas(Some(canvas));
        }

        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.state = Some(pollster::block_on(State::new(window)));
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(proxy) = self.proxy.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    let state = State::new(window).await;
                    let _ = proxy.send_event(state);
                });
            }
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, mut state: State) {
        let size = state.window.inner_size();
        state.resize(size);
        state.surface_configured = true;
        state.window.request_redraw();
        self.state = Some(state);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else { return };

        // Wake the render loop if external code (e.g. MIDI load) requested it
        if REDRAW_FLAG.swap(false, Ordering::Relaxed) {
            state.window.request_redraw();
        }

        if state.input(&event) {
            state.window.request_redraw();
            return;
        }

        match event {
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event: KeyEvent {
                    state: ElementState::Pressed,
                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                    ..
                },
                ..
            } => event_loop.exit(),
            WindowEvent::Resized(physical_size) => {
                state.surface_configured = true;
                state.resize(physical_size);
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if !state.surface_configured { return; }
                // iOS PWA: detect orientation changes that bypass Resized events
                #[cfg(target_arch = "wasm32")]
                state.check_viewport_resize();
                state.update();
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.resize(state.size)
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(wgpu::SurfaceError::Timeout) => {
                        log::warn!("Surface timeout")
                    }
                    Err(e) => {
                        log::error!("Surface error: {e:?}");
                        event_loop.exit();
                    }
                }
                // Only request the next frame if there's ongoing activity
                if state.needs_animation() {
                    state.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

// -- Entry points --

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    cfg_if::cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            console_log::init_with_level(log::Level::Info).expect("Couldn't initialize logger");
        } else {
            env_logger::init();
        }
    }

    let event_loop = EventLoop::<State>::with_user_event().build()?;

    #[cfg(target_arch = "wasm32")]
    let proxy = event_loop.create_proxy();

    let mut app = App {
        state: None,
        #[cfg(target_arch = "wasm32")]
        proxy: Some(proxy),
    };

    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn run_web() -> Result<(), JsValue> {
    run().map_err(|e| JsValue::from_str(&e.to_string()))
}
