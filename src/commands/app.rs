use std::io;

use crate::audio::mixer;
use crate::state::{self, State};

pub fn run(state_file: Option<&str>) {
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

    let (mut master_mix, master_queue) = mixer::Mixer::new(Some(44100));

    let framerate = 60;

    let mut submission = mixer::Submission::new();
    let time = std::time::Instant::now();
    for mut source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        let channels = source.spec().channels;
        let sample_rate = source.spec().sample_rate;
        let len = source.len();

        let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
        println!("{}: {:.2}s", source.path().display(), time_secs);

        let chunk_size = (sample_rate * u32::from(channels)) / framerate;
        // TODO dont panic
        let chunk = source
            .next_chunk(chunk_size as usize)
            .unwrap()
            .iter()
            .step_by(channels as usize)
            .copied()
            .collect();
        submission.add(sample_rate, chunk);
    }

    if let Err(e) = master_queue.send(submission) {
        log::error!("Failed to submit 16ms of audio: {}", e);
    }

    log::debug!("Submitted 16ms of audio in {:?}", time.elapsed());

    let time = std::time::Instant::now();
    for _ in 0..44100 / framerate {
        use sample::Signal;
        master_mix.next();
    }
    log::debug!("Mixed 16ms of audio in {:?}", time.elapsed());
}
