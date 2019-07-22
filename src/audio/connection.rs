use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub enum ConnectionTarget {
    System,
    Scope(String),
}

#[derive(Deserialize, Serialize)]
pub struct Connection {
    pub channel: u32,

    pub target: ConnectionTarget,
    pub target_channel: u32,
}
