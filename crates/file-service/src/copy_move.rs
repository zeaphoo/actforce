use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;

use crate::resolve_path;

/// Copy a file or directory
pub async fn copy_path(root: PathBuf, src_segment: &str, dest_segment: &str) -> Result<serde_json::Value, Response<Body>> {
    let src = resolve_path(&root, src_segment)?;

    // Build dest path - no resolve check needed, just join
    let dest = root.join(dest_segment.trim_start_matches('/'));

    if !dest.starts_with(&root) {
        return Err((StatusCode::FORBIDDEN, "Access denied").into_response());
    }

    if src.is_dir() {
        copy_dir_recursive(&src, &dest).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to copy directory").into_response())?;
    } else {
        // Ensure parent
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create parent directory").into_response())?;
        }
        tokio::fs::copy(&src, &dest).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to copy file").into_response())?;
    }

    Ok(serde_json::json!({"success": true}))
}

/// Move/rename a file or directory
pub async fn move_path(root: PathBuf, src_segment: &str, dest_segment: &str) -> Result<serde_json::Value, Response<Body>> {
    let src = resolve_path(&root, src_segment)?;

    if src == root {
        return Err((StatusCode::FORBIDDEN, "Cannot move root").into_response());
    }

    let dest = root.join(dest_segment.trim_start_matches('/'));

    if !dest.starts_with(&root) {
        return Err((StatusCode::FORBIDDEN, "Access denied").into_response());
    }

    // Ensure parent dir exists
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create parent directory").into_response())?;
    }

    tokio::fs::rename(&src, &dest).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to move").into_response())?;

    Ok(serde_json::json!({"success": true}))
}

async fn copy_dir_recursive(src: &std::path::Path, dest: &std::path::Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(dest).await?;

    let mut read_dir = tokio::fs::read_dir(src).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let src_entry = entry.path();
        let dest_entry = dest.join(entry.file_name());
        if src_entry.is_dir() {
            Box::pin(copy_dir_recursive(&src_entry, &dest_entry)).await?;
        } else {
            tokio::fs::copy(&src_entry, &dest_entry).await?;
        }
    }
    Ok(())
}
