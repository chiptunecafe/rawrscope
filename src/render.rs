pub mod quad;

use ultraviolet as uv;
use vk_shader_macros::include_glsl;

// TODO FIX CURSED STRUCT ALIGNMENT
// needed for dynamic bind offsets
#[repr(C, align(256))]
#[derive(Clone, Copy)]
struct Uniforms {
    pub resolution: [f32; 4],
    pub transform: uv::Mat4,
    pub thickness: f32,
    pub base_index: i32,
}
unsafe impl bytemuck::Zeroable for Uniforms {}
unsafe impl bytemuck::Pod for Uniforms {} // uv::Mat4 is ok

struct BufferExt {
    pub len: usize,
    pub buf: wgpu::Buffer,
    pub bind: wgpu::BindGroup,
}

struct DynamicBuffer<'a> {
    buffer: Option<BufferExt>,
    label: &'a str,
}

impl<'a> DynamicBuffer<'a> {
    fn new(label: &'a str) -> Self {
        Self {
            buffer: None,
            label,
        }
    }

    fn buffer(&self) -> Option<&BufferExt> {
        self.buffer.as_ref()
    }

    fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        data: &[u8],
        usage: wgpu::BufferUsage,
        bind_fn: &dyn Fn(&wgpu::Buffer) -> wgpu::BindGroup,
    ) {
        match self.buffer.as_mut() {
            Some(db) if db.len == data.len() => {
                if data.len() == db.len {
                    let staging = device.create_buffer_with_data(data, wgpu::BufferUsage::COPY_SRC);
                    encoder.copy_buffer_to_buffer(&staging, 0, &db.buf, 0, db.len as u64);
                }
            }
            _ => {
                if self.buffer.is_some() {
                    log::info!(
                        "Resizing DynamicBuffer {}; newsize={}",
                        self.label,
                        data.len()
                    );
                } else {
                    log::info!(
                        "Initializing DynamicBuffer {}; size={}",
                        self.label,
                        data.len()
                    );
                }

                let buffer =
                    device.create_buffer_with_data(data, usage | wgpu::BufferUsage::COPY_DST);
                let binding = bind_fn(&buffer);

                self.buffer = Some(BufferExt {
                    len: data.len(),
                    buf: buffer,
                    bind: binding,
                })
            }
        }
    }

    fn clear(&mut self) {
        log::info!("Clearing DynamicBuffer {}", self.label);
        self.buffer.take();
    }
}

pub struct Renderer {
    line_ssbo_bind_layout: wgpu::BindGroupLayout,
    line_ssbo: DynamicBuffer<'static>,

    line_uniform_bind_layout: wgpu::BindGroupLayout,
    line_uniform: DynamicBuffer<'static>,

    line_texture: wgpu::Texture,
    line_pipeline: wgpu::RenderPipeline,

    line_copy: quad::QuadRenderer,

    output_texture: wgpu::Texture,

