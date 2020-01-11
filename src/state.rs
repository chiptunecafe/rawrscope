use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
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
    ParseError { source: serde_yaml::Error },
}

#[derive(Debug, Snafu)]
pub enum WriteError {
    #[snafu(display("Failed to open project from {} for writing: {}", path.display(), source))]
    CreateError { path: PathBuf, source: io::Error },

    #[snafu(display("Failed to write project: {}", source))]
    IoError { source: io::Error },

    #[snafu(display("Failed to serialize project: {}", source))]
    SerializeError { source: serde_yaml::Error },
}

#[derive(Derivative, Deserialize, Serialize)]
#[derivative(Default)]
pub struct GlobalAppearance {
    #[derivative(Default(value = "60"))]
    pub framerate: u32,
    #[derivative(Default(value = "1"))]
    pub grid_rows: u32,
    #[derivative(Default(value = "1"))]
    pub grid_columns: u32,
}

// TODO maybe move some of this stuff into a separate module
#[derive(Deserialize, Serialize)]
pub struct GridRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct PlaybackState {
    pub frame: u32,
    #[derivative(Default(value = "true"))]
    pub playing: bool,
}

#[derive(Derivative)]
#[derivative(Default)]
pub struct DebugState {
    pub stutter_test: bool,
    #[derivative(Default(value = "true"))]
    pub multithreaded_centering: bool,
    pub frametimes: VecDeque<f32>,
}

#[derive(Default, Deserialize, Serialize)]
pub struct State {
    pub audio_sources: Vec<audio::source::AudioSource>,
    pub scopes: HashMap<String, scope::Scope>,
    pub appearance: GlobalAppearance,

    #[serde(skip)]
    pub file_path: PathBuf,

    #[serde(skip)]
    pub playback: PlaybackState,

    #[serde(skip)]
    pub debug: DebugState,
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

        let mut state: State = serde_yaml::from_reader(file).context(ParseError)?;
        state.file_path = path.to_path_buf();

        // load audio sources
        warnings.extend(
            state
                .audio_sources
                .iter_mut()
                .filter_map(|s| s.load().err().map(Box::new))
                .map(|b| b as Box<dyn std::error::Error>),
        );

        // initialize scope mixers
        for (scope_name, scope) in state.scopes.iter_mut() {
            let sample_rates = state
                .audio_sources
                .iter_mut()
                .filter(|source| {
                    source.connections.iter().any(|conn| match &conn.target {
                        audio::connection::ConnectionTarget::Scope { name, .. } => {
                            name == scope_name
                        }
                        _ => false,
                    })
                })
                .filter_map(|source| source.as_loaded())
                .map(|loaded| loaded.spec().sample_rate)
                .collect::<Vec<_>>();

            scope.configure_mixer(sample_rates);
        }

        Ok((state, warnings))
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> Result<(), WriteError> {
        let path = path.as_ref();
        let file = fs::File::create(path).context(CreateError {
            path: path.to_path_buf(),
        })?;

        serde_yaml::to_writer(file, self).context(SerializeError)?;

        log::info!("Saved project! ({})", path.display());

        Ok(())
    }
}
