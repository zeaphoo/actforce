use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use tracing::{info, error, warn};
use std::path::PathBuf;
use uuid::Uuid;

use crate::config::CONFIG;
use crate::models::AttachmentInfo;
use agent_core::Attachment;

/// Save a single base64 attachment
pub async fn save_single_attachment_from_base64(
    attachment: &AttachmentInfo,
    conversation_id: &str,
) -> Result<Attachment> {
    let conversation_dir = CONFIG.chat_attachments_dir.join(conversation_id);
    std::fs::create_dir_all(&conversation_dir)?;
    
    save_single_attachment(attachment, &conversation_dir, conversation_id)
}

/// Save attachments to filesystem
pub async fn save_attachments(
    attachments: &[AttachmentInfo],
    conversation_id: &str,
) -> Result<Vec<Attachment>> {
    if attachments.is_empty() {
        return Ok(vec![]);
    }

    // Create conversation-specific directory
    let conversation_dir = CONFIG.chat_attachments_dir.join(conversation_id);
    std::fs::create_dir_all(&conversation_dir)?;

    let mut saved_attachments = Vec::new();

    for attachment in attachments {
        match save_single_attachment(attachment, &conversation_dir, conversation_id) {
            Ok(saved) => {
                info!(
                    "Attachment saved: {} -> {} ({} bytes)",
                    attachment.name, saved.file_path, attachment.size
                );
                saved_attachments.push(saved);
            }
            Err(e) => {
                error!("Failed to save attachment {}: {}", attachment.name, e);
                // Continue processing other attachments
            }
        }
    }

    Ok(saved_attachments)
}

/// Save a single attachment
fn save_single_attachment(
    attachment: &AttachmentInfo,
    conversation_dir: &PathBuf,
    conversation_id: &str,
) -> Result<Attachment> {
    let file_data_base64 = &attachment.data;

    if file_data_base64.is_empty() {
        warn!("Attachment {} has no data, skipping", attachment.name);
        return Err(anyhow::anyhow!("Empty attachment data"));
    }

    // Generate unique filename
    let file_ext = get_extension_from_path(&attachment.name)
        .or_else(|| get_extension_from_mime(&attachment.mime_type))
        .unwrap_or_else(|| ".bin".to_string());
    
    let unique_file_name = format!("{}{}", Uuid::new_v4().to_string().replace("-", ""), file_ext);
    let file_path = conversation_dir.join(&unique_file_name);

    // Decode base64 and save file
    // Handle data URL format (data:image/png;base64,...)
    let clean_base64 = if let Some(pos) = file_data_base64.find(',') {
        &file_data_base64[pos + 1..]
    } else {
        file_data_base64.as_str()
    };

    let file_data = BASE64.decode(clean_base64)?;
    std::fs::write(&file_path, &file_data)?;

    // Build relative path
    let relative_path = format!("{}/{}", conversation_id, unique_file_name);
    
    // Generate accessible URL
    let file_url = format!("/api/chat/files/{}", relative_path);

    // Build metadata
    let mut metadata = std::collections::HashMap::new();
    metadata.insert("url".to_string(), serde_json::json!(file_url));

    Ok(Attachment {
        attachment_id: format!("att_{}", Uuid::new_v4().to_string()[..8].to_string()),
        file_path: relative_path,
        file_name: attachment.name.clone(),
        file_type: attachment.mime_type.clone(),
        file_size: attachment.size,
        metadata,
        uploaded_at: chrono::Utc::now(),
    })
}

/// Get file extension from path
fn get_extension_from_path(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
}

/// Get file extension from MIME type
fn get_extension_from_mime(mime_type: &str) -> Option<String> {
    let ext = match mime_type {
        "image/jpeg" => ".jpg",
        "image/png" => ".png",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "application/pdf" => ".pdf",
        "text/plain" => ".txt",
        "text/markdown" => ".md",
        "application/json" => ".json",
        "text/csv" => ".csv",
        "application/msword" => ".doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => ".docx",
        "application/vnd.ms-excel" => ".xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => ".xlsx",
        _ => return None,
    };
    Some(ext.to_string())
}

/// Get full file path from relative path
pub fn get_attachment_file_path(relative_path: &str) -> PathBuf {
    CONFIG.chat_attachments_dir.join(relative_path)
}
