use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use sample::{types::I24, Sample};
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use crate::audio;

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
    pub connections: Vec<audio::connection::Connection>,

    #[serde(skip)]
    pub wav_reader: Option<hound::WavReader<io::BufReader<fs::File>>>,
    #[serde(skip)]
    reader_position: u32,
}

impl AudioSource {
    pub fn load(&mut self) -> Result<(), LoadError> {
        let file = fs::File::open(&self.path).context(OpenError {
            path: self.path.clone(),
        })?;

        let wav_reader = hound::WavReader::new(io::BufReader::new(file)).context(WavError {
            path: self.path.clone(),
        })?;

        self.wav_reader = Some(wav_reader);

        Ok(())
    }

    pub fn unload(&mut self) {
        self.wav_reader = None;
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
                connections: self.connections.as_slice(),
                wav_reader,
                reader_position: &mut self.reader_position,
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
    pub connections: &'a [audio::connection::Connection],
    wav_reader: &'a mut hound::WavReader<io::BufReader<fs::File>>,
    reader_position: &'a mut u32,
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

    pub fn chunk_at(&mut self, pos: u32, len: usize) -> Result<Vec<f32>, ReadError> {
        self.wav_reader.seek(pos).context(SeekError { pos })?;
        *self.reader_position = pos;
        self.next_chunk(len)
    }

    fn fade(
        spec: hound::WavSpec,
        reader_pos: u32,
        len: u32,
        in_len: Option<f32>,
        out_len: Option<f32>,
    ) -> impl Fn((usize, Result<f32, hound::Error>)) -> Result<f32, hound::Error> {
        move |(idx, samp)| {
            let len = len * u32::from(spec.channels);
            let idx = (idx + reader_pos as usize) / spec.channels as usize * spec.channels as usize;
            let mut s = samp?;

            let in_samps = in_len.map(|v| (v * spec.sample_rate as f32) as usize);
            let out_samps = out_len.map(|v| (v * spec.sample_rate as f32) as usize);

            match in_samps {
                Some(l) if idx < l => s *= idx as f32 / l as f32,
                _ => (),
            }

            match out_samps {
                Some(l) if (len as usize - idx) < l => s *= (len as usize - idx) as f32 / l as f32,
                _ => (),
            }

            Ok(s)
        }
    }

    // cursed
    pub fn next_chunk(&mut self, len: usize) -> Result<Vec<f32>, ReadError> {
        let spec = self.spec();
        let total_len = self.len();

        let chunk = match self.spec().sample_format {
            hound::SampleFormat::Int => match self.spec().bits_per_sample {
                8 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .map(|v| v.map(i8::to_sample))
                        .enumerate()
                        .map(Self::fade(
                            spec,
                            *self.reader_position,
                            total_len,
                            self.fade_in,
                            self.fade_out,
                        ))
                        .collect::<Result<Vec<f32>, hound::Error>>()
                        .context(DecodeError)
                }
                16 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .map(|v| v.map(i16::to_sample))
                        .enumerate()
                        .map(Self::fade(
                            spec,
                            *self.reader_position,
                            total_len,
                            self.fade_in,
                            self.fade_out,
                        ))
                        .collect::<Result<Vec<f32>, hound::Error>>()
                        .context(DecodeError)
                }
                24 => {
                    let samples = self.wav_reader.samples::<i32>();
                    samples
                        .take(len)
                        .map(|v| v.map(I24::new_unchecked).map(I24::to_sample))
                        .enumerate()
                        .map(Self::fade(
                            spec,
                            *self.reader_position,
                            total_len,
                            self.fade_in,
                            self.fade_out,
                        ))
                        .collect::<Result<Vec<f32>, hound::Error>>()
                        .context(DecodeError)
                }
                v => Err(ReadError::UnsupportedDepth { depth: v }),
            },
            hound::SampleFormat::Float => match self.spec().bits_per_sample {
                32 => {
                    let samples = self.wav_reader.samples();
                    samples
                        .take(len)
                        .enumerate()
                        .map(Self::fade(
                            spec,
                            *self.reader_position,
                            total_len,
                            self.fade_in,
                            self.fade_out,
                        ))
                        .collect::<Result<Vec<f32>, hound::Error>>()
                        .context(DecodeError)
                }
                v => Err(ReadError::UnsupportedDepth { depth: v }),
            },
        };

        *self.reader_position += len as u32;

        chunk
    }
}
