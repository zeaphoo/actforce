pub mod archive;
pub mod content_type;
pub mod copy_move;
pub mod delete;
pub mod list;
pub mod meta;
pub mod mkdir;
pub mod search;
pub mod serve;
pub mod upload;

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::path::{Path, PathBuf};

/// Resolve a relative path segment against a root, preventing traversal.
/// The root is canonicalized first so relative roots (e.g. "data/storage/...")
/// compare correctly against the canonicalized target; otherwise `starts_with`
/// on a relative vs absolute path always fails and returns 403 Access denied.
pub fn resolve_path(root: &Path, segment: &str) -> Result<PathBuf, Response<Body>> {
    let canonical_root = root.canonicalize().map_err(|_| {
        (StatusCode::FORBIDDEN, "Access denied").into_response()
    })?;
    let clean = segment.trim_start_matches('/');
    let joined = canonical_root.join(clean);
    let canonical = joined.canonicalize().map_err(|_| {
        (StatusCode::NOT_FOUND, "File not found").into_response()
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err((StatusCode::FORBIDDEN, "Access denied").into_response());
    }
    Ok(canonical)
}

/// Check if a path is within root (no traversal check, just existence + prefix)
pub fn validate_path(root: &Path, rel: &str) -> Result<PathBuf, Response<Body>> {
    let canonical_root = root.canonicalize().map_err(|_| {
        (StatusCode::NOT_FOUND, "Not found").into_response()
    })?;
    let clean = rel.trim_start_matches('/');
    let joined = canonical_root.join(clean);
    if !joined.exists() {
        return Err((StatusCode::NOT_FOUND, "Not found").into_response());
    }
    let canonical = joined.canonicalize().map_err(|_| {
        (StatusCode::NOT_FOUND, "File not found").into_response()
    })?;
    if !canonical.starts_with(&canonical_root) {
        return Err((StatusCode::FORBIDDEN, "Access denied").into_response());
    }
    Ok(canonical)
}

/// Add CORS headers to a response builder.
/// Call before `.body()` — e.g. `with_cors(Response::builder()).body(...)`.
pub fn with_cors(builder: axum::http::response::Builder) -> axum::http::response::Builder {
    builder
        .header("access-control-allow-origin", "*")
        .header("access-control-allow-methods", "*")
        .header("access-control-allow-headers", "*")
}
