use std::io;
use std::panic::{set_hook, take_hook};

use cpal::traits::{DeviceTrait, HostTrait};
use snafu::{OptionExt, ResultExt, Snafu};
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

use crate::audio::{connection::ConnectionTarget, mixer, playback};
use crate::config;
use crate::panic;
use crate::state::{self, State};

// Errors are usually used when the app should quit
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("No output device available on host \"{:?}\"", host))]
    NoOutputDevice { host: cpal::HostId },

    #[snafu(display("Failed to create window: {}", source))]
    WindowCreation { source: winit::error::OsError },

    #[snafu(display("No sufficient graphics card available!"))]
    AdapterSelection,

    #[snafu(display("Failed to create master audio player: {}", source))]
    MasterCreation { source: playback::CreateError },
}

fn load_state(state_file: Option<&str>) -> state::State {
    match state_file {
        Some(path) => match State::from_file(path) {
            Ok((state, warnings)) => {
                for w in warnings {
                    log::warn!("{}", w);
                }
                log::info!("Loaded project from {}", path);
                state
            }
            Err(state::ReadError::OpenError { ref source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                log::warn!("Project not found, writing default...");
                let state = State::default();
                if let Err(e) = state.write(path) {
                    log::warn!("Failed to write new project: {}", e);
                } else {
                    log::debug!("Created new project at {}", path);
                }
                state
            }
            Err(e) => {
                log::error!("Failed to load project: {}", e);
                State::default()
            }
        },
        None => State::default(),
    }
}

fn audio_host(config: &config::Config) -> cpal::Host {
    match &config.audio.host {
        Some(host_name) => {
            if let Some((id, _n)) = cpal::available_hosts()
                .iter()
                .map(|host_id| (host_id, format!("{:?}", host_id)))
                .find(|(_id, n)| n == host_name)
            {
                cpal::host_from_id(*id).unwrap_or_else(|err| {
                    log::warn!(
                        "Could not use host \"{}\": {}, using default...",
                        host_name,
                        err
                    );
                    cpal::default_host()
                })
            } else {
                log::warn!("Host \"{}\" does not exist! Using default...", host_name);
                cpal::default_host()
            }
        }
        None => cpal::default_host(),
    }
}

fn audio_device(config: &config::Config, host: &cpal::Host) -> Result<cpal::Device, Error> {
    match &config.audio.device {
        Some(dev_name) => match host.output_devices() {
            Ok(mut iter) => match iter.find(|dev| {
                dev.name()
                    .ok()
                    .map(|name| &name == dev_name)
                    .unwrap_or(false)
            }) {
                Some(d) => Ok(d),
                None => {
                    log::warn!(
                        "Output device \"{}\" does not exist ... using default",
                        dev_name
                    );
                    host.default_output_device()
                        .context(NoOutputDevice { host: host.id() })
                }
            },
            Err(e) => {
                log::warn!(
                    "Failed to query output devices: {} ... attempting to use default",
                    e
                );
                host.default_output_device()
                    .context(NoOutputDevice { host: host.id() })
            }
        },
        None => host
            .default_output_device()
            .context(NoOutputDevice { host: host.id() }),
    }
}

fn _run(state_file: Option<&str>) -> Result<(), Error> {
    set_hook(panic::dialog(take_hook()));

    // load config
    let config = config::Config::load();
    let mut state = load_state(state_file);

    // create window
    let event_loop = EventLoop::new();
    let window = Window::new(&event_loop).context(WindowCreation)?;
    let mut window_size = window.inner_size().to_physical(window.hidpi_factor());

    // initialize wgpu adapter and device
    let surface = wgpu::Surface::create(&window);
    let adapter = wgpu::Adapter::request(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance, // maybe do not request high perf
        backends: wgpu::BackendBit::PRIMARY,
    })
    .context(AdapterSelection)?;

    let (device, mut queue) = adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    });

    // create swapchain
    let mut swap_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: window_size.width as u32,
        height: window_size.height as u32,
        present_mode: wgpu::PresentMode::Vsync,
    };
    let mut swapchain = device.create_swap_chain(&surface, &swap_desc);

    // initialize audio device
    let audio_host = audio_host(&config);
    let audio_dev = audio_device(&config, &audio_host)?;

    // create and configure master mixer
    let mut master = playback::Player::new(audio_host, audio_dev).context(MasterCreation)?;

    let mut mixer_config = mixer::MixerBuilder::new();
    mixer_config.channels(master.channels() as usize);
    mixer_config.target_sample_rate(master.sample_rate());

    for source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        if source
            .connections
            .iter()
            .any(|conn| conn.target == ConnectionTarget::Master)
        {
            let sample_rate = source.spec().sample_rate;
            mixer_config.source_rate(sample_rate);
        }
    }

    if let Err(e) = master.rebuild_mixer(mixer_config) {
        log::warn!("Failed to rebuild master mixer: {}", e);
    }

    // initialize imgui
    let mut imgui = imgui::Context::create();
    let mut imgui_plat = imgui_winit_support::WinitPlatform::init(&mut imgui);
    imgui_plat.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Default,
    );
    imgui.set_ini_filename(None);

    let font_size = (13.0 * window.hidpi_factor()) as f32;
    imgui.io_mut().font_global_scale = (1.0 / window.hidpi_factor()) as f32;

    imgui
        .fonts()
        .add_font(&[imgui::FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

    let mut imgui_renderer =
        imgui_wgpu::Renderer::new(&mut imgui, &device, &mut queue, swap_desc.format, None);

    // TODO remove hardcoded vars
    let framerate = 60u16;
    let frame_secs = 1.0 / f32::from(framerate);

    let window_ms = 50;

    event_loop.run(move |event, _, control_flow| {
        let sub_builder = master.submission_builder(); // TODO optimize

        imgui_plat.handle_event(imgui.io_mut(), &window, &event);

        match event {
            event::Event::WindowEvent { event, .. } => match event {
                event::WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                event::WindowEvent::Resized(_) => {
                    window_size = window.inner_size().to_physical(window.hidpi_factor());

                    swap_desc.width = window_size.width as u32;
                    swap_desc.height = window_size.height as u32;

                    swapchain = device.create_swap_chain(&surface, &swap_desc);
                }
                _ => {}
            },
            event::Event::EventsCleared => {
                *control_flow = ControlFlow::Poll;

                // create audio submission
                let mut sub = sub_builder.create(frame_secs);

                let f = state.playback.frame;
                let mut loaded_sources = state
                    .audio_sources
                    .iter_mut()
                    .filter_map(|s| s.as_loaded())
                    .collect::<Vec<_>>();

                let sources_exhausted = loaded_sources.iter().all(|source| {
                    f > source.len() / (source.spec().sample_rate / u32::from(framerate))
                });

                if !sources_exhausted && state.playback.playing {
                    for source in &mut loaded_sources {
                        let channels = source.spec().channels;
                        let sample_rate = source.spec().sample_rate;

                        let window_len = sample_rate * window_ms / 1000 * u32::from(channels);
                        let window_pos =
                            (sample_rate / u32::from(framerate)) * state.playback.frame;

                        // TODO dont panic
                        let window = source
                            .chunk_at(window_pos, window_len as usize)
                            .unwrap()
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        let chunk_len = sub
                            .length_of_channel(sample_rate)
                            .expect("submission missing sample rate!")
                            * channels as usize;

                        let chunk = &window[0..chunk_len.min(window.len())];

                        for conn in source.connections {
                            let channel_iter = chunk
                                .iter()
                                .skip(conn.channel as usize)
                                .step_by(channels as usize)
                                .copied();
                            match conn.target {
                                ConnectionTarget::Master => {
                                    sub.add(
                                        sample_rate,
                                        conn.target_channel as usize,
                                        channel_iter,
                                    );
                                }
                                _ => log::warn!("scope connections unimplemented"),
                            }
                        }
                    }
                } else if sources_exhausted && state.playback.playing {
                    state.playback.playing = false;
                }

                if let Err(e) = master.submit(sub) {
                    log::error!("Failed to submit audio to master: {}", e);
                }

                // begin rendering
                let swap_frame = swapchain.get_next_texture();

                imgui_plat
                    .prepare_frame(imgui.io_mut(), &window)
                    .expect("Failed to prepare UI rendering"); // TODO do not expect (need to figure out err handling in event loop)
                let im_ui = imgui.frame();

                crate::ui::ui(&mut state, &im_ui);

                let mut encoder: wgpu::CommandEncoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { todo: 0 });

                // clear screen
                {
                    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &swap_frame.view,
                            resolve_target: None,
                            load_op: wgpu::LoadOp::Clear,
                            store_op: wgpu::StoreOp::Store,
                            clear_color: wgpu::Color {
                                r: 0.3,
                                g: 0.3,
                                b: 0.3,
                                a: 1.0,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });
                }

                imgui_plat.prepare_render(&im_ui, &window);
                imgui_renderer
                    .render(im_ui.render(), &device, &mut encoder, &swap_frame.view)
                    .expect("Failed to render UI"); // TODO do not expect

                queue.submit(&[encoder.finish()]);

                if state.playback.playing {
                    state.playback.frame += 1;
                }
            }
            _ => {}
        }
    });
}

pub fn run(state_file: Option<&str>) {
    if let Err(e) = _run(state_file) {
        log::error!("{}", e)
    }
}
