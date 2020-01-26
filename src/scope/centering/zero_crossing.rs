use serde::{Deserialize, Serialize};

use crate::scope::centering;

#[derive(Deserialize, Serialize)]
pub struct ZeroCrossing {
    pub trigger_width: f32,
}
impl centering::Algorithm for ZeroCrossing {
    fn calculate_offset(&self, data: &[f32], sample_rate: u32, window_len: usize) -> usize {
        let trigger_samples = (sample_rate as f32 * self.trigger_width) as usize;
        let start = window_len / 2;
        let end = (start + trigger_samples).min(data.len()) - 1;

        for i in start..end {
            if data[i] <= 0.0 && data[i + 1] >= 0.0 {
                return i - window_len / 2;
            }
        }

        0
    }

    fn lookahead(&self) -> f32 {
        self.trigger_width
    }
}
