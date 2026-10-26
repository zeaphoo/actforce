use axum::{
    extract::{State, Json, Query, ws::{WebSocket, WebSocketUpgrade, Message}},
    http::StatusCode,
    response::{sse::{Event, Sse}, IntoResponse},
    routing::{get, post, patch},
    Router,
    Json as AxumJson,
};
use futures::{stream::Stream, SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{convert::Infallible, time::Duration, sync::Arc};
use tokio_stream::wrappers::ReceiverStream;
use tracing::{info, error};

use axum::http::HeaderMap;
use chrono::Utc;
use sqlx::Row;

use crate::auth::{decode_jwt, extract_bearer_token};

use agent_core::{
    LLMConfig, ChatEvent, ChatLoopManager, ChatScope, Attachment,
};

use crate::{
    database::{
        create_conversation, create_message, create_message_with_id, get_messages_by_conversation, 
        get_conversations_by_identity, get_identity_ids_for_user, DbPool, update_conversation_title,
        get_conversation_by_id_any, auto_name_conversation_if_empty
    },
    models::{ChatRequest, ChatStreamRequest, MessageResponse, ConversationResponse, ConversationCreate},
    attachment_handler::{save_single_attachment_from_base64, get_attachment_file_path},
    config::CONFIG,
};

/// LLM client configuration - dynamically built from environment
fn get_llm_base_url() -> String {
    let auto_start = std::env::var("INFERENCE_AUTO_START")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);

    if auto_start {
        // 使用本地启动的推理服务
        format!("http://{}:{}/v1", CONFIG.inference_host, CONFIG.inference_port)
    } else {
        // 使用远端 LLM 服务
        std::env::var("LLM_BASE_URL")
            .unwrap_or_else(|_| format!("http://{}:{}/v1", CONFIG.inference_host, CONFIG.inference_port))
    }
}

fn get_llm_api_key() -> Option<String> {
    std::env::var("LLM_API_KEY").ok()
}

fn get_llm_model() -> String {
    CONFIG.llm_model.clone()
}

/// Chat completion message (supports both text and multimodal)
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum ChatCompletionContent {
    Text(String),
    Multimodal(Vec<MultimodalContent>),
}

#[derive(Debug, Clone, Serialize)]
struct MultimodalContent {
    #[serde(rename = "type")]
    content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_url: Option<ImageUrlContent>,
}

#[derive(Debug, Clone, Serialize)]
struct ImageUrlContent {
    url: String,
}

/// Chat completion message
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionMessage {
    role: String,
    content: ChatCompletionContent,
}

/// Chat completion request
#[derive(Debug, Clone, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatCompletionMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_body: Option<Value>,
}

/// Chat completion choice
#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionChoice {
    message: ChatCompletionChoiceMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionChoiceMessage {
    content: String,
}

/// Chat completion response
#[derive(Debug, Clone, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

/// Streaming delta
#[derive(Debug, Clone, Deserialize)]
struct StreamDelta {
    #[serde(default)]
    content: Option<String>,
}

