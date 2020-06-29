use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::scope::centering;

#[derive(Deserialize, Serialize)]
pub struct ZeroCrossing;
impl centering::Algorithm for ZeroCrossing {
    fn center(&mut self, data: &[f32], center_range: &RangeInclusive<usize>) -> usize {
        let center = data.len() / 2;

        for i in 0..(center_range.end() - center_range.start()) / 2 {
            let lhs = center - i;
            let rhs = center + i;

            if data[lhs] <= 0.0 && data[lhs + 1] >= 0.0 {
                return lhs;
            }

            if data[rhs] <= 0.0 && data[rhs + 1] >= 0.0 {
                return rhs;
            }
        }

        center
    }
}
