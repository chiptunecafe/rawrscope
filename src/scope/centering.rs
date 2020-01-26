use ambassador::{delegatable_trait, Delegate};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

mod none;
pub use none::NoCentering;

mod zero_crossing;
pub use zero_crossing::ZeroCrossing;

#[delegatable_trait]
pub trait Algorithm: Serialize + DeserializeOwned {
    fn calculate_offset(&self, data: &[f32], sample_rate: u32, window_len: usize) -> usize;
    fn lookahead(&self) -> f32;
}

#[derive(Delegate, Deserialize, Serialize)]
#[delegate(Algorithm)]
pub enum Centering {
    NoCentering(NoCentering),
    ZeroCrossing(ZeroCrossing),
}
