use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use derivative::Derivative;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

use crate::audio;
use crate::scope;

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

#[derive(Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub struct GlobalAppearance {
    #[derivative(Default(value = "1"))]
    pub grid_rows: u32,
    #[derivative(Default(value = "1"))]
    pub grid_columns: u32,
}

// TODO maybe move some of this stuff into a separate module
#[derive(Deserialize, Serialize)]
pub struct GridRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

#[derive(Default, Deserialize, Serialize)]
pub struct State {
    pub audio_sources: Vec<audio::source::AudioSource>,
    pub scopes: HashMap<String, scope::Scope>,
    pub appearance: GlobalAppearance,
}

impl State {
    pub fn from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<(Self, Vec<Box<dyn std::error::Error>>), ReadError> {
        let mut warnings = Vec::<Box<dyn std::error::Error>>::new();

        let path = path.as_ref();
        let file = fs::File::open(path).context(OpenError {
            path: path.to_path_buf(),
        })?;

        let mut state: State = ron::de::from_reader(file).context(ParseError)?;

        warnings.extend(
            state
                .audio_sources
                .iter_mut()
                .filter_map(|s| s.load().err().map(Box::new))
                .map(|b| b as Box<dyn std::error::Error>),
        );

        Ok((state, warnings))
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
