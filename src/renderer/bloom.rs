use wgpu::util::DeviceExt;

/// Offscreen texture format used for all bloom render targets.
/// Rgba16Float eliminates banding in bloom gradients.
const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Multi-pass bloom post-processing renderer.
///
/// Pipeline:
/// 1. Scene pass (external) renders to `scene_texture`
/// 2. Bright extract: scene_texture -> bright_texture
/// 3. Horizontal blur: bright_texture -> blur_h_texture
/// 4. Vertical blur: blur_h_texture -> bright_texture (reused)
/// 5. Composite: scene_texture + bright_texture -> swapchain
pub struct BloomRenderer {
    // Offscreen textures
    scene_texture: wgpu::Texture,
    scene_view: wgpu::TextureView,
    bright_texture: wgpu::Texture,
    bright_view: wgpu::TextureView,
    blur_h_texture: wgpu::Texture,
    blur_h_view: wgpu::TextureView,
    sampler: wgpu::Sampler,

    // Shared bind group layout: texture_2d + sampler (used by all fullscreen passes)
    texture_bind_group_layout: wgpu::BindGroupLayout,

    // Extract pass
    extract_pipeline: wgpu::RenderPipeline,
    extract_bind_group: wgpu::BindGroup,

    // Blur pass (shared pipeline, different bind groups / uniforms per direction)
    blur_pipeline: wgpu::RenderPipeline,
    blur_h_uniform_buffer: wgpu::Buffer,
    blur_v_uniform_buffer: wgpu::Buffer,
    blur_uniform_bind_group_layout: wgpu::BindGroupLayout,
    blur_h_tex_bind_group: wgpu::BindGroup,
    blur_h_uniform_bind_group: wgpu::BindGroup,
    blur_v_tex_bind_group: wgpu::BindGroup,
    blur_v_uniform_bind_group: wgpu::BindGroup,

    // Composite pass
    composite_pipeline: wgpu::RenderPipeline,
    composite_scene_bind_group: wgpu::BindGroup,
    composite_bloom_bind_group: wgpu::BindGroup,
}

/// 16-byte aligned blur direction uniform.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    direction: [f32; 2],
    _padding: [f32; 2],
}

impl BloomRenderer {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ---- Sampler (linear filtering, clamp to edge) ----
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Bloom Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // ---- Offscreen textures ----
        let scene_texture = create_offscreen_texture(device, width, height, "Scene Texture");
        let scene_view = scene_texture.create_view(&Default::default());
        let bright_texture = create_offscreen_texture(device, width, height, "Bright Texture");
        let bright_view = bright_texture.create_view(&Default::default());
        let blur_h_texture = create_offscreen_texture(device, width, height, "Blur H Texture");
        let blur_h_view = blur_h_texture.create_view(&Default::default());

        // ---- Shared bind group layout: texture + sampler ----
        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Bloom Texture BGL"),
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

