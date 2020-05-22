use std::ops::RangeInclusive;

use ambassador::{delegatable_trait, Delegate};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod none;
pub use none::NoCentering;

mod zero_crossing;
pub use zero_crossing::ZeroCrossing;

mod peak_speed;
pub use peak_speed::PeakSpeed;

#[delegatable_trait]
pub trait Algorithm: Serialize + DeserializeOwned {
    // TODO not sure if range is allowed to be inclusive
    fn center(&self, data: &[f32], center_range: &RangeInclusive<usize>) -> usize;
}

#[derive(Delegate, Deserialize, Serialize)]
#[delegate(Algorithm)]
pub enum Centering {
    NoCentering(NoCentering),
    ZeroCrossing(ZeroCrossing),
    PeakSpeed(PeakSpeed),
}
