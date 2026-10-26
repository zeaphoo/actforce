use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelType {
    GuildText = 0,
    Dm = 1,
    GroupDm = 3,
    GuildCategory = 4,
}

impl Default for ChannelType {
    fn default() -> Self {
        Self::GuildText
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    pub id: String,
    #[serde(rename = "type")]
    pub channel_type: ChannelType,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub last_message_id: Option<String>,
    pub permission_overwrites: Vec<PermissionOverwrite>,
    pub recipients: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionOverwrite {
    pub allow: String,
    pub deny: String,
    pub id: String,
    #[serde(rename = "type")]
    pub overwrite_type: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelCreateRequest {
    pub name: String,
    pub topic: Option<String>,
    #[serde(rename = "type", default)]
    pub channel_type: ChannelType,
    pub recipients: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelUpdateRequest {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: Option<i32>,
}
