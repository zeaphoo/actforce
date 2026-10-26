use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db::{PgPool, SqlitePool};
use crate::error::TeamChatError;
use crate::models::channel::{Channel, ChannelType};
use crate::models::message::{Embed, Message, MessageReference, MessageType};
use crate::snowflake;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelRow {
    pub id: String,
    pub channel_type: i32,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub last_message_id: Option<String>,
    pub recipients: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MessageRow {
    pub id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub embeds: serde_json::Value,
    pub attachments: serde_json::Value,
    pub mentions: serde_json::Value,
    pub pinned: i32,
    pub message_type: i32,
    pub message_reference: Option<serde_json::Value>,
    pub timestamp: String,
    pub edited_timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PgChannelRow {
    pub id: String,
    pub channel_type: i32,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub position: i32,
    pub last_message_id: Option<String>,
    pub recipients: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct PgMessageRow {
    pub id: String,
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub content: String,
    pub embeds: serde_json::Value,
    pub attachments: serde_json::Value,
    pub mentions: serde_json::Value,
    pub pinned: bool,
    pub message_type: i32,
    pub message_reference: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub edited_timestamp: Option<DateTime<Utc>>,
}

fn channel_type_from_i32(t: i32) -> ChannelType {
    match t {
        0 => ChannelType::GuildText,
        1 => ChannelType::Dm,
        3 => ChannelType::GroupDm,
        4 => ChannelType::GuildCategory,
        _ => ChannelType::GuildText,
    }
}

fn channel_row_to_channel(row: ChannelRow) -> Channel {
    let recipients: Vec<String> = serde_json::from_value(row.recipients).unwrap_or_default();
    Channel {
        id: row.id,
        channel_type: channel_type_from_i32(row.channel_type),
        name: row.name,
        topic: row.topic,
        position: row.position,
        last_message_id: row.last_message_id,
        permission_overwrites: Vec::new(),
        recipients,
        created_at: row.created_at.parse().unwrap_or_else(|_| Utc::now()),
    }
}

fn pg_channel_row_to_channel(row: PgChannelRow) -> Channel {
    let recipients: Vec<String> = serde_json::from_value(row.recipients).unwrap_or_default();
    Channel {
        id: row.id,
        channel_type: channel_type_from_i32(row.channel_type),
        name: row.name,
        topic: row.topic,
        position: row.position,
        last_message_id: row.last_message_id,
        permission_overwrites: Vec::new(),
        recipients,
        created_at: row.created_at,
    }
}

fn message_row_to_message(row: MessageRow) -> Message {
    Message {
        id: row.id,
        channel_id: row.channel_id,
        sender_id: row.sender_id,
        sender_name: row.sender_name,
        content: row.content,
        timestamp: row.timestamp.parse().unwrap_or_else(|_| Utc::now()),
        edited_timestamp: row.edited_timestamp.and_then(|s| s.parse().ok()),
        embeds: serde_json::from_value(row.embeds).unwrap_or_default(),
        attachments: serde_json::from_value(row.attachments).unwrap_or_default(),
        mentions: serde_json::from_value(row.mentions).unwrap_or_default(),
        pinned: row.pinned != 0,
        message_type: match row.message_type {
            19 => MessageType::Reply,
            21 => MessageType::ThreadStarter,
            _ => MessageType::Default,
        },
        message_reference: row.message_reference.and_then(|v| serde_json::from_value(v).ok()),
    }
}

fn pg_message_row_to_message(row: PgMessageRow) -> Message {
    Message {
        id: row.id,
        channel_id: row.channel_id,
        sender_id: row.sender_id,
        sender_name: row.sender_name,
        content: row.content,
        timestamp: row.timestamp,
        edited_timestamp: row.edited_timestamp,
        embeds: serde_json::from_value(row.embeds).unwrap_or_default(),
        attachments: serde_json::from_value(row.attachments).unwrap_or_default(),
        mentions: serde_json::from_value(row.mentions).unwrap_or_default(),
        pinned: row.pinned,
        message_type: match row.message_type {
            19 => MessageType::Reply,
            21 => MessageType::ThreadStarter,
            _ => MessageType::Default,
        },
        message_reference: row.message_reference.and_then(|v| serde_json::from_value(v).ok()),
    }
}

#[async_trait]
pub trait TeamChatStore: Send + Sync + 'static {
    async fn create_channel(&self, name: &str, topic: Option<&str>, channel_type: ChannelType, recipients: &[String]) -> Result<Channel, TeamChatError>;
    async fn get_channel(&self, id: &str) -> Result<Option<Channel>, TeamChatError>;
    async fn list_channels(&self) -> Result<Vec<Channel>, TeamChatError>;
    async fn update_channel(&self, id: &str, name: Option<&str>, topic: Option<&str>, position: Option<i32>) -> Result<Option<Channel>, TeamChatError>;
    async fn delete_channel(&self, id: &str) -> Result<bool, TeamChatError>;

    async fn create_message(&self, channel_id: &str, sender_id: &str, sender_name: &str, content: &str, embeds: &[Embed], message_reference: Option<MessageReference>) -> Result<Message, TeamChatError>;
    async fn get_message(&self, id: &str) -> Result<Option<Message>, TeamChatError>;
    async fn list_messages(&self, channel_id: &str, before: Option<&str>, after: Option<&str>, limit: u32) -> Result<Vec<Message>, TeamChatError>;
    async fn update_message(&self, id: &str, content: &str) -> Result<Option<Message>, TeamChatError>;
    async fn delete_message(&self, id: &str) -> Result<bool, TeamChatError>;
    async fn update_last_message_id(&self, channel_id: &str, message_id: &str) -> Result<(), TeamChatError>;
}

// ── MemoryStore ──────────────────────────────────────────────

#[derive(Clone)]
pub struct MemoryStore {
    channels: Arc<RwLock<HashMap<String, Channel>>>,
    messages: Arc<RwLock<HashMap<String, Message>>>,
    channel_messages: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(HashMap::new())),
            channel_messages: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl TeamChatStore for MemoryStore {
    async fn create_channel(&self, name: &str, topic: Option<&str>, channel_type: ChannelType, recipients: &[String]) -> Result<Channel, TeamChatError> {
        let id = snowflake::generate();
        let channel = Channel {
            id: id.clone(),
            channel_type,
            name: Some(name.to_string()),
            topic: topic.map(|s| s.to_string()),
            position: 0,
            last_message_id: None,
            permission_overwrites: Vec::new(),
            recipients: recipients.to_vec(),
            created_at: Utc::now(),
        };
        self.channels.write().await.insert(id.clone(), channel.clone());
        self.channel_messages.write().await.insert(id.clone(), Vec::new());
        Ok(channel)
    }

    async fn get_channel(&self, id: &str) -> Result<Option<Channel>, TeamChatError> {
        Ok(self.channels.read().await.get(id).cloned())
    }

    async fn list_channels(&self) -> Result<Vec<Channel>, TeamChatError> {
        Ok(self.channels.read().await.values().cloned().collect())
    }

    async fn update_channel(&self, id: &str, name: Option<&str>, topic: Option<&str>, position: Option<i32>) -> Result<Option<Channel>, TeamChatError> {
        let mut chs = self.channels.write().await;
        if let Some(ch) = chs.get_mut(id) {
            if let Some(n) = name { ch.name = Some(n.to_string()); }
            if let Some(t) = topic { ch.topic = Some(t.to_string()); }
            if let Some(p) = position { ch.position = p; }
            return Ok(Some(ch.clone()));
        }
        Ok(None)
    }

    async fn delete_channel(&self, id: &str) -> Result<bool, TeamChatError> {
        self.channel_messages.write().await.remove(id);
        Ok(self.channels.write().await.remove(id).is_some())
    }

    async fn create_message(&self, channel_id: &str, sender_id: &str, sender_name: &str, content: &str, embeds: &[Embed], message_reference: Option<MessageReference>) -> Result<Message, TeamChatError> {
        let id = snowflake::generate();
        let msg = Message {
            id: id.clone(),
            channel_id: channel_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            edited_timestamp: None,
            embeds: embeds.to_vec(),
            attachments: Vec::new(),
            mentions: Vec::new(),
            pinned: false,
            message_type: if message_reference.is_some() { MessageType::Reply } else { MessageType::Default },
            message_reference,
        };
        self.messages.write().await.insert(id.clone(), msg.clone());
        self.channel_messages.write().await.entry(channel_id.to_string()).or_default().push(id.clone());
        if let Some(ch) = self.channels.write().await.get_mut(channel_id) {
            ch.last_message_id = Some(id);
        }
        Ok(msg)
    }

    async fn get_message(&self, id: &str) -> Result<Option<Message>, TeamChatError> {
        Ok(self.messages.read().await.get(id).cloned())
    }

    async fn list_messages(&self, channel_id: &str, before: Option<&str>, _after: Option<&str>, limit: u32) -> Result<Vec<Message>, TeamChatError> {
        let msg_ids = self.channel_messages.read().await.get(channel_id).cloned().unwrap_or_default();
        let msgs = self.messages.read().await;
        let mut result: Vec<Message> = msg_ids.iter().filter_map(|id| msgs.get(id).cloned()).collect();
        if let Some(before_id) = before {
            result.retain(|m| m.id.as_str() < before_id);
        }
        result.sort_by(|a, b| b.id.cmp(&a.id));
        result.truncate(limit as usize);
        result.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(result)
    }

    async fn update_message(&self, id: &str, content: &str) -> Result<Option<Message>, TeamChatError> {
        let mut msgs = self.messages.write().await;
        if let Some(m) = msgs.get_mut(id) {
            m.content = content.to_string();
            m.edited_timestamp = Some(Utc::now());
            return Ok(Some(m.clone()));
        }
        Ok(None)
    }

    async fn delete_message(&self, id: &str) -> Result<bool, TeamChatError> {
        Ok(self.messages.write().await.remove(id).is_some())
    }

    async fn update_last_message_id(&self, channel_id: &str, message_id: &str) -> Result<(), TeamChatError> {
        if let Some(ch) = self.channels.write().await.get_mut(channel_id) {
            ch.last_message_id = Some(message_id.to_string());
        }
        Ok(())
    }
}

impl Default for MemoryStore {
    fn default() -> Self { Self::new() }
}

// ── SqliteStore ─────────────────────────────────────────────

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }
}

