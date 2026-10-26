use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Dispatch = 0,
    Heartbeat = 1,
    Subscribe = 2,
    Resume = 6,
    InvalidSession = 9,
    Hello = 10,
    HeartbeatAck = 11,
}

impl Opcode {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Dispatch),
            1 => Some(Self::Heartbeat),
            2 => Some(Self::Subscribe),
            6 => Some(Self::Resume),
            9 => Some(Self::InvalidSession),
            10 => Some(Self::Hello),
            11 => Some(Self::HeartbeatAck),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayReceive {
    pub op: u8,
    #[serde(default)]
    pub d: serde_json::Value,
    #[serde(default)]
    pub s: Option<u64>,
    #[serde(default)]
    pub t: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GatewaySend {
    pub op: u8,
    pub d: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub t: Option<String>,
}

impl GatewaySend {
    pub fn dispatch(event: &str, data: serde_json::Value, seq: u64) -> Self {
        Self {
            op: Opcode::Dispatch.to_u8(),
            d: data,
            s: Some(seq),
            t: Some(event.to_string()),
        }
    }

    pub fn hello(heartbeat_interval: u64) -> Self {
        Self {
            op: Opcode::Hello.to_u8(),
            d: serde_json::json!({ "heartbeat_interval": heartbeat_interval }),
            s: None,
            t: None,
        }
    }

    pub fn heartbeat_ack() -> Self {
        Self {
            op: Opcode::HeartbeatAck.to_u8(),
            d: serde_json::json!({}),
            s: None,
            t: None,
        }
    }

    pub fn invalid_session() -> Self {
        Self {
            op: Opcode::InvalidSession.to_u8(),
            d: serde_json::json!({ "resumable": false }),
            s: None,
            t: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubscribePayload {
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResumePayload {
    pub session_id: String,
    pub seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypingStartEvent {
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: String,
}
