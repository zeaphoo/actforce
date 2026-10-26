use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::resolve_path;

/// Get file metadata (size, mtime, type)
pub async fn file_metadata(root: PathBuf, segment: &str) -> Result<serde_json::Value, Response<Body>> {
    let path = resolve_path(&root, segment)?;
    let metadata = tokio::fs::metadata(&path).await.map_err(|_| {
        (StatusCode::NOT_FOUND, "File not found").into_response()
    })?;

    let mtime = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    Ok(serde_json::json!({
        "name": path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
        "size": if metadata.is_file() { metadata.len() } else { 0 },
        "mtime": mtime,
        "is_dir": metadata.is_dir(),
        "is_symlink": metadata.is_symlink(),
    }))
}

/// Compute SHA-256 hash of a file
pub async fn file_hash(root: PathBuf, segment: &str) -> Result<String, Response<Body>> {
    let path = resolve_path(&root, segment)?;
    let data = tokio::fs::read(&path).await.map_err(|_| {
        (StatusCode::NOT_FOUND, "File not found").into_response()
    })?;

    let mut hasher = Sha256::new();
    hasher.update(&data);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}