#[async_trait]
impl TeamChatStore for SqliteStore {
    async fn create_channel(&self, name: &str, topic: Option<&str>, channel_type: ChannelType, recipients: &[String]) -> Result<Channel, TeamChatError> {
        let id = snowflake::generate();
        let now = Utc::now().to_rfc3339();
        let recipients_json = serde_json::to_string(recipients).unwrap_or_else(|_| "[]".into());
        sqlx::query("INSERT INTO channels (id, channel_type, name, topic, position, recipients, created_at) VALUES (?, ?, ?, ?, 0, ?, ?)")
            .bind(&id).bind(channel_type as i32).bind(name).bind(topic).bind(&recipients_json).bind(&now)
            .execute(&self.pool).await?;
        Ok(Channel { id, channel_type, name: Some(name.to_string()), topic: topic.map(|s| s.to_string()), position: 0, last_message_id: None, permission_overwrites: Vec::new(), recipients: recipients.to_vec(), created_at: Utc::now() })
    }

    async fn get_channel(&self, id: &str) -> Result<Option<Channel>, TeamChatError> {
        let row = sqlx::query_as::<_, ChannelRow>("SELECT * FROM channels WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(channel_row_to_channel))
    }

    async fn list_channels(&self) -> Result<Vec<Channel>, TeamChatError> {
        let rows = sqlx::query_as::<_, ChannelRow>("SELECT * FROM channels ORDER BY position ASC")
            .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(channel_row_to_channel).collect())
    }

