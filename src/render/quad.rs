use ultraviolet as uv;

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    tex_coord: [f32; 2],
}

const QUAD_VERTS: [Vertex; 4] = [
    Vertex {
        pos: [-1.0, -1.0, 0.0],
        tex_coord: [0.0, 0.0],
    },
    Vertex {
        pos: [1.0, -1.0, 0.0],
        tex_coord: [1.0, 0.0],
    },
    Vertex {
        pos: [-1.0, 1.0, 0.0],
        tex_coord: [0.0, 1.0],
    },
    Vertex {
        pos: [1.0, 1.0, 0.0],
        tex_coord: [1.0, 1.0],
    },
];

pub struct QuadRenderer {
    vertex_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

impl QuadRenderer {
    pub fn new(
        device: &wgpu::Device,
        tex_view: &wgpu::TextureView,
        color_format: wgpu::TextureFormat,
        transform: uv::Mat4,
    ) -> Self {
        // create buffers
        let vertex_buf = device
            .create_buffer_mapped(4, wgpu::BufferUsage::VERTEX)
            .fill_from_slice(&QUAD_VERTS);

        let transform_slice = transform.as_slice();
        let uniform_buf = device
            .create_buffer_mapped(
                transform_slice.len(),
                wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
            )
            .fill_from_slice(transform_slice);

        // create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare_function: wgpu::CompareFunction::Always,
        });

        // load shaders
        let vs_module = device.create_shader_module(
            &wgpu::read_spirv(std::io::Cursor::new(
                &include_bytes!("../../shaders/quad.vert.spv")[..],
            ))
            .unwrap(),
        );
        let fs_module = device.create_shader_module(
            &wgpu::read_spirv(std::io::Cursor::new(
                &include_bytes!("../../shaders/quad.frag.spv")[..],
            ))
            .unwrap(),
        );

        // create pipeline layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            bindings: &[
                wgpu::BindGroupLayoutBinding {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer { dynamic: false },
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        multisampled: false,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                },
                wgpu::BindGroupLayoutBinding {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            bind_group_layouts: &[&bind_group_layout],
        });

        // create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            bindings: &[
                wgpu::Binding {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer {
                        buffer: &uniform_buf,
                        range: 0..(transform_slice.len() * 4) as u64,
                    },
                },
                wgpu::Binding {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(tex_view),
                },
                wgpu::Binding {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        // create pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: &pipeline_layout,
            vertex_stage: wgpu::ProgrammableStageDescriptor {
                module: &vs_module,
                entry_point: "main",
            },
            fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
                module: &fs_module,
                entry_point: "main",
            }),
            rasterization_state: Some(wgpu::RasterizationStateDescriptor {
                front_face: wgpu::FrontFace::Cw,
                cull_mode: wgpu::CullMode::None,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: color_format,
                color_blend: wgpu::BlendDescriptor::REPLACE,
                alpha_blend: wgpu::BlendDescriptor::REPLACE,
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[wgpu::VertexBufferDescriptor {
                stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float3,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Float2,
                        offset: 4 * 3,
                        shader_location: 1,
                    },
                ],
            }],
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
        });

        QuadRenderer {
            vertex_buf,
            uniform_buf,
            bind_group,
            pipeline,
        }
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: target,
                resolve_target: None,
                load_op: wgpu::LoadOp::Load,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::BLACK,
            }],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffers(0, &[(&self.vertex_buf, 0)]);

        pass.draw(0..4, 0..1);
    }

    pub fn update_transform(
        &self,
        device: &wgpu::Device,
        queue: &mut wgpu::Queue,
        transform: uv::Mat4,
    ) {
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });

        let transform_slice = transform.as_slice();
        let staging_buf = device
            .create_buffer_mapped(transform_slice.len(), wgpu::BufferUsage::COPY_SRC)
            .fill_from_slice(transform_slice);

        encoder.copy_buffer_to_buffer(
            &staging_buf,
            0,
            &self.uniform_buf,
            0,
            (transform_slice.len() * 4) as u64,
        );

        queue.submit(&[encoder.finish()]);
    }
}
