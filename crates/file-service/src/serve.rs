use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::content_type::detect_content_type;
use crate::resolve_path;

/// Serve a file with range support, etag, conditional requests, and If-Range
pub async fn serve_file(
    root: PathBuf,
    segment: &str,
    method: &axum::http::Method,
    headers: &HeaderMap,
) -> Response<Body> {
    let path = match resolve_path(&root, segment) {
        Ok(p) => p,
        Err(resp) => return resp,
    };

    if !path.is_file() {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file metadata").into_response(),
    };

    let file_size = metadata.len();
    let modified = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs());

    // ETag matching dufs format: "{mtime}-{size}"
    let etag = format!("\"{}-{}\"", modified.unwrap_or(0), file_size);

    // Check If-None-Match
    if let Some(if_none_match) = headers.get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val == etag || val == "*" {
                return Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header(header::ETAG, &etag)
                    .body(Body::empty())
                    .unwrap();
            }
        }
    }

    // Check If-Modified-Since
    if let Some(if_modified_since) = headers.get(header::IF_MODIFIED_SINCE) {
        if let Some(mtime) = modified {
            if let Ok(val) = if_modified_since.to_str() {
                if let Ok(t) = chrono::DateTime::parse_from_rfc2822(val) {
                    if mtime <= t.timestamp() as u64 {
                        return Response::builder()
                            .status(StatusCode::NOT_MODIFIED)
                            .body(Body::empty())
                            .unwrap();
                    }
                }
            }
        }
    }

    // Check If-Range: if resource changed, serve full file (ignore range)
    let mut use_range = true;
    if let Some(if_range) = headers.get(header::IF_RANGE) {
        if let Ok(val) = if_range.to_str() {
            use_range = if val == etag {
                true
            } else if let Ok(t) = chrono::DateTime::parse_from_rfc2822(val) {
                modified.map_or(false, |m| m <= t.timestamp() as u64)
            } else {
                false
            };
        }
    }

    // HEAD request returns headers only
    if method == axum::http::Method::HEAD {
        let content_type = detect_content_type(&path).await;
        let mut resp = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, file_size)
            .header(header::ETAG, &etag)
            .header(header::ACCEPT_RANGES, "bytes");
        if let Some(mtime) = modified {
            if let Some(ts) = chrono::DateTime::from_timestamp(mtime as i64, 0) {
                resp = resp.header(header::LAST_MODIFIED, ts.to_rfc2822());
            }
        }
        return resp.body(Body::empty()).unwrap();
    }

    // Handle range requests
    let range_header = headers.get(header::RANGE).and_then(|v| v.to_str().ok());

    if let Some(range_str) = range_header {
        if use_range {
            match parse_range(range_str, file_size) {
                Ok((start, end)) => {
                    let content_type = detect_content_type(&path).await;
                    let length = end - start + 1;

                    let mut file = match tokio::fs::File::open(&path).await {
                        Ok(f) => f,
                        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to open file").into_response(),
                    };

                    use tokio::io::{AsyncReadExt, AsyncSeekExt};
                    let _ = file.seek(std::io::SeekFrom::Start(start)).await;
                    let mut buf = vec![0u8; length as usize];
                    let _ = file.read_exact(&mut buf).await;

                    let content_range = format!("bytes {}-{}/{}", start, end, file_size);

                    return Response::builder()
                        .status(StatusCode::PARTIAL_CONTENT)
                        .header(header::CONTENT_TYPE, content_type)
                        .header(header::CONTENT_LENGTH, length)
                        .header(header::CONTENT_RANGE, &content_range)
                        .header(header::ETAG, &etag)
                        .header(header::ACCEPT_RANGES, "bytes")
                        .body(Body::from(buf))
                        .unwrap();
                }
                Err(RangeError::Invalid) => {
                    // 416 Range Not Satisfiable
                    return Response::builder()
                        .status(StatusCode::RANGE_NOT_SATISFIABLE)
                        .header(header::CONTENT_RANGE, format!("bytes */{}", file_size))
                        .body(Body::empty())
                        .unwrap();
                }
                Err(RangeError::NoRange) => {
                    // No valid range unit — serve full file
                }
            }
        }
    }

    // Full file response — stream from disk
    match tokio::fs::File::open(&path).await {
        Ok(file) => {
            use tokio_util::io::ReaderStream;
            let stream = ReaderStream::new(file);
            let body = Body::from_stream(stream);

            let content_type = detect_content_type(&path).await;

            let filename = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut resp = Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, file_size)
                .header(header::ETAG, &etag)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("inline; filename=\"{}\"", filename),
                );

            if let Some(mtime) = modified {
                if let Some(ts) = chrono::DateTime::from_timestamp(mtime as i64, 0) {
                    resp = resp.header(header::LAST_MODIFIED, ts.to_rfc2822());
                }
            }

            resp.body(body).unwrap()
        }
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read file").into_response(),
    }
}

#[derive(Debug)]
enum RangeError {
    Invalid,
    NoRange,
}

/// Parse HTTP Range header, returns (start, end) inclusive
fn parse_range(range_str: &str, file_size: u64) -> Result<(u64, u64), RangeError> {
    let range_str = match range_str.strip_prefix("bytes=") {
        Some(s) => s,
        None => return Err(RangeError::NoRange),
    };

    let first = match range_str.split(',').next() {
        Some(s) => s.trim(),
        None => return Err(RangeError::Invalid),
    };

    let mut pair = first.split('-');
    let start_str = match pair.next() {
        Some(s) => s,
        None => return Err(RangeError::Invalid),
    };
    let end_str = match pair.next() {
        Some(s) => s,
        None => return Err(RangeError::Invalid),
    };

    if file_size == 0 {
        return Err(RangeError::Invalid);
    }

    if start_str.is_empty() {
        // suffix range: bytes=-N
        let suffix: u64 = end_str.parse().map_err(|_| RangeError::Invalid)?;
        return Ok((
            file_size.saturating_sub(suffix),
            file_size - 1,
        ));
    }

    let start: u64 = start_str.parse().map_err(|_| RangeError::Invalid)?;
    if start >= file_size {
        return Err(RangeError::Invalid);
    }

    let end: u64 = if end_str.is_empty() {
        file_size - 1
    } else {
        end_str.parse::<u64>().map_err(|_| RangeError::Invalid)?.min(file_size - 1)
    };

    if start > end {
        return Err(RangeError::Invalid);
    }

    Ok((start, end))
}

/// Serve a directory listing as JSON
pub async fn serve_dir_json(root: PathBuf, segment: &str, hidden: &[String]) -> Result<serde_json::Value, Response<Body>> {
    crate::list::list_dir_json(root, segment, hidden).await
}
