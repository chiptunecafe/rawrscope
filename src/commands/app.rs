use std::panic::{set_hook, take_hook};
use std::sync::Arc;
use std::{io, thread, time};

use futures::executor::block_on;
use parking_lot::{Condvar, Mutex};
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

struct AsyncSwapchain {
    swapchain: Arc<Mutex<(usize, wgpu::SwapChain)>>,
    thread_handle: thread::JoinHandle<()>,
    cvar_pair: Arc<(Mutex<bool>, Condvar)>,
}

impl AsyncSwapchain {
    fn new(
        initial_swapchain: wgpu::SwapChain,
        event_proxy: winit::event_loop::EventLoopProxy<(usize, wgpu::SwapChainFrame)>,
    ) -> Self {
        let cvar_pair = Arc::new((Mutex::new(false), Condvar::new()));
        let swapchain = Arc::new(Mutex::new((0, initial_swapchain)));

        let cvar_pair_2 = cvar_pair.clone();
        let swapchain_2 = swapchain.clone();

        tracing::debug!("Starting swapchain thread");
        let thread_handle = thread::spawn(move || {
            let sp = tracing::debug_span!("swapchain_thread");
            let _e = sp.enter();

            let &(ref lock, ref cvar) = &*cvar_pair_2;
            loop {
                {
                    let mut image_requested = lock.lock();
                    if !*image_requested {
                        cvar.wait(&mut image_requested);
                    }
                    *image_requested = false;
                }

                tracing::trace!("Received image request");
                let (gen, image) = {
                    let mut swapchain = swapchain_2.lock();
                    (
                        swapchain.0,
                        swapchain
                            .1
                            .get_current_frame()
                            .expect("swapchain timed out"),
                    )
                };
                event_proxy
                    .send_event((gen, image))
                    .expect("event loop closed");

                tracing::trace!("Parking until image is released");
                thread::park();
            }
        });

        Self {
            swapchain,
            thread_handle,
            cvar_pair,
        }
    }

    fn generation(&self) -> usize {
        self.swapchain.lock().0
    }

    fn notify_presented(&self) {
        tracing::trace!("Unparking swapchain thread");
        self.thread_handle.thread().unpark();
    }

    fn replace_swapchain<F: Fn() -> wgpu::SwapChain>(&mut self, builder: F) {
        tracing::debug!("Replacing swapchain");
        let mut swapchain = self.swapchain.lock();
        swapchain.0 += 1;

        let sp = tracing::debug_span!("rebuild_swapchain");
        let _e = sp.enter();
        swapchain.1 = builder();
    }

    fn request_image(&mut self) {
        let mut requested = self.cvar_pair.0.lock();
        *requested = true;
        self.cvar_pair.1.notify_one();
    }
}

// Errors are usually used when the app should quit
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("Failed to create window: {}", source))]
    WindowCreation { source: winit::error::OsError },

    #[snafu(display("No sufficient graphics card available!"))]
    AdapterSelection,

    #[snafu(display("Failed to request a wgpu device: {}", source))]
    DeviceRequest { source: wgpu::RequestDeviceError },

    #[snafu(display("Failed to create master audio player: {}", source))]
    MasterCreation { source: playback::CreateError },
}

fn load_state(state_file: Option<&str>) -> state::State {
    let sp = tracing::debug_span!("load_project", path = ?state_file);
    let _e = sp.enter();

    match state_file {
        Some(path) => match State::from_file(path) {
            Ok((state, warnings)) => {
                for w in warnings {
                    tracing::warn!("{}", w);
                }
                state
            }
            Err(state::ReadError::OpenError { ref source, .. })
                if source.kind() == io::ErrorKind::NotFound =>
            {
                tracing::warn!("Project not found... writing default");
                let sp = tracing::debug_span!("write_default");
                let _e = sp.enter();

                let state = State::default();
                if let Err(e) = state.write(path) {
                    tracing::warn!("{}", e);
                }
                state
            }
            Err(e) => {
                tracing::error!("{}", e);
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
        uv::Mat4::from_nonuniform_scale(uv::Vec3::new(1.0, window_ratio / scope_ratio, 1.0))
    } else {
        // pillarbox
        uv::Mat4::from_nonuniform_scale(uv::Vec3::new(scope_ratio / window_ratio, 1.0, 1.0))
    }
}

fn rebuild_master(
    master: &mut playback::Player,
    state: &mut State,
) -> Result<(), samplerate::Error> {
    let sp = tracing::debug_span!("rebuild_master");
    let _e = sp.enter();

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
            tracing::debug!("Adding source sample rate {}hz", sample_rate);
            mixer_config.source_rate(sample_rate);
        }
    }

    master.rebuild_mixer(mixer_config)
}

