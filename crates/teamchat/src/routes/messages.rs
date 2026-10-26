use axum::{
    extract::{Path, Query, State},
    Json,
};

use crate::error::TeamChatError;
use crate::models::message::{MessageCreateRequest, MessageUpdateRequest, MessagesQuery};
use crate::server::TeamChatState;
use crate::store::TeamChatStore;

pub async fn list<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path(channel_id): Path<String>,
    Query(query): Query<MessagesQuery>,
) -> Result<Json<Vec<serde_json::Value>>, TeamChatError> {
    let limit = query.limit.unwrap_or(50).min(100);
    let messages = state
        .store
        .list_messages(&channel_id, query.before.as_deref(), query.after.as_deref(), limit)
        .await?;
    Ok(Json(messages.iter().map(|m| serde_json::to_value(m).unwrap()).collect()))
}

pub async fn get<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let msg = state
        .store
        .get_message(&message_id)
        .await?
        .ok_or_else(|| TeamChatError::NotFound("message not found".into()))?;

    let _ = channel_id;
    Ok(Json(serde_json::to_value(msg).unwrap()))
}

pub async fn create<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path(channel_id): Path<String>,
    Json(req): Json<MessageCreateRequest>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    if req.content.is_empty() && req.embeds.as_ref().map_or(true, |e| e.is_empty()) {
        return Err(TeamChatError::BadRequest("content or embeds required".into()));
    }

    let sender_name = req.sender_name.as_deref().unwrap_or(&req.sender_id);
    let msg = state
        .store
        .create_message(
            &channel_id,
            &req.sender_id,
            sender_name,
            &req.content,
            &req.embeds.unwrap_or_default(),
            req.message_reference,
        )
        .await?;

    let payload = serde_json::to_value(&msg).unwrap();

    state.hub.broadcast(&channel_id, &serde_json::json!({
        "op": 0,
        "t": "MESSAGE_CREATE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}

pub async fn update<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path((channel_id, message_id)): Path<(String, String)>,
    Json(req): Json<MessageUpdateRequest>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let content = req.content.unwrap_or_default();
    let msg = state
        .store
        .update_message(&message_id, &content)
        .await?
        .ok_or_else(|| TeamChatError::NotFound("message not found".into()))?;

    let payload = serde_json::to_value(&msg).unwrap();
    state.hub.broadcast(&channel_id, &serde_json::json!({
        "op": 0,
        "t": "MESSAGE_UPDATE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}

pub async fn delete<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path((channel_id, message_id)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let deleted = state.store.delete_message(&message_id).await?;
    if !deleted {
        return Err(TeamChatError::NotFound("message not found".into()));
    }

    let payload = serde_json::json!({ "id": message_id, "channel_id": channel_id });
    state.hub.broadcast(&channel_id, &serde_json::json!({
        "op": 0,
        "t": "MESSAGE_DELETE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}
