use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Scope {
    pub channels: i16,
    pub window_size: f32,
}

impl Scope {
    pub fn wanted_length(&self) -> f32 {
        self.window_size
    }
}