/// Streaming choice
#[derive(Debug, Clone, Deserialize)]
struct StreamChoice {
    delta: StreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

/// Streaming response chunk
#[derive(Debug, Clone, Deserialize)]
struct StreamChunk {
    choices: Vec<StreamChoice>,
}

/// SSE event type
#[derive(Debug, Clone, Serialize)]
struct SseEvent {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

/// Create an error SSE stream
fn error_stream(message: &str) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use futures::stream;
    let msg = message.to_string();
    let _ev = SseEvent {
        event_type: "error".to_string(),
        text: Some(msg),
    };
    let data = format!("data: {}\n\n", message);
    let event = Event::default().data(data);
    let stream = stream::once(async { Ok(event) });
    Sse::new(stream)
}

/// Application state for chat handlers
#[derive(Clone)]
pub struct ChatState {
    pub db_pool: DbPool,
    pub loop_manager: Arc<ChatLoopManager>,
}

/// Create chat router
/// Router that only serves the root path — mounted at /api/chat/ so that
/// POST /api/chat/ (trailing slash, what the frontend actually sends) works.
/// The main chat_router mounted at /api/chat serves /api/chat (no slash).
pub fn chat_root_trailing_router() -> Router<ChatState> {
    Router::new().route("/", post(send_message))
}

pub fn chat_router() -> Router<ChatState> {
    Router::new()
        .route("/", post(send_message))
        // 使用新的 AgentChatLoop 架构（支持步骤执行、文件处理、事件推送）
        .route("/stream", post(stream_message_with_agent_loop))
        // WebSocket 聊天接口（替代 SSE，支持持久连接和多轮对话）
        .route("/ws", get(chat_websocket))
        // 旧的 LLM 直接调用架构（已弃用，保留作为备份）
        // .route("/stream", post(stream_message))
        .route("/upload", post(upload_file))
        .route("/conversations", get(list_conversations).post(create_conversation_handler))
        .route("/conversations/{id}/messages", get(get_messages_handler))
        .route("/conversations/{id}/running", get(get_conversation_running_handler))
        .route("/conversations/{id}/stream/reconnect", get(reconnect_stream_handler))
        .route("/conversations/{id}", patch(update_conversation_handler).delete(delete_conversation_handler))
        // catch-all 路由必须放在最后
        .route("/files/{*file_path}", get(serve_chat_file))
}

/// Extract identity_id from headers
fn extract_identity_id(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Identity-Id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

/// Check if anonymous access is allowed. If not and user is not authenticated, return error.
async fn check_anonymous_allowed(
    pool: &DbPool,
    headers: &HeaderMap,
) -> Result<(), StatusCode> {
    // agentlite parity: a *valid* token authenticates; an invalid/expired token
    // is treated as unauthenticated (not allowed through). The frontend maps
    // 403 to "匿名使用已被禁用" itself, so no detail body is needed.
    if let Some(token) = extract_bearer_token(headers) {
        if crate::auth::decode_jwt(&token).is_some() {
            return Ok(());
        }
    }
    // Check allow_anonymous setting
    let result = sqlx::query("SELECT value_json FROM system_settings WHERE key = 'allow_anonymous'")
        .fetch_optional(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(row) = result {
        let value_json: String = row.get("value_json");
        let allow: bool = serde_json::from_str(&value_json).unwrap_or(true);
        if !allow {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(())
}

/// Resolve identity IDs for conversation queries.
/// - Authenticated: returns all identity IDs for the user (cross-device)
/// - Token expired but identity linked to user: falls back to user's identities
/// - Anonymous: returns just the current identity_id
async fn resolve_identity_ids(
    pool: &DbPool,
    headers: &HeaderMap,
    identity_id: &str,
) -> Vec<String> {
    // Try authenticated path
    if let Some(token) = extract_bearer_token(headers) {
        if let Some(claims) = decode_jwt(&token) {
            if let Ok(ids) = get_identity_ids_for_user(pool, &claims.sub).await {
                if !ids.is_empty() {
                    let mut result = ids;
                    if !result.contains(&identity_id.to_string()) {
                        result.push(identity_id.to_string());
                    }
                    return result;
                }
            }
        }
    }
    // Token expired or missing: check if identity is linked to a user
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT user_id FROM user_identities WHERE id = ?"
    )
    .bind(identity_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if let Some((Some(user_id),)) = row {
        if let Ok(ids) = get_identity_ids_for_user(pool, &user_id).await {
            if !ids.is_empty() {
                let mut result = ids;
                if !result.contains(&identity_id.to_string()) {
                    result.push(identity_id.to_string());
                }
                return result;
            }
        }
    }
    vec![identity_id.to_string()]
}

/// agentlite parity: frontend expects attachment items shaped as
/// {name, type, size, path, url}; Attachment::to_dict() provides exactly that.
fn attachments_to_frontend(atts: &[agent_core::Attachment]) -> Vec<serde_json::Value> {
    atts.iter().map(|a| a.to_dict()).collect()
}

/// Build messages with attachments support
fn build_messages_with_attachments(
    history_messages: Vec<crate::models::Message>,
    current_message: &str,
    attachments_meta: Option<Vec<Attachment>>,
) -> Vec<ChatCompletionMessage> {
    // Convert history to messages
    let mut messages: Vec<ChatCompletionMessage> = history_messages.iter()
        .map(|m| ChatCompletionMessage {
            role: m.role.clone(),
            content: ChatCompletionContent::Text(m.content.clone()),
        })
        .collect();

    // Add current message with attachments
    if let Some(attachments) = attachments_meta {
        let has_images = attachments.iter().any(|att| att.file_type.starts_with("image/"));
        
        if has_images {
            // Build multimodal message
            let mut multimodal_content = vec![
                MultimodalContent {
                    content_type: "text".to_string(),
                    text: Some(current_message.to_string()),
                    image_url: None,
                }
            ];

            for att in &attachments {
                if att.file_type.starts_with("image/") {
                    let file_path = get_attachment_file_path(&att.file_path);
                    if file_path.exists() {
                        if let Ok(image_data) = std::fs::read(&file_path) {
                            let image_base64 = base64::Engine::encode(
                                &base64::engine::general_purpose::STANDARD,
                                &image_data
                            );
                            multimodal_content.push(MultimodalContent {
                                content_type: "image_url".to_string(),
                                text: None,
                                image_url: Some(ImageUrlContent {
                                    url: format!("data:{};base64,{}", att.file_type, image_base64),
                                }),
                            });
                        }
                    }
                }
            }

            messages.push(ChatCompletionMessage {
                role: "user".to_string(),
                content: ChatCompletionContent::Multimodal(multimodal_content),
            });
        } else {
            // Non-image attachments: add file info to text
            let file_info = attachments.iter()
                .map(|att| format!("- {} ({}, {} bytes)", att.file_name, att.file_type, att.file_size))
                .collect::<Vec<_>>()
                .join("\n");
            
            let content_with_files = format!("{}\n\n附加文件:\n{}", current_message, file_info);
            messages.push(ChatCompletionMessage {
                role: "user".to_string(),
                content: ChatCompletionContent::Text(content_with_files),
            });
        }
    } else {
        // No attachments
        messages.push(ChatCompletionMessage {
            role: "user".to_string(),
            content: ChatCompletionContent::Text(current_message.to_string()),
        });
    }

    messages
}

/// Send message handler
async fn send_message(
    State(state): State<ChatState>,
    headers: HeaderMap,
    Json(request): Json<ChatRequest>,
) -> Result<AxumJson<MessageResponse>, StatusCode> {
    let pool = &state.db_pool;
    
    check_anonymous_allowed(pool, &headers).await?;

    // Get identity_id from header
    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    // Get or create conversation
    let conversation_id = match &request.conversation_id {
        Some(id) => {
            // agentlite parity: verify the conversation exists and belongs to
            // this identity, otherwise 404 "conversation not found"
            let identity_ids = resolve_identity_ids(pool, &headers, &identity_id).await;
            get_conversation_by_id_any(pool, id, &identity_ids).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;
            id.clone()
        }
        None => {
            let title = if request.message.len() > 30 {
                request.message.chars().take(30).collect::<String>()
            } else {
                request.message.clone()
            };
            create_conversation(pool, Some(title), &identity_id).await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        }
    };
    
    // Process attachments (if any)
    let attachments_meta = if let Some(ref attachments) = request.attachments {
        let mut parsed = Vec::new();
        for att in attachments {
            if let Ok(uploaded) = serde_json::from_value::<crate::models::UploadedFileInfo>(att.clone()) {
                let file_path = crate::config::CONFIG.chat_attachments_dir.join(&uploaded.path);
                let new_path = if file_path.parent().map_or(false, |p| p.file_name().map_or(false, |n| n == "temp")) {
                    let conversation_dir = crate::config::CONFIG.chat_attachments_dir.join(&conversation_id);
                    let _ = std::fs::create_dir_all(&conversation_dir);
                    let dest = conversation_dir.join(file_path.file_name().unwrap());
                    if let Err(e) = std::fs::rename(&file_path, &dest) {
                        error!("Failed to move file from temp: {}", e);
                        continue;
                    }
                    format!("{}/{}", conversation_id, file_path.file_name().unwrap().to_string_lossy())
                } else {
                    uploaded.path.clone()
                };
                let file_url = format!("/api/chat/files/{}", new_path);
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("url".to_string(), serde_json::json!(file_url));
                parsed.push(Attachment {
                    attachment_id: format!("att_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
                    file_name: uploaded.file_name,
                    file_path: new_path,
                    file_type: uploaded.mime_type,
                    file_size: uploaded.file_size,
                    metadata,
                    uploaded_at: chrono::Utc::now(),
                });
            } else if let Ok(base64_att) = serde_json::from_value::<crate::models::AttachmentInfo>(att.clone()) {
                match save_single_attachment_from_base64(&base64_att, &conversation_id).await {
                    Ok(saved) => parsed.push(saved),
                    Err(e) => error!("Failed to save base64 attachment: {}", e),
                }
            }
        }
        if parsed.is_empty() { None } else {
            info!("Processed {} attachments for conversation {}", parsed.len(), conversation_id);
            Some(parsed)
        }
    } else {
        None
    };
    
    // Build content_json with attachments
    let content_json = if let Some(ref atts) = attachments_meta {
        Some(serde_json::json!({
            "text": request.message,
            "attachments": attachments_to_frontend(atts),
        }).to_string())
    } else {
        None
    };
    
    // Save user message with attachments metadata
    create_message(pool, &conversation_id, "user", &request.message, content_json.as_deref()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Auto-name conversation if title is empty
    let _ = auto_name_conversation_if_empty(pool, &conversation_id, &request.message).await;
    
    // Get history
    let history = get_messages_by_conversation(pool, &conversation_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Build messages with attachments
    let messages = build_messages_with_attachments(history, &request.message, attachments_meta);
    
    // Call LLM
    let response_content = call_llm(
        messages,
        request.temperature,
        request.top_p,
        request.max_tokens,
        request.enable_thinking,
        request.chat_template_args,
    ).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Save assistant response
    let msg_id = create_message(pool, &conversation_id, "assistant", &response_content, None).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(MessageResponse {
        id: msg_id,
        role: "assistant".to_string(),
        content: response_content,
        metadata: None,
        created_at: chrono::Utc::now(),
    }))
}

/// Stream message handler
async fn stream_message(
    State(state): State<ChatState>,
    headers: HeaderMap,
    Json(request): Json<ChatStreamRequest>,
) -> impl IntoResponse {
    let pool = state.db_pool.clone();
    let request_clone = request.clone();
    let identity_id = extract_identity_id(&headers).unwrap_or_default();
    
    // Create channel for streaming
    let (tx, rx) = tokio::sync::mpsc::channel(100);
    
    // Spawn task to handle everything
    tokio::spawn(async move {
        if identity_id.is_empty() {
            let _ = tx.send(SseEvent {
                event_type: "error".to_string(),
                text: Some("X-Identity-Id header is required".to_string()),
            }).await;
            return;
        }
        
        // Get or create conversation
        let conversation_id: String = match request_clone.conversation_id {
            Some(id_str) => {
                // agentlite parity: verify the conversation exists and belongs
                // to this identity, otherwise 404 "conversation not found"
                let identity_ids = resolve_identity_ids(&pool, &headers, &identity_id).await;
                match get_conversation_by_id_any(&pool, &id_str, &identity_ids).await {
                    Ok(Some(_)) => id_str,
                    _ => {
                        let _ = tx.send(SseEvent {
                            event_type: "error".to_string(),
                            text: Some("conversation not found".to_string()),
                        }).await;
                        return;
                    }
                }
            }
            None => {
                let title = if request_clone.message.len() > 30 {
                    request_clone.message.chars().take(30).collect::<String>()
                } else {
                    request_clone.message.clone()
                };
                match create_conversation(&pool, Some(title), &identity_id).await {
                    Ok(id) => id,
                    Err(_) => {
                        let _ = tx.send(SseEvent {
                            event_type: "error".to_string(),
                            text: Some("Failed to create conversation".to_string()),
                        }).await;
                        return;
                    }
                }
            }
        };
        
        // Process attachments (if any)
        let attachments_meta = if let Some(ref attachments) = request_clone.attachments {
            // Parse attachments - handle both AttachmentInfo and UploadedFileInfo
            let mut parsed_attachments: Vec<Attachment> = Vec::new();
            for att in attachments {
                // Try to parse as UploadedFileInfo first
                if let Ok(uploaded) = serde_json::from_value::<crate::models::UploadedFileInfo>(att.clone()) {
                    // Handle uploaded file - move from temp to conversation directory
                    let file_path = crate::config::CONFIG.chat_attachments_dir.join(&uploaded.path);
                    
                    let new_path = if file_path.parent().map_or(false, |p| p.file_name().map_or(false, |n| n == "temp")) {
                        // Move from temp to conversation directory
                        let conversation_dir = crate::config::CONFIG.chat_attachments_dir.join(&conversation_id);
                        let _ = std::fs::create_dir_all(&conversation_dir);
                        let dest = conversation_dir.join(file_path.file_name().unwrap());
                        
                        if let Err(e) = std::fs::rename(&file_path, &dest) {
                            error!("Failed to move file from temp: {}", e);
                            continue;
                        }
                        
                        info!("Moved file from temp to conversation: {}", uploaded.file_name);
                        format!("{}/{}", conversation_id, file_path.file_name().unwrap().to_string_lossy())
                    } else {
                        // File already in conversation directory
                        uploaded.path.clone()
                    };
                    
                    // Generate URL
                    let file_url = format!("/api/chat/files/{}", new_path);
                    
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("url".to_string(), serde_json::json!(file_url));
                    
                    parsed_attachments.push(Attachment {
                        attachment_id: format!("att_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
                        file_name: uploaded.file_name,
                        file_path: new_path,
                        file_type: uploaded.mime_type,
                        file_size: uploaded.file_size,
                        metadata,
                        uploaded_at: chrono::Utc::now(),
                    });
                } else if let Ok(base64_att) = serde_json::from_value::<crate::models::AttachmentInfo>(att.clone()) {
                    // Handle base64 attachment
                    match save_single_attachment_from_base64(&base64_att, &conversation_id).await {
                        Ok(saved) => {
                            parsed_attachments.push(saved);
                        }
                        Err(e) => {
                            error!("Failed to save base64 attachment: {}", e);
                        }
                    }
                }
            }
            
            if !parsed_attachments.is_empty() {
                info!("Processed {} attachments for conversation {}", parsed_attachments.len(), conversation_id);
                Some(parsed_attachments)
            } else {
                None
            }
        } else {
            None
        };
        
        // Build content_json with attachments
        let content_json = if let Some(ref atts) = attachments_meta {
            Some(serde_json::json!({
                "text": request_clone.message,
                "attachments": attachments_to_frontend(atts),
            }).to_string())
        } else {
            None
        };
        
        // Save user message with attachments metadata
        if let Err(_) = create_message(&pool, &conversation_id, "user", &request_clone.message, content_json.as_deref()).await {
            let _ = tx.send(SseEvent {
                event_type: "error".to_string(),
                text: Some("Failed to save message".to_string()),
            }).await;
            return;
        }
        
        // Auto-name conversation if title is empty
        let _ = auto_name_conversation_if_empty(&pool, &conversation_id, &request_clone.message).await;
        
        // 首先发送完整的用户消息（包含附件URL），让前端立即显示
        if let Some(ref atts) = attachments_meta {
            let user_msg = serde_json::json!({
                "type": "user_message",
                "message": {
                    "role": "user",
                    "content": request_clone.message,
                    "conversation_id": conversation_id,
                    "attachments": attachments_to_frontend(atts),
                }
            });
            if let Err(e) = tx.send(SseEvent {
                event_type: "data".to_string(),
                text: Some(serde_json::to_string(&user_msg).unwrap_or_default()),
            }).await {
                error!("Failed to send user_message event: {}", e);
            }
        } else {
            let user_msg = serde_json::json!({
                "type": "user_message",
                "message": {
                    "role": "user",
                    "content": request_clone.message,
                    "conversation_id": conversation_id,
                }
            });
            if let Err(e) = tx.send(SseEvent {
                event_type: "data".to_string(),
                text: Some(serde_json::to_string(&user_msg).unwrap_or_default()),
            }).await {
                error!("Failed to send user_message event: {}", e);
            }
        }
        
        // Get history
        let history = match get_messages_by_conversation(&pool, &conversation_id).await {
            Ok(msgs) => msgs,
            Err(_) => {
                let _ = tx.send(SseEvent {
                    event_type: "error".to_string(),
                    text: Some("Failed to get history".to_string()),
                }).await;
                return;
            }
        };
        
        // Build messages with attachments
        let messages = build_messages_with_attachments(history, &request_clone.message, attachments_meta);
        
        // Call LLM streaming
        match call_llm_stream(
            messages,
            request_clone.temperature,
            request_clone.top_p,
            request_clone.max_tokens,
            request_clone.enable_thinking,
            request_clone.chat_template_args,
            tx.clone(),
        ).await {
            Ok(full_content) => {
                // Save assistant response
                let _ = create_message(&pool, &conversation_id, "assistant", &full_content, None).await;
            }
            Err(e) => {
                let _ = tx.send(SseEvent {
                    event_type: "error".to_string(),
                    text: Some(format!("LLM error: {}", e)),
                }).await;
            }
        }
    });
    
    // Convert receiver to SSE stream
    let stream = ReceiverStream::new(rx).map(|event: SseEvent| -> Result<Event, Infallible> {
        let data = serde_json::to_string(&event).unwrap_or_default();
        // Event::data() will automatically add "data: " prefix and "\n\n" suffix
        Ok(Event::default().data(data))
    });
    
    Sse::new(stream)
}

/// 使用 AgentChatLoop 处理流式聊天(新架构)
async fn stream_message_with_agent_loop(
    State(state): State<ChatState>,
    headers: HeaderMap,
    Json(request): Json<ChatStreamRequest>,
) -> impl IntoResponse {
    let pool = state.db_pool.clone();
    let manager = state.loop_manager.clone();
    let identity_id = extract_identity_id(&headers).unwrap_or_default();
    let request_clone = request.clone();
    
    // Create channel for streaming ChatEvent
    let (event_tx, event_rx) = tokio::sync::mpsc::channel::<ChatEvent>(100);
    
    // Spawn task to handle everything
    tokio::spawn(async move {
        if identity_id.is_empty() {
            let _ = event_tx.send(ChatEvent::Error { 
                message: "X-Identity-Id header is required".to_string() 
            }).await;
            return;
        }

        // Check anonymous access (for non-authenticated users)
        if extract_bearer_token(&headers).is_none() {
            let result = sqlx::query("SELECT value_json FROM system_settings WHERE key = 'allow_anonymous'")
                .fetch_optional(&pool)
                .await;
            if let Ok(Some(row)) = result {
                let value_json: String = row.get("value_json");
                let allow: bool = serde_json::from_str(&value_json).unwrap_or(true);
                if !allow {
                    let _ = event_tx.send(ChatEvent::Error {
                        message: "匿名使用已被禁用，请先登录".to_string()
                    }).await;
                    return;
                }
            }
        }
        
        // Get or create conversation
        let conversation_id: String = match request_clone.conversation_id {
            Some(id_str) => {
                // agentlite parity: verify the conversation exists and belongs
                // to this identity, otherwise 404 "conversation not found"
                let identity_ids = resolve_identity_ids(&pool, &headers, &identity_id).await;
                match get_conversation_by_id_any(&pool, &id_str, &identity_ids).await {
                    Ok(Some(_)) => id_str,
                    _ => {
                        let _ = event_tx.send(ChatEvent::Error {
                            message: "conversation not found".to_string()
                        }).await;
                        return;
                    }
                }
            }
            None => {
                let title = if request_clone.message.len() > 30 {
                    request_clone.message.chars().take(30).collect::<String>()
                } else {
                    request_clone.message.clone()
                };
                match create_conversation(&pool, Some(title), &identity_id).await {
                    Ok(id) => id,
                    Err(_) => {
                        let _ = event_tx.send(ChatEvent::Error { 
                            message: "Failed to create conversation".to_string() 
                        }).await;
                        return;
                    }
                }
            }
        };
        
        // Process attachments (if any) — 在保存用户消息之前处理，以便将附件元数据写入 content_json
        let attachments = if let Some(ref atts) = request_clone.attachments {
            let mut parsed = Vec::new();
            for att in atts {
                if let Ok(uploaded) = serde_json::from_value::<crate::models::UploadedFileInfo>(att.clone()) {
                    let file_path = crate::config::CONFIG.chat_attachments_dir.join(&uploaded.path);
                    let new_path = if file_path.parent().map_or(false, |p| p.file_name().map_or(false, |n| n == "temp")) {
                        let conversation_dir = crate::config::CONFIG.chat_attachments_dir.join(&conversation_id);
                        let _ = std::fs::create_dir_all(&conversation_dir);
                        let dest = conversation_dir.join(file_path.file_name().unwrap());
                        if let Err(e) = std::fs::rename(&file_path, &dest) {
                            error!("Failed to move file from temp: {}", e);
                            continue;
                        }
                        format!("{}/{}", conversation_id, file_path.file_name().unwrap().to_string_lossy())
                    } else {
                        uploaded.path.clone()
                    };
                    let file_url = format!("/api/chat/files/{}", new_path);
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("url".to_string(), serde_json::json!(file_url));
                    parsed.push(Attachment {
                        attachment_id: format!("att_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
                        file_name: uploaded.file_name,
                        file_path: new_path,
                        file_type: uploaded.mime_type,
                        file_size: uploaded.file_size,
                        metadata,
                        uploaded_at: chrono::Utc::now(),
                    });
                } else if let Ok(base64_att) = serde_json::from_value::<crate::models::AttachmentInfo>(att.clone()) {
                    match save_single_attachment_from_base64(&base64_att, &conversation_id).await {
                        Ok(saved) => parsed.push(saved),
                        Err(e) => error!("Failed to save base64 attachment: {}", e),
                    }
                }
            }
            if parsed.is_empty() { None } else {
                info!("Processed {} attachments for conversation {}", parsed.len(), conversation_id);
                Some(parsed)
            }
        } else {
            None
        };

        // Build content_json with attachments (与 Python 对齐：存储 text + attachments)
        let user_content_json = if let Some(ref atts) = attachments {
            Some(serde_json::json!({
                "text": request_clone.message,
                "attachments": attachments_to_frontend(atts),
            }).to_string())
        } else {
            None
        };

        // 预生成消息 ID，前后端一致
        let user_message_id = crate::database::gen_prefix_time_random_id("ms", 8);
        let streaming_message_id = crate::database::gen_prefix_time_random_id("ms", 8);

        // Save user message with pre-generated ID
        if let Err(_) = crate::database::create_message_with_id(&pool, &conversation_id, "user", &request_clone.message, user_content_json.as_deref(), Some(&user_message_id)).await {
            let _ = event_tx.send(ChatEvent::Error { 
                message: "Failed to save message".to_string() 
            }).await;
            return;
        }
        
        // Auto-name conversation if title is empty
        let _ = auto_name_conversation_if_empty(&pool, &conversation_id, &request_clone.message).await;
        
        // 通过 ChatLoopManager 获取或创建 chat loop（复用 history）
        let scope = ChatScope {
            // agentlite uses "conv_{id}" subjects; core.rs branches on this
            // prefix to derive the conversation_id for work dirs / outputs
            subject: format!("conv_{}", conversation_id),
            identity_id: Some(identity_id.clone()),
            user_id: None,
        };

        let llm_config = LLMConfig {
            base_url: get_llm_base_url(),
            api_key: get_llm_api_key(),
            model: Some(get_llm_model()),
            debug: crate::config::CONFIG.llm_debug,
            max_retries: crate::config::CONFIG.llm_max_retries,
            retry_min_wait_secs: crate::config::CONFIG.llm_retry_min_wait as u64,
            retry_max_wait_secs: crate::config::CONFIG.llm_retry_max_wait as u64,
            max_seq_len: crate::config::CONFIG.llm_max_seq_len,
        };

        // 标记 in_flight 并获取 loop
        manager.mark_in_flight(&conversation_id, 1).await;
        let chat_loop = manager.get_loop(scope, Some(llm_config)).await;
        manager.touch(&conversation_id).await;

        // agentlite parity: reject a new stream while one is already running
        if *chat_loop.running.read().await {
            let _ = event_tx.send(ChatEvent::Error {
                message: "该对话已有任务正在执行，请等待完成后再发送新消息".to_string()
            }).await;
            manager.mark_in_flight(&conversation_id, -1).await;
            return;
        }

        // 预设 streaming_message_id，loop 内不再重新生成
        *chat_loop.streaming_message_id.write().await = Some(streaming_message_id.clone());

        // 发送 init 事件，前端不再自编 ID
        let _ = event_tx.send(ChatEvent::Init {
            user_message_id: user_message_id.clone(),
            streaming_message_id: streaming_message_id.clone(),
        }).await;

        // 调用 chat_stream
        let start = std::time::Instant::now();
        match chat_loop.chat_stream(
            &request_clone.message,
            None, // system_prompt
            request_clone.temperature.unwrap_or(0.0),
            request_clone.top_p.unwrap_or(0.9),
            attachments,
        ).await {
            Ok(mut rx) => {
                let mut full_content = String::new();
                let mut collected_attachments: Vec<serde_json::Value> = Vec::new();

                while let Some(event) = rx.recv().await {
                    // agentlite parity: record events for reconnect/snapshot replay
                    chat_loop.event_buffer.write().await.push(event.clone());
                    if let ChatEvent::Token { text } = &event {
                        full_content.push_str(text);
                    }
                    if let ChatEvent::Attachments { attachments } = &event {
                        collected_attachments.extend(attachments.clone());
                    }
                    if matches!(event, ChatEvent::Done { .. }) {
                        // done
                    }
                    let _ = event_tx.send(event).await;
                }

                // Persist assistant message to SQLite (agentlite parity: the
                // AgentChatLoop only writes the JSONL log; without this the
                // assistant reply disappears from history after refresh).
                if !full_content.trim().is_empty() {
                    let _ = create_message_with_id(
                        &state.db_pool,
                        &conversation_id,
                        "assistant",
                        &full_content,
                        None,
                        Some(&streaming_message_id),
                    ).await;
                }
            }
            Err(e) => {
                error!("AgentChatLoop error: {}", e);
                let _ = event_tx.send(ChatEvent::Error { 
                    message: format!("Chat error: {}", e) 
                }).await;
            }
        }

        // 标记 in_flight 完成
        manager.mark_in_flight(&conversation_id, -1).await;
    });
    
    // Convert ChatEvent receiver to SSE stream
    let stream = ReceiverStream::new(event_rx).map(|event: ChatEvent| -> Result<Event, Infallible> {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Ok(Event::default().data(data))
    });
    
    Sse::new(stream)
}

/// WebSocket 聊天处理 — 多路复用：一个 identity 一个连接，消息按 conversation_id 路由
async fn chat_websocket(
    ws: WebSocketUpgrade,
    State(state): State<ChatState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let identity_id = params.get("identity_id").cloned().unwrap_or_default();
    ws.on_upgrade(move |socket| handle_ws(socket, state, identity_id))
}

/// 注入 conversation_id 到序列化的事件 JSON 中，供前端多路复用路由
fn inject_cid(payload: &str, conversation_id: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    if let Some(obj) = value.as_object_mut() {
        obj.insert("conversation_id".to_string(), serde_json::json!(conversation_id));
    }
    serde_json::to_string(&value).unwrap_or_default()
}

/// 快捷发送带 conversation_id 的文本事件
fn send_event(payload: String) -> Message {
    Message::Text(payload.into())
}

async fn handle_ws(socket: WebSocket, state: ChatState, identity_id: String) {
    // Validate/create identity
    {
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM user_identities WHERE id = ?")
            .bind(&identity_id)
            .fetch_optional(&state.db_pool)
            .await
            .ok()
            .flatten();
        if existing.is_none() {
            let _ = sqlx::query("INSERT OR IGNORE INTO user_identities (id) VALUES (?)")
                .bind(&identity_id)
                .execute(&state.db_pool)
                .await;
        }
    }

    let (mut ws_tx, mut ws_rx) = socket.split();
    let ws_tx = Arc::new(tokio::sync::Mutex::new(ws_tx));

    while let Some(msg) = ws_rx.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) | Err(_) => break,
            _ => continue,
        };

        let data: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let msg_type = data.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "send" => {
                let tx = ws_tx.clone();
                let st = state.clone();
                let idt = identity_id.clone();
                tokio::spawn(async move {
                    handle_ws_send(tx, st, idt, data).await;
                });
            }
            "sync" => {
                let tx = ws_tx.clone();
                let st = state.clone();
                tokio::spawn(async move {
                    handle_ws_sync(tx, st, data).await;
                });
            }
            _ => {
                let _ = ws_tx.lock().await.send(send_event(json!({
                    "type": "error",
                    "message": format!("unknown message type: {}", msg_type),
                }).to_string().into())).await;
            }
        }
    }
}

type WsTx = Arc<tokio::sync::Mutex<futures::stream::SplitSink<WebSocket, Message>>>;

async fn ws_send_err(tx: &WsTx, cid: &str, msg: &str) {
    let _ = tx.lock().await.send(send_event(inject_cid(
        &json!({"type": "error", "message": msg}).to_string(), cid
    ))).await;
}

async fn handle_ws_send(tx: WsTx, state: ChatState, identity_id: String, data: serde_json::Value) {

    let message = data.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let conversation_id = data.get("conversation_id").and_then(|v| v.as_str()).map(|s| s.to_string());

    if message.is_empty() {
        ws_send_err(&tx, conversation_id.as_deref().unwrap_or(""), "message is required").await;
        return;
    }

    // ── DB: get or create conversation ──
    let conversation_id = match conversation_id {
        Some(id) => {
            let identity_ids = resolve_identity_ids(&state.db_pool, &HeaderMap::new(), &identity_id).await;
            match get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await {
                Ok(Some(_)) => id,
                _ => { ws_send_err(&tx, &id, "conversation not found").await; return; }
            }
        }
        None => {
            let title = if message.chars().count() > 30 { message.chars().take(30).collect::<String>() } else { message.to_string() };
            match create_conversation(&state.db_pool, Some(title), &identity_id).await {
                Ok(id) => id,
                Err(_) => { ws_send_err(&tx, "", "Failed to create conversation").await; return; }
            }
        }
    };

    // ── Process attachments ──
    let attachments_raw = data.get("attachments").cloned();
    let attachments = if let Some(ref atts) = attachments_raw {
        let atts_arr = atts.as_array().cloned().unwrap_or_default();
        let mut parsed = Vec::new();
        for att in &atts_arr {
            if let Ok(uploaded) = serde_json::from_value::<crate::models::UploadedFileInfo>(att.clone()) {
                let file_path = crate::config::CONFIG.chat_attachments_dir.join(&uploaded.path);
                let new_path = if file_path.parent().map_or(false, |p| p.file_name().map_or(false, |n| n == "temp")) {
                    let conversation_dir = crate::config::CONFIG.chat_attachments_dir.join(&conversation_id);
                    let _ = std::fs::create_dir_all(&conversation_dir);
                    let dest = conversation_dir.join(file_path.file_name().unwrap());
                    if let Err(e) = std::fs::rename(&file_path, &dest) { error!("{}", e); continue; }
                    format!("{}/{}", conversation_id, file_path.file_name().unwrap().to_string_lossy())
                } else { uploaded.path.clone() };
                let file_url = format!("/api/chat/files/{}", new_path);
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("url".to_string(), serde_json::json!(file_url));
                parsed.push(Attachment {
                    attachment_id: format!("att_{}", uuid::Uuid::new_v4().to_string()[..8].to_string()),
                    file_name: uploaded.file_name, file_path: new_path,
                    file_type: uploaded.mime_type, file_size: uploaded.file_size,
                    metadata, uploaded_at: chrono::Utc::now(),
                });
            } else if let Ok(base64_att) = serde_json::from_value::<crate::models::AttachmentInfo>(att.clone()) {
                // agentlite parity: WebSocket messages may carry base64
                // attachments (AttachmentInfo with a `data` field)
                match crate::attachment_handler::save_single_attachment_from_base64(&base64_att, &conversation_id).await {
                    Ok(saved) => parsed.push(saved),
                    Err(e) => error!("Failed to save base64 attachment: {}", e),
                }
            }
        }
        if parsed.is_empty() { None } else { Some(parsed) }
    } else { None };

    // ── Save user message ──
    let user_content_json = if let Some(ref atts) = attachments {
        Some(serde_json::json!({ "text": message, "attachments": attachments_to_frontend(atts) }).to_string())
    } else { None };

    let user_message_id = crate::database::gen_prefix_time_random_id("ms", 8);
    let streaming_message_id = crate::database::gen_prefix_time_random_id("ms", 8);

    if let Err(_) = crate::database::create_message_with_id(&state.db_pool, &conversation_id, "user", message, user_content_json.as_deref(), Some(&user_message_id)).await {
        ws_send_err(&tx, &conversation_id, "Failed to save message").await;
        return;
    }
    let _ = auto_name_conversation_if_empty(&state.db_pool, &conversation_id, message).await;

    // ── Get or create chat loop ──
    let scope = ChatScope {
        // agentlite uses "conv_{id}" subjects; core.rs branches on this prefix
        subject: format!("conv_{}", conversation_id),
        identity_id: Some(identity_id.clone()),
        user_id: None,
    };
    let llm_config = LLMConfig {
        base_url: get_llm_base_url(), api_key: get_llm_api_key(),
        model: Some(get_llm_model()), debug: crate::config::CONFIG.llm_debug,
        max_retries: 3, retry_min_wait_secs: 1, retry_max_wait_secs: 10,
        max_seq_len: crate::config::CONFIG.llm_max_seq_len,
    };

    state.loop_manager.mark_in_flight(&conversation_id, 1).await;
    let chat_loop = state.loop_manager.get_loop(scope, Some(llm_config)).await;
    state.loop_manager.touch(&conversation_id).await;

    if *chat_loop.running.read().await {
        state.loop_manager.mark_in_flight(&conversation_id, -1).await;
        ws_send_err(&tx, &conversation_id, "该对话已有任务正在执行，请等待完成后再发送新消息").await;
        return;
    }

    *chat_loop.streaming_message_id.write().await = Some(streaming_message_id.clone());

    let cid = &conversation_id;

    // ── Send init event ──
    let _ = tx.lock().await.send(send_event(inject_cid(
        &json!({
            "type": "init",
            "user_message_id": user_message_id,
            "streaming_message_id": streaming_message_id,
        }).to_string(), cid
    ))).await;

    // ── Send user_message event ──
    let msg_body = if let Some(ref atts) = attachments {
        json!({ "role": "user", "content": message, "attachments": attachments_to_frontend(atts) })
    } else {
        json!({ "role": "user", "content": message })
    };
    let _ = tx.lock().await.send(send_event(inject_cid(
        &json!({
            "type": "user_message",
            "user_message_id": user_message_id,
            "message": msg_body,
        }).to_string(), cid
    ))).await;

    // ── Stream via AgentChatLoop ──
    match chat_loop.chat_stream(message, None, 0.0, 0.9, attachments).await {
        Ok(mut rx) => {
            while let Some(event) = rx.recv().await {
                // agentlite parity: record events for reconnect/snapshot replay
                chat_loop.event_buffer.write().await.push(event.clone());
                let payload = serde_json::to_string(&event).unwrap_or_default();
                let _ = tx.lock().await.send(send_event(inject_cid(&payload, cid))).await;
            }
        }
        Err(e) => {
            error!("AgentChatLoop error: {}", e);
            let _ = tx.lock().await.send(send_event(inject_cid(
                &json!({"type": "error", "message": format!("Chat error: {}", e)}).to_string(), cid
            ))).await;
        }
    }

    state.loop_manager.mark_in_flight(&conversation_id, -1).await;
}

async fn handle_ws_sync(tx: WsTx, state: ChatState, data: serde_json::Value) {
    let conversation_id = match data.get("conversation_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => { ws_send_err(&tx, "", "conversation_id required for sync").await; return; }
    };

    let scope = ChatScope {
        subject: format!("conv_{}", conversation_id),
        identity_id: None,
        user_id: None,
    };
    let chat_loop = state.loop_manager.get_loop(scope, None).await;

    if !*chat_loop.running.read().await {
        let streaming_id = chat_loop.streaming_message_id.read().await.clone();
        let _ = tx.lock().await.send(send_event(inject_cid(
            &json!({
                "type": "idle",
                "streaming_message_id": streaming_id,
            }).to_string(), &conversation_id
        ))).await;
        return;
    }

    let cid = &conversation_id;

    // ── Build snapshot ──
    let buffer = chat_loop.event_buffer.read().await;
    let mut full_content = String::new();
    let mut step_statuses: std::collections::HashMap<String, agent_core::StepStatusInfo> = std::collections::HashMap::new();
    let mut attachments: Vec<serde_json::Value> = Vec::new();

    for event in buffer.iter() {
        match event {
            ChatEvent::Token { text } => full_content.push_str(text),
            ChatEvent::StepStatus { status } => { step_statuses.insert(status.id.clone(), status.clone()); }
            ChatEvent::Attachments { attachments: atts } => { attachments.extend(atts.clone()); }
            _ => {}
        }
    }
    let is_queued = *chat_loop.is_queued.read().await;
    let queue_position = *chat_loop.queue_position.read().await;
    let streaming_id = chat_loop.streaming_message_id.read().await.clone();
    drop(buffer);

    step_statuses.retain(|_, v| v.visible.unwrap_or(true) && v.auto_hide != Some(true));

    let _ = tx.lock().await.send(send_event(inject_cid(
        &json!({
            "type": "snapshot",
            "content": full_content,
            "step_statuses": step_statuses,
            "attachments": attachments,
            "is_queued": is_queued,
            "queue_position": queue_position,
            "streaming_message_id": streaming_id,
        }).to_string(), cid
    ))).await;

    // ── Poll incremental events ──
    let mut last_idx = chat_loop.event_buffer.read().await.len();
    while *chat_loop.running.read().await {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let buf = chat_loop.event_buffer.read().await;
        let current = buf[last_idx..].to_vec();
        for event in current {
            let payload = serde_json::to_string(&event).unwrap_or_default();
            let _ = tx.lock().await.send(send_event(inject_cid(&payload, cid))).await;
        }
        last_idx = buf.len();
    }

    // ── Send done ──
    let _ = tx.lock().await.send(send_event(inject_cid(
        &serde_json::to_string(&ChatEvent::Done {
            content: String::new(),
            elapsed_seconds: 0.0,
            elapsed_display: String::new(),
            completed_at: Utc::now().to_rfc3339(),
        }).unwrap_or_default(), cid
    ))).await;

    // ── Persist assistant message to SQLite (agentlite parity: without this,
    //    history is lost on refresh because only the JSONL log is written) ──
    if !full_content.trim().is_empty() {
        let msg_id = chat_loop
            .streaming_message_id
            .read()
            .await
            .clone()
            .unwrap_or_else(|| crate::database::gen_prefix_time_random_id("ms", 8));
        let _ = create_message_with_id(
            &state.db_pool,
            &conversation_id,
            "assistant",
            &full_content,
            None,
            Some(&msg_id),
        ).await;
    }
}

/// Call LLM service (non-streaming)
/// Rough token count estimate using character-based heuristic.
fn estimate_tokens_chat(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    std::cmp::max(1, text.chars().count() * 2 / 5)
}

fn estimate_message_tokens_chat(msg: &ChatCompletionMessage) -> usize {
    let content_tokens = match &msg.content {
        ChatCompletionContent::Text(t) => estimate_tokens_chat(t),
        ChatCompletionContent::Multimodal(parts) => {
            parts.iter()
                .filter_map(|p| p.text.as_deref())
                .map(|t| estimate_tokens_chat(t))
                .sum()
        }
    };
    4 + content_tokens
}

fn truncate_completion_messages(
    messages: Vec<ChatCompletionMessage>,
    max_seq_len: usize,
    output_budget: usize,
) -> Vec<ChatCompletionMessage> {
    let budget = max_seq_len.saturating_sub(output_budget);
    let total: usize = messages.iter().map(|m| estimate_message_tokens_chat(m)).sum();
    if total <= budget {
        return messages;
    }

    // Collect system messages first (before borrowing for non_system)
    let system_msgs: Vec<ChatCompletionMessage> = messages.iter()
        .filter(|m| m.role == "system")
        .cloned()
        .collect();
    let system_tokens: usize = system_msgs.iter()
        .map(|m| estimate_message_tokens_chat(m))
        .sum();
    let available = budget.saturating_sub(system_tokens);

    let non_system: Vec<&ChatCompletionMessage> = messages.iter()
        .filter(|m| m.role != "system").collect();
    let mut kept: Vec<&ChatCompletionMessage> = Vec::new();
    let mut kept_tokens = 0usize;
    for msg in non_system.iter().rev() {
        let mt = estimate_message_tokens_chat(msg);
        if kept_tokens + mt <= available {
            kept.push(msg);
            kept_tokens += mt;
        } else { break; }
    }
    kept.reverse();

    let dropped = non_system.len() - kept.len();
    if dropped > 0 {
        tracing::warn!(
            "Truncated {} messages ({} -> {} tokens) for max_seq_len={}",
            dropped, total, system_tokens + kept_tokens, max_seq_len
        );
    }

    let mut result = system_msgs;
    result.extend(kept.into_iter().cloned());
    result
}

pub async fn call_llm(
    messages: Vec<ChatCompletionMessage>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<i32>,
    enable_thinking: Option<bool>,
    chat_template_args: Option<Value>,
) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    
    // Build extra_body with chat_template_args
    let enable_thinking_val = enable_thinking.unwrap_or(false);
    let mut extra_body = json!({
        "enable_thinking": enable_thinking_val,
        "chat_template_kwargs": {"enable_thinking": enable_thinking_val}
    });
    
    // Merge custom chat_template_args if provided
    if let Some(args) = chat_template_args {
        if let Some(obj) = extra_body.as_object_mut() {
            if let Some(user_args) = args.as_object() {
                for (key, value) in user_args {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }
    }
    
    // Token-aware truncation
    let max_seq_len = crate::config::CONFIG.llm_max_seq_len;
    let output_budget = std::cmp::max(2048, max_seq_len / 5);
    let messages = truncate_completion_messages(messages, max_seq_len, output_budget);

    let request = ChatCompletionRequest {
        model: get_llm_model().to_string(),
        messages,
        temperature: temperature.or(Some(0.6)),
        top_p: top_p.or(Some(0.9)),
        max_tokens: None,
        stream: Some(false),
        extra_body: Some(extra_body),
    };
    
    // LLM debug mode: log request
    if crate::config::CONFIG.llm_debug {
        tracing::debug!("LLM 请求: {}", serde_json::to_string_pretty(&request).unwrap_or_default());
    }
    
    let response = client
        .post(format!("{}/chat/completions", get_llm_base_url()))
        .json(&request)
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("LLM request failed: {}", response.status()));
    }
    
    let completion: ChatCompletionResponse = response.json().await?;
    
    // LLM debug mode: log response
    if crate::config::CONFIG.llm_debug {
        if let Some(choice) = completion.choices.first() {
            let content_preview = if choice.message.content.len() > 500 {
                format!("{}...", &choice.message.content.chars().take(500).collect::<String>())
            } else {
                choice.message.content.clone()
            };
            tracing::debug!("LLM 响应 ({} 字符): {}", choice.message.content.len(), content_preview);
        }
    }
    
    if let Some(choice) = completion.choices.first() {
        Ok(choice.message.content.trim().to_string())
    } else {
        Err(anyhow::anyhow!("No response from LLM"))
    }
}

/// Call LLM service with streaming
async fn call_llm_stream(
    messages: Vec<ChatCompletionMessage>,
    temperature: Option<f32>,
    top_p: Option<f32>,
    max_tokens: Option<i32>,
    enable_thinking: Option<bool>,
    chat_template_args: Option<Value>,
    tx: tokio::sync::mpsc::Sender<SseEvent>,
) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    
    // Build extra_body with chat_template_args
    let enable_thinking_val = enable_thinking.unwrap_or(false);
    let mut extra_body = json!({
        "enable_thinking": enable_thinking_val,
        "chat_template_kwargs": {"enable_thinking": enable_thinking_val}
    });
    
    // Merge custom chat_template_args if provided
    if let Some(args) = chat_template_args {
        if let Some(obj) = extra_body.as_object_mut() {
            if let Some(user_args) = args.as_object() {
                for (key, value) in user_args {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }
    }
    
    // Token-aware truncation
    let max_seq_len = crate::config::CONFIG.llm_max_seq_len;
    let output_budget = std::cmp::max(2048, max_seq_len / 5);
    let messages = truncate_completion_messages(messages, max_seq_len, output_budget);

    let request = ChatCompletionRequest {
        model: get_llm_model().to_string(),
        messages,
        temperature: temperature.or(Some(0.6)),
        top_p: top_p.or(Some(0.9)),
        max_tokens: None,
        stream: Some(true),
        extra_body: Some(extra_body),
    };
    
    let response = client
        .post(format!("{}/chat/completions", get_llm_base_url()))
        .json(&request)
        .send()
        .await?;
    
    if !response.status().is_success() {
        return Err(anyhow::anyhow!("LLM request failed: {}", response.status()));
    }
    
    // Read streaming response
    let mut stream = response.bytes_stream();
    let mut full_content = String::new();
    let mut buffer = String::new();
    
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);
        
        // Process SSE lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();
            
            if line.starts_with("data: ") {
                let data = &line[6..];
                
                if data == "[DONE]" {
                    continue;
                }
                
                // Parse chunk
                if let Ok(chunk_data) = serde_json::from_str::<StreamChunk>(data) {
                    if let Some(choice) = chunk_data.choices.first() {
                        if let Some(content) = &choice.delta.content {
                            full_content.push_str(content);
                            
                            // Send token event
                            let _ = tx.send(SseEvent {
                                event_type: "token".to_string(),
                                text: Some(content.clone()),
                            }).await;
                        }
                        
                        // Check for finish
                        if choice.finish_reason.is_some() {
                            break;
                        }
                    }
                }
            }
        }
    }
    
    // Send done event
    let _ = tx.send(SseEvent {
        event_type: "done".to_string(),
        text: None,
    }).await;
    
    Ok(full_content)
}

/// List conversations handler (requires identity_id query param)
async fn list_conversations(
    State(state): State<ChatState>,
    headers: HeaderMap,
) -> Result<AxumJson<Vec<ConversationResponse>>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;

    // Collect conversations from all resolved identity IDs
    let mut all_conversations = Vec::new();
    for id in &identity_ids {
        if let Ok(convs) = get_conversations_by_identity(&state.db_pool, id).await {
            all_conversations.extend(convs);
        }
    }
    // De-duplicate by id
    all_conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let mut seen = std::collections::HashSet::new();
    all_conversations.retain(|c| seen.insert(c.id.clone()));

    // Check running status for each conversation
    let mut responses = Vec::new();
    for conv in all_conversations {
        let running = state.loop_manager.is_running_by_subject(&conv.id).await;
        let mut resp = ConversationResponse::from(conv);
        resp.running = running;
        responses.push(resp);
    }

    Ok(AxumJson(responses))
}

/// Create conversation handler
async fn create_conversation_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    Json(request): Json<ConversationCreate>,
) -> Result<AxumJson<ConversationResponse>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    let id = create_conversation(&state.db_pool, request.title, &identity_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Get the created conversation
    let conversations = get_conversations_by_identity(&state.db_pool, &identity_id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let conversation = conversations.into_iter()
        .find(|c| c.id == id)
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(ConversationResponse::from(conversation)))
}

