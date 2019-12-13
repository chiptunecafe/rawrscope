pub struct Renderer {
    tex: wgpu::Texture,
}

impl Renderer {
    fn new(gpu: &wgpu::Device) -> Self {
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
                usage: wgpu::TextureUsage::SAMPLED | wgpu::TextureUsage::COPY_SRC,
            }),
        }
    }
}
