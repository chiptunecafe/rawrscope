use std::io;
use std::panic::{set_hook, take_hook};

use cpal::traits::{DeviceTrait, HostTrait};
use glutin::{Event, WindowEvent};
use snafu::{OptionExt, ResultExt, Snafu};

use crate::audio::{connection::ConnectionTarget, mixer, playback};
use crate::config;
use crate::panic;
use crate::state::{self, State};

// Errors are usually used when the app should quit
#[derive(Debug, Snafu)]
enum Error {
    #[snafu(display("No output device available on host \"{:?}\"", host))]
    NoOutputDevice { host: cpal::HostId },

    #[snafu(display("Failed to create master audio player: {}", source))]
    MasterCreation { source: playback::CreateError },

    #[snafu(display("Failed to create main window: {}", source))]
    ContextCreation { source: glutin::CreationError },

    #[snafu(display("Falied to make GL context current: {}", source))]
    ContextCurrent { source: glutin::ContextError },

    #[snafu(display("Failed to set up blend2d rendering: {}", source))]
    RenderInitialize { source: blend2d::error::Error },

    #[snafu(display("Failed to render oscilloscope: {}", source))]
    Render { source: blend2d::error::Error },
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

    let config = config::Config::load();
    let mut state = load_state(state_file);

    let audio_host = audio_host(&config);
    let audio_dev = audio_device(&config, &audio_host)?;

    let mut event_loop = glutin::EventsLoop::new();

    let window_size = (1920, 1080);
    let hdpi_fac = event_loop.get_primary_monitor().get_hidpi_factor();

    let window_builder = glutin::WindowBuilder::new()
        .with_title("rawrscope")
        .with_dimensions(glutin::dpi::LogicalSize::from_physical(
            window_size,
            hdpi_fac,
        ))
        .with_resizable(false);
    let context = glutin::ContextBuilder::new()
        .with_vsync(true)
        .with_gl(glutin::GlRequest::Latest)
        .with_gl_profile(glutin::GlProfile::Core)
        .build_windowed(window_builder, &event_loop)
        .context(ContextCreation)?;

    let context = unsafe { context.make_current() }
        .map_err(|e| e.1)
        .context(ContextCurrent)?;
    gl::load_with(|name| context.get_proc_address(name) as *const _);

    let mut image = blend2d::image::Image::new(
        window_size.0 as i32,
        window_size.1 as i32,
        blend2d::image::ImageFormat::PRgb32,
    )
    .context(RenderInitialize)?;
    let mut blend_ctx = blend2d::context::Context::new(&mut image).context(RenderInitialize)?;

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

    let framerate = 60u16;
    let frame_secs = 1.0 / f32::from(framerate);

    let window_ms = 50;

    let mut loaded_sources = state
        .audio_sources
        .iter_mut()
        .filter_map(|s| s.as_loaded())
        .collect::<Vec<_>>();

    let sub_builder = master.submission_builder();
    let mut frame = 0;
    let mut running = true;
    while running {
        event_loop.poll_events(|event| {
            if let Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } = event
            {
                log::info!("Goodbye!");
                running = false;
            }
        });

        let time = std::time::Instant::now();

        blend_ctx.clear_all().context(Render)?;

        // draw midlines
        blend_ctx.set_stroke_width(2.0);
        blend_ctx.set_stroke_style_rgba32(0x28_28_28_FF);
        for i in 0..loaded_sources.len() {
            let h = window_size.1 / loaded_sources.len() as u32;
            let y = h * i as u32 + h / 2;

            // TODO reduce numerical casts
            blend_ctx
                .stroke_line(0.0, f64::from(y), f64::from(window_size.0), f64::from(y))
                .context(Render)?;
        }

        if loaded_sources
            .iter()
            .any(|source| frame < source.len() / (source.spec().sample_rate / u32::from(framerate)))
        {
            let mut sub = sub_builder.create(frame_secs);

            blend_ctx.set_stroke_width(2.5);
            blend_ctx.set_stroke_style_rgba32(0xFF_FF_FF_FF);

            let n_sources = loaded_sources.len();
            for (i, source) in loaded_sources.iter_mut().enumerate() {
                let channels = source.spec().channels;
                let sample_rate = source.spec().sample_rate;

                let window_len = sample_rate * window_ms / 1000 * u32::from(channels);
                let window_pos = (sample_rate / u32::from(framerate)) * frame;

                // TODO dont panic
                let window = source
                    .chunk_at(window_pos, window_len as usize)
                    .unwrap()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();

                let h = window_size.1 / n_sources as u32;
                let y = h * i as u32 + h / 2;

                let mut path = blend2d::path::Path::with_capacity(window.len());
                path.move_to(0.0, f64::from(y) - f64::from(window[0]));
                for (j, v) in window[1..].iter().enumerate() {
                    path.line_to(
                        j as f64 / window.len() as f64 * f64::from(window_size.0),
                        f64::from(y) - f64::from(*v),
                    );
                }

                blend_ctx.stroke_path(&path).context(Render)?;

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
                            sub.add(sample_rate, conn.target_channel as usize, channel_iter);
                        }
                        _ => unimplemented!(),
                    }
                }
            }

            if let Err(e) = master.submit(sub) {
                log::error!("Failed to submit audio to master: {}", e);
            }
        }

        dbg!(time.elapsed());

        context.swap_buffers().unwrap();

        frame += 1;
    }

    Ok(())
}

pub fn run(state_file: Option<&str>) {
    if let Err(e) = _run(state_file) {
        log::error!("{}", e)
    }
}
