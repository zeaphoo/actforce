use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;

use crate::resolve_path;

/// Delete a file or directory
pub async fn delete_path(root: PathBuf, segment: &str) -> Result<serde_json::Value, Response<Body>> {
    let path = resolve_path(&root, segment)?;

    if path == root {
        return Err((StatusCode::FORBIDDEN, "Cannot delete root").into_response());
    }

    if path.is_dir() {
        tokio::fs::remove_dir_all(&path).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete directory").into_response())?;
    } else {
        tokio::fs::remove_file(&path).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to delete file").into_response())?;
    }

    Ok(serde_json::json!({"success": true}))
}
