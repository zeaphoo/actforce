use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Upload a file (PUT: create/overwrite). Streams body to temp file, then renames.
pub async fn upload_file(
    root: PathBuf,
    segment: &str,
    body: Body,
) -> Result<serde_json::Value, Response<Body>> {
    let full_path = root.join(segment.trim_start_matches('/'));

    if full_path.is_dir() {
        return Err((StatusCode::CONFLICT, "Cannot overwrite a directory").into_response());
    }

    if let Some(parent) = full_path.parent() {
        if !parent.exists() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create parent directory").into_response())?;
        }
    }

    // Stream body to temp file
    let temp_path = full_path.with_extension("tmp");
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create temp file").into_response())?;

    let mut stream = body.into_data_stream();
    use futures::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read body").into_response()
        })?;
        file.write_all(&chunk).await.map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write file").into_response()
        })?;
    }
    file.flush().await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to flush file").into_response()
    })?;
    drop(file);

    tokio::fs::rename(&temp_path, &full_path)
        .await
        .map_err(|_| {
            let _ = std::fs::remove_file(&temp_path);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response()
        })?;

    Ok(serde_json::json!({"success": true}))
}

/// Parse X-Update-Range header for PATCH resumable uploads.
/// Accepts: "append", "bytes=START-", "bytes=START-END"
pub fn parse_update_range(header_value: &str) -> Option<u64> {
    let trimmed = header_value.trim();
    if trimmed.eq_ignore_ascii_case("append") {
        return None; // caller should stat the file for offset
    }
    let range_str = trimmed.strip_prefix("bytes=")?;
    let start_str = range_str.split('-').next()?;
    start_str.parse().ok()
}

/// Append to a file (PATCH: resumable upload). Streams body.
pub async fn append_file(
    root: PathBuf,
    segment: &str,
    body: Body,
    offset: Option<u64>,
) -> Result<serde_json::Value, Response<Body>> {
    let full_path = root.join(segment.trim_start_matches('/'));

    if full_path.is_dir() {
        return Err((StatusCode::CONFLICT, "Cannot append to a directory").into_response());
    }

    let existing_size = tokio::fs::metadata(&full_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Resolve offset: given offset or stat-based append
    let resolved_offset = match offset {
        Some(o) => o,
        None => existing_size,
    };

    // Validate offset (dufs allows mid-file overwrite and gap; we mirror)
    if resolved_offset > existing_size + 64 * 1024 * 1024 {
        return Err((
            StatusCode::RANGE_NOT_SATISFIABLE,
            format!("Offset {} too far beyond file size {}", resolved_offset, existing_size),
        ).into_response());
    }

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(existing_size == 0)
        .open(&full_path)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file for append").into_response())?;

    if resolved_offset < existing_size {
        use tokio::io::AsyncSeekExt;
        file.seek(std::io::SeekFrom::Start(resolved_offset)).await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Failed to seek").into_response())?;
    }

    // Stream body into file
    let mut stream = body.into_data_stream();
    use futures::StreamExt;
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read body").into_response()
        })?;
        file.write_all(&chunk).await.map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write data").into_response()
        })?;
        written += chunk.len() as u64;
    }
    file.flush().await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Failed to flush").into_response()
    })?;

    Ok(serde_json::json!({"success": true, "size": resolved_offset.max(existing_size) + written}))
}
