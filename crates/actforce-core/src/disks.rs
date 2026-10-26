//! Disks file service API — native file operations via file-service crate
//!
//! Path semantics mirror agentlite's dufs layout:
//! - paths starting with `raw/` or `processed/` are relative to `disks_dir`
//! - everything else defaults to the `raw` directory (agentlite's dufs root)
//! - `/default/raw/{path}` and `/default/processed/{path}` are shortcut routes
use axum::{
    body::Body,
    extract::{Path, Query, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;

use crate::config::CONFIG;

#[derive(Deserialize, Default)]
struct ListQuery {
    json: Option<bool>,
    simple: Option<bool>,
    q: Option<String>,
    zip: Option<bool>,
    hash: Option<bool>,
    sort: Option<String>,
    order: Option<String>,
    page: Option<usize>,
    limit: Option<usize>,
}

pub fn disks_router() -> Router {
    Router::new()
        .route("/default", get(list_root))
        .route("/default/", get(list_root))
        .route("/default/raw/{*path}", get(handle_get_raw))
        .route("/default/processed/{*path}", get(handle_get_processed))
        .route(
            "/default/{*path}",
            get(handle_get)
                .put(handle_put)
                .delete(handle_delete)
                .patch(handle_patch)
                // MKCOL / MOVE are extension methods (WebDAV) that axum's
                // MethodFilter cannot express, so they fall through here.
                .fallback(handle_extra_methods),
        )
}

/// Resolve a request path against agentlite's disks layout.
///
/// Returns `(root, segment)`: `raw/...` and `processed/...` paths are resolved
/// against `disks_dir`; anything else defaults to the `raw` directory.
fn resolve_disk_root(file_path: &str) -> (std::path::PathBuf, String) {
    let trimmed = file_path.trim_start_matches('/');
    if trimmed.starts_with("raw/") || trimmed.starts_with("processed/") {
        (CONFIG.disks_dir.clone(), trimmed.to_string())
    } else {
        (CONFIG.disks_raw_dir.clone(), trimmed.to_string())
    }
}

async fn list_root(Query(query): Query<ListQuery>) -> Response<Body> {
    handle_list("", query).await
}

/// GET /default/raw/{path} — shortcut into the raw directory (agentlite API)
async fn handle_get_raw(
    Path(file_path): Path<String>,
    Query(query): Query<ListQuery>,
    req: axum::extract::Request,
) -> Response<Body> {
    handle_get(Path(format!("raw/{}", file_path)), Query(query), req).await
}

/// GET /default/processed/{path} — shortcut into the processed directory (agentlite API)
async fn handle_get_processed(
    Path(file_path): Path<String>,
    Query(query): Query<ListQuery>,
    req: axum::extract::Request,
) -> Response<Body> {
    handle_get(Path(format!("processed/{}", file_path)), Query(query), req).await
}

async fn handle_get(
    Path(file_path): Path<String>,
    Query(query): Query<ListQuery>,
    req: axum::extract::Request,
) -> Response<Body> {
    let (root, segment) = resolve_disk_root(&file_path);

    if let Some(search_q) = &query.q {
        if !search_q.is_empty() {
            let sort = file_service::search::parse_search_sort(query.sort.as_deref());
            let desc = query.order.as_deref() == Some("desc");
            return match file_service::search::search_files(root, &segment, search_q, sort, desc, &[]).await {
                Ok(v) => axum::Json(v).into_response(),
                Err(r) => r,
            };
        }
    }

    if query.zip.unwrap_or(false) {
        return match file_service::archive::archive_dir(root, &segment).await {
            Ok(r) => r,
            Err(r) => r,
        };
    }

    if query.hash.unwrap_or(false) {
        return match file_service::meta::file_hash(root, &segment).await {
            Ok(h) => axum::Json(serde_json::json!({"hash": h})).into_response(),
            Err(r) => r,
        };
    }

    let full = root.join(segment.trim_start_matches('/'));
    if full.is_dir() || query.json.unwrap_or(false) {
        return handle_list(&segment, query).await;
    }

    file_service::serve::serve_file(root, &segment, req.method(), req.headers()).await
}

async fn handle_list(segment: &str, query: ListQuery) -> Response<Body> {
    let (root, _) = resolve_disk_root(segment);

    if query.simple.unwrap_or(false) {
        return match file_service::list::list_dir_simple(root, segment, &[]).await {
            Ok(text) => Response::builder()
                .status(axum::http::StatusCode::OK)
                .header("content-type", "text/plain; charset=utf-8")
                .body(Body::from(text))
                .unwrap(),
            Err(r) => r,
        };
    }

    let sort_by = file_service::list::parse_sort_params(
        query.sort.as_deref(),
        query.order.as_deref(),
    );

    let opts = file_service::list::ListOptions {
        sort_by: sort_by.0,
        sort_order: sort_by.1,
        page: query.page.unwrap_or(1),
        limit: query.limit.unwrap_or(100),
        ..Default::default()
    };

    match file_service::list::list_dir(root, segment, opts).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}

async fn handle_put(
    Path(file_path): Path<String>,
    body: Body,
) -> Response<Body> {
    let (root, segment) = resolve_disk_root(&file_path);
    match file_service::upload::upload_file(root, &segment, body).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}

async fn handle_delete(
    Path(file_path): Path<String>,
) -> Response<Body> {
    let (root, segment) = resolve_disk_root(&file_path);
    match file_service::delete::delete_path(root, &segment).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}

async fn handle_patch(
    Path(file_path): Path<String>,
    headers: axum::http::HeaderMap,
    body: Body,
) -> Response<Body> {
    let (root, segment) = resolve_disk_root(&file_path);

    let offset = headers
        .get("X-Update-Range")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| file_service::upload::parse_update_range(v));

    match file_service::upload::append_file(root, &segment, body, offset).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}

/// Handler for non-standard methods (MKCOL / MOVE) that axum's MethodFilter
/// cannot route directly; dispatch by the actual request method.
async fn handle_extra_methods(
    Path(file_path): Path<String>,
    req: Request,
) -> Response<Body> {
    match req.method().as_str() {
        "MKCOL" => handle_mkcol_inner(&file_path).await,
        "MOVE" => handle_move_inner(&file_path, req.headers()).await,
        _ => (StatusCode::METHOD_NOT_ALLOWED, "Method not allowed").into_response(),
    }
}

/// MKCOL — create a directory (dufs-compatible, used by file-manager frontend)
async fn handle_mkcol_inner(file_path: &str) -> Response<Body> {
    let (root, segment) = resolve_disk_root(file_path);
    match file_service::mkdir::create_dir(root, &segment).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}

/// MOVE — move/rename a file or directory (dufs-compatible, used by file-manager frontend)
///
/// The frontend sends `Destination: <basePath>/<dest>` (percent-encoded), e.g.
/// `Destination: /disks/default/foo/bar.txt`. Accept both the relative path form
/// and a full URL form (`scheme://host/path`).
async fn handle_move_inner(file_path: &str, headers: &axum::http::HeaderMap) -> Response<Body> {
    let dest_header = headers
        .get("Destination")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();

    let decoded = percent_encoding::percent_decode_str(dest_header)
        .decode_utf8_lossy();

    // Strip the API base path prefix (up to and including "disks/default") so
    // the remaining segment is relative to the disks root; works for both
    // "/disks/default/..." and "/api/disks/default/..." forms, and full URLs.
    let dest_path = decoded.trim_start_matches('/');
    let dest_segment = match dest_path {
        d if d.contains("disks/default") => d
            .split("disks/default")
            .nth(1)
            .unwrap_or("")
            .trim_start_matches('/')
            .to_string(),
        d => d.to_string(),
    };

    if dest_segment.is_empty() {
        return (StatusCode::BAD_REQUEST, "Missing Destination path").into_response();
    }

    let (root, src_segment) = resolve_disk_root(file_path);
    match file_service::copy_move::move_path(root, &src_segment, &dest_segment).await {
        Ok(v) => axum::Json(v).into_response(),
        Err(r) => r,
    }
}