/// Get messages handler
async fn get_messages_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<AxumJson<Vec<MessageResponse>>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;

    // Verify ownership
    get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let messages = get_messages_by_conversation(&state.db_pool, &id).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(
        messages.into_iter()
            .map(MessageResponse::from)
            .collect()
    ))
}

/// Check if a conversation has a running task, and return the streaming message ID
async fn get_conversation_running_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<AxumJson<serde_json::Value>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;

    // Verify ownership
    get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let scope = ChatScope {
        subject: format!("conv_{}", id),
        identity_id: Some(identity_id),
        user_id: None,
    };

    let loop_ = state.loop_manager.get_loop(scope, None).await;
    let running = *loop_.running.read().await;
    let started_at = *loop_.started_at.read().await;
    let streaming_message_id = loop_.streaming_message_id.read().await.clone();
    let is_queued = *loop_.is_queued.read().await;
    let queue_position = *loop_.queue_position.read().await;

    Ok(AxumJson(json!({
        "running": running,
        "started_at": started_at,
        "streaming_message_id": streaming_message_id,
        "is_queued": is_queued,
        "queue_position": queue_position,
    })))
}

/// 重连到正在执行的流式任务：合并 buffer 为快照，再转发后续增量事件
async fn reconnect_stream_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(_params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;
    get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let scope = ChatScope {
        subject: format!("conv_{}", id),
        identity_id: Some(identity_id),
        user_id: None,
    };

    let loop_ = state.loop_manager.get_loop(scope, None).await;

    let (tx, rx) = tokio::sync::mpsc::channel::<ChatEvent>(100);

    tokio::spawn(async move {
        // 1. 合并 buffer 为快照
        let buffer = loop_.event_buffer.read().await;
        let mut full_content = String::new();
        let mut step_statuses: std::collections::HashMap<String, agent_core::StepStatusInfo> = std::collections::HashMap::new();
        let mut attachments: Vec<serde_json::Value> = Vec::new();

        for event in buffer.iter() {
            match event {
                ChatEvent::Token { text } => full_content.push_str(text),
                ChatEvent::StepStatus { status } => {
                    step_statuses.insert(status.id.clone(), status.clone());
                }
                ChatEvent::Attachments { attachments: atts } => {
                    attachments.extend(atts.clone());
                }
                _ => {}
            }
        }
        drop(buffer);

        // 丢弃 visible 为 false 或 auto_hide 的步骤状态
        step_statuses.retain(|_, v| v.visible.unwrap_or(true) && v.auto_hide != Some(true));

        let _ = tx.send(ChatEvent::Snapshot {
            content: full_content,
            step_statuses,
            attachments,
        }).await;

        // 2. 轮询增量事件
        let mut last_idx = loop_.event_buffer.read().await.len();
        while *loop_.running.read().await {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let buf = loop_.event_buffer.read().await;
            let current = buf[last_idx..].to_vec();
            for event in current {
                let _ = tx.send(event).await;
            }
            last_idx = buf.len();
        }

        // 3. 任务已完成
        let _ = tx.send(ChatEvent::Done {
            content: String::new(),
            elapsed_seconds: 0.0,
            elapsed_display: String::new(),
            completed_at: Utc::now().to_rfc3339(),
        }).await;
    });

    let stream = ReceiverStream::new(rx).map(|event: ChatEvent| -> Result<Event, Infallible> {
        let data = serde_json::to_string(&event).unwrap_or_default();
        Ok(Event::default().data(data))
    });

    Ok(Sse::new(stream))
}