fn _run(state_file: Option<&str>) -> Result<(), Error> {
    let sp = tracing::info_span!("init");
    let init_entered = sp.enter();

    set_hook(panic::dialog(take_hook()));

    // load config
    let config = config::Config::load();
    let mut state = load_state(state_file);

    // create window
    let sp = tracing::debug_span!("window");
    let win_entered = sp.enter();
    let event_loop = EventLoop::<(usize, wgpu::SwapChainFrame)>::with_user_event();
    let window = WindowBuilder::new()
        .with_inner_size(winit::dpi::PhysicalSize::new(1600.0, 900.0))
        .with_title("rawrscope")
        .with_resizable(true)
        .build(&event_loop)
        .context(WindowCreation)?;
    let mut window_size = window.inner_size();
    drop(win_entered);

    // initialize wgpu adapter and device
    let sp = tracing::debug_span!("gpu");
    let gpu_entered = sp.enter();

    let instance = wgpu::Instance::new(config.video.backend.to_wgpu_backend());
    let surface = unsafe { instance.create_surface(&window) };

    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance, // maybe do not request high perf
        compatible_surface: Some(&surface),
    }))
    .context(AdapterSelection)?;

    let (device, mut queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            features: wgpu::Features::empty(),
            limits: wgpu::Limits::default(),
            shader_validation: true,
        },
        None,
    ))
    .context(DeviceRequest)?;
    drop(gpu_entered);

    // create swapchain
    let sp = tracing::debug_span!("swapchain");
    let swap_init_entered = sp.enter();
    let mut swap_desc = wgpu::SwapChainDescriptor {
        usage: wgpu::TextureUsage::OUTPUT_ATTACHMENT,
        format: wgpu::TextureFormat::Bgra8Unorm,
        width: window_size.width as u32,
        height: window_size.height as u32,
        present_mode: wgpu::PresentMode::Fifo,
    };
    let init_swapchain = device.create_swap_chain(&surface, &swap_desc);
    let mut swapchain = AsyncSwapchain::new(init_swapchain, event_loop.create_proxy());
    drop(swap_init_entered);

    // create and configure master player
    let sp = tracing::debug_span!("audio");
    let audio_init_entered = sp.enter();
    let mut master = playback::Player::new(&config).context(MasterCreation)?;
    if let Err(e) = rebuild_master(&mut master, &mut state) {
        tracing::error!("{}", e);
    }
    drop(audio_init_entered);

    // initialize imgui
    let sp = tracing::debug_span!("imgui");
    let imgui_init_entered = sp.enter();
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
    drop(imgui_init_entered);

    // set up renderers
    let sp = tracing::debug_span!("renderers");
    let renderers_init_entered = sp.enter();
    let mut imgui_renderer =
        imgui_wgpu::Renderer::new(&mut imgui, &device, &queue, swap_desc.format);

    let mut scope_renderer = crate::render::Renderer::new(&device, &mut queue);
    let preview_renderer = crate::render::quad::QuadRenderer::new(
        &device,
        &scope_renderer.texture_view(),
        swap_desc.format,
        preview_transform(window.inner_size().into(), (1920, 1080)),
    );
    drop(renderers_init_entered);

    let buffer_duration = time::Duration::from_secs_f32(config.audio.buffer_ms / 1000.0);

    let scope_frame_secs = 1.0 / state.appearance.framerate as f32;
    let scope_frame_duration = time::Duration::from_secs_f32(scope_frame_secs);
    let mut scope_timer = time::Instant::now() - buffer_duration;

    let mut frame_timer = time::Instant::now();

    let mut reprocess = true;
    let mut command_buffers: Vec<wgpu::CommandBuffer> = Vec::new();

    drop(init_entered);

    let sp = tracing::info_span!("main");
    let _e = sp.enter();
    event_loop.run(move |event, _, control_flow| {
        imgui_plat.handle_event(imgui.io_mut(), &window, &event);

        // update ui
        imgui_plat
            .prepare_frame(imgui.io_mut(), &window)
            .expect("Failed to prepare UI rendering"); // TODO do not expect (need to figure out err handling in event loop)

        *control_flow = ControlFlow::WaitUntil(scope_timer);

        match event {
            event::Event::WindowEvent { event, .. } => match event {
                event::WindowEvent::CloseRequested => {
                    tracing::debug!("Exit requested");
                    *control_flow = ControlFlow::Exit;
                }
                event::WindowEvent::Resized(size) => {
                    let sp = tracing::debug_span!("resize", size = ?size);
                    let _e = sp.enter();

                    *control_flow = ControlFlow::Poll;
                    window_size = size;

                    swap_desc.width = window_size.width as u32;
                    swap_desc.height = window_size.height as u32;

                    swapchain.replace_swapchain(|| device.create_swap_chain(&surface, &swap_desc));

                    preview_renderer.update_transform(
                        &mut queue,
                        preview_transform(size.into(), (1920, 1080)),
                    );
                }
                event::WindowEvent::MouseInput { .. }
                | event::WindowEvent::CursorMoved { .. }
                | event::WindowEvent::KeyboardInput { .. }
                | event::WindowEvent::MouseWheel { .. } => {
                    tracing::trace!("Submitting winit redraw request");
                    window.request_redraw();
                }
                _ => {}
            },
            event::Event::RedrawRequested(_) => {
                tracing::trace!("Requesting new swapchain image");
                swapchain.request_image();
            }
            event::Event::UserEvent((generation, swap_frame)) => {
                // guard against outdated swapchain images
                let swap_generation = swapchain.generation();
                if generation != swap_generation {
                    tracing::warn!(
                        gen = generation,
                        cur_gen = swap_generation,
                        "Forgetting outdated swapchain image",
                    );
                    std::mem::forget(swap_frame);
                    swapchain.notify_presented();
                    return;
                }

                // update ui
                let sp = tracing::debug_span!("ui");
                let ui_entered = sp.enter();

                let im_ui = imgui.frame();
                let mut ext_events = ui::ExternalEvents::default();
                ui::ui(&mut state, &im_ui, &mut ext_events);

                // process external events
                if ext_events.contains(ui::ExternalEvents::REBUILD_MASTER) {
                    if let Err(e) = rebuild_master(&mut master, &mut state) {
                        tracing::warn!("Failed to rebuild master mixer: {}", e);
                    }
                }
                if ext_events.contains(ui::ExternalEvents::REDRAW_SCOPES) {
                    reprocess = true;
                }
                drop(ui_entered);

                // begin rendering
                let sp = tracing::debug_span!("render");
                let _e = sp.enter();

                let mut encoder: wgpu::CommandEncoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("present"),
                    });

                // clear screen and draw ui
                {
                    let sp = tracing::trace_span!("ui");
                    let _ui_render_entered = sp.enter();

                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                            attachment: &swap_frame.output.view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.3, g: 0.3, b: 0.3, a: 1.0 }),
                                store: true,
                            },
                        }],
                        depth_stencil_attachment: None,
                    });

                    // copy scopes to screen
                    preview_renderer.render(&mut pass);

                    imgui_plat.prepare_render(&im_ui, &window);
                    imgui_renderer
                        .render(im_ui.render(), &queue, &device, &mut pass)
                        .expect("Failed to render UI"); // TODO do not expect
                }

                // finish rendering
                command_buffers.push(encoder.finish());
                tracing::debug!(n_buffers = command_buffers.len(), "Submitting all pending command buffers");
                queue.submit(command_buffers.split_off(0));

                // write frametime to state
                state
                    .debug
                    .frametimes
                    .push_back(frame_timer.elapsed().as_secs_f32() * 1000.0);
                if state.debug.frametimes.len() > 200 {
                    state.debug.frametimes.pop_front();
                }
                frame_timer = time::Instant::now();

                drop(swap_frame);
                swapchain.notify_presented();
            }
            event::Event::NewEvents(event::StartCause::ResumeTimeReached { .. }) => {
                let sp = tracing::debug_span!("update_audio");
                let _e = sp.enter();

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
                        let sp =
                            tracing::trace_span!("process", source = %source.path().file_name().unwrap().to_string_lossy());
                        let _e = sp.enter();

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
                            tracing::trace!(conn = ?conn, "Connecting source");

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
                                        tracing::warn!("Scope channels unimplemented");
                                    }
                                    if let Some((wanted_length, sub)) =
                                        scope_submissions.get_mut(name)
                                    {
                                        let sub_len = (sample_rate as f32 * *wanted_length) as u32;
                                        let offset = playhead_offset.saturating_sub(sub_len / 2);

                                        sub.add(sample_rate, 0, channel_iter.skip(offset as usize));
                                    } else {
                                        tracing::warn!(target = %name, "Unknown connection target");
                                    }
                                }
                            }
                        }
                    }

                    // submit and process scope audio
                    for (name, (_, sub)) in scope_submissions.into_iter() {
                        tracing::trace!(scope = %name, "Submitting audio");
                        state.scopes.get_mut(&name).unwrap().submit(sub);
                    }

                    // TODO add logging spans per scope for per-scope logging
                    let sp = tracing::debug_span!("centering");
                    let centering_entered = sp.enter();
                    if state.debug.multithreaded_centering {
                        state
                            .scopes
                            .values_mut()
                            .par_bridge()
                            .for_each(|scope| scope.process());
                    } else {
                        state
                            .scopes
                            .iter_mut()
                            .for_each(|(_, scope)| scope.process());
                    }
                    drop(centering_entered);

                    // render scopes
                    let mut encoder: wgpu::CommandEncoder =
                        device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("scope render"),
                        });
                    scope_renderer.render(&device, &queue, &mut encoder, &state);
                    command_buffers.push(encoder.finish());

                    tracing::trace!("Submitting winit redraw request");
                    window.request_redraw();
                }

                // pause when done
                if sources_exhausted && state.playback.playing {
                    state.playback.playing = false;
                }

                // submit master audio
                tracing::trace!("Submitting master audio");
                if let Err(e) = master.submit(sub) {
                    tracing::error!("Failed to submit audio to master: {}", e);
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
        tracing::error!("{}", e)
    }
}
