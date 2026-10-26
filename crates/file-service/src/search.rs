use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::resolve_path;

/// Shared shape with list.rs DirEntry for consistency
#[derive(Serialize)]
pub struct SearchEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub size: u64,
    pub modified: u64,
    #[serde(rename = "is_dir")]
    pub is_dir: bool,
    #[serde(rename = "is_symlink")]
    pub is_symlink: bool,
}

#[derive(Serialize)]
pub struct SearchResult {
    pub path: String,
    pub entries: Vec<SearchEntry>,
    pub total: usize,
}

const MAX_SUBPATHS_COUNT: usize = 1000;

/// Search sort options
#[derive(Clone, Copy)]
pub enum SearchSort {
    Name,
    Modified,
    Size,
}

pub fn parse_search_sort(sort_param: Option<&str>) -> SearchSort {
    match sort_param.unwrap_or("name") {
        "mtime" | "modified" => SearchSort::Modified,
        "size" => SearchSort::Size,
        _ => SearchSort::Name,
    }
}

/// Recursively search for files containing `query` in their name.
/// Respects hidden file patterns.
pub async fn search_files(
    root: PathBuf,
    segment: &str,
    query: &str,
    sort: SearchSort,
    desc: bool,
    hidden: &[String],
) -> Result<serde_json::Value, Response<Body>> {
    let base = if segment.is_empty() {
        root.clone()
    } else {
        resolve_path(&root, segment)?
    };

    let query_lower = query.to_lowercase();
    let mut entries: Vec<SearchEntry> = Vec::new();

    // Clone hidden for use in filter_entry closure
    let hidden_filter = hidden.to_vec();

    let walker = walkdir::WalkDir::new(&base)
        .follow_links(false)
        .into_iter()
        .filter_entry(move |e| {
            let name = e.file_name().to_str().unwrap_or("");
            let is_dir = e.file_type().is_dir();
            if crate::content_type::is_hidden(&hidden_filter, name, is_dir) {
                if is_dir {
                    // Don't descend into hidden directories
                    return false;
                }
            }
            // Skip dot-prefixed entries as well
            !name.starts_with('.')
        });

    let mut subpath_counts: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();

    for entry in walker {
        let entry = entry.map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read directory").into_response()
        })?;

        if entry.path() == base {
            continue;
        }

        if let Some(parent) = entry.path().parent() {
            *subpath_counts.entry(parent.to_path_buf()).or_insert(0) += 1;
        }

        let name_lower = entry.file_name().to_string_lossy().to_lowercase();
        if !name_lower.contains(&query_lower) {
            continue;
        }

        let metadata = entry.metadata().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to read metadata").into_response()
        })?;

        let is_dir = metadata.is_dir();
        let dir_size = if is_dir {
            (*subpath_counts.get(entry.path()).unwrap_or(&0)).min(MAX_SUBPATHS_COUNT) as u64
        } else {
            metadata.len()
        };

        entries.push(SearchEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            entry_type: if is_dir { "dir".to_string() } else { "file".to_string() },
            size: dir_size,
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0),
            is_dir,
            is_symlink: metadata.is_symlink(),
        });
    }

    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        let cmp = match sort {
            SearchSort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SearchSort::Modified => a.modified.cmp(&b.modified),
            SearchSort::Size => a.size.cmp(&b.size),
        };
        if desc { cmp.reverse() } else { cmp }
    });

    let total = entries.len();
    let search_path = if segment.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", segment.trim_start_matches('/'))
    };

    Ok(serde_json::to_value(SearchResult {
        path: search_path,
        entries,
        total,
    })
    .unwrap_or(serde_json::json!({"path": "/", "entries": [], "total": 0})))
}
