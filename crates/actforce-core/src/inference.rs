use axum::{
    extract::State,
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::process::{
    ProcessManager,
    InferenceProcessStartRequest, MessageResponse as InferenceMessageResponse,
    LLMHealthChecker,
};

use crate::database::DbPool;
use crate::config::CONFIG;

/// Application state for inference handlers
#[derive(Clone)]
pub struct InferenceState {
    pub db_pool: DbPool,
    pub process_manager: Arc<ProcessManager>,
}

/// Create inference router
pub fn inference_router() -> Router<InferenceState> {
    Router::new()
        .route("/start", post(start_inference))
        .route("/stop", post(stop_inference))
        .route("/restart", post(restart_inference))
        .route("/status", get(get_status))
        .route("/health", get(get_health))
}

/// Start inference process handler
async fn start_inference(
    State(state): State<InferenceState>,
    axum::Json(request): axum::Json<InferenceProcessStartRequest>,
) -> Result<AxumJson<InferenceMessageResponse>, StatusCode> {
    let pm = &state.process_manager;
    
    if pm.is_running().await {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    pm.start(request.config.clone()).await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(InferenceMessageResponse {
        message: format!(
            "Inference process started successfully ({}: {})",
            request.config.engine_type,
            request.config.model_name
        ),
        success: true,
    }))
}

/// Stop inference process handler
async fn stop_inference(
    State(state): State<InferenceState>,
) -> Result<AxumJson<InferenceMessageResponse>, StatusCode> {
    let pm = &state.process_manager;
    
    if !pm.is_running().await {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    pm.stop().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(InferenceMessageResponse {
        message: "Inference process stopped".to_string(),
        success: true,
    }))
}

/// Restart inference process handler
async fn restart_inference(
    State(state): State<InferenceState>,
) -> Result<AxumJson<InferenceMessageResponse>, StatusCode> {
    let pm = &state.process_manager;
    
    if !pm.is_running().await {
        return Err(StatusCode::BAD_REQUEST);
    }
    
    pm.restart().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    
    Ok(AxumJson(InferenceMessageResponse {
        message: "Inference process restarted successfully".to_string(),
        success: true,
    }))
}

/// Get status handler
async fn get_status(
    State(state): State<InferenceState>,
) -> AxumJson<crate::process::InferenceProcessStatus> {
    let mut status = state.process_manager.get_status().await;
    if !CONFIG.show_sensitive_info {
        status.model_name = None;
        status.mmproj_path = None;
        status.venv_path = None;
    }
    AxumJson(status)
}

/// Get health handler
async fn get_health(
    State(state): State<InferenceState>,
) -> AxumJson<Value> {
    let config = state.process_manager.get_status().await;
    
    let checker = if let (Some(host), Some(port)) = (config.host.clone(), config.port) {
        LLMHealthChecker::new(&host, port, 5.0)
    } else {
        LLMHealthChecker::new(&CONFIG.inference_host, CONFIG.inference_port, 5.0)
    };
    
    let status = checker.check_health().await;
    let process_state = state.process_manager.state().await;
    let process_is_ready = state.process_manager.is_ready().await;
    
    AxumJson(json!({
        "ready": status.is_ready,
        "running": status.is_running,
        "process_state": process_state.to_string(),
        "is_ready": process_is_ready,
        "message": status.message,
        "response_time_ms": status.response_time_ms,
        "last_check": status.last_check.to_rfc3339(),
        "error": status.error,
    }))
}
