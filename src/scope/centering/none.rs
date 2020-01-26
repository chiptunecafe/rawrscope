use serde::{Deserialize, Serialize};

use crate::scope::centering;

#[derive(Deserialize, Serialize)]
pub struct NoCentering;
impl centering::Algorithm for NoCentering {
    fn calculate_offset(&self, _: &[f32], _: u32, _: usize) -> usize {
        0
    }

    fn lookahead(&self) -> f32 {
        0.0
    }
}
