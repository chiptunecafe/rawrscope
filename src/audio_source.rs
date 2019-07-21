use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sample::{types::I24, Sample};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum LoadError {
    #[snafu(display("Failed to load audio file from {}: {}", path.display(), source))]
    OpenError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to create WAV reader for {}: {}", path.display(), source))]
    WavError { path: PathBuf, source: hound::Error },
}

#[derive(Debug, Snafu)]
pub enum ReadError {
    #[snafu(display("Could not seek to position {} in audio file: {}", pos, source))]
    SeekError { pos: u32, source: io::Error },

    #[snafu(display("Failed to read WAV file: {}", source))]
    DecodeError { source: hound::Error },

    #[snafu(display("Unsupported sample bit depth: {}", depth))]
    UnsupportedDepth { depth: u16 },
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

    pub fn as_loaded(&mut self) -> Option<AsLoaded> {
        if let Some(wav_reader) = self.wav_reader.as_mut() {
            Some(AsLoaded {
                path: self.path.as_path(),
                fade_in: self.fade_in,
                fade_out: self.fade_out,
                gain: self.gain,
                wav_reader,
            })
        } else {
            None
        }
    }
}

pub struct AsLoaded<'a> {
    path: &'a Path,
    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,
    pub gain: f32,
    wav_reader: &'a mut hound::WavReader<fs::File>,
}

impl<'a> AsLoaded<'a> {
    pub fn path(&self) -> &Path {
        self.path
    }

    pub fn spec(&self) -> hound::WavSpec {
        self.wav_reader.spec()
    }

    pub fn len(&self) -> u32 {
        self.wav_reader.len()
    }

    pub fn chunk_at(&mut self, pos: u32, len: usize) -> Result<Vec<f64>, ReadError> {
        self.wav_reader.seek(pos).context(SeekError { pos })?;
        self.next_chunk(len)
    }

    // cursed
    pub fn next_chunk(&mut self, len: usize) -> Result<Vec<f64>, ReadError> {
        match self.spec().sample_format {
            hound::SampleFormat::Int => match self.spec().bits_per_sample {
                8 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .map(|v| v.context(DecodeError).map(i8::to_sample))
                        .collect()
                }
                16 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .map(|v| v.context(DecodeError).map(i16::to_sample))
                        .collect()
                }
                24 => {
                    let samples = self.wav_reader.samples::<i32>();
                    samples
                        .take(len)
                        .map(|v| {
                            v.context(DecodeError)
                                .map(I24::new_unchecked)
                                .map(I24::to_sample)
                        })
                        .collect()
                }
                v => Err(ReadError::UnsupportedDepth { depth: v }),
            },
            hound::SampleFormat::Float => match self.spec().bits_per_sample {
                32 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .map(|v| v.context(DecodeError).map(f32::to_sample))
                        .collect()
                }
                v => Err(ReadError::UnsupportedDepth { depth: v }),
            },
        }
    }
}
