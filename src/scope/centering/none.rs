use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::scope::centering;

#[derive(Deserialize, Serialize)]
pub struct NoCentering;
impl centering::Algorithm for NoCentering {
    fn center(&self, data: &[f32], _: &RangeInclusive<usize>) -> usize {
        data.len() / 2
    }
}
