use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use crate::resolve_path;

#[derive(Debug, Clone, Serialize)]
pub struct DirEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub size: u64,
    pub modified: u64,
    #[serde(rename = "is_dir")]
    pub is_dir: bool,
    #[serde(rename = "is_symlink")]
    pub is_symlink: bool,
    /// Direct child count for directories (None for files)
    #[serde(rename = "children_count", skip_serializing_if = "Option::is_none")]
    pub children_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirListing {
    pub path: String,
    pub entries: Vec<DirEntry>,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ListOptions {
    pub sort_by: SortBy,
    pub sort_order: SortOrder,
    pub page: usize,
    pub limit: usize,
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Name,
    Modified,
    Size,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Asc,
    Desc,
}

impl Default for ListOptions {
    fn default() -> Self {
        Self {
            sort_by: SortBy::Name,
            sort_order: SortOrder::Asc,
            page: 1,
            limit: 100,
            hidden: vec![".git".into(), ".DS_Store".into()],
        }
    }
}

/// Parse sort parameters from query string params
pub fn parse_sort_params(
    sort_param: Option<&str>,
    order_param: Option<&str>,
) -> (SortBy, SortOrder) {
    let sort_by = match sort_param.unwrap_or("name") {
        "mtime" | "modified" => SortBy::Modified,
        "size" => SortBy::Size,
        _ => SortBy::Name,
    };

    let sort_order = match order_param.unwrap_or("asc") {
        "desc" => SortOrder::Desc,
        _ => SortOrder::Asc,
    };

    (sort_by, sort_order)
}

/// List directory as JSON — hidden files filtered
pub async fn list_dir_json(
    root: PathBuf,
    segment: &str,
    hidden: &[String],
) -> Result<serde_json::Value, Response<Body>> {
    let path = match resolve_path(&root, segment) {
        Ok(p) => p,
        Err(resp) => return Err(resp),
    };

    if !path.is_dir() {
        return Err((StatusCode::NOT_FOUND, "Not a directory").into_response());
    }

    let mut entries = Vec::new();

    let mut read_dir = match tokio::fs::read_dir(&path).await {
        Ok(r) => r,
        Err(_) => return Err((StatusCode::FORBIDDEN, "Cannot read directory").into_response()),
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match tokio::fs::metadata(entry.path()).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = metadata.is_dir();
        let is_symlink = metadata.is_symlink();

        if crate::content_type::is_hidden(hidden, &name, is_dir) {
            continue;
        }

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Count direct children for directories (for the frontend sub-item display)
        let children_count = if is_dir {
            match tokio::fs::read_dir(entry.path()).await {
                Ok(mut rd) => {
                    let mut n: u64 = 0;
                    while let Ok(Some(_)) = rd.next_entry().await {
                        n += 1;
                    }
                    Some(n)
                }
                Err(_) => None,
            }
        } else {
            None
        };

        entries.push(DirEntry {
            name,
            entry_type: if is_dir { "dir".to_string() } else { "file".to_string() },
            size: if is_dir { 0 } else { metadata.len() },
            modified,
            is_dir,
            is_symlink,
            children_count,
        });
    }

    // Sort: directories first
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
        }
        std::cmp::Ordering::Equal
    });

    let total = entries.len();
    Ok(serde_json::to_value(DirListing {
        path: segment.to_string(),
        entries,
        total,
        page: None,
        total_pages: None,
    })
    .unwrap_or_default())
}

/// List directory with full options (sort, pagination, hidden filter)
pub async fn list_dir(
    root: PathBuf,
    segment: &str,
    opts: ListOptions,
) -> Result<serde_json::Value, Response<Body>> {
    let mut value = list_dir_json(root, segment, &opts.hidden).await?;

    // Apply sort
    if let Some(entries) = value.get_mut("entries").and_then(|e| e.as_array_mut()) {
        entries.sort_by(|a, b| {
            let a_is_dir = a.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
            let b_is_dir = b.get("is_dir").and_then(|v| v.as_bool()).unwrap_or(false);
            if a_is_dir != b_is_dir {
                return if a_is_dir { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
            }

            let cmp = match opts.sort_by {
                SortBy::Name => {
                    let a_name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    let b_name = b.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    a_name.to_lowercase().cmp(&b_name.to_lowercase())
                }
                SortBy::Modified => {
                    let a_m = a.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
                    let b_m = b.get("modified").and_then(|v| v.as_u64()).unwrap_or(0);
                    a_m.cmp(&b_m)
                }
                SortBy::Size => {
                    let a_s = a.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    let b_s = b.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
                    a_s.cmp(&b_s)
                }
            };

            if opts.sort_order == SortOrder::Desc { cmp.reverse() } else { cmp }
        });

        // Paginate
        let total = entries.len();
        if opts.limit > 0 && opts.limit < total {
            let total_pages = (total + opts.limit - 1) / opts.limit;
            let page = opts.page.min(total_pages).max(1);
            let start = (page - 1) * opts.limit;
            let end = (start + opts.limit).min(total);
            *entries = entries[start..end].to_vec();

            let map = value.as_object_mut().unwrap();
            map.insert("page".to_string(), serde_json::Value::Number(page.into()));
            map.insert("total_pages".to_string(), serde_json::Value::Number(total_pages.into()));
            map.insert("total".to_string(), serde_json::Value::Number(total.into()));
        }
    }

    Ok(value)
}

/// List directory in simple text format (one entry per line, dirs end with /)
pub async fn list_dir_simple(
    root: PathBuf,
    segment: &str,
    hidden: &[String],
) -> Result<String, Response<Body>> {
    let path = match resolve_path(&root, segment) {
        Ok(p) => p,
        Err(resp) => return Err(resp),
    };

    if !path.is_dir() {
        return Err((StatusCode::NOT_FOUND, "Not a directory").into_response());
    }

    let mut names = Vec::new();
    let mut read_dir = match tokio::fs::read_dir(&path).await {
        Ok(r) => r,
        Err(_) => return Err((StatusCode::FORBIDDEN, "Cannot read directory").into_response()),
    };

    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = match tokio::fs::metadata(entry.path()).await {
            Ok(m) => m,
            Err(_) => continue,
        };

        if crate::content_type::is_hidden(hidden, &name, metadata.is_dir()) {
            continue;
        }

        if metadata.is_dir() {
            names.push(format!("{}/\n", name));
        } else {
            names.push(format!("{}\n", name));
        }
    }

    names.sort();
    Ok(names.into_iter().collect::<String>())
}
