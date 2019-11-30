use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub enum MasterChannel {
    Left,
    Right,
}

#[derive(Deserialize, Serialize)]
pub enum ConnectionTarget {
    Master { channel: MasterChannel },
    Scope { name: String, port: String },
}

impl ConnectionTarget {
    pub fn is_master(&self) -> bool {
        match self {
            ConnectionTarget::Master { .. } => true,
            _ => false,
        }
    }
}

#[derive(Deserialize, Serialize)]
pub struct Connection {
    pub channel: u32,
    pub target: ConnectionTarget,
}
