use std::io;

mod audio_source;
mod state;

fn main() {
    match state::State::from_file("test.rprj") {
        Ok((mut state, warnings)) => {
            println!("loaded state");

            if warnings.is_empty() {
                println!("no warnings");
            } else {
                println!("warnings:");
                for w in &warnings {
                    println!("{}", w);
                }
            }

            for mut source in state.audio_sources.iter_mut().filter_map(|s| s.as_loaded()) {
                println!("{}", source.path().display());

                let channels = source.spec().channels;
                let sample_rate = source.spec().sample_rate;
                let len = source.len();

                let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
                println!("length: {:.2}s", time_secs);

                match source.next_chunk(5000) {
                    Ok(chunk) => println!("5000th sample: {}", chunk[4999]),
                    Err(e) => println!("could not read first 5000 samples: {}", e),
                }
            }
        }
        Err(state::ReadError::OpenError { ref source, .. })
            if source.kind() == io::ErrorKind::NotFound =>
        {
            println!("project not found, writing default...");
            let state = state::State::default();
            if let Err(e) = state.write("test.rprj") {
                println!("{}", e);
            }
        }
        Err(e) => println!("{}", e),
    }
}