    async fn update_channel(&self, id: &str, name: Option<&str>, topic: Option<&str>, position: Option<i32>) -> Result<Option<Channel>, TeamChatError> {
        if let Some(n) = name { sqlx::query("UPDATE channels SET name = ? WHERE id = ?").bind(n).bind(id).execute(&self.pool).await?; }
        if let Some(t) = topic { sqlx::query("UPDATE channels SET topic = ? WHERE id = ?").bind(t).bind(id).execute(&self.pool).await?; }
        if let Some(p) = position { sqlx::query("UPDATE channels SET position = ? WHERE id = ?").bind(p).bind(id).execute(&self.pool).await?; }
        self.get_channel(id).await
    }

    async fn delete_channel(&self, id: &str) -> Result<bool, TeamChatError> {
        let r = sqlx::query("DELETE FROM channels WHERE id = ?").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM messages WHERE channel_id = ?").bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    async fn create_message(&self, channel_id: &str, sender_id: &str, sender_name: &str, content: &str, embeds: &[Embed], message_reference: Option<MessageReference>) -> Result<Message, TeamChatError> {
        let id = snowflake::generate();
        let now = Utc::now().to_rfc3339();
        let embeds_json = serde_json::to_string(embeds).unwrap_or_else(|_| "[]".into());
        let ref_json = message_reference.as_ref().and_then(|r| serde_json::to_string(r).ok());
        let mt = if message_reference.is_some() { 19i32 } else { 0i32 };
        sqlx::query("INSERT INTO messages (id, channel_id, sender_id, sender_name, content, embeds, message_type, message_reference, timestamp) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(&id).bind(channel_id).bind(sender_id).bind(sender_name).bind(content).bind(&embeds_json).bind(mt).bind(&ref_json).bind(&now)
            .execute(&self.pool).await?;
        self.update_last_message_id(channel_id, &id).await?;
        Ok(Message { id, channel_id: channel_id.to_string(), sender_id: sender_id.to_string(), sender_name: sender_name.to_string(), content: content.to_string(), timestamp: Utc::now(), edited_timestamp: None, embeds: embeds.to_vec(), attachments: Vec::new(), mentions: Vec::new(), pinned: false, message_type: if message_reference.is_some() { MessageType::Reply } else { MessageType::Default }, message_reference })
    }

    async fn get_message(&self, id: &str) -> Result<Option<Message>, TeamChatError> {
        let row = sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE id = ?")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(message_row_to_message))
    }

    async fn list_messages(&self, channel_id: &str, before: Option<&str>, _after: Option<&str>, limit: u32) -> Result<Vec<Message>, TeamChatError> {
        let rows: Vec<MessageRow> = if let Some(before_id) = before {
            sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE channel_id = ? AND id < ? ORDER BY timestamp DESC LIMIT ?")
                .bind(channel_id).bind(before_id).bind(limit as i32)
                .fetch_all(&self.pool).await?
        } else {
            sqlx::query_as::<_, MessageRow>("SELECT * FROM messages WHERE channel_id = ? ORDER BY timestamp DESC LIMIT ?")
                .bind(channel_id).bind(limit as i32)
                .fetch_all(&self.pool).await?
        };
        let mut msgs: Vec<Message> = rows.into_iter().map(message_row_to_message).collect();
        msgs.reverse();
        Ok(msgs)
    }

    async fn update_message(&self, id: &str, content: &str) -> Result<Option<Message>, TeamChatError> {
        let now = Utc::now().to_rfc3339();
        let r = sqlx::query("UPDATE messages SET content = ?, edited_timestamp = ? WHERE id = ?")
            .bind(content).bind(&now).bind(id).execute(&self.pool).await?;
        if r.rows_affected() == 0 { return Ok(None); }
        self.get_message(id).await
    }

    async fn delete_message(&self, id: &str) -> Result<bool, TeamChatError> {
        let r = sqlx::query("DELETE FROM messages WHERE id = ?").bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    async fn update_last_message_id(&self, channel_id: &str, message_id: &str) -> Result<(), TeamChatError> {
        sqlx::query("UPDATE channels SET last_message_id = ? WHERE id = ?")
            .bind(message_id).bind(channel_id).execute(&self.pool).await?;
        Ok(())
    }
}

// ── PostgresStore ────────────────────────────────────────────

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub fn new(pool: PgPool) -> Self { Self { pool } }
}

#[async_trait]
impl TeamChatStore for PostgresStore {
    async fn create_channel(&self, name: &str, topic: Option<&str>, channel_type: ChannelType, recipients: &[String]) -> Result<Channel, TeamChatError> {
        let id = snowflake::generate();
        let recipients_json = serde_json::to_value(recipients).unwrap_or_default();
        let row = sqlx::query_as::<_, PgChannelRow>(
            "INSERT INTO channels (id, channel_type, name, topic, position, recipients, created_at) VALUES ($1, $2, $3, $4, 0, $5, NOW()) RETURNING id, channel_type, name, topic, position, last_message_id, recipients, created_at"
        ).bind(&id).bind(channel_type as i32).bind(name).bind(topic).bind(&recipients_json)
         .fetch_one(&self.pool).await?;
        Ok(pg_channel_row_to_channel(row))
    }

