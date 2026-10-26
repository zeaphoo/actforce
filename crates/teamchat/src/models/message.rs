use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MessageType {
    Default = 0,
    Reply = 19,
    ThreadStarter = 21,
}

impl Default for MessageType {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub edited_timestamp: Option<DateTime<Utc>>,
    pub embeds: Vec<Embed>,
    pub attachments: Vec<Attachment>,
    pub mentions: Vec<String>,
    pub pinned: bool,
    #[serde(rename = "type")]
    pub message_type: MessageType,
    pub message_reference: Option<MessageReference>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Embed {
    pub title: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub embed_type: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub color: Option<u32>,
    pub fields: Option<Vec<EmbedField>>,
    pub footer: Option<EmbedFooter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
    pub inline: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedFooter {
    pub text: String,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub url: String,
    pub content_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageReference {
    pub message_id: String,
    pub channel_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageCreateRequest {
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub content: String,
    pub embeds: Option<Vec<Embed>>,
    pub message_reference: Option<MessageReference>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageUpdateRequest {
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessagesQuery {
    pub limit: Option<u32>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub around: Option<String>,
}
