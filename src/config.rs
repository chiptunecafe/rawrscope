use std::fs;
use std::io::{self, Read, Write};

use derivative::Derivative;
use serde::{Deserialize, Serialize};
use snafu::{OptionExt, ResultExt, Snafu};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoBackend {
    Primary,
    Secondary,
    Vulkan,
    Gl,
    Metal,
    Dx12,
    Dx11,
}

impl VideoBackend {
    pub fn to_wgpu_backend(&self) -> wgpu::BackendBit {
        match self {
            VideoBackend::Primary => wgpu::BackendBit::PRIMARY,
            VideoBackend::Secondary => wgpu::BackendBit::SECONDARY,
            VideoBackend::Vulkan => wgpu::BackendBit::VULKAN,
            VideoBackend::Gl => wgpu::BackendBit::GL,
            VideoBackend::Metal => wgpu::BackendBit::METAL,
            VideoBackend::Dx12 => wgpu::BackendBit::DX12,
            VideoBackend::Dx11 => wgpu::BackendBit::DX11,
        }
    }
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default)]
pub struct Audio {
    pub host: Option<String>,
    pub device: Option<String>,
    #[derivative(Default(value = "10.0"))]
    pub buffer_ms: f32,
}

#[derive(Clone, Debug, Derivative, Deserialize, Serialize)]
#[derivative(Default)]
#[serde(default)]
pub struct Video {
    #[derivative(Default(value = "VideoBackend::Primary"))]
    pub backend: VideoBackend,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub audio: Audio,
    pub video: Video,
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
    fn config_dir() -> Result<directories_next::ProjectDirs, Error> {
        directories_next::ProjectDirs::from("", "rytone", "rawrscope").context(HomeDirectory)
    }

    pub fn load() -> Self {
        let sp = tracing::debug_span!("load_config");
        let _e = sp.enter();

        let mut path = match Config::config_dir() {
            Ok(p) => p,
            Err(_) => {
                tracing::warn!("No suitable home directory found! Using default config...");
                return Default::default();
            }
        }
        .config_dir()
        .to_path_buf();
        path.push("rawrscope.toml");

        let mut file = match fs::File::open(path) {
            Ok(f) => f,
            Err(ref e) if e.kind() == io::ErrorKind::NotFound => {
                tracing::debug!("No config found... using default");
                return Default::default();
            }
            Err(e) => {
                tracing::warn!(err = %e, "Failed to load config... using default");
                return Default::default();
            }
        };

        let mut buffer = Vec::new();
        if let Err(e) = file.read_to_end(&mut buffer) {
            tracing::warn!(err = %e, "Failed to read config... using default");
            return Default::default();
        }

        match toml::from_slice(&buffer) {
            Ok(config) => config,
            Err(e) => {
                tracing::warn!(err = %e, "Failed to parse config... using default");
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
