use std::io;

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

    for mut source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
        println!("{}", source.path().display());

        let channels = source.spec().channels;
        let sample_rate = source.spec().sample_rate;
        let len = source.len();

        let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
        println!("length: {:.2}s", time_secs);

        let time = std::time::Instant::now();
        match source.next_chunk(10000) {
            Ok(chunk) => println!("10000th sample: {}", chunk[9999]),
            Err(e) => println!("could not read first 10000 samples: {}", e),
        }
        println!("(read in {:?})", time.elapsed());
    }
}