    async fn get_channel(&self, id: &str) -> Result<Option<Channel>, TeamChatError> {
        let row = sqlx::query_as::<_, PgChannelRow>("SELECT id, channel_type, name, topic, position, last_message_id, recipients, created_at FROM channels WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(pg_channel_row_to_channel))
    }

    async fn list_channels(&self) -> Result<Vec<Channel>, TeamChatError> {
        let rows = sqlx::query_as::<_, PgChannelRow>("SELECT id, channel_type, name, topic, position, last_message_id, recipients, created_at FROM channels ORDER BY position ASC")
            .fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(pg_channel_row_to_channel).collect())
    }

    async fn update_channel(&self, id: &str, name: Option<&str>, topic: Option<&str>, position: Option<i32>) -> Result<Option<Channel>, TeamChatError> {
        if let Some(n) = name { sqlx::query("UPDATE channels SET name = $1 WHERE id = $2").bind(n).bind(id).execute(&self.pool).await?; }
        if let Some(t) = topic { sqlx::query("UPDATE channels SET topic = $1 WHERE id = $2").bind(t).bind(id).execute(&self.pool).await?; }
        if let Some(p) = position { sqlx::query("UPDATE channels SET position = $1 WHERE id = $2").bind(p).bind(id).execute(&self.pool).await?; }
        self.get_channel(id).await
    }

