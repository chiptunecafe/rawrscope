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
    /*
    let audio_dev = match audio_host.default_output_device() {
        Some(d) => d,
        None => {
            log::error!("No output device available!");
            return;
        }
    };
    */
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

    let master = match playback::Player::new(audio_host, audio_dev) {
        Ok(p) => p,
        Err(e) => {
            log::error!("{}", e);
            return;
        }
    };

    for source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        let channels = source.spec().channels;
        let sample_rate = source.spec().sample_rate;
        let len = source.len();

        let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
        println!("{}: {:.2}s", source.path().display(), time_secs);
    }

    let framerate = 60;
    let frame_len = std::time::Duration::from_micros(1_000_000 / u64::from(framerate));
    let mut loaded_sources = state
        .audio_sources
        .iter_mut()
        .filter_map(|s| s.as_loaded())
        .collect::<Vec<_>>();
    loop {
        let st = std::time::Instant::now();

        let mut submissions = Vec::new();
        for _ in 0..master.channels() {
            submissions.push(mixer::Submission::new());
        }

        for source in &mut loaded_sources {
            let channels = source.spec().channels;
            let sample_rate = source.spec().sample_rate;

            let chunk_size = (sample_rate * u32::from(channels)) / framerate;
            // TODO dont panic
            let chunk = source
                .next_chunk(chunk_size as usize)
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
                        match submissions.get_mut(conn.target_channel as usize) {
                            // TODO maybe dont clone
                            Some(sub) => sub.add(sample_rate, channel_iter),
                            None => log::warn!(
                                "Invalid connection to master channel {}",
                                conn.target_channel
                            ),
                        }
                    }
                    _ => unimplemented!(),
                }
            }
        }

        for (i, sub) in submissions.drain(..).enumerate() {
            if let Err(e) = master.submit(i, sub) {
                log::error!("Failed to submit to master channel {}: {}", i, e);
            }
        }

        let elapsed = st.elapsed();
        std::thread::sleep((frame_len - elapsed) / 2);
    }
}
