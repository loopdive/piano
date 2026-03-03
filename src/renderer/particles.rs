use crate::renderer::quad::QuadInstance;
use wgpu::util::DeviceExt;

const MAX_PARTICLES: usize = 2000;

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub life: f32,
    pub max_life: f32,
    pub size: f32,
    pub color: [f32; 3],
}

pub struct ParticleSystem {
    pub particles: Vec<Particle>,
    pipeline: wgpu::RenderPipeline,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    /// Monotonically increasing counter used for pseudo-random generation.
    seed: u32,
}

/// Simple pseudo-random hash based on an integer seed.
/// Returns a value in [0.0, 1.0).
fn pseudo_random(seed: u32) -> f32 {
    let mut s = seed;
    s ^= s.wrapping_shl(13);
    s ^= s.wrapping_shr(17);
    s ^= s.wrapping_shl(5);
    (s % 10000) as f32 / 10000.0
}

impl ParticleSystem {
    pub fn new(
        device: &wgpu::Device,
        globals_bind_group_layout: &wgpu::BindGroupLayout,
        target_format: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Shader"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("../shaders/particle.wgsl").into(),
            ),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Particle Pipeline Layout"),
            bind_group_layouts: &[globals_bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Particle Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_particle"),
                compilation_options: Default::default(),
                buffers: &[QuadInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_particle"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
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
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let indices: [u16; 6] = [0, 1, 2, 2, 1, 3];
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Particle Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Instance Buffer"),
            size: (MAX_PARTICLES * std::mem::size_of::<QuadInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            particles: Vec::new(),
            pipeline,
            index_buffer,
            instance_buffer,
            seed: 0,
        }
    }

    /// Spawn `count` particles at (x, y) with the given base color.
    /// Uses an index-based pseudo-random approach (no external RNG crate).
    pub fn spawn(&mut self, x: f32, y: f32, color: [f32; 3], count: usize) {
        for i in 0..count {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let s = self.seed.wrapping_add(i as u32);
            self.seed = self.seed.wrapping_add(1);

            // Random angle spread: mostly upward with some sideways scatter
            let angle_base = std::f32::consts::PI; // straight up in screen coords (negative y)
            let angle_spread = std::f32::consts::PI * 0.6;
            let angle = angle_base + (pseudo_random(s) - 0.5) * angle_spread;

            let speed = 30.0 + pseudo_random(s.wrapping_mul(7)) * 60.0;
            let vx = angle.cos() * speed;
            let vy = angle.sin() * speed;

            let life = 0.3 + pseudo_random(s.wrapping_mul(13)) * 0.5;
            let size = 3.0 + pseudo_random(s.wrapping_mul(19)) * 5.0;

            self.particles.push(Particle {
                x,
                y,
                vx,
                vy,
                life,
                max_life: life,
                size,
                color,
            });
        }
    }

    /// Advance particle simulation by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        for p in &mut self.particles {
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.vy += 20.0 * dt; // slight gravity
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
    }

    /// Upload living particles to the GPU and issue draw calls.
    /// Must be called within a render pass that targets the scene offscreen texture.
    pub fn draw<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        globals_bind_group: &'a wgpu::BindGroup,
        queue: &wgpu::Queue,
    ) {
        if self.particles.is_empty() {
            return;
        }

        let instances: Vec<QuadInstance> = self
            .particles
            .iter()
            .map(|p| {
                let alpha = (p.life / p.max_life).clamp(0.0, 1.0);
                let size = p.size * alpha;
                QuadInstance {
                    pos: [p.x - size / 2.0, p.y - size / 2.0],
                    size: [size, size],
                    color: [
                        p.color[0] * alpha,
                        p.color[1] * alpha,
                        p.color[2] * alpha,
                        1.0,
                    ],
                }
            })
            .collect();

        let count = instances.len().min(MAX_PARTICLES) as u32;
        queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&instances[..count as usize]),
        );

        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, globals_bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..count);
    }
}
