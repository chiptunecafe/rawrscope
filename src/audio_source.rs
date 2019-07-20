use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct AudioSource {
    pub path: PathBuf,

    pub fade_in: Option<f32>,
    pub fade_out: Option<f32>,

    pub gain: f32,
}
