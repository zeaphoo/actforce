use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============== Database Entities ==============

/// User entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub login_id: Option<String>,
    pub role: String,
    pub display_name: Option<String>,
    pub password_hash: Option<String>,
    pub password_salt: Option<String>,
    pub password_hash_algo: Option<String>,
    pub password_hash_iterations: Option<i32>,
    pub extra_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User identity entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct UserIdentity {
    pub id: String,
    pub user_id: Option<String>,
    pub identifier: Option<String>,
    pub source: Option<String>,
    pub extra_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Message entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub content_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Conversation entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Conversation {
    pub id: String,
    pub title: Option<String>,
    pub owner_identity_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Skill entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub content: Option<String>,
    pub program: Option<String>,
    pub extra_json: Option<String>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// System setting entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemSetting {
    pub key: String,
    pub value_json: Option<String>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Quick prompt entity
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuickPrompt {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub prompt: String,
    pub icon: Option<String>,
    pub category: Option<String>,
    pub sort_order: i32,
    pub is_enabled: bool,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ============== Request/Response DTOs ==============

/// Message creation request
#[derive(Debug, Clone, Deserialize)]
pub struct MessageCreate {
    pub role: String,
    pub content: String,
}

/// Message response
#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl From<Message> for MessageResponse {
    fn from(msg: Message) -> Self {
        // Extract metadata from content_json (excluding 'text' field)
        let metadata = msg.content_json.as_ref().and_then(|json_str| {
            serde_json::from_str::<serde_json::Value>(json_str).ok().and_then(|json| {
                if let Some(obj) = json.as_object() {
                    let mut meta = serde_json::Map::new();
                    for (k, v) in obj {
                        if k != "text" {
                            meta.insert(k.clone(), v.clone());
                        }
                    }
                    if meta.is_empty() {
                        None
                    } else {
                        Some(serde_json::Value::Object(meta))
                    }
                } else {
                    None
                }
            })
        });
        
        Self {
            id: msg.id,
            role: msg.role,
            content: msg.content,
            metadata,
            created_at: msg.created_at,
        }
    }
}

/// Conversation creation request
#[derive(Debug, Clone, Deserialize)]
pub struct ConversationCreate {
    pub title: Option<String>,
}

/// Conversation response
#[derive(Debug, Clone, Serialize)]
pub struct ConversationResponse {
    pub id: String,
    pub title: Option<String>,
    pub owner_identity_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub running: bool,
}

impl From<Conversation> for ConversationResponse {
    fn from(conv: Conversation) -> Self {
        Self {
            id: conv.id,
            title: conv.title,
            owner_identity_id: conv.owner_identity_id,
            created_at: conv.created_at,
            updated_at: conv.updated_at,
            running: false,
        }
    }
}

/// Chat request
#[derive(Debug, Clone, Deserialize)]
pub struct AttachmentInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub mime_type: String,
    pub size: i64,
    pub data: String,
}

/// Uploaded file info (after file upload)
#[derive(Debug, Clone, Deserialize)]
pub struct UploadedFileInfo {
    pub file_id: String,
    pub file_name: String,
    pub file_size: i64,
    #[serde(rename = "content_type")]
    pub mime_type: String,
    pub path: String,
}

/// Chat request
#[derive(Debug, Clone, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    #[serde(default)]
    pub identity_id: Option<String>,
    #[serde(default)]
    pub conversation_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    #[serde(default)]
    pub chat_template_args: Option<Value>,
    #[serde(default)]
    pub attachments: Option<Vec<serde_json::Value>>,  // Can be either AttachmentInfo or UploadedFileInfo
}

/// Chat stream request
#[derive(Debug, Clone, Deserialize)]
pub struct ChatStreamRequest {
    pub message: String,
    #[serde(default)]
    pub conversation_id: Option<String>,  // Accept both string and number
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<i32>,
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    #[serde(default)]
    pub chat_template_args: Option<Value>,
    #[serde(default)]
    pub attachments: Option<Vec<serde_json::Value>>,  // Can be either AttachmentInfo or UploadedFileInfo
}

// ============== Quick Prompt DTOs ==============

/// Quick prompt response
#[derive(Debug, Clone, Serialize)]
pub struct QuickPromptResponse {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub prompt: String,
    pub icon: Option<String>,
    pub category: Option<String>,
    pub sort_order: i32,
    pub is_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<QuickPrompt> for QuickPromptResponse {
    fn from(prompt: QuickPrompt) -> Self {
        Self {
            id: prompt.id,
            title: prompt.title,
            description: prompt.description,
            prompt: prompt.prompt,
            icon: prompt.icon,
            category: prompt.category,
            sort_order: prompt.sort_order,
            is_enabled: prompt.is_enabled,
            created_at: prompt.created_at,
            updated_at: prompt.updated_at,
        }
    }
}

/// Quick prompt create request
#[derive(Debug, Clone, Deserialize)]
pub struct QuickPromptCreate {
    pub title: String,
    pub description: Option<String>,
    pub prompt: String,
    pub icon: Option<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
}

/// Quick prompt update request
#[derive(Debug, Clone, Deserialize)]
pub struct QuickPromptUpdate {
    pub title: Option<String>,
    pub description: Option<String>,
    pub prompt: Option<String>,
    pub icon: Option<String>,
    pub category: Option<String>,
    pub sort_order: Option<i32>,
    pub is_enabled: Option<bool>,
}

/// Quick prompt list response
#[derive(Debug, Clone, Serialize)]
pub struct QuickPromptListResponse {
    pub prompts: Vec<QuickPromptResponse>,
    pub total: usize,
}

fn default_true() -> bool {
    true
}
