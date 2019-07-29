use std::io;
use std::panic::{set_hook, take_hook};

use cpal::traits::{DeviceTrait, HostTrait};

use crate::audio::{connection::ConnectionTarget, mixer, playback};
use crate::config;
use crate::panic;
use crate::state::{self, State};

pub fn run(state_file: Option<&str>) {
    set_hook(panic::dialog(take_hook()));

    let config = config::Config::load();

    let mut state = match state_file {
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
                    log::error!("Failed to write new project: {}", e);
                }
                log::debug!("Created new project at {}", path);
                state
            }
            Err(e) => {
                log::error!("Failed to load project: {}", e);
                State::default()
            }
        },
        None => State::default(),
    };

    // TODO USE HOST FROM CONFIG FILE!!!!!
    let audio_host = cpal::default_host();
    let audio_dev = match config.audio.device {
        Some(dev_name) => match audio_host.output_devices() {
            Ok(mut iter) => match iter.find(|dev| {
                dev.name()
                    .ok()
                    .map(|name| name == dev_name)
                    .unwrap_or(false)
            }) {
                Some(d) => d,
                None => {
                    log::error!("Output device \"{}\" does not exist!", dev_name);
                    return;
                }
            },
            Err(e) => {
                log::error!("Failed to query output devices: {}", e);
                return;
            }
        },
        None => match audio_host.default_output_device() {
            Some(d) => d,
            None => {
                log::error!("No output device available!");
                return;
            }
        },
    };

    let mut master = match playback::Player::new(audio_host, audio_dev) {
        Ok(p) => p,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };

    let mut mixer_config = mixer::MixerBuilder::new();
    mixer_config.channels(master.channels() as usize);
    mixer_config.target_sample_rate(master.sample_rate());

    for source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        let channels = source.spec().channels;
        let sample_rate = source.spec().sample_rate;
        let len = source.len();

        mixer_config.source_rate(sample_rate);

        let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
        println!("{}: {:.2}s", source.path().display(), time_secs);
    }

    if let Err(e) = master.rebuild_mixer(mixer_config) {
        log::warn!("Failed to rebuild master mixer: {}", e);
    }

    let sub_builder = master.submission_builder();

    let framerate = 60u16;
    let frame_secs = 1.0 / f32::from(framerate);

    let frame_len = std::time::Duration::from_micros(1_000_000 / u64::from(framerate));
    let mut loaded_sources = state
        .audio_sources
        .iter_mut()
        .filter_map(|s| s.as_loaded())
        .collect::<Vec<_>>();
    loop {
        let st = std::time::Instant::now();

        let mut sub = sub_builder.create(frame_secs);

        for source in &mut loaded_sources {
            let channels = source.spec().channels;
            let sample_rate = source.spec().sample_rate;

            let chunk_len = sub
                .length_of_channel(sample_rate)
                .expect("submission missing sample rate!")
                * channels as usize;

            // TODO dont panic
            let chunk = source
                .next_chunk(chunk_len)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>();

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

        let elapsed = st.elapsed();
        std::thread::sleep((frame_len - elapsed) / 2);
    }
}
