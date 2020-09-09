// TODO use wgu::StagingBelt for uploading data
pub mod quad;

use ultraviolet as uv;
use vk_shader_macros::include_glsl;
use wgpu::util::{self as wgu, DeviceExt};

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
        queue: &wgpu::Queue,
        data: &[u8],
        usage: wgpu::BufferUsage,
        bind_fn: &dyn Fn(&wgpu::Buffer) -> wgpu::BindGroup,
    ) {
        let sp = tracing::trace_span!("upload_data", buf = %self.label);
        let _e = sp.enter();

        match self.buffer.as_mut() {
            Some(db) if db.len == data.len() => {
                if data.len() == db.len {
                    queue.write_buffer(&db.buf, 0, data);
                }
            }
            _ => {
                if self.buffer.is_some() {
                    tracing::debug!(
                        buf = %self.label,
                        len = data.len(),
                        "Resizing DynamicBuffer",
                    );
                } else {
                    tracing::debug!(
                        buf = %self.label,
                        len = data.len(),
                        "Initializing DynamicBuffer",
                    );
                }

                let buffer = device.create_buffer_init(&wgu::BufferInitDescriptor {
                    contents: data,
                    usage: usage | wgpu::BufferUsage::COPY_DST,
                    label: Some(self.label),
                });
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
        tracing::debug!(buf = %self.label, "Clearing DynamicBuffer");
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
        let sp = tracing::debug_span!("new_scope_renderer");
        let _e = sp.enter();

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("scope render init"),
        });

        let line_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: 1920, // TODO do not hardcode dims
                height: 1080,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            label: Some("scope line intermediate texture"),
        });

        let line_vs = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            include_glsl!("shaders/line.vert")[..].into(),
        ));
        let line_fs = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            include_glsl!("shaders/line.frag")[..].into(),
        ));

        let line_ssbo_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("scope line ssbo bind layout"),
            });

        let line_uniform_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::UniformBuffer {
                        dynamic: true,
                        min_binding_size: None,
                    },
                    count: None,
                }],
                label: Some("scope uniform bind layout"),
            });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&line_ssbo_bind_layout, &line_uniform_bind_layout],
            push_constant_ranges: &[],
            label: Some("line pipeline layout"),
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: Some(&line_pipeline_layout),
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
                clamp_depth: false,
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
            label: Some("line pipeline"),
        });

        let line_ssbo = DynamicBuffer::new("scope line ssbo");
        let line_uniform = DynamicBuffer::new("scope line uniform");

        let line_copy = quad::QuadRenderer::new(
            &device,
            &line_texture.create_view(&wgpu::TextureViewDescriptor::default()),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            uv::Mat4::identity(),
        );

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: wgpu::Extent3d {
                width: 1920, // TODO do not hardcode dims
                height: 1080,
                depth: 1,
            },
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
                    attachment: &line_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                },
                wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &output_texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                },
            ],
            depth_stencil_attachment: None,
        });

        queue.submit(std::iter::once(encoder.finish()));

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
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        state: &crate::state::State,
    ) {
        let sp = tracing::trace_span!("render_scopes");
        let _e = sp.enter();

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

        let sp = tracing::trace_span!("update_data");
        let update_entered = sp.enter();
        for scope in state.scopes.values() {
            let out = scope.output();

            let uniform = Uniforms {
                resolution: [1920.0, 1080.0, 0.0, 0.0],
                transform: uv::Mat4::from_translation(uv::Vec3::new(
                    -1.0 + grid_cell_width * scope.rect.x as f32,
                    1.0 - grid_cell_height * (scope.rect.y as f32 + 0.5 * scope.rect.h as f32),
                    0.0,
                )) * uv::Mat4::from_nonuniform_scale(uv::Vec3::new(
                    1.0 / scope.output().len() as f32 * grid_cell_width * scope.rect.w as f32,
                    grid_cell_height * scope.rect.h as f32,
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
        drop(update_entered);

        // update line ssbo and uniforms
        if !state.scopes.is_empty() {
            let line_data = bytemuck::cast_slice(&line_data);
            let line_layout = &self.line_ssbo_bind_layout;
            self.line_ssbo.upload(
                device,
                queue,
                line_data,
                wgpu::BufferUsage::STORAGE,
                &|buffer| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: line_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(buffer.slice(..)),
                        }],
                        label: Some("scope line ssbo bind group"),
                    })
                },
            );

            let uniform_data = bytemuck::cast_slice(&line_uniforms);
            let uniform_layout = &self.line_uniform_bind_layout;
            self.line_uniform.upload(
                device,
                queue,
                uniform_data,
                wgpu::BufferUsage::UNIFORM,
                &|buffer| {
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        layout: uniform_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Buffer(
                                buffer.slice(0..std::mem::size_of::<Uniforms>() as u64),
                            ),
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
                let line_view = self
                    .line_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut line_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &line_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: true,
                        },
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
        {
            let output_view = self
                .output_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut copy_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                    attachment: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(if self.flick && state.debug.stutter_test {
                            wgpu::Color::BLUE
                        } else {
                            wgpu::Color::BLACK
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });
            self.line_copy.render(&mut copy_pass);
        }

        self.flick = !self.flick;
    }

    pub fn texture_view(&self) -> wgpu::TextureView {
        self.output_texture
            .create_view(&wgpu::TextureViewDescriptor::default())
    }
}