        // ---- Blur uniform bind group layout ----
        let blur_uniform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Blur Uniform BGL"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        // ---- Bind groups ----
        let extract_bind_group = create_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &scene_view,
            &sampler,
            "Extract BG",
        );

        // Blur H reads from bright_texture
        let blur_h_tex_bind_group = create_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &bright_view,
            &sampler,
            "Blur H Tex BG",
        );

        // Blur V reads from blur_h_texture
        let blur_v_tex_bind_group = create_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &blur_h_view,
            &sampler,
            "Blur V Tex BG",
        );

        // Composite bind groups
        let composite_scene_bind_group = create_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &scene_view,
            &sampler,
            "Composite Scene BG",
        );
        let composite_bloom_bind_group = create_texture_bind_group(
            device,
            &texture_bind_group_layout,
            &bright_view,
            &sampler,
            "Composite Bloom BG",
        );

        // ---- Blur uniform buffers ----
        let blur_spread = 6.0; // wider blur for smooth visible glow
        let blur_h_uniforms = BlurUniforms {
            direction: [blur_spread / width as f32, 0.0],
            _padding: [0.0; 2],
        };
        let blur_v_uniforms = BlurUniforms {
            direction: [0.0, blur_spread / height as f32],
            _padding: [0.0; 2],
        };
        let blur_h_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur H Uniform"),
            contents: bytemuck::cast_slice(&[blur_h_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let blur_v_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Blur V Uniform"),
            contents: bytemuck::cast_slice(&[blur_v_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let blur_h_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur H Uniform BG"),
            layout: &blur_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: blur_h_uniform_buffer.as_entire_binding(),
            }],
        });
        let blur_v_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur V Uniform BG"),
            layout: &blur_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: blur_v_uniform_buffer.as_entire_binding(),
            }],
        });

        // ---- Pipelines ----
        let extract_pipeline = create_extract_pipeline(device, &texture_bind_group_layout);
        let blur_pipeline = create_blur_pipeline(
            device,
            &texture_bind_group_layout,
            &blur_uniform_bind_group_layout,
        );
        let composite_pipeline =
            create_composite_pipeline(device, &texture_bind_group_layout, surface_format);

        Self {
            scene_texture,
            scene_view,
            bright_texture,
            bright_view,
            blur_h_texture,
            blur_h_view,
            sampler,
            texture_bind_group_layout,
            extract_pipeline,
            extract_bind_group,
            blur_pipeline,
            blur_h_uniform_buffer,
            blur_v_uniform_buffer,
            blur_uniform_bind_group_layout,
            blur_h_tex_bind_group,
            blur_h_uniform_bind_group,
            blur_v_tex_bind_group,
            blur_v_uniform_bind_group,
            composite_pipeline,
            composite_scene_bind_group,
            composite_bloom_bind_group,
        }
    }

    /// The texture view where the scene pass should render into.
    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene_view
    }

    /// Returns the offscreen texture format that the scene must render to.
    pub fn offscreen_format() -> wgpu::TextureFormat {
        OFFSCREEN_FORMAT
    }

    /// Pass 2: Extract bright pixels from the scene texture into bright_texture.
    pub fn extract_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Extract Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bright_view,
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
        pass.set_pipeline(&self.extract_pipeline);
        pass.set_bind_group(0, &self.extract_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Pass 3: Horizontal Gaussian blur — bright_texture -> blur_h_texture.
    pub fn blur_h_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Blur H Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.blur_h_view,
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
        pass.set_pipeline(&self.blur_pipeline);
        pass.set_bind_group(0, &self.blur_h_tex_bind_group, &[]);
        pass.set_bind_group(1, &self.blur_h_uniform_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Pass 4: Vertical Gaussian blur — blur_h_texture -> bright_texture (reused).
    pub fn blur_v_pass(&self, encoder: &mut wgpu::CommandEncoder) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Blur V Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.bright_view,
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
        pass.set_pipeline(&self.blur_pipeline);
        pass.set_bind_group(0, &self.blur_v_tex_bind_group, &[]);
        pass.set_bind_group(1, &self.blur_v_uniform_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Pass 5: Composite scene + bloom onto the final swapchain target.
    pub fn composite_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Bloom Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
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
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, &self.composite_scene_bind_group, &[]);
        pass.set_bind_group(1, &self.composite_bloom_bind_group, &[]);
        pass.draw(0..3, 0..1);
    }

    /// Recreate all offscreen textures and bind groups after a window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        // Recreate textures at new size
        self.scene_texture = create_offscreen_texture(device, width, height, "Scene Texture");
        self.scene_view = self.scene_texture.create_view(&Default::default());
        self.bright_texture = create_offscreen_texture(device, width, height, "Bright Texture");
        self.bright_view = self.bright_texture.create_view(&Default::default());
        self.blur_h_texture = create_offscreen_texture(device, width, height, "Blur H Texture");
        self.blur_h_view = self.blur_h_texture.create_view(&Default::default());

        // Recreate all bind groups that reference the textures
        self.extract_bind_group = create_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.scene_view,
            &self.sampler,
            "Extract BG",
        );
        self.blur_h_tex_bind_group = create_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.bright_view,
            &self.sampler,
            "Blur H Tex BG",
        );
        self.blur_v_tex_bind_group = create_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.blur_h_view,
            &self.sampler,
            "Blur V Tex BG",
        );
        self.composite_scene_bind_group = create_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.scene_view,
            &self.sampler,
            "Composite Scene BG",
        );
        self.composite_bloom_bind_group = create_texture_bind_group(
            device,
            &self.texture_bind_group_layout,
            &self.bright_view,
            &self.sampler,
            "Composite Bloom BG",
        );

        // Update blur direction uniforms with new dimensions.
        // The buffers have COPY_DST usage so they will be written in the next frame
        // via queue.write_buffer from the caller. We store the new values here
        // by recreating the uniform bind groups (buffers themselves are reused).
        let blur_spread = 6.0;
        let blur_h_uniforms = BlurUniforms {
            direction: [blur_spread / width as f32, 0.0],
            _padding: [0.0; 2],
        };
        let blur_v_uniforms = BlurUniforms {
            direction: [0.0, blur_spread / height as f32],
            _padding: [0.0; 2],
        };

        // We need to write the new direction values. Since we don't have access to the queue
        // here, recreate the buffers with the correct initial data.
        self.blur_h_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blur H Uniform"),
                contents: bytemuck::cast_slice(&[blur_h_uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
        self.blur_v_uniform_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blur V Uniform"),
                contents: bytemuck::cast_slice(&[blur_v_uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        self.blur_h_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur H Uniform BG"),
            layout: &self.blur_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.blur_h_uniform_buffer.as_entire_binding(),
            }],
        });
        self.blur_v_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur V Uniform BG"),
            layout: &self.blur_uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.blur_v_uniform_buffer.as_entire_binding(),
            }],
        });
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

fn create_offscreen_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    label: &str,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: OFFSCREEN_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

fn create_texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

fn create_extract_pipeline(
    device: &wgpu::Device,
    texture_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Bloom Extract Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/bloom_extract.wgsl").into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Extract Pipeline Layout"),
        bind_group_layouts: &[texture_bgl],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Extract Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_extract"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: OFFSCREEN_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_blur_pipeline(
    device: &wgpu::Device,
    texture_bgl: &wgpu::BindGroupLayout,
    blur_uniform_bgl: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Blur Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/blur.wgsl").into()),
    });

    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Blur Pipeline Layout"),
        bind_group_layouts: &[texture_bgl, blur_uniform_bgl],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Blur Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_blur"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: OFFSCREEN_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn create_composite_pipeline(
    device: &wgpu::Device,
    texture_bgl: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Composite Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/composite.wgsl").into()),
    });

    // Composite uses two texture bind groups: group 0 = scene, group 1 = bloom
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Composite Pipeline Layout"),
        bind_group_layouts: &[texture_bgl, texture_bgl],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Composite Pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_composite"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}
