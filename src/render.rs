pub mod quad;

use ultraviolet as uv;

pub struct Renderer {
    line_texture: wgpu::Texture,
    line_copy: quad::QuadRenderer,
    output_texture: wgpu::Texture,
    flick: bool,
}

impl Renderer {
    pub fn new(device: &wgpu::Device) -> Self {
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
        Renderer {
            line_texture,
            line_copy,
            output_texture,
            flick: false,
        }
    }

    pub fn render(&mut self, encoder: &mut wgpu::CommandEncoder, state: &crate::state::State) {
        // render lines
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
