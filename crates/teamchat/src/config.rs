use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub bind: String,
    pub port: u16,
    pub heartbeat_interval_ms: u64,
    pub session_ring_size: usize,
    pub session_timeout_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0".to_string(),
            port: 3001,
            heartbeat_interval_ms: 45_000,
            session_ring_size: 256,
            session_timeout_secs: 180,
        }
    }
}
