use std::path::Path;

/// Guess MIME type from file extension (without charset detection)
pub fn guess_mime(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("xml") => "application/xml; charset=utf-8",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("md") => "text/markdown; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("mp3") => "audio/mpeg",
        Some("m4a") => "audio/mp4",
        Some("ogg") | Some("oga") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("opus") => "audio/opus",
        Some("wav") => "audio/wav",
        Some("weba") => "audio/webm",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("ogv") => "video/ogg",
        Some("pdf") => "application/pdf",
        Some("zip") => "application/zip",
        Some("gz") => "application/gzip",
        Some("tar") => "application/x-tar",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Detect content type with charset sniffing.
/// Reads first 1024 bytes, detects if content is text, then detects encoding.
pub async fn detect_content_type(path: &Path) -> String {
    let mut buffer: Vec<u8> = vec![];
    match tokio::fs::File::open(path).await {
        Ok(file) => {
            use tokio::io::AsyncReadExt;
            let _ = file.take(1024).read_to_end(&mut buffer).await;
        }
        Err(_) => {
            let mime = mime_guess::from_path(path).first();
            return mime.map(|m| m.to_string()).unwrap_or_else(|| "application/octet-stream".into());
        }
    }

    let mime = mime_guess::from_path(path).first();
    let is_text = content_inspector::inspect(&buffer).is_text();

    if is_text {
        let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
        detector.feed(&buffer, buffer.len() < 1024);
        let enc = detector.guess(None, chardetng::Utf8Detection::Allow);
        let charset = format!("; charset={}", enc.name());
        match mime {
            Some(m) => format!("{m}{charset}"),
            None => format!("text/plain{charset}"),
        }
    } else {
        match mime {
            Some(m) => m.to_string(),
            None => "application/octet-stream".into(),
        }
    }
}

/// Guess if a file extension represents a text-editable file
pub fn is_text_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    matches!(
        ext.as_deref(),
        Some("txt")
            | Some("md")
            | Some("json")
            | Some("xml")
            | Some("html")
            | Some("htm")
            | Some("css")
            | Some("js")
            | Some("jsx")
            | Some("ts")
            | Some("tsx")
            | Some("py")
            | Some("rs")
            | Some("toml")
            | Some("yaml")
            | Some("yml")
            | Some("ini")
            | Some("cfg")
            | Some("conf")
            | Some("log")
            | Some("csv")
            | Some("sh")
            | Some("bash")
            | Some("env")
            | Some("gitignore")
            | Some("dockerfile")
            | Some("sql")
            | Some("bat")
            | Some("ps1")
            | Some("c")
            | Some("cpp")
            | Some("h")
            | Some("hpp")
            | Some("java")
            | Some("go")
            | Some("rb")
            | Some("php")
            | Some("swift")
            | Some("kt")
            | Some("scala")
            | Some("r")
            | Some("lua")
            | Some("vim")
            | Some("makefile")
    )
}

/// Check if extension is previewable as image
pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "svg" | "bmp" | "ico"
    )
}

/// Check if extension is previewable as video
pub fn is_video_ext(ext: &str) -> bool {
    matches!(ext, "mp4" | "webm" | "ogv")
}

/// Check if extension is previewable as audio
pub fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "m4a" | "ogg" | "oga" | "flac" | "opus" | "wav" | "weba"
    )
}

/// Glob pattern match for hidden file filtering
pub fn matches_glob(pattern: &str, target: &str) -> bool {
    glob::Pattern::new(pattern).map(|p| p.matches(target)).unwrap_or(false)
}

/// Check if a filename should be hidden based on glob patterns.
/// Patterns ending with `/` only match directories.
pub fn is_hidden(hidden: &[String], filename: &str, is_dir: bool) -> bool {
    hidden.iter().any(|pattern| {
        if is_dir {
            if let Some(stripped) = pattern.strip_suffix('/') {
                return matches_glob(stripped, filename);
            }
        }
        matches_glob(pattern, filename)
    })
}
