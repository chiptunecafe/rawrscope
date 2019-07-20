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

    pub fn is_loaded(&self) -> bool {
        self.wav_reader.is_some()
    }

    pub fn as_loaded(&self) -> Option<AsLoaded> {
        self.wav_reader.as_ref().map(|wav| AsLoaded {
            path: &self.path,

            fade_in: self.fade_in,
            fade_out: self.fade_out,
            gain: self.gain,

            wav_reader: wav,
        })
    }
}

pub struct AsLoaded<'a> {
    path: &'a PathBuf,

    fade_in: Option<f32>,
    fade_out: Option<f32>,
    gain: f32,

    wav_reader: &'a hound::WavReader<fs::File>,
}

impl<'a> AsLoaded<'a> {
    pub fn path(&self) -> &PathBuf {
        self.path
    }

    pub fn spec(&self) -> hound::WavSpec {
        self.wav_reader.spec()
    }

    pub fn len(&self) -> u32 {
        self.wav_reader.len()
    }
}
