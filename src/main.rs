use std::io;

mod audio_source;
mod state;

fn main() {
    match state::State::from_file("test.rprj") {
        Ok(mut state) => {
            println!("loaded state");
            match state.load_audio_sources() {
                Ok(_) => {
                    println!("loaded audio sources");

                    for source in state.audio_sources {
                        println!("{}", source.path.display());

                        let as_loaded = source.as_loaded().unwrap();

                        let channels = as_loaded.spec().channels;
                        let sample_rate = as_loaded.spec().sample_rate;
                        let len = as_loaded.len();

                        let time_secs = (len / u32::from(channels)) as f32 / sample_rate as f32;
                        println!("length: {:.2}s", time_secs);
                    }
                }
                Err(e) => println!("failed to load audio sources: {}", e),
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
