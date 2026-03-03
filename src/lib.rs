pub mod keyboard;
pub mod note;
pub mod renderer;

use std::iter;
use winit::{
    event::*,
    event_loop::EventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowBuilder},
};

use renderer::quad::QuadInstance;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

struct State<'a> {
    surface: wgpu::Surface<'a>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    window: &'a Window,
    quad_renderer: renderer::quad::QuadRenderer,
    keyboard_instance_buffer: wgpu::Buffer,
    keyboard_instance_count: u32,
    #[allow(dead_code)] // used only on wasm32
    start_time: f64,
    current_time: f64,
    song: note::Song,
    note_instance_buffer: wgpu::Buffer,
    note_instance_count: u32,
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

        let quad_renderer = renderer::quad::QuadRenderer::new(&device, surface_format);

        let keyboard_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Keyboard Instances"),
            size: (88 * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let keyboard_instance_count = 0;

        let start_time = 0.0;
        let song = note::demo_song();
        let note_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Note Instances"),
            size: (200 * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            surface, device, queue, config, size, window,
            quad_renderer, keyboard_instance_buffer, keyboard_instance_count,
            start_time, current_time: 0.0, song, note_instance_buffer,
            note_instance_count: 0,
        }
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

    fn update(&mut self) {
        // Time tracking
        #[cfg(target_arch = "wasm32")]
        {
            let perf = web_sys::window().unwrap().performance().unwrap();
            self.current_time = perf.now() / 1000.0 - self.start_time;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.current_time += 1.0 / 60.0;
        }

        let screen_w = self.size.width as f32;
        let screen_h = self.size.height as f32;
        let keyboard_height = screen_h * 0.2;
        let keyboard_y = screen_h - keyboard_height;
        let scroll_speed = 200.0; // pixels per second
        let t = self.current_time as f32;

        let mut note_instances = Vec::new();

        for n in &self.song.notes {
            // Note bottom reaches keyboard_y when t == n.start_time
            let note_bottom_y = keyboard_y - (n.start_time - t) * scroll_speed;
            let note_height = n.duration * scroll_speed;
            let note_top_y = note_bottom_y - note_height;

            // Skip if off screen
            if note_bottom_y < 0.0 || note_top_y > keyboard_y {
                continue;
            }

            let (key_x, key_w) = keyboard::key_rect(n.pitch, screen_w);

            // Clamp to fall area
            let visible_top = note_top_y.max(0.0);
            let visible_bottom = note_bottom_y.min(keyboard_y);
            if visible_bottom <= visible_top {
                continue;
            }

            // Color gradient by pitch
            let pr = n.pitch as f32 / 87.0;
            let r = 0.1 + pr * 0.2;
            let g = 0.3 + pr * 0.5;
            let b = 0.8 + pr * 0.2;

            note_instances.push(QuadInstance {
                pos: [key_x, visible_top],
                size: [key_w, visible_bottom - visible_top],
                color: [r * n.velocity, g * n.velocity, b * n.velocity, 1.0],
            });
        }

        if !note_instances.is_empty() {
            self.queue.write_buffer(
                &self.note_instance_buffer,
                0,
                bytemuck::cast_slice(&note_instances),
            );
        }
        self.note_instance_count = note_instances.len() as u32;

        // Determine active keys
        let mut active_keys = [false; 88];
        for n in &self.song.notes {
            if t >= n.start_time && t < n.start_time + n.duration {
                active_keys[n.pitch as usize] = true;
            }
        }

        // Rebuild keyboard instances with highlighting
        let mut kb_instances = Vec::new();

        // White keys first (drawn underneath)
        for pitch in 0..88u8 {
            if !keyboard::is_black_key(pitch) {
                let (x, w) = keyboard::key_rect(pitch, screen_w);
                let color = if active_keys[pitch as usize] {
                    [0.3, 0.6, 1.0, 1.0]
                } else {
                    [0.9, 0.9, 0.9, 1.0]
                };
                kb_instances.push(QuadInstance {
                    pos: [x, keyboard_y],
                    size: [w - 1.0, keyboard_height],
                    color,
                });
            }
        }

        // Black keys on top
        for pitch in 0..88u8 {
            if keyboard::is_black_key(pitch) {
                let (x, w) = keyboard::key_rect(pitch, screen_w);
                let color = if active_keys[pitch as usize] {
                    [0.2, 0.4, 0.9, 1.0]
                } else {
                    [0.15, 0.15, 0.15, 1.0]
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
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.quad_renderer.update_globals(
            &self.queue,
            self.size.width as f32,
            self.size.height as f32,
        );

        let mut encoder = self.device.create_command_encoder(
            &wgpu::CommandEncoderDescriptor { label: Some("Render Encoder") },
        );
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
            // Draw notes first (behind keyboard)
            if self.note_instance_count > 0 {
                self.quad_renderer.draw(&mut render_pass, &self.note_instance_buffer, self.note_instance_count);
            }
            // Draw keyboard on top
            self.quad_renderer.draw(&mut render_pass, &self.keyboard_instance_buffer, self.keyboard_instance_count);
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
