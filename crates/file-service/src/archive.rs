use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::io::Write;
use std::path::PathBuf;

use crate::resolve_path;

/// Stream a directory as a ZIP archive
/// Uses the `zip` crate for creating in-memory archives then streams the result.
pub async fn archive_dir(root: PathBuf, segment: &str) -> Result<Response<Body>, Response<Body>> {
    let dir = resolve_path(&root, segment)?;

    if !dir.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "Not a directory").into_response());
    }

    let dir_name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("archive")
        .to_string();

    let dir_clone = dir.clone();
    let dir_name_clone = dir_name.clone();

    let zip_data = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let options = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);

            add_dir_to_zip(&mut zip, &dir_clone, &dir_name_clone, &options)?;
            zip.finish()?;
        }
        Ok(buf)
    })
    .await
    .map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Archive creation failed").into_response()
    })?
    .map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "Archive creation failed").into_response()
    })?;

    let filename = format!("{}.zip", dir_name);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(Body::from(zip_data))
        .map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build response")
                .into_response()
        })?;

    Ok(response)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::io::Cursor<&mut Vec<u8>>>,
    dir: &std::path::Path,
    prefix: &str,
    options: &zip::write::FileOptions<()>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    zip.add_directory(prefix, *options)?;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());

        if path.is_dir() {
            add_dir_to_zip(zip, &path, &name, options)?;
        } else {
            zip.start_file(name, *options)?;
            let data = std::fs::read(&path)?;
            zip.write_all(&data)?;
        }
    }

    Ok(())
}
