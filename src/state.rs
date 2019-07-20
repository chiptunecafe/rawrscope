use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum ReadError {
    #[snafu(display("Failed to open project from {}: {}", path.display(), source))]
    OpenError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to parse project: {}", source))]
    ParseError { source: ron::de::Error },
}

#[derive(Debug, Snafu)]
pub enum WriteError {
    #[snafu(display("Failed to open project from {} for writing: {}", path.display(), source))]
    CreateError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to write project: {}", source))]
    IoError { source: io::Error },

    #[snafu(display("Failed to serialize project: {}", source))]
    SerializeError { source: ron::ser::Error },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AudioSource {
    pub path: PathBuf,

    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,

    pub gain: f32,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct State {
    pub audio_sources: Vec<AudioSource>,
}

impl State {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ReadError> {
        let path = path.as_ref();
        let file = fs::File::open(path).context(OpenError {
            path: path.to_path_buf(),
        })?;

        ron::de::from_reader(file).context(ParseError)
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), WriteError> {
        let path = path.as_ref();
        let mut file = fs::File::create(path).context(CreateError {
            path: path.to_path_buf(),
        })?;

        let serialized =
            ron::ser::to_string_pretty(self, Default::default()).context(SerializeError)?;

        file.write_all(serialized.as_ref()).context(IoError)
    }
}
