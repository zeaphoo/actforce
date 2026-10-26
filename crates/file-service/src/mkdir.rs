use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;

/// Create a directory (MKCOL)
pub async fn create_dir(root: PathBuf, segment: &str) -> Result<serde_json::Value, Response<Body>> {
    let path = root.join(segment.trim_start_matches('/'));

    if path.exists() {
        return Err((StatusCode::CONFLICT, "Already exists").into_response());
    }

    tokio::fs::create_dir_all(&path).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create directory").into_response())?;

    Ok(serde_json::json!({"success": true}))
}
