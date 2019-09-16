use serde::{Deserialize, Serialize};

use crate::state::GridRect;

#[derive(Serialize, Deserialize)]
pub struct Scope {
    pub channels: i16,
    pub window_size: f32,

    // appearance
    pub line_width: f32,
    pub rect: GridRect,
}

impl Scope {
    pub fn wanted_length(&self) -> f32 {
        self.window_size
    }
}
