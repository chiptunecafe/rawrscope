use std::collections::HashMap;
use std::io;
use std::panic::{set_hook, take_hook};

use cpal::traits::{DeviceTrait, HostTrait};
use glium::{
    glutin::{self, Event, WindowEvent},
    program, uniform, Surface,
};
use nalgebra as na;
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
    ContextCreation {
        source: glium::backend::glutin::DisplayCreationError,
    },

    #[snafu(display("Failed to swap buffers: {}", source))]
    SwapBuffers { source: glium::SwapBuffersError },

    #[snafu(display("Failed to update scope texture: {}", source))]
    Texture {
        source: glium::texture::TextureCreationError,
    },

    #[snafu(display("Failed to create vertex buffer: {}", source))]
    VertexBuffer {
        source: glium::vertex::BufferCreationError,
    },

    #[snafu(display("Failed to create vertex buffer: {}", source))]
    IndexBuffer {
        source: glium::index::BufferCreationError,
    },

    #[snafu(display("Failed to compile shaders: {}", source))]
    ShaderCompilation {
        source: glium::program::ProgramChooserCreationError,
    },

    #[snafu(display("Failed to display scope: {}", source))]
    GlRender { source: glium::DrawError },
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
    let context_builder = glutin::ContextBuilder::new()
        .with_vsync(true)
        .with_gl(glutin::GlRequest::Latest)
        .with_gl_profile(glutin::GlProfile::Core);

    let display = glium::Display::new(window_builder, context_builder, &event_loop)
        .context(ContextCreation)?;

    #[derive(Copy, Clone)]
    struct LineVertex {
        position: [f32; 2],
    };
    glium::implement_vertex!(LineVertex, position);

    let mut line_buffers: HashMap<usize, glium::VertexBuffer<LineVertex>> = HashMap::new();

    let shader_prog = program!(&display, 330 => {
        vertex: r#"
#version 330

in vec2 position;
uniform mat3 transform;

void main() {
    gl_Position = vec4(transform * vec3(position, 1.0), 1.0);
}
        "#,
        geometry: r#"
#version 330

layout(lines) in;
layout(triangle_strip, max_vertices = 4) out;

uniform vec2 resolution;
uniform float thickness;

void main() {
    // (half thickness?)
    vec2 thickness_norm = vec2(thickness) / resolution;

    vec2 a = gl_in[0].gl_Position.xy;
    vec2 b = gl_in[1].gl_Position.xy;

    vec2 m = normalize(b - a);
    vec2 n = vec2(-m.y, m.x);

    // extend endcaps
    a -= thickness_norm * m;
    b += thickness_norm * m;

    // write quad verts
    gl_Position = vec4(a + n * thickness_norm, 0.0, 1.0);
    EmitVertex();

    gl_Position = vec4(a - n * thickness_norm, 0.0, 1.0);
    EmitVertex();

    gl_Position = vec4(b - n * thickness_norm, 0.0, 1.0);
    EmitVertex();

    gl_Position = vec4(b + n * thickness_norm, 0.0, 1.0);
    EmitVertex();
}
        "#,
        fragment: r#"
#version 330

out vec4 f_color;

void main() {
    f_color = vec4(1);
}
        "#,
    })
    .context(ShaderCompilation)?;

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

        let mut target = display.draw();
        target.clear_color(0.0, 0.0, 0.0, 1.0);

        if loaded_sources
            .iter()
            .any(|source| frame < source.len() / (source.spec().sample_rate / u32::from(framerate)))
        {
            let mut sub = sub_builder.create(frame_secs);

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

                let buffer = line_buffers.entry(i).or_insert_with(|| {
                    log::debug!("Creating line buffer #{}", i);
                    glium::VertexBuffer::dynamic(
                        &display,
                        window
                            .iter()
                            .enumerate()
                            .map(|(i, v)| LineVertex {
                                position: [i as f32, *v],
                            })
                            .collect::<Vec<_>>()
                            .as_slice(),
                    )
                    .unwrap() // TODO remove somehow
                });

                if buffer.len() != window.len() {
                    log::debug!("Resizing line buffer #{}", i);
                    *buffer = glium::VertexBuffer::dynamic(
                        &display,
                        window
                            .iter()
                            .enumerate()
                            .map(|(i, v)| LineVertex {
                                position: [i as f32, *v],
                            })
                            .collect::<Vec<_>>()
                            .as_slice(),
                    )
                    .context(VertexBuffer)?;
                } else {
                    buffer.write(
                        window
                            .iter()
                            .enumerate()
                            .map(|(i, v)| LineVertex {
                                position: [i as f32, *v],
                            })
                            .collect::<Vec<_>>()
                            .as_slice(),
                    );
                }

                let y_shift = (i as f32 + 0.5) / (n_sources as f32) * 2.0 - 1.0;

                let transform: na::Matrix3<f32> = na::Matrix3::new_nonuniform_scaling(
                    &na::Vector2::new(1.0 / window_len as f32 * 2.0, 1.0 / n_sources as f32 * 2.0),
                )
                .append_translation(&na::Vector2::new(-1.0, -y_shift));

                target
                    .draw(
                        &*buffer,
                        glium::index::NoIndices(glium::index::PrimitiveType::LineStrip),
                        &shader_prog,
                        &uniform! {
                            transform: <_ as Into<[[f32; 3]; 3]>>::into(transform),
                            resolution: [window_size.0 as f32, window_size.1 as f32],
                            thickness: 2.5f32,
                        },
                        &Default::default(),
                    )
                    .context(GlRender)?;

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

        target.finish().context(SwapBuffers)?;
        frame += 1;
    }

    Ok(())
}

pub fn run(state_file: Option<&str>) {
    if let Err(e) = _run(state_file) {
        log::error!("{}", e)
    }
}
