use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

pub fn aupage_router() -> Router {
    // Mounted at /api/aupage via nest() in main.rs → full path /api/aupage/export
    Router::new().route("/export", post(export_aupage))
}

#[derive(Deserialize)]
struct ExportRequest {
    input_file: String,
    #[serde(default)]
    format: String,
    #[serde(default)]
    filename: Option<String>,
}

async fn export_aupage(
    Json(req): Json<ExportRequest>,
) -> impl IntoResponse {
    let fmt = req.format.to_lowercase();
    if !matches!(fmt.as_str(), "pdf" | "docx" | "html") {
        return (StatusCode::BAD_REQUEST, "Format must be pdf, docx, or html".to_string()).into_response();
    }

    // Find input file
    let input_path = match find_file(&req.input_file) {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, format!("File not found: {}", req.input_file)).into_response(),
    };

    // Find aupage binary
    let aupage_bin = match find_aupage_binary() {
        Some(p) => p,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "aupage binary not found".to_string()).into_response(),
    };

    // Create temp output file
    let suffix = format!(".{}", fmt);
    let temp_dir = std::env::temp_dir();
    let output_path = temp_dir.join(format!("aupage_export_{}{}", uuid::Uuid::new_v4(), suffix));

    let cmd = Command::new(&aupage_bin)
        .arg("export")
        .arg(&input_path)
        .arg("-f")
        .arg(&fmt)
        .arg("-o")
        .arg(&output_path)
        .arg("--backend")
        .arg("auto")
        .output();

    match cmd {
        Ok(output) if output.status.success() => {
            // Read the output file
            match tokio::fs::read(&output_path).await {
                Ok(bytes) => {
                    // Clean up
                    let _ = std::fs::remove_file(&output_path);

                    let content_type = match fmt.as_str() {
                        "pdf" => "application/pdf",
                        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        "html" => "text/html",
                        _ => "application/octet-stream",
                    };

                    let filename = req.filename.unwrap_or_else(|| {
                        // Derive from input filename: strip .aupage.json/.aupage, append format extension
                        let base = req.input_file
                            .strip_suffix(".aupage.json")
                            .or_else(|| req.input_file.strip_suffix(".aupage"))
                            .unwrap_or(&req.input_file);
                        format!("{}.{}", base, fmt)
                    });

                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(axum::http::header::CONTENT_TYPE, content_type.parse().unwrap());
                    headers.insert(
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
                    );

                    (StatusCode::OK, headers, bytes).into_response()
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&output_path);
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read output file: {}", e)).into_response()
                }
            }
        }
        Ok(output) => {
            let _ = std::fs::remove_file(&output_path);
            let stderr = String::from_utf8_lossy(&output.stderr);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Export failed: {}", stderr)).into_response()
        }
        Err(e) => {
            let _ = std::fs::remove_file(&output_path);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to execute aupage: {}", e)).into_response()
        }
    }
}

fn find_file(filename: &str) -> Option<PathBuf> {
    let search_dirs = [
        std::env::var("ACTFORCE_DATA_ROOT").ok().map(PathBuf::from),
        Some(PathBuf::from("data")),
        Some(PathBuf::from("raw")),
        Some(PathBuf::from("/tmp")),
    ];

    for dir in search_dirs.iter().flatten() {
        let candidate = dir.join(filename);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn find_aupage_binary() -> Option<PathBuf> {
    which::which("aupage").ok()
}
