pub mod quad;

pub struct Renderer {
    tex: wgpu::Texture,
}

impl Renderer {
    pub fn new(gpu: &wgpu::Device) -> Self {
        Renderer {
            tex: gpu.create_texture(&wgpu::TextureDescriptor {
                size: wgpu::Extent3d {
                    width: 1920,
                    height: 1080,
                    depth: 1,
                },
                array_layer_count: 1,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT | wgpu::TextureUsage::SAMPLED,
            }),
        }
    }

    pub fn render(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment: &self.tex.create_default_view(),
                resolve_target: None,
                load_op: wgpu::LoadOp::Clear,
                store_op: wgpu::StoreOp::Store,
                clear_color: wgpu::Color::BLACK,
            }],
            depth_stencil_attachment: None,
        });
    }

    pub fn texture_view(&self) -> wgpu::TextureView {
        self.tex.create_default_view()
    }
}
