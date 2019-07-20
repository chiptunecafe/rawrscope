use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to open project from {}: {}", path.display(), source))]
    OpenError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to create project {}: {}", path.display(), source))]
    CreateError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to parse project: {}", source))]
    ParseError { source: serde_yaml::Error },

    #[snafu(display("Failed to serialize project: {}", source))]
    SerializeError { source: serde_yaml::Error },
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Deserialize, Serialize)]
pub struct AudioSource {
    pub path: PathBuf,

    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,

    pub gain: f32,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct State {
    pub audio_sources: HashMap<String, AudioSource>,
}

impl State {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = fs::File::open(path).context(OpenError {
            path: path.to_path_buf(),
        })?;

        serde_yaml::from_reader(file).context(ParseError)
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let file = fs::File::create(path).context(CreateError {
            path: path.to_path_buf(),
        })?;

        serde_yaml::to_writer(&file, self).context(SerializeError)
    }
}
