use std::panic::{set_hook, take_hook};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::{io, thread, time};

use crossbeam_channel as chan;
use futures::executor::block_on;
use parking_lot::Mutex;
use rayon::prelude::*;
use snafu::{OptionExt, ResultExt, Snafu};
use ultraviolet as uv;
use winit::{
    event,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use crate::audio::{
    connection::{ConnectionTarget, MasterChannel},
    mixer, playback,
};
use crate::config;
use crate::panic;
use crate::state::{self, State};
use crate::ui;

// Errors are usually used when the app should quit
#[derive(Debug, Snafu)]
enum Error {
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

fn preview_transform(window_res: (u32, u32), scope_res: (u32, u32)) -> uv::Mat4 {
    let window_ratio = window_res.0 as f32 / window_res.1 as f32;
    let scope_ratio = scope_res.0 as f32 / scope_res.1 as f32;

    if scope_ratio > window_ratio {
        // letterbox
        uv::Mat4::from_nonuniform_scale(uv::Vec4::new(1.0, window_ratio / scope_ratio, 1.0, 1.0))
    } else {
        // pillarbox
        uv::Mat4::from_nonuniform_scale(uv::Vec4::new(scope_ratio / window_ratio, 1.0, 1.0, 1.0))
    }
}

fn rebuild_master(
    master: &mut playback::Player,
    state: &mut State,
) -> Result<(), samplerate::Error> {
    let mut mixer_config = mixer::MixerBuilder::new();
    mixer_config.channels(master.channels() as usize);
    mixer_config.target_sample_rate(master.sample_rate());

    for source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        if source
            .connections
            .iter()
            .any(|conn| conn.target.is_master())
        {
            let sample_rate = source.spec().sample_rate;
            mixer_config.source_rate(sample_rate);
        }
    }

    master.rebuild_mixer(mixer_config)
}

fn _run(state_file: Option<&str>) -> Result<(), Error> {
    set_hook(panic::dialog(take_hook()));

    // load config
    let config = config::Config::load();
    let mut state = load_state(state_file);

    // create window
    let event_loop = EventLoop::<(wgpu::SwapChainOutput, usize)>::with_user_event();
    let window = WindowBuilder::new()
        .with_inner_size(winit::dpi::PhysicalSize::new(1600.0, 900.0))
        .with_title("rawrscope")
        .with_resizable(true)
        .build(&event_loop)
        .context(WindowCreation)?;
    let mut window_size = window.inner_size();

    // initialize wgpu adapter and device
    // (maybe do this without block_on)
    let surface = wgpu::Surface::create(&window);
    let adapter = block_on(wgpu::Adapter::request(
        &wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance, // maybe do not request high perf
            compatible_surface: Some(&surface),
        },
        wgpu::BackendBit::PRIMARY,
    ))
    .context(AdapterSelection)?;

    let (device, mut queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        extensions: wgpu::Extensions {
            anisotropic_filtering: false,
        },
        limits: wgpu::Limits::default(),
    }));

    // create swapchain
    let mut swap_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: window_size.width as u32,
        height: window_size.height as u32,
        present_mode: wgpu::PresentMode::Fifo,
    };
    let swapchain = Arc::new(Mutex::new(device.create_swap_chain(&surface, &swap_desc)));

    // create swapchain acquisition thread
    let (get_swapchain, get_swapchain_rx) = chan::bounded(0);
    let ev_proxy = event_loop.create_proxy();
    let swapchain_generation = Arc::new(AtomicUsize::new(0));
    let thread_sc = swapchain.clone();
    let thread_scg = swapchain_generation.clone();
    thread::spawn(move || {
        while get_swapchain_rx.recv().is_ok() {
            let gen = thread_scg.load(Ordering::SeqCst);
            let image = thread_sc
                .lock()
                .get_next_texture()
                .expect("swapchain timed out");
            ev_proxy
                .send_event((image, gen))
                .expect("could not send swapchain image");
        }
    });

    // create and configure master player
    let mut master = playback::Player::new(&config).context(MasterCreation)?;

    if let Err(e) = rebuild_master(&mut master, &mut state) {
        log::warn!("Failed to rebuild master mixer: {}", e);
    }

    // initialize imgui
    let mut imgui = imgui::Context::create();
    let mut imgui_plat = imgui_winit_support::WinitPlatform::init(&mut imgui);
    imgui_plat.attach_window(
        imgui.io_mut(),
        &window,
        imgui_winit_support::HiDpiMode::Locked(1.0),
    );
    imgui.set_ini_filename(None);

    let font_size = 15.0;

    imgui.fonts().add_font(&[imgui::FontSource::TtfData {
        data: include_bytes!("../../fonts/Roboto-Regular.ttf"),
        size_pixels: font_size,
        config: None,
    }]);

    let mut imgui_renderer =
        imgui_wgpu::Renderer::new(&mut imgui, &device, &mut queue, swap_desc.format, None);

    let mut scope_renderer = crate::render::Renderer::new(&device, &mut queue);
    let preview_renderer = crate::render::quad::QuadRenderer::new(
        &device,
        &scope_renderer.texture_view(),
        swap_desc.format,
        preview_transform(window.inner_size().into(), (1920, 1080)),
    );

    let buffer_duration = time::Duration::from_secs_f32(config.audio.buffer_ms / 1000.0);

    let scope_frame_secs = 1.0 / state.appearance.framerate as f32;
    let scope_frame_duration = time::Duration::from_secs_f32(scope_frame_secs);
    let mut scope_timer = time::Instant::now() - buffer_duration;

    let mut frame_timer = time::Instant::now();

    let mut reprocess = true;
    let mut command_buffers: Vec<wgpu::CommandBuffer> = Vec::new();

    event_loop.run(move |event, _, control_flow| {
        imgui_plat.handle_event(imgui.io_mut(), &window, &event);

        // update ui
        imgui_plat
            .prepare_frame(imgui.io_mut(), &window)
            .expect("Failed to prepare UI rendering"); // TODO do not expect (need to figure out err handling in event loop)

        *control_flow = ControlFlow::WaitUntil(scope_timer);

        match event {
            event::Event::WindowEvent { event, .. } => match event {
                event::WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,
                event::WindowEvent::Resized(_) => {
                    *control_flow = ControlFlow::Poll;
                    window_size = window.inner_size();

                    swap_desc.width = window_size.width as u32;
                    swap_desc.height = window_size.height as u32;

                    *swapchain.lock() = device.create_swap_chain(&surface, &swap_desc);
                    swapchain_generation.fetch_add(1, Ordering::SeqCst);

                    preview_renderer.update_transform(
                        &device,
                        &mut queue,
                        preview_transform(window.inner_size().into(), (1920, 1080)),
                    );
                }
                event::WindowEvent::MouseInput { .. }
                | event::WindowEvent::CursorMoved { .. }
                | event::WindowEvent::KeyboardInput { .. }
                | event::WindowEvent::MouseWheel { .. } => window.request_redraw(),
                _ => {}
            },
            event::Event::RedrawRequested(_) => {
                if get_swapchain.try_send(()).is_err() {
                    log::debug!("Could not query for a swapchain image (probably busy)")
                }
            }
            event::Event::UserEvent((swap_frame, generation)) => {
                // guard against outdated swapchain images
                if generation != swapchain_generation.load(Ordering::SeqCst) {
                    std::mem::forget(swap_frame);
                    return;
                }

                // update ui
                let im_ui = imgui.frame();
                let mut ext_events = ui::ExternalEvents::default();
                ui::ui(&mut state, &im_ui, &mut ext_events);

                // process external events
                if ext_events.contains(ui::ExternalEvents::REBUILD_MASTER) {
                    if let Err(e) = rebuild_master(&mut master, &mut state) {
                        log::warn!("Failed to rebuild master mixer: {}", e);
                    }
                }
                if ext_events.contains(ui::ExternalEvents::REDRAW_SCOPES) {
                    reprocess = true;
                }

                // begin rendering
                let mut encoder: wgpu::CommandEncoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("present"),
                    });
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

                // copy scopes to screen
                preview_renderer.render(&mut encoder, &swap_frame.view, None);

                // render ui
                imgui_plat.prepare_render(&im_ui, &window);
                imgui_renderer
                    .render(im_ui.render(), &device, &mut encoder, &swap_frame.view)
                    .expect("Failed to render UI"); // TODO do not expect

                // finish rendering
                command_buffers.push(encoder.finish());
                queue.submit(&command_buffers.split_off(0));

                // write frametime to state
                state
                    .debug
                    .frametimes
                    .push_back(frame_timer.elapsed().as_secs_f32() * 1000.0);
                if state.debug.frametimes.len() > 200 {
                    state.debug.frametimes.pop_front();
                }
                frame_timer = time::Instant::now();
            }
            event::Event::NewEvents(event::StartCause::ResumeTimeReached { .. }) => {
                let now = time::Instant::now();

                // create audio submission
                let sub_builder = master.submission_builder(); // TODO optimize
                let mut sub = sub_builder.create(scope_frame_secs);

                let f = state.playback.frame;
                let framerate = state.appearance.framerate;

                let mut loaded_sources = state
                    .audio_sources
                    .iter_mut()
                    .filter_map(|s| s.as_loaded())
                    .collect::<Vec<_>>();

                // TODO this is scuffed
                let sources_exhausted = loaded_sources
                    .iter()
                    .all(|source| f > source.len() / (source.spec().sample_rate / framerate));

                // process any pending audio
                if !sources_exhausted && state.playback.playing || reprocess {
                    reprocess = false;
                    // create scope submissions
                    let mut scope_submissions = state
                        .scopes
                        .iter()
                        .map(|(name, scope)| {
                            (
                                name.clone(),
                                (scope.wanted_length(), scope.build_submission()),
                            )
                        }) // TODO maybe avoid clone
                        .collect::<std::collections::HashMap<_, _>>();

                    let scope_window_secs = state
                        .scopes
                        .iter()
                        .map(|(_, s)| s.wanted_length())
                        .max_by(|a, b| a.partial_cmp(b).unwrap()) // time shouldnt be NaN
                        .unwrap_or(0.0);
                    let full_window_secs =
                        scope_window_secs.max(scope_frame_secs + scope_window_secs / 2.);

                    for source in &mut loaded_sources {
                        let channels = source.spec().channels;
                        let sample_rate = source.spec().sample_rate;

                        let scope_window_len =
                            (sample_rate as f32 * scope_window_secs * f32::from(channels)) as u32;
                        let full_window_len =
                            (sample_rate as f32 * full_window_secs * f32::from(channels)) as u32;

                        let playhead = (sample_rate / framerate) * state.playback.frame;
                        let window_pos = playhead.saturating_sub(scope_window_len / 2);

                        let window = source
                            .chunk_at(window_pos, full_window_len as usize)
                            .unwrap() // safe - no sources should be exhausted
                            .iter()
                            .copied()
                            .collect::<Vec<_>>();

                        for conn in source.connections {
                            let channel_iter = window
                                .iter()
                                .skip(conn.channel as usize)
                                .step_by(channels as usize)
                                .copied();
                            let playhead_offset = (playhead - window_pos) / channels as u32;

                            match conn.target {
                                ConnectionTarget::Master { ref channel } => {
                                    // only submit master when playing
                                    if state.playback.playing {
                                        sub.add(
                                            sample_rate,
                                            match channel {
                                                MasterChannel::Left => 0,
                                                MasterChannel::Right => 1,
                                            },
                                            channel_iter.skip(playhead_offset as usize),
                                        );
                                    }
                                }
                                ConnectionTarget::Scope { ref name, channel } => {
                                    if channel != 0 {
                                        log::warn!("scope channels unimplemented!");
                                    }
                                    if let Some((wanted_length, sub)) =
                                        scope_submissions.get_mut(name)
                                    {
                                        let sub_len = (sample_rate as f32 * *wanted_length) as u32;
                                        let offset = playhead_offset.saturating_sub(sub_len / 2);

                                        sub.add(sample_rate, 0, channel_iter.skip(offset as usize));
                                    } else {
                                        log::warn!("connection to undefined scope {}!", name);
                                    }
                                }
                            }
                        }
                    }

                    // submit and process scope audio
                    for (name, (_, sub)) in scope_submissions.into_iter() {
                        state.scopes.get_mut(&name).unwrap().submit(sub);
                    }

                    if state.debug.multithreaded_centering {
                        state
                            .scopes
                            .par_iter_mut()
                            .for_each(|(_, scope)| scope.process());
                    } else {
                        state
                            .scopes
                            .iter_mut()
                            .for_each(|(_, scope)| scope.process());
                    }

                    // render scopes
                    let mut encoder: wgpu::CommandEncoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("scope render"),
                        });
                    scope_renderer.render(&device, &mut encoder, &state);
                    command_buffers.push(encoder.finish());

                    window.request_redraw();
                }

                // pause when done
                if sources_exhausted && state.playback.playing {
                    state.playback.playing = false;
                }

                // submit master audio
                if let Err(e) = master.submit(sub) {
                    log::error!("Failed to submit audio to master: {}", e);
                }

                if state.playback.playing {
                    state.playback.frame += 1;
                }

                // update scope timer
                scope_timer += scope_frame_duration;
                if now.saturating_duration_since(scope_timer) > buffer_duration {
                    scope_timer = now - buffer_duration;
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
