pub mod quad;

use ultraviolet as uv;
use vk_shader_macros::include_glsl;

// TODO: scuffed alignment; seems to work but its too big somehow
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct Uniforms {
    pub resolution: [f32; 4],
    pub transform: uv::Mat4,
    pub thickness: f32,
    pub base_index: i32,
}

pub struct Renderer {
    line_ssbo: wgpu::Buffer,
    line_data_length: usize,
    line_uniform: wgpu::Buffer,
    line_texture: wgpu::Texture,
    line_ssbo_bind_layout: wgpu::BindGroupLayout,
    line_ssbo_bind: wgpu::BindGroup,
    line_uniform_bind: wgpu::BindGroup,
    line_pipeline: wgpu::RenderPipeline,

    line_copy: quad::QuadRenderer,

    output_texture: wgpu::Texture,
    flick: bool,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &mut wgpu::Queue) -> Self {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });

        let line_ssbo = device.create_buffer(&wgpu::BufferDescriptor {
            size: 1,
            usage: wgpu::BufferUsage::STORAGE
                | wgpu::BufferUsage::STORAGE_READ
                | wgpu::BufferUsage::COPY_DST,
        });

        let line_uniform = device
            .create_buffer_mapped(1, wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST)
            .fill_from_slice(&[Uniforms {
                resolution: [1920.0, 1080.0, 0.0, 0.0],
                transform: uv::Mat4::identity(),
                thickness: 0.0,
                base_index: 0,
            }]);

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
        });

        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &line_texture.create_default_view(),
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::BLACK,
            }],
            depth_stencil_attachment: None,
        });

        let line_vs = device.create_shader_module(include_glsl!("shaders/line.vert"));
        let line_fs = device.create_shader_module(include_glsl!("shaders/line.frag"));

        let line_ssbo_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::StorageBuffer {
                        dynamic: false,
                        readonly: true,
                    },
                }],
            });

        let line_uniform_bind_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                bindings: &[wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX | wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                }],
            });

        let line_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&line_ssbo_bind_layout, &line_uniform_bind_layout],
        });

        let line_ssbo_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &line_ssbo_bind_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &line_ssbo,
                    range: 0..1,
                },
            }],
        });

        let line_uniform_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &line_uniform_bind_layout,
            bindings: &[wgpu::Binding {
                binding: 0,
                resource: wgpu::BindingResource::Buffer {
                    buffer: &line_uniform,
                    range: 0..std::mem::size_of::<Uniforms>() as u64,
                },
            }],
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
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

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
        });

        queue.submit(&[encoder.finish()]);

        Renderer {
            line_ssbo,
            line_data_length: 1,
            line_uniform,
            line_texture,
            line_ssbo_bind_layout,
            line_ssbo_bind,
            line_uniform_bind,
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
        struct LineRenderData {
            data_range: std::ops::Range<u32>,
            uniform_staging: wgpu::Buffer,
        }
        let mut line_data = Vec::new(); // TODO allocate entire size immediately
        let mut line_render_data = Vec::new();
        for (_, scope) in &state.scopes {
            let uniform = Uniforms {
                resolution: [1920.0, 1080.0, 0.0, 0.0],
                transform: uv::Mat4::from_translation(uv::Vec3::new(
                    -1.0 + grid_cell_width * scope.rect.x as f32,
                    -1.0 + grid_cell_height * (scope.rect.y as f32 + 0.5 * scope.rect.h as f32),
                    0.0,
                )) * uv::Mat4::from_nonuniform_scale(uv::Vec4::new(
                    1.0 / scope.output().len() as f32 * grid_cell_width * scope.rect.w as f32,
                    -1.0 * grid_cell_height * scope.rect.h as f32, // flipped since negative is up
                    1.0,
                    1.0,
                )),
                thickness: scope.line_width,
                base_index: line_data.len() as i32,
            };
            let render_data = LineRenderData {
                data_range: line_data.len() as u32
                    ..line_data.len() as u32 + scope.output().len() as u32,
                uniform_staging: device
                    .create_buffer_mapped(1, wgpu::BufferUsage::COPY_SRC)
                    .fill_from_slice(&[uniform]),
            };
            line_render_data.push(render_data);

            line_data.extend_from_slice(scope.output());
        }

        // update line ssbo
        if line_data.len() == self.line_data_length && line_data.len() > 1 {
            // create staging buffer
            let copy_buffer = device
                .create_buffer_mapped(line_data.len(), wgpu::BufferUsage::COPY_SRC)
                .fill_from_slice(&line_data);
            // copy to existing buffer
            encoder.copy_buffer_to_buffer(
                &copy_buffer,
                0,
                &self.line_ssbo,
                0,
                (line_data.len() * 4) as u64,
            );
        } else if line_data.len() > 1 {
            log::info!(
                "resizing line ssbo ({} -> {})",
                self.line_data_length,
                line_data.len()
            );
            // create new line data buffer
            let line_ssbo = device
                .create_buffer_mapped(
                    line_data.len(),
                    wgpu::BufferUsage::STORAGE
                        | wgpu::BufferUsage::STORAGE_READ
                        | wgpu::BufferUsage::COPY_DST,
                )
                .fill_from_slice(&line_data);
            // create new binding
            let line_ssbo_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &self.line_ssbo_bind_layout,
                bindings: &[wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &line_ssbo,
                        range: 0..(line_data.len() * 4) as u64,
                    },
                }],
            });
            // update fields in self
            self.line_ssbo = line_ssbo;
            self.line_ssbo_bind = line_ssbo_bind;
            self.line_data_length = line_data.len();
        }

        if self.line_data_length > 1 {
            // render lines (each line has to be a separate pass :/)
            {
                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &self.line_texture.create_default_view(),
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Clear,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::TRANSPARENT,
                    }],
                    depth_stencil_attachment: None,
                });
            }
            for render_data in line_render_data.into_iter() {
                encoder.copy_buffer_to_buffer(
                    &render_data.uniform_staging,
                    0,
                    &self.line_uniform,
                    0,
                    std::mem::size_of::<Uniforms>() as u64,
                );

                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                        attachment: &self.line_texture.create_default_view(),
                        resolve_target: None,
                        load_op: wgpu::LoadOp::Load,
                        store_op: wgpu::StoreOp::Store,
                        clear_color: wgpu::Color::BLACK,
                    }],
                    depth_stencil_attachment: None,
                });
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(0, &self.line_ssbo_bind, &[]);
                pass.set_bind_group(1, &self.line_uniform_bind, &[]);

                let begin = render_data.data_range.start * 6;
                let end = (render_data.data_range.end - 2) * 6;
                pass.draw(begin..end, 0..1);
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
