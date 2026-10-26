//! File processing & full-text search API (native file service, no dufs).
//!
//! The dufs process-management endpoints (start/stop/restart/update-config/status)
//! were removed: the file service is built-in and always available. What remains
//! are the operations that were previously exposed under /api/dufs but are real
//! functionality of the native file pipeline: manually trigger a processing scan,
//! read the processing status, and full-text search over processed files.

use axum::{
    http::StatusCode,
    response::Json as AxumJson,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::database::DbPool;

#[derive(Clone)]
pub struct FileOpsState {
    pub db_pool: DbPool,
}

#[derive(serde::Deserialize)]
struct SearchQuery {
    q: String,
    limit: Option<usize>,
}

#[derive(serde::Serialize)]
pub struct FileOpsMessageResponse {
    pub message: String,
    pub success: bool,
}

/// Mounted at /api → full paths: /api/process, /api/process/status, /api/search
pub fn file_ops_router() -> Router<FileOpsState> {
    Router::new()
        .route("/process", post(trigger_processing))
        .route("/process/status", get(get_processing_status))
        .route("/search", get(search_files_handler))
}

async fn trigger_processing() -> Result<AxumJson<FileOpsMessageResponse>, (StatusCode, AxumJson<Value>)> {
    tokio::spawn(async {
        crate::file_processing::trigger_file_processing_safe().await;
    });

    Ok(AxumJson(FileOpsMessageResponse {
        message: "File processing task started".to_string(),
        success: true,
    }))
}

async fn get_processing_status() -> AxumJson<Value> {
    match crate::file_processing::get_file_processing_status() {
        Some(status) => AxumJson(status),
        None => AxumJson(json!({"status": "no_data", "message": "No file processing has been done yet"})),
    }
}

async fn search_files_handler(
    axum::extract::Query(params): axum::extract::Query<SearchQuery>,
) -> Result<AxumJson<Value>, (StatusCode, AxumJson<Value>)> {
    let query = params.q.trim();
    if query.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            AxumJson(json!({"detail": "Search query cannot be empty"})),
        ));
    }

    let limit = params.limit.unwrap_or(20);

    match crate::file_processing::search_files(query, limit).await {
        Ok(results) => {
            let total = results.len();
            Ok(AxumJson(json!({
                "query": query,
                "limit": limit,
                "total": total,
                "results": results,
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(json!({"detail": format!("Search failed: {}", e)})),
        )),
    }
}
