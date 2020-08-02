use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub enum MasterChannel {
    Left,
    Right,
}

#[derive(Debug, Deserialize, Serialize)]
pub enum ConnectionTarget {
    Master { channel: MasterChannel },
    Scope { name: String, channel: u32 },
}

impl ConnectionTarget {
    pub fn is_master(&self) -> bool {
        match self {
            ConnectionTarget::Master { .. } => true,
            _ => false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Connection {
    pub channel: u32,
    pub target: ConnectionTarget,
}
