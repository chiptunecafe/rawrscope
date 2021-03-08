use std::time::Instant;

use anyhow::{Context, Result};
use futures::executor::block_on;
use winit::{
    event::{Event as WinitEvent, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

use crate::swapchain::AsyncSwapchain;

pub struct App {
    // Window structure
    window: Window,

    // WGPU structures
    _instance: wgpu::Instance,
    surface: wgpu::Surface,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,

    // Imgui and integration structures
    imgui: imgui::Context,
    im_plat: imgui_winit_support::WinitPlatform,
    im_renderer: imgui_wgpu::Renderer,

    // UI state
    ui: crate::ui::Ui,
}

impl App {
    pub fn new(
        args: &crate::Args,
        event_loop: &EventLoop<crate::swapchain::Event>,
    ) -> Result<Self> {
        // Pretty up the project path for the titlebar
        let path_display = args
            .project_file
            .as_ref()
            .map(|p| format!("{}", p.display()))
            .unwrap_or_else(|| String::from("new project"));

        // Open a window
        let window = WindowBuilder::new()
            .with_inner_size(winit::dpi::PhysicalSize::new(1600.0, 900.0))
            .with_title(format!("rawrscope ({})", path_display)) // TODO include project path
            .with_resizable(true)
            .build(&event_loop)
            .context("Failed to open a window")?;

        // Initialize WGPU
        let instance = wgpu::Instance::new(wgpu::BackendBit::PRIMARY); // TODO configurable backend
        let surface = unsafe { instance.create_surface(&window) };
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
        }))
        .context("No GPU adapters available")?;
        let (device, queue) = block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
            },
            None,
        ))
        .context("No suitable GPU adapter available")?;

        // Initialize Imgui and integrations
        let mut imgui = imgui::Context::create();
        let mut im_plat = imgui_winit_support::WinitPlatform::init(&mut imgui);
        im_plat.attach_window(
            imgui.io_mut(),
            &window,
            imgui_winit_support::HiDpiMode::Locked(1.0),
        );
        imgui.set_ini_filename(None);

        imgui.fonts().add_font(&[imgui::FontSource::TtfData {
            data: include_bytes!("../fonts/Roboto-Regular.ttf"),
            size_pixels: 15.0,
            config: None,
        }]);

        let color_fmt = adapter.get_swap_chain_preferred_format(&surface);
        let im_renderer = imgui_wgpu::Renderer::new(
            &mut imgui,
            &device,
            &queue,
            imgui_wgpu::RendererConfig {
                texture_format: color_fmt,
                ..Default::default()
            },
        );

        Ok(Self {
            window,

            _instance: instance,
            surface,
            adapter,
            device,
            queue,

            imgui,
            im_plat,
            im_renderer,

            ui: Default::default(),
        })
    }

    pub fn run(mut self, event_loop: EventLoop<crate::swapchain::Event>) -> ! {
        // Build initial swapchain
        let color_fmt = self.adapter.get_swap_chain_preferred_format(&self.surface);
        let mut swapchain_desc = wgpu::SwapChainDescriptor {
            usage: wgpu::TextureUsage::RENDER_ATTACHMENT,
            format: color_fmt,
            width: 1600,
            height: 900,
            present_mode: wgpu::PresentMode::Fifo,
        };
        let mut swapchain = AsyncSwapchain::new(
            self.device
                .create_swap_chain(&self.surface, &swapchain_desc),
            event_loop.create_proxy(),
        );
        let mut pending_resize = false;
        let mut pending_redraws = 0;

        let mut last_frame = Instant::now();

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            // Submit window events to imgui winit platform
            self.im_plat
                .handle_event(self.imgui.io_mut(), &self.window, &event);

            match event {
                // TODO logic
                WinitEvent::MainEventsCleared => {}

                // Handle some base window events
                WinitEvent::WindowEvent {
                    event: WindowEvent::Resized(size),
                    ..
                } => {
                    swapchain_desc.width = size.width;
                    swapchain_desc.height = size.height;
                    pending_resize = true;
                }
                WinitEvent::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                }

                // Ignore this one (clogs up redraw pipeline)
                WinitEvent::WindowEvent {
                    event: WindowEvent::Moved(_),
                    ..
                } => (),

                // Redraw on any other window event
                WinitEvent::WindowEvent { .. } => {
                    // Imgui requires 2 redraws for some actions
                    pending_redraws = 2;
                    self.window.request_redraw();
                }

                // Request a swapchain image on any redraw
                WinitEvent::RedrawRequested(_) => swapchain.request_image(),

                // Redraw screen with new swapchain image
                WinitEvent::UserEvent(swap_image) => {
                    tracing::trace!("Received swapchain image");

                    match swap_image {
                        Ok(swap_image) => {
                            // Update imgui delta time
                            let frametime = last_frame.elapsed();
                            self.imgui.io_mut().update_delta_time(frametime);
                            last_frame = Instant::now();

                            // Prepare imgui winit platform frame
                            self.im_plat
                                .prepare_frame(self.imgui.io_mut(), &self.window)
                                .expect("Failed to prepare imgui winit platform");

                            // Build UI
                            let ui = self.imgui.frame();
                            {
                                self.ui.build(&ui);
                            }

                            // Render UI
                            let mut encoder = self.device.create_command_encoder(
                                &wgpu::CommandEncoderDescriptor {
                                    label: Some("rawrscope present encoder"),
                                },
                            );

                            // TODO do that cursor thing the imgui example does
                            self.im_plat.prepare_render(&ui, &self.window);

                            {
                                let mut pass =
                                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                                        label: Some("rawrscope ui"),
                                        color_attachments: &[
                                            wgpu::RenderPassColorAttachmentDescriptor {
                                                attachment: &swap_image.output.view,
                                                resolve_target: None,
                                                ops: wgpu::Operations {
                                                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                                    store: true,
                                                },
                                            },
                                        ],
                                        depth_stencil_attachment: None,
                                    });
                                self.im_renderer
                                    .render(ui.render(), &self.queue, &self.device, &mut pass)
                                    .expect("Failed to render imgui UI");
                            }

                            self.queue.submit(Some(encoder.finish()));
                        }

                        // Report any spurious swapchain errors
                        Err(e) => {
                            tracing::error!("Swapchain image acquisition failed: {}", e);
                        }
                    }

                    // Notify swapchain that we have presented the given image
                    swapchain.notify_presented();

                    // Decrement redraw counter, requesting another redraw if necessary
                    pending_redraws -= 1;
                    if pending_redraws > 0 {
                        self.window.request_redraw();
                    }
                }
                _ => (),
            }

            // Apply any pending resizes when it is safe to do so
            if pending_resize && swapchain.presented() {
                tracing::trace!("Resizing swapchain...");
                pending_resize = false;

                swapchain.replace_swapchain(
                    self.device
                        .create_swap_chain(&self.surface, &swapchain_desc),
                );
            }
        })
    }
}
