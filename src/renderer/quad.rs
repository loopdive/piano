use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct QuadInstance {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
}

impl QuadInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<QuadInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                // pos: vec2<f32>
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // size: vec2<f32>
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                // color: vec4<f32>
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct LabelInstance {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub glyph_uv: [f32; 2],
    pub glyph_size: [f32; 2],
}

impl LabelInstance {
    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LabelInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 4]>()
                        + std::mem::size_of::<[f32; 4]>()) as wgpu::BufferAddress,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: (std::mem::size_of::<[f32; 4]>()
                        + std::mem::size_of::<[f32; 4]>()
                        + std::mem::size_of::<[f32; 2]>()) as wgpu::BufferAddress,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Globals {
    screen_size: [f32; 2],
    note_mode: f32,
    _padding: f32,
}

/// 8x8 bitmap font glyphs for note names: C, C#, D, D#, E, F, F#, G, G#, A, A#, B.
/// Each glyph is [u8; 8] where bit 7 = leftmost pixel.
/// Natural notes are centered; sharp notes have letter left + # right.
fn font_atlas_data() -> Vec<u8> {
    // 12 glyphs, each 8x8 pixels → 96x8 atlas
    let glyphs: [[u8; 8]; 12] = [
        // C
        [0x00, 0x18, 0x24, 0x20, 0x24, 0x18, 0x00, 0x00],
        // C#
        [0x00, 0x62, 0x97, 0x82, 0x97, 0x62, 0x00, 0x00],
        // D
        [0x00, 0x38, 0x24, 0x24, 0x24, 0x38, 0x00, 0x00],
        // D#
        [0x00, 0xE2, 0x97, 0x92, 0x97, 0xE2, 0x00, 0x00],
        // E
        [0x00, 0x3C, 0x20, 0x38, 0x20, 0x3C, 0x00, 0x00],
        // F
        [0x00, 0x3C, 0x20, 0x38, 0x20, 0x20, 0x00, 0x00],
        // F#
        [0x00, 0xF2, 0x87, 0xE2, 0x87, 0x82, 0x00, 0x00],
        // G
        [0x00, 0x18, 0x20, 0x2C, 0x24, 0x18, 0x00, 0x00],
        // G#
        [0x00, 0x62, 0x87, 0xB2, 0x97, 0x62, 0x00, 0x00],
        // A
        [0x00, 0x18, 0x24, 0x3C, 0x24, 0x24, 0x00, 0x00],
        // A#
        [0x00, 0x62, 0x97, 0xF2, 0x97, 0x92, 0x00, 0x00],
        // B
        [0x00, 0x38, 0x24, 0x38, 0x24, 0x38, 0x00, 0x00],
    ];

    let mut pixels = vec![0u8; 96 * 8];
    for (gi, glyph) in glyphs.iter().enumerate() {
        for (row, &bits) in glyph.iter().enumerate() {
            for col in 0..8u8 {
                let lit = (bits >> (7 - col)) & 1;
                pixels[row * 96 + gi * 8 + col as usize] = lit * 255;
            }
        }
    }
    pixels
}

pub struct QuadRenderer {
    pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    /// Separate buffer with note_mode=1.0 for notes pass (rounded corners + border)
    globals_buffer_notes: wgpu::Buffer,
    globals_bind_group_notes: wgpu::BindGroup,
    globals_bind_group_layout: wgpu::BindGroupLayout,
    index_buffer: wgpu::Buffer,
    // Label rendering
    label_pipeline: wgpu::RenderPipeline,
    font_atlas_bind_group: wgpu::BindGroup,
}

impl QuadRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/scene.wgsl").into()),
        });

        let globals = Globals {
            screen_size: [800.0, 600.0],
            note_mode: 0.0,
            _padding: 0.0,
        };
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform"),
            contents: bytemuck::cast_slice(&[globals]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_notes = Globals {
            screen_size: [800.0, 600.0],
            note_mode: 1.0,
            _padding: 0.0,
        };
        let globals_buffer_notes = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform (Notes)"),
            contents: bytemuck::cast_slice(&[globals_notes]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let globals_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Globals Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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

        let globals_bind_group_notes = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals Bind Group (Notes)"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer_notes.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[&globals_bind_group_layout],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[QuadInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_quad"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Font atlas texture (96x8, R8Unorm)
        let atlas_data = font_atlas_data();
        let atlas_size = wgpu::Extent3d { width: 96, height: 8, depth_or_array_layers: 1 };
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Font Atlas"),
            size: atlas_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(96),
                rows_per_image: Some(8),
            },
            atlas_size,
        );
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Font Atlas Sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let font_atlas_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Font Atlas BGL"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
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

        let font_atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Font Atlas BG"),
            layout: &font_atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let label_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Label Pipeline Layout"),
                bind_group_layouts: &[&globals_bind_group_layout, &font_atlas_bind_group_layout],
                immediate_size: 0,
            });

        let label_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Label Pipeline"),
            layout: Some(&label_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_label"),
                compilation_options: Default::default(),
                buffers: &[LabelInstance::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_label"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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

        Self {
            pipeline,
            globals_buffer,
            globals_bind_group,
            globals_buffer_notes,
            globals_bind_group_notes,
            globals_bind_group_layout,
            index_buffer,
            label_pipeline,
            font_atlas_bind_group,
        }
    }

    pub fn globals_bind_group(&self) -> &wgpu::BindGroup {
        &self.globals_bind_group
    }

    pub fn globals_bind_group_notes(&self) -> &wgpu::BindGroup {
        &self.globals_bind_group_notes
    }

    pub fn globals_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.globals_bind_group_layout
    }

    pub fn update_globals(&self, queue: &wgpu::Queue, width: f32, height: f32) {
        let globals = Globals {
            screen_size: [width, height],
            note_mode: 0.0,
            _padding: 0.0,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::cast_slice(&[globals]));

        let globals_notes = Globals {
            screen_size: [width, height],
            note_mode: 1.0,
            _padding: 0.0,
        };
        queue.write_buffer(&self.globals_buffer_notes, 0, bytemuck::cast_slice(&[globals_notes]));
    }

    /// Draw quads with note_mode=0.0 (keyboard/UI — sharp rectangles)
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

    /// Draw quads with note_mode=1.0 (notes — rounded corners + bright border)
    pub fn draw_notes<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instance_buffer: &'a wgpu::Buffer,
        instance_count: u32,
    ) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group_notes, &[]);
        render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..instance_count);
    }

    /// Draw note letter labels using the font atlas
    pub fn draw_labels<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
        instance_buffer: &'a wgpu::Buffer,
        instance_count: u32,
    ) {
        render_pass.set_pipeline(&self.label_pipeline);
        render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
        render_pass.set_bind_group(1, &self.font_atlas_bind_group, &[]);
        render_pass.set_vertex_buffer(0, instance_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..instance_count);
    }
}