/// Update conversation title handler
async fn update_conversation_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<AxumJson<serde_json::Value>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;
    
    let title = params.get("title")
        .ok_or(StatusCode::BAD_REQUEST)?;
    
    // Verify ownership
    get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Update title
    update_conversation_title(&state.db_pool, &id, title).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(json!({
        "status": "ok",
        "message": "conversation updated",
        "title": title
    })))
}

/// Delete conversation handler
async fn delete_conversation_handler(
    State(state): State<ChatState>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<String>,
    axum::extract::Query(_params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<AxumJson<serde_json::Value>, StatusCode> {
    check_anonymous_allowed(&state.db_pool, &headers).await?;

    let identity_id = extract_identity_id(&headers)
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let identity_ids = resolve_identity_ids(&state.db_pool, &headers, &identity_id).await;
    
    // Verify ownership
    get_conversation_by_id_any(&state.db_pool, &id, &identity_ids).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    
    // Delete messages first
    sqlx::query("DELETE FROM messages WHERE conversation_id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Delete conversation
    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    // Delete attachment directory for this conversation
    let attachment_dir = crate::config::CONFIG.chat_attachments_dir.join(&id);
    if attachment_dir.exists() {
        if let Err(e) = tokio::fs::remove_dir_all(&attachment_dir).await {
            error!("Failed to delete attachment directory for conversation {}: {}", id, e);
        } else {
            info!("Deleted attachment directory for conversation {}: {:?}", id, attachment_dir);
        }
    }
    
    Ok(AxumJson(json!({
        "status": "ok",
        "message": "conversation deleted"
    })))
}

/// File upload handler
use axum::extract::{Multipart, Path as AxumPath};
use axum::response::Response;
use axum::body::Body;
use std::path::PathBuf;
use uuid::Uuid;
use tokio::fs;
use http::header;

pub async fn upload_file(
    State(_state): State<ChatState>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, axum::Json<serde_json::Value>)> {
    let mut file_name = String::from("unknown");
    let mut file_data = Vec::new();
    let mut conversation_id: Option<String> = None;
    let mut content_type: Option<String> = None;
    
    // Extract file data from multipart form
    while let Some(field) = multipart.next_field().await.map_err(|_| (StatusCode::BAD_REQUEST, axum::Json(json!({"detail": "Bad request"}))))? {
        let name = field.name().unwrap_or("").to_string();
        
        match name.as_str() {
            "file" => {
                // Browser-reported MIME (agentlite parity: the frontend echoes
                // it back and uses it to detect images for the multimodal channel)
                content_type = field.content_type().map(|m| m.to_string());
                file_name = field.file_name().unwrap_or("unknown").to_string();
                file_data = field.bytes().await.map_err(|_| (StatusCode::BAD_REQUEST, axum::Json(json!({"detail": "Bad request"}))))?.to_vec();
            }
            "conversation_id" => {
                conversation_id = field.text().await.ok();
            }
            _ => {}
        }
    }
    
    if file_data.is_empty() {
        return Err((StatusCode::BAD_REQUEST, axum::Json(json!({"detail": "No file data"}))));
    }

    // Check file size limit
    let max_size = crate::config::CONFIG.upload_max_size;
    if file_data.len() > max_size {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, axum::Json(json!({
            "detail": format!("文件过大，最大允许 {}MB", max_size / (1024 * 1024))
        }))));
    }
    
    // Generate filename: original_name + random string
    let path = PathBuf::from(&file_name);
    let original_name = path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let ext = path.extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s))
        .unwrap_or_default();
    let random_str = Uuid::new_v4().to_string().replace("-", "")[..8].to_string();
    let unique_filename = format!("{}_{}{}", original_name, random_str, ext);
    
    // Determine save path
    let save_dir = if let Some(conv_id) = &conversation_id {
        crate::config::CONFIG.chat_attachments_dir.join(conv_id)
    } else {
        crate::config::CONFIG.chat_attachments_dir.join("temp")
    };
    
    std::fs::create_dir_all(&save_dir).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({"detail": "Request failed"}))))?;
    let file_path = save_dir.join(&unique_filename);
    
    // Write file
    std::fs::write(&file_path, &file_data).map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, axum::Json(json!({"detail": "Request failed"}))))?;
    
    let file_size = file_data.len() as i64;
    let relative_path = if let Some(conv_id) = &conversation_id {
        format!("{}/{}", conv_id, unique_filename)
    } else {
        format!("temp/{}", unique_filename)
    };
    
    info!("File uploaded: {} -> {} ({} bytes)", file_name, file_path.display(), file_size);
    
    Ok(AxumJson(json!({
        "success": true,
        "file_id": unique_filename,
        "file_name": file_name,
        "file_size": file_size,
        "content_type": content_type.unwrap_or_else(|| "application/octet-stream".to_string()),
        "conversation_id": conversation_id,
        "path": relative_path,
    })))
}

