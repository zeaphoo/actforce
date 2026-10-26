use axum::{
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

pub fn markdown_router() -> Router {
    // Mounted at /api/markdown via nest() in main.rs → full path /api/markdown/export
    Router::new().route("/export", post(export_markdown))
}

#[derive(Deserialize)]
struct ExportRequest {
    content: String,
    #[serde(default)]
    format: String,
    #[serde(default)]
    filename: Option<String>,
}

async fn export_markdown(
    Json(req): Json<ExportRequest>,
) -> impl IntoResponse {
    let fmt = req.format.to_lowercase();
    if !matches!(fmt.as_str(), "pdf" | "docx") {
        return (StatusCode::BAD_REQUEST, "Format must be pdf or docx".to_string()).into_response();
    }

    // Find agent-doc binary
    let agent_doc_bin = match find_agent_doc_binary() {
        Some(p) => p,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "agent-doc binary not found".to_string()).into_response(),
    };

    // Create temp input markdown file
    let temp_dir = std::env::temp_dir();
    let id = uuid::Uuid::new_v4();
    let md_path = temp_dir.join(format!("markdown_export_{}.md", id));

    if let Err(e) = std::fs::write(&md_path, &req.content) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to write temp file: {}", e)).into_response();
    }

    // Create temp output path
    let suffix = format!(".{}", fmt);
    let output_path = temp_dir.join(format!("markdown_export_{}{}", id, suffix));

    let cmd = Command::new(&agent_doc_bin)
        .arg("convert")
        .arg(&md_path)
        .arg("-o")
        .arg(&output_path)
        .arg("--backend")
        .arg("auto")
        .output();

    // Clean up input temp file
    let _ = std::fs::remove_file(&md_path);

    match cmd {
        Ok(output) if output.status.success() => {
            match tokio::fs::read(&output_path).await {
                Ok(bytes) => {
                    let _ = std::fs::remove_file(&output_path);

                    let content_type = match fmt.as_str() {
                        "pdf" => "application/pdf",
                        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        _ => "application/octet-stream",
                    };

                    let filename = req.filename.unwrap_or_else(|| "document".to_string());
                    let download_name = format!("{}.{}", filename, fmt);

                    let mut headers = axum::http::HeaderMap::new();
                    headers.insert(axum::http::header::CONTENT_TYPE, content_type.parse().unwrap());
                    headers.insert(
                        axum::http::header::CONTENT_DISPOSITION,
                        format!("attachment; filename=\"{}\"", download_name).parse().unwrap(),
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
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to execute agent-doc: {}", e)).into_response()
        }
    }
}

fn find_agent_doc_binary() -> Option<PathBuf> {
    which::which("agent-doc").ok()
}
