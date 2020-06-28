use std::ops::RangeInclusive;

use ambassador::{delegatable_trait, Delegate};
use derivative::Derivative;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod none;
pub use none::NoCentering;

mod zero_crossing;
pub use zero_crossing::ZeroCrossing;

#[delegatable_trait]
pub trait Algorithm: Serialize + DeserializeOwned {
    // TODO not sure if range is allowed to be inclusive
    fn center(&self, data: &[f32], center_range: &RangeInclusive<usize>) -> usize;
}

#[derive(Delegate, Derivative, Deserialize, Serialize)]
#[delegate(Algorithm)]
pub enum Centering {
    NoCentering(NoCentering),
    ZeroCrossing(ZeroCrossing),
}

impl std::fmt::Display for Centering {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Centering::NoCentering(_) => write!(f, "None"),
            Centering::ZeroCrossing(_) => write!(f, "Zero Crossing"),
        }
    }
}
