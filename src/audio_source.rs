use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum LoadError {
    #[snafu(display("Failed to load audio file from {}: {}", path.display(), source))]
    OpenError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to create WAV reader for {}: {}", path.display(), source))]
    WavError { path: PathBuf, source: hound::Error },
}

#[derive(Deserialize, Serialize)]
pub struct AudioSource {
    pub path: PathBuf,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub gain: f32,

    #[serde(skip)]
    pub wav_reader: Option<hound::WavReader<fs::File>>,
}

impl AudioSource {
    pub fn load(&mut self) -> Result<(), LoadError> {
        let file = fs::File::open(&self.path).context(OpenError {
            path: self.path.clone(),
        })?;

        let wav_reader = hound::WavReader::new(file).context(WavError {
            path: self.path.clone(),
        })?;

        self.wav_reader = Some(wav_reader);

        Ok(())
    }

    pub fn channels(&self) -> Option<u16> {
        self.wav_reader.as_ref().map(|r| r.spec().channels)
    }

    pub fn sample_rate(&self) -> Option<u32> {
        self.wav_reader.as_ref().map(|r| r.spec().sample_rate)
    }

    pub fn num_samples(&self) -> Option<u32> {
        self.wav_reader.as_ref().map(|r| r.len())
    }
}
