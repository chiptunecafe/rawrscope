use serde::{Deserialize, Serialize};

use crate::audio::mixer;

#[derive(Serialize, Deserialize)]
pub struct Scope {
    pub channels: i16,
    pub window_size: f32,
    #[serde(skip)]
    pub buffer: Vec<f32>,
}

impl Scope {
    pub fn wanted_length(&self) -> f32 {
        self.window_size
    }
}
