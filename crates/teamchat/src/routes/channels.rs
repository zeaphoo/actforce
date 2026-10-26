use axum::{
    extract::{Path, State},
    Json,
};

use crate::error::TeamChatError;
use crate::models::channel::{ChannelCreateRequest, ChannelUpdateRequest};
use crate::server::TeamChatState;
use crate::store::TeamChatStore;

pub async fn list<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
) -> Result<Json<Vec<serde_json::Value>>, TeamChatError> {
    let channels = state.store.list_channels().await?;
    Ok(Json(channels.iter().map(|c| serde_json::to_value(c).unwrap()).collect()))
}

pub async fn get<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let channel = state
        .store
        .get_channel(&channel_id)
        .await?
        .ok_or_else(|| TeamChatError::NotFound("channel not found".into()))?;
    Ok(Json(serde_json::to_value(channel).unwrap()))
}

pub async fn create<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Json(req): Json<ChannelCreateRequest>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    if req.name.is_empty() {
        return Err(TeamChatError::BadRequest("name is required".into()));
    }
    let recipients = req.recipients.unwrap_or_default();
    let channel = state
        .store
        .create_channel(&req.name, req.topic.as_deref(), req.channel_type, &recipients)
        .await?;

    let payload = serde_json::to_value(&channel).unwrap();
    state.hub.broadcast(&channel.id, &serde_json::json!({
        "op": 0,
        "t": "CHANNEL_CREATE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}

pub async fn update<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path(channel_id): Path<String>,
    Json(req): Json<ChannelUpdateRequest>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let channel = state
        .store
        .update_channel(&channel_id, req.name.as_deref(), req.topic.as_deref(), req.position)
        .await?
        .ok_or_else(|| TeamChatError::NotFound("channel not found".into()))?;

    let payload = serde_json::to_value(&channel).unwrap();
    state.hub.broadcast(&channel_id, &serde_json::json!({
        "op": 0,
        "t": "CHANNEL_UPDATE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}

pub async fn delete<S: TeamChatStore>(
    State(state): State<TeamChatState<S>>,
    Path(channel_id): Path<String>,
) -> Result<Json<serde_json::Value>, TeamChatError> {
    let deleted = state.store.delete_channel(&channel_id).await?;
    if !deleted {
        return Err(TeamChatError::NotFound("channel not found".into()));
    }

    let payload = serde_json::json!({ "id": channel_id });
    state.hub.broadcast(&channel_id, &serde_json::json!({
        "op": 0,
        "t": "CHANNEL_DELETE",
        "d": payload,
    }).to_string());

    Ok(Json(payload))
}
