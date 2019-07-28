use std::fs;
use std::io::{self, Read, Write};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Audio {
    pub host: Option<String>,
    pub device: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Config {
    pub audio: Audio,
}

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Failed to create config file: {}", source))]
    CreateError { source: io::Error },

    #[snafu(display("Failed to serialize config: {}", source))]
    SerializeError { source: toml::ser::Error },

    #[snafu(display("Failed to write to config file: {}", source))]
    WriteError { source: io::Error },
}

// TODO do not store config in working dir
impl Config {
    pub fn load() -> Self {
        let mut file = match fs::File::open("rawrscope.toml") {
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
        let mut file = fs::File::create("rawrscope.toml").context(CreateError)?;
        let serialized = toml::to_string_pretty(self).context(SerializeError)?;
        file.write_all(serialized.as_ref()).context(WriteError)
    }
}