/// Serve chat files
pub async fn serve_chat_file(
    AxumPath(file_path): AxumPath<String>,
) -> Result<Response<Body>, StatusCode> {
    // Security: prevent path traversal
    let file_path = file_path.trim_start_matches('/');
    if file_path.contains("..") {
        return Err(StatusCode::BAD_REQUEST);
    }
    // agentlite parity path resolution:
    //   {conv}/outputs/{file} -> chat_files_dir/{conv}/outputs/{file}
    //   {conv}/{file}         -> chat_attachments_dir/{conv}/{file}
    //   {file} (no slash)     -> disks_dir/{file}
    let parts: Vec<&str> = file_path.splitn(3, '/').collect();
    let (full_path, base_dir): (std::path::PathBuf, std::path::PathBuf) = if parts.len() >= 3 && parts[1] == "outputs" {
        (
            crate::config::CONFIG.chat_files_dir.join(parts[0]).join("outputs").join(parts[2]),
            crate::config::CONFIG.chat_files_dir.join(parts[0]).join("outputs"),
        )
    } else if parts.len() >= 2 {
        (
            crate::config::CONFIG.chat_attachments_dir.join(parts[0]).join(parts[1..].join("/")),
            crate::config::CONFIG.chat_attachments_dir.join(parts[0]),
        )
    } else {
        (crate::config::CONFIG.disks_dir.join(file_path), crate::config::CONFIG.disks_dir.clone())
    };
    
    // Check if file exists
    if !full_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    // Check if it's a file
    let metadata = fs::metadata(&full_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !metadata.is_file() {
        return Err(StatusCode::NOT_FOUND);
    }
    
    // Security: verify file is within allowed directory
    let canonical_path = fs::canonicalize(&full_path).await.map_err(|_| StatusCode::FORBIDDEN)?;
    let canonical_base = fs::canonicalize(&base_dir)
        .await
        .map_err(|_| StatusCode::FORBIDDEN)?;
    
    if !canonical_path.starts_with(&canonical_base) {
        return Err(StatusCode::FORBIDDEN);
    }
    
    // Determine content type based on file extension
    let content_type = match full_path.extension().and_then(|e| e.to_str()) {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("xls") => "application/vnd.ms-excel",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("doc") => "application/msword",
        _ => "application/octet-stream",
    };
    
    // Read file
    let file_content = fs::read(&full_path).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    info!("Serving file: {} ({})", full_path.display(), content_type);
    
    // Determine disposition: inline for viewable types, attachment for others
    let inline_types = [
        "image/jpeg", "image/png", "image/gif", "image/webp", "image/svg+xml",
        "application/pdf", "text/plain", "application/json",
    ];
    let disposition = if inline_types.contains(&content_type) { "inline" } else { "attachment" };

    // RFC 5987 encode filename to support non-ASCII characters (e.g. Chinese)
    let raw_name = full_path.file_name().unwrap_or_default().to_string_lossy();
    let encoded_name: String = raw_name.chars().flat_map(|c| {
        if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            vec![c.to_string()]
        } else {
            c.to_string().as_bytes().iter()
                .map(|b| format!("%{:02X}", b))
                .collect()
        }
    }).collect();
    let content_disposition = format!("{}; filename*=UTF-8''{}", disposition, encoded_name);

    // Build response
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, file_content.len())
        .header(header::CONTENT_DISPOSITION, content_disposition)
        .body(Body::from(file_content))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}
