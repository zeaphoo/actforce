use crate::types::*;
use anyhow::{Result, Context};
use ironpress::{HtmlConverter, Margin, PageSize};
use std::fs;
use std::path::{Path, PathBuf};

/// Export an AuPage document to PDF using ironpress (HTML → PDF)
pub fn export(doc: &AuPageDocument, images: &[EmbeddedImage], output_path: &Path, show_mode: &ShowMode) -> Result<()> {
    let mut html_content = crate::html_export::export_for_pdf(doc, images, show_mode);

    // Remove page-break-inside: avoid which blocks image embedding
    html_content = remove_page_break_avoid(&html_content);

    // Save intermediate HTML for debugging when AUPAGE_DEBUG=true
    if std::env::var("AUPAGE_DEBUG").as_deref() == Ok("true") {
        let html_path = output_path.with_extension("debug.html");
        fs::write(&html_path, &html_content)
            .with_context(|| format!("Failed to write debug HTML to {}", html_path.display()))?;
    }

    // ironpress's parse_png rejects indexed PNGs (color type 3) silently.
    // charton may produce indexed PNGs. Extract data URIs, re-encode as RGB,
    // write to temp files, and reference by file path.
    let temp_files = extract_data_uris_to_files(&mut html_content, output_path)?;

    // Build converter with page size and margins from document metadata
    let margin = doc.meta.as_ref().and_then(|m| m.margin.as_ref());
    let page_width = doc.meta.as_ref().and_then(|m| m.page_width);

    let ps = PageSize::new(
        page_width.map(|w| w as f32).unwrap_or(595.28),
        841.89, // A4 height in points
    );
    let mg = Margin::new(
        margin.map(|m| m.top as f32).unwrap_or(72.0),
        margin.map(|m| m.right as f32).unwrap_or(72.0),
        margin.map(|m| m.bottom as f32).unwrap_or(72.0),
        margin.map(|m| m.left as f32).unwrap_or(72.0),
    );

    let converter = HtmlConverter::new()
        .page_size(ps)
        .margin(mg);

    let pdf_bytes = converter
        .convert(&html_content)
        .map_err(|e| anyhow::anyhow!("ironpress HTML to PDF conversion failed: {}", e))?;

    fs::write(output_path, pdf_bytes)
        .with_context(|| format!("Failed to write PDF to {}", output_path.display()))?;

    // Cleanup temp chart image files
    for path in &temp_files {
        let _ = fs::remove_file(path);
    }

    Ok(())
}

/// Find all `data:image/png;base64,...` URIs in src attributes, decode and write
/// them to temp files in the output directory, and replace the URIs with file paths.
/// Returns the list of temp files for cleanup.
fn extract_data_uris_to_files(html: &mut String, output_path: &Path) -> Result<Vec<PathBuf>> {
    let prefix = "data:image/png;base64,";
    let mut temp_files = Vec::new();
    let parent = output_path.parent().unwrap_or(Path::new("."));

    let mut search_from = 0;
    while let Some(start) = html[search_from..].find(prefix) {
        let abs_start = search_from + start;
        let data_start = abs_start + prefix.len();

        // Find the closing quote (either " or the next space)
        let data_end = html[data_start..]
            .find('"')
            .map(|p| data_start + p)
            .unwrap_or(html.len());

        let encoded = &html[data_start..data_end];

        // Decode base64
        let raw_png = match decode_base64_standalone(encoded) {
            Some(d) => d,
            None => {
                search_from = data_end;
                continue;
            }
        };

        // Re-encode as non-indexed RGB PNG. charton may produce indexed PNGs
        // (color type 3) which ironpress's parse_png rejects silently, causing
        // images to be dropped from the PDF.
        let png_data = reencode_png_rgb(&raw_png).unwrap_or(raw_png);

        // Write to temp file in output directory
        let temp_path = parent.join(format!(
            ".aupage_chart_{}.png",
            temp_files.len()
        ));
        fs::write(&temp_path, &png_data)
            .with_context(|| format!("Failed to write chart temp file {}", temp_path.display()))?;
        temp_files.push(temp_path.clone());

        // Replace data URI with file path in HTML
        let file_path_str = temp_path.to_string_lossy().to_string();
        html.replace_range(abs_start..data_end, &file_path_str);

        search_from = abs_start + file_path_str.len();
    }

    Ok(temp_files)
}

fn decode_base64_standalone(input: &str) -> Option<Vec<u8>> {
    fn table(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let bytes: Vec<u8> = input.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut result = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut chunks = bytes.chunks_exact(4);

    for chunk in &mut chunks {
        let a = table(chunk[0])?;
        let b = table(chunk[1])?;
        result.push((a << 2) | (b >> 4));
        if chunk[2] != b'=' {
            let c = table(chunk[2])?;
            result.push((b << 4) | (c >> 2));
            if chunk[3] != b'=' {
                let d = table(chunk[3])?;
                result.push((c << 6) | d);
            }
        }
    }

    match chunks.remainder() {
        [] | [_] => {}
        [a, b] => {
            let a = table(*a)?;
            let b = table(*b)?;
            result.push((a << 2) | (b >> 4));
        }
        [a, b, c] => {
            let a = table(*a)?;
            let b = table(*b)?;
            result.push((a << 2) | (b >> 4));
            if *c != b'=' {
                let c = table(*c)?;
                result.push((b << 4) | (c >> 2));
            }
        }
        _ => return None,
    }

    Some(result)
}

/// Decode a PNG and re-encode as non-indexed RGB (color type 2).
/// Returns `None` if the input is not a valid PNG image.
fn reencode_png_rgb(raw: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(raw).ok()?;
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    rgb.write_to(
        &mut std::io::Cursor::new(&mut out),
        image::ImageFormat::Png,
    )
    .ok()?;
    Some(out)
}

/// Remove page-break-inside: avoid and page-break-after: avoid from CSS
/// as these block ironpress image embedding.
fn remove_page_break_avoid(html: &str) -> String {
    html.replace("page-break-inside: avoid;", "")
        .replace("page-break-after: avoid;", "")
}
