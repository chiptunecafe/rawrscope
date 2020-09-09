use ultraviolet as uv;
use vk_shader_macros::include_glsl;
use wgpu::util::{self as wgu, DeviceExt};

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 3],
    tex_coord: [f32; 2],
}
unsafe impl bytemuck::Zeroable for Vertex {}
unsafe impl bytemuck::Pod for Vertex {}

const QUAD_VERTS: [Vertex; 4] = [
    Vertex {
        pos: [-1.0, -1.0, 0.0],
        tex_coord: [0.0, 1.0],
    },
    Vertex {
        pos: [1.0, -1.0, 0.0],
        tex_coord: [1.0, 1.0],
    },
    Vertex {
        pos: [-1.0, 1.0, 0.0],
        tex_coord: [0.0, 0.0],
    },
    Vertex {
        pos: [1.0, 1.0, 0.0],
        tex_coord: [1.0, 0.0],
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
        let sp = tracing::debug_span!("new_quad_renderer");
        let _e = sp.enter();

        // create buffers
        let vertex_buf = device.create_buffer_init(&wgu::BufferInitDescriptor {
            label: Some("quad vertex buffer"),
            contents: bytemuck::bytes_of(&QUAD_VERTS),
            usage: wgpu::BufferUsage::VERTEX,
        });

        let transform_slice = transform.as_slice();
        let uniform_buf = device.create_buffer_init(&wgu::BufferInitDescriptor {
            label: Some("quad uniform buffer"),
            contents: bytemuck::cast_slice(transform_slice),
            usage: wgpu::BufferUsage::UNIFORM | wgpu::BufferUsage::COPY_DST,
        });

        // create sampler
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("quad texture sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            lod_min_clamp: -100.0,
            lod_max_clamp: 100.0,
            compare: None,
            anisotropy_clamp: None,
        });

        // load shaders
        // TODO should we cache these?
        let vs_module = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            include_glsl!("shaders/quad.vert")[..].into(),
        ));
        let fs_module = device.create_shader_module(wgpu::ShaderModuleSource::SpirV(
            include_glsl!("shaders/quad.frag")[..].into(),
        ));

        // create pipeline layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStage::VERTEX,
                    ty: wgpu::BindingType::UniformBuffer {
                        dynamic: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::SampledTexture {
                        multisampled: false,
                        component_type: wgpu::TextureComponentType::Float,
                        dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStage::FRAGMENT,
                    ty: wgpu::BindingType::Sampler { comparison: false },
                    count: None,
                },
            ],
            label: Some("quad bind layout"),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("quad pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(uniform_buf.slice(..)),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(tex_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
            label: Some("quad bind group"),
        });

        // create pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            layout: Some(&pipeline_layout),
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
                clamp_depth: false,
                depth_bias: 0,
                depth_bias_slope_scale: 0.0,
                depth_bias_clamp: 0.0,
            }),
            primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
            color_states: &[wgpu::ColorStateDescriptor {
                format: color_format,
                color_blend: wgpu::BlendDescriptor {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha_blend: wgpu::BlendDescriptor {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                write_mask: wgpu::ColorWrite::ALL,
            }],
            depth_stencil_state: None,
            vertex_state: wgpu::VertexStateDescriptor {
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
            },
            sample_count: 1,
            sample_mask: !0,
            alpha_to_coverage_enabled: false,
            label: Some("quad pipeline"),
        });

        QuadRenderer {
            vertex_buf,
            uniform_buf,
            bind_group,
            pipeline,
        }
    }

    pub fn render<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        let sp = tracing::trace_span!("render_quad");
        let _e = sp.enter();

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));

        pass.draw(0..4, 0..1);
    }

    pub fn update_transform(&self, queue: &mut wgpu::Queue, transform: uv::Mat4) {
        let sp = tracing::debug_span!("update_quad_transform");
        let _e = sp.enter();

        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::cast_slice(transform.as_slice()),
        );
    }
}
