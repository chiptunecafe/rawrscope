use serde::{Deserialize, Serialize};
use std::{f32, usize};

use crate::scope::centering;

#[derive(Deserialize, Serialize)]
pub struct PeakSpeed {
    pub trigger_width: f32,
}
impl centering::Algorithm for PeakSpeed {
    fn calculate_offset(&self, data: &[f32], sample_rate: u32, window_len: usize) -> usize {
        let window_half = window_len / 2;
        // can't seek data stream backwards, must return center + window_half
        let start = window_half;
        let samples_size_requested = ((sample_rate as f32) * self.trigger_width) as usize;
        let samples_size_actual = if (start + samples_size_requested) <= data.len() {
            samples_size_requested
        } else {
            data.len() - start
        };

        let end = (start + samples_size_actual) - 1;
        log::debug!("Running PeakSpeed calculations with {} samples to check from {} - {}.", samples_size_actual, start, end);

        // first pass
        // iterate over data, find highest value
        let mut max = f32::NEG_INFINITY;
        let mut maxs = vec![0; samples_size_actual];
        let mut maxs_next = 0;

        for i in start..=end {
            let current_sample = data[i];

            if current_sample > max {
                max = current_sample;
                maxs = vec![0; maxs.len()];
                maxs_next = 0;
            }
            if current_sample == max {
                maxs[maxs_next] = i;
                maxs_next += 1;
            }
        }
        log::debug!("Found max & #maxs: {}, {}.", max, maxs_next);

        // second pass
        // iterate over data from last max backwards
        // find lowest reachable value
        let mut min = f32::INFINITY;

        for min_checking in (start..=maxs[maxs_next - 1]).rev() {
            let current_sample_min = data[min_checking];

            if current_sample_min < min {
                min = current_sample_min;
            }
        }
        let mid = (max + min) / (2 as f32);
        log::debug!("Found min & mid: {}, {}.", min, mid);

        // third pass
        // iterate over maxs, from max to previous max or start
        // find quickest-reached value below mid
        let mut center = start;
        let mut speed = usize::max_value();

        for speed_checking in 0..(maxs_next) {
            let speed_checking_start = if speed_checking == 0 { start } else { maxs[speed_checking - 1] + 1 };
            let speed_checking_end = maxs[speed_checking];

            for i in (speed_checking_start..=speed_checking_end).rev() {
                let current_sample = data[i];
                if current_sample < mid {
                    if speed_checking_end - i < speed {
                        center = i;
                        speed = speed_checking_end - i;
                    }
                    break;
                }
            }
        }
        log::debug!("Fastest mid @ {} ({} samples).", center, speed);

        log::debug!("Returning from PeakSpeed calculations with value {}.", center - window_half);
        return center - window_half;
    }

    fn lookahead(&self) -> f32 {
        self.trigger_width
    }
}
