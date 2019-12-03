use std::fs;
use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use snafu::{OptionExt, ResultExt, Snafu};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Audio {
    pub host: Option<String>,
    pub device: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub audio: Audio,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not find a home directory!"))]
    HomeDirectory,

    #[snafu(display("Failed to create config file: {}", source))]
    CreateError { source: io::Error },

    #[snafu(display("Failed to create config directory: {}", source))]
    CreateDirectory { source: io::Error },

    #[snafu(display("Failed to serialize config: {}", source))]
    SerializeError { source: toml::ser::Error },

    #[snafu(display("Failed to write to config file: {}", source))]
    WriteError { source: io::Error },
}

impl Config {
    fn config_dir() -> Result<directories::ProjectDirs, Error> {
        directories::ProjectDirs::from("", "rytone", "rawrscope").context(HomeDirectory)
    }

    pub fn load() -> Self {
        let mut path = match Config::config_dir() {
            Ok(p) => p,
            Err(_) => {
                log::warn!("No suitable home directory found! Using default config...");
                return Default::default();
            }
        }
        .config_dir()
        .to_path_buf();
        path.push("rawrscope.toml");

        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                log::debug!("No config found... using default");
                return Default::default();
            }
            Err(e) => {
                log::warn!("Failed to load config: {} ... using default", e);
                return Default::default();
            }
        };

        let mut buffer = Vec::new();
        if let Err(e) = file.read_to_end(&mut buffer) {
            log::warn!("Failed to read config: {} ... using default", e);
            return Default::default();
        }

        match toml::from_slice(&buffer) {
            Ok(config) => config,
            Err(e) => {
                log::warn!("Failed to parse config: {} ... using default", e);
                Default::default()
            }
        }
    }

    pub fn write(&self) -> Result<(), Error> {
        let dir = Config::config_dir()?;

        fs::DirBuilder::new()
            .recursive(true)
            .create(dir.config_dir())
            .context(CreateDirectory)?;

        let mut path = dir.config_dir().to_path_buf();
        path.push("rawrscope.toml");

        let mut file = fs::File::create(path).context(CreateError)?;
        let serialized = toml::to_string_pretty(self).context(SerializeError)?;
        file.write_all(serialized.as_ref()).context(WriteError)
    }
}