    async fn delete_channel(&self, id: &str) -> Result<bool, TeamChatError> {
        let r = sqlx::query("DELETE FROM channels WHERE id = $1").bind(id).execute(&self.pool).await?;
        sqlx::query("DELETE FROM messages WHERE channel_id = $1").bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    async fn create_message(&self, channel_id: &str, sender_id: &str, sender_name: &str, content: &str, embeds: &[Embed], message_reference: Option<MessageReference>) -> Result<Message, TeamChatError> {
        let id = snowflake::generate();
        let embeds_json = serde_json::to_value(embeds).unwrap_or_default();
        let ref_json = message_reference.as_ref().and_then(|r| serde_json::to_value(r).ok());
        let mt = if message_reference.is_some() { 19i32 } else { 0i32 };
        let row = sqlx::query_as::<_, PgMessageRow>(
            "INSERT INTO messages (id, channel_id, sender_id, sender_name, content, embeds, message_type, message_reference, timestamp) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW()) RETURNING id, channel_id, sender_id, sender_name, content, embeds, attachments, mentions, pinned, message_type, message_reference, timestamp, edited_timestamp"
        ).bind(&id).bind(channel_id).bind(sender_id).bind(sender_name).bind(content).bind(&embeds_json).bind(mt).bind(&ref_json)
         .fetch_one(&self.pool).await?;
        self.update_last_message_id(channel_id, &id).await?;
        Ok(pg_message_row_to_message(row))
    }

    async fn get_message(&self, id: &str) -> Result<Option<Message>, TeamChatError> {
        let row = sqlx::query_as::<_, PgMessageRow>("SELECT id, channel_id, sender_id, sender_name, content, embeds, attachments, mentions, pinned, message_type, message_reference, timestamp, edited_timestamp FROM messages WHERE id = $1")
            .bind(id).fetch_optional(&self.pool).await?;
        Ok(row.map(pg_message_row_to_message))
    }

    async fn list_messages(&self, channel_id: &str, before: Option<&str>, _after: Option<&str>, limit: u32) -> Result<Vec<Message>, TeamChatError> {
        let rows: Vec<PgMessageRow> = if let Some(before_id) = before {
            sqlx::query_as::<_, PgMessageRow>("SELECT id, channel_id, sender_id, sender_name, content, embeds, attachments, mentions, pinned, message_type, message_reference, timestamp, edited_timestamp FROM messages WHERE channel_id = $1 AND id < $2 ORDER BY timestamp DESC LIMIT $3")
                .bind(channel_id).bind(before_id).bind(limit as i64)
                .fetch_all(&self.pool).await?
        } else {
            sqlx::query_as::<_, PgMessageRow>("SELECT id, channel_id, sender_id, sender_name, content, embeds, attachments, mentions, pinned, message_type, message_reference, timestamp, edited_timestamp FROM messages WHERE channel_id = $1 ORDER BY timestamp DESC LIMIT $2")
                .bind(channel_id).bind(limit as i64)
                .fetch_all(&self.pool).await?
        };
        let mut msgs: Vec<Message> = rows.into_iter().map(pg_message_row_to_message).collect();
        msgs.reverse();
        Ok(msgs)
    }

    async fn update_message(&self, id: &str, content: &str) -> Result<Option<Message>, TeamChatError> {
        let r = sqlx::query("UPDATE messages SET content = $1, edited_timestamp = NOW() WHERE id = $2")
            .bind(content).bind(id).execute(&self.pool).await?;
        if r.rows_affected() == 0 { return Ok(None); }
        self.get_message(id).await
    }

    async fn delete_message(&self, id: &str) -> Result<bool, TeamChatError> {
        let r = sqlx::query("DELETE FROM messages WHERE id = $1").bind(id).execute(&self.pool).await?;
        Ok(r.rows_affected() > 0)
    }

    async fn update_last_message_id(&self, channel_id: &str, message_id: &str) -> Result<(), TeamChatError> {
        sqlx::query("UPDATE channels SET last_message_id = $1 WHERE id = $2")
            .bind(message_id).bind(channel_id).execute(&self.pool).await?;
        Ok(())
    }
}