    flick: bool,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &mut wgpu::Queue) -> Self {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scope render init"),
        });

        let line_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: 1920, // TODO do not hardcode dims
                height: 1080,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: Some("scope line intermediate texture"),
        });

        let line_vs = device.create_shader_module(include_glsl!("shaders/line.vert"));
        let line_fs = device.create_shader_module(include_glsl!("shaders/line.frag"));

        let line_ssbo_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                }],
                label: Some("scope line ssbo bind layout"),
            });

        let line_uniform_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: true },
                }],
                label: Some("scope uniform bind layout"),
            });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&line_ssbo_bind_layout, &line_uniform_bind_layout],
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &line_pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &line_vs,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &line_fs,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: wgpu::CullMode::None,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleList,
            color_states: &[wgpu::ColorStateDescriptor {
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                color_blend: wgpu::BlendDescriptor::REPLACE, // TODO blend
                alpha_blend: wgpu::BlendDescriptor {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Max,
                },
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
                index_format: wgpu::IndexFormat::Uint16,
                vertex_buffers: &[],
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        let line_ssbo = DynamicBuffer::new("scope line ssbo");
        let line_uniform = DynamicBuffer::new("scope line uniform");

        let line_copy = quad::QuadRenderer::new(
            &device,
            &line_texture.create_default_view(),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            uv::Mat4::identity(),
        );

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: 1920, // TODO do not hardcode dims
                height: 1080,
                depth: 1,
            },
            array_layer_count: 1,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: Some("scope output texture"),
        });

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &line_texture.create_default_view(),
                    resolve_target: None,
                    load_op: wgpu::LoadOp::Clear,
                    store_op: wgpu::StoreOp::Store,
                    clear_color: wgpu::Color::TRANSPARENT,
                },
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &output_texture.create_default_view(),
                    resolve_target: None,
                    load_op: wgpu::LoadOp::Clear,
                    store_op: wgpu::StoreOp::Store,
                    clear_color: wgpu::Color::BLACK,
                },
            ],
            depth_stencil_attachment: None,
        });

        queue.submit(&[encoder.finish()]);

        Renderer {
            line_ssbo_bind_layout,
            line_ssbo,

            line_uniform_bind_layout,
            line_uniform,

            line_texture,
            line_pipeline,

            line_copy,

            output_texture,

            flick: false,
        }
    }

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        state: &crate::state::State,
    ) {
        let grid_cell_width = 2.0 / state.appearance.grid_columns as f32;
        let grid_cell_height = 2.0 / state.appearance.grid_rows as f32;

        // prepare line data
        struct LineRenderInfo {
            length: u32,
            uniform_offset: u32,
        }

        // TODO maybe immediately reserve the memory for these
        let mut line_data = Vec::new();
        let mut line_uniforms = Vec::new();
        let mut line_render_info = Vec::new();

        for scope in state.scopes.values() {
            let out = scope.output();

            let uniform = Uniforms {
                resolution: [1920.0, 1080.0, 0.0, 0.0],
                transform: uv::Mat4::from_translation(uv::Vec3::new(
                    -1.0 + grid_cell_width * scope.rect.x as f32,
                    1.0 - grid_cell_height * (scope.rect.y as f32 + 0.5 * scope.rect.h as f32),
                    0.0,
                )) * uv::Mat4::from_nonuniform_scale(uv::Vec4::new(
                    1.0 / scope.output().len() as f32 * grid_cell_width * scope.rect.w as f32,
                    grid_cell_height * scope.rect.h as f32,
                    1.0,
                    1.0,
                )),
                thickness: scope.line_width,
                base_index: line_data.len() as i32,
            };
            let render_info = LineRenderInfo {
                length: out.len() as u32,
                uniform_offset: (line_uniforms.len() * std::mem::size_of::<Uniforms>()) as u32,
            };

            line_data.extend_from_slice(out);
            line_uniforms.push(uniform);
            line_render_info.push(render_info);
        }

        // update line ssbo and uniforms
        if !state.scopes.is_empty() {
            let line_data = bytemuck::cast_slice(&line_data);
            let line_layout = &self.line_ssbo_bind_layout;
            self.line_ssbo.upload(
                device,
                encoder,
                line_data,
                wgpu::BufferUsage::STORAGE | wgpu::BufferUsage::STORAGE_READ,
                &|buffer| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: line_layout,
                        bindings: &[wgpu::Binding {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer {
                                buffer,
                                range: 0..line_data.len() as u64,
                            },
                        }],
                        label: Some("scope line ssbo bind group"),
                    })
                },
            );

            let uniform_data = bytemuck::cast_slice(&line_uniforms);
            let uniform_layout = &self.line_uniform_bind_layout;
            self.line_uniform.upload(
                device,
                encoder,
                uniform_data,
                wgpu::BufferUsage::UNIFORM,
                &|buffer| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: uniform_layout,
                        bindings: &[wgpu::Binding {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer {
                                buffer,
                                range: 0..std::mem::size_of::<Uniforms>() as u64,
                            },
                        }],
                        label: Some("scope line uniform bind group"),
                    })
                },
            );
        } else {
            self.line_ssbo.clear();
        }

        // TODO make this guard logic a bit cleaner
        if let Some(ssbo) = self.line_ssbo.buffer() {
            if let Some(uniforms) = self.line_uniform.buffer() {
                // render lines
                let line_view = self.line_texture.create_default_view();
                let mut line_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &line_view,
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::TRANSPARENT,
                    }],
                    depth_stencil_attachment: None,
                });
                line_pass.set_pipeline(&self.line_pipeline);
                line_pass.set_bind_group(0, &ssbo.bind, &[]);
                for render_data in &line_render_info {
                    line_pass.set_bind_group(1, &uniforms.bind, &[render_data.uniform_offset]);
                    let end = (render_data.length - 1) * 6;
                    line_pass.draw(0..end, 0..1);
                }
            }
        }

        // copy lines to output texture
        self.line_copy.render(
            encoder,
            &self.output_texture.create_default_view(),
            Some(if self.flick && state.debug.stutter_test {
                wgpu::Color::BLUE
            } else {
                wgpu::Color::BLACK
            }),
        );

        self.flick = !self.flick;
    }

    pub fn texture_view(&self) -> wgpu::TextureView {
        self.output_texture.create_default_view()
    }
}
