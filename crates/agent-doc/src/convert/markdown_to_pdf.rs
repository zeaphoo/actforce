/// Markdown to PDF converter.
///
/// Supports two backends:
/// - **native**: uses `markdown2pdf` crate (pure Rust, no external dependencies)
/// - **pandoc**: calls pandoc binary with xelatex for CJK support

use std::path::Path;

use anyhow::{Context, Result};
use markdown2pdf::fonts::FontSource;

// Re-export for convenience
pub use markdown2pdf::config::ConfigSource;
pub use markdown2pdf::fonts::FontConfig;

/// Native PDF converter using markdown2pdf (pure Rust).
pub struct MarkdownToPdfConverter;

impl Default for MarkdownToPdfConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownToPdfConverter {
    pub fn new() -> Self {
        Self
    }

    /// Convert markdown text to PDF bytes using markdown2pdf (native).
    pub fn convert(&self, markdown_text: &str) -> Result<Vec<u8>> {
        let font_config = build_cjk_font_config();
        let style_config = ConfigSource::Embedded(PDF_STYLE_TOML);
        let bytes = markdown2pdf::parse_into_bytes(
            markdown_text.to_string(),
            style_config,
            font_config.as_ref(),
        )
        .with_context(|| "markdown2pdf 转换失败")?;
        Ok(bytes)
    }

    /// Convert markdown text to PDF file using markdown2pdf (native).
    pub fn convert_to_file<P: AsRef<Path>>(&self, markdown_text: &str, output: P) -> Result<()> {
        let font_config = build_cjk_font_config();
        let style_config = ConfigSource::Embedded(PDF_STYLE_TOML);
        markdown2pdf::parse_into_file(
            markdown_text.to_string(),
            output.as_ref(),
            style_config,
            font_config.as_ref(),
        )
        .with_context(|| "markdown2pdf 转换失败")?;
        Ok(())
    }

    /// Convert markdown file to PDF file using markdown2pdf (native).
    pub fn convert_file(&self, input: &Path, output: &Path) -> Result<()> {
        let markdown_text = std::fs::read_to_string(input)
            .with_context(|| format!("无法读取文件: {:?}", input))?;
        self.convert_to_file(&markdown_text, output)
    }
}

/// Build a FontConfig optimized for CJK text rendering.
fn build_cjk_font_config() -> Option<FontConfig> {
    // Try to find a CJK font file on the system.
    // Also respects AGENT_DOC_CJK_FONT env var for custom font paths.
    let font_path = find_cjk_font_file()?;
    Some(
        FontConfig::new()
            .with_default_font_source(FontSource::File(font_path.into()))
            .with_code_font_source(FontSource::Builtin("Courier")),
    )
}

/// Find an available CJK font file path on the system.
fn find_cjk_font_file() -> Option<String> {
    // 1. Check environment variable for custom font path
    if let Ok(path) = std::env::var("AGENT_DOC_CJK_FONT") {
        let p = std::path::Path::new(&path);
        if p.exists() {
            return Some(path);
        }
    }

    // 2. Probe common CJK font names via fc-match to get actual file paths
    let candidates = [
        "Noto Sans CJK SC",
        "Noto Sans CJK TC",
        "Noto Sans CJK JP",
        "WenQuanYi Micro Hei",
        "PingFang SC",
        "Microsoft YaHei",
        "DroidSansFallback",
    ];

    for name in &candidates {
        if let Some(path) = get_font_file_via_fc_match(name) {
            return Some(path);
        }
    }

    None
}

/// Use fc-match to get the actual font file path for a font name.
fn get_font_file_via_fc_match(name: &str) -> Option<String> {
    let output = std::process::Command::new("fc-match")
        .arg("--format=%{file}")
        .arg(name)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || !std::path::Path::new(&path).exists() {
        return None;
    }

    Some(path)
}

/// Convenience function: native converter
pub fn markdown_to_pdf(markdown_text: &str) -> Result<Vec<u8>> {
    let converter = MarkdownToPdfConverter::new();
    converter.convert(markdown_text)
}

/// PDF style configuration matching the DOCX converter's heading hierarchy:
/// H1: 22pt, H2: 18pt, H3+: 14pt, all bold. Body at 10.5pt (standard Chinese doc).
/// Code blocks at 10pt with gray background. Links in gray with underline.
const PDF_STYLE_TOML: &str = r##"
[defaults]
font_size_pt = 10.5

[page]
margins = 25.0

[headings.h1]
font_size_pt = 22.0
font_weight = "bold"
margin_before_pt = 16.0
margin_after_pt = 8.0

[headings.h2]
font_size_pt = 18.0
font_weight = "bold"
margin_before_pt = 14.0
margin_after_pt = 6.0

[headings.h3]
font_size_pt = 14.0
font_weight = "bold"
margin_before_pt = 12.0
margin_after_pt = 4.0

[headings.h4]
font_size_pt = 14.0
font_weight = "bold"
margin_before_pt = 10.0
margin_after_pt = 4.0

[headings.h5]
font_size_pt = 14.0
font_weight = "bold"
margin_before_pt = 8.0
margin_after_pt = 4.0

[headings.h6]
font_size_pt = 14.0
font_weight = "bold"
margin_before_pt = 6.0
margin_after_pt = 4.0

[paragraph]
margin_before_pt = 4.0
margin_after_pt = 4.0

[code_block]
font_size_pt = 10.0
background_color = "#F5F5F5"

[code_inline]
font_size_pt = 10.0

[link]
text_color = "#505050"
underline = true

[table]
cell_padding = [4.0, 6.0, 4.0, 6.0]
margin_before_pt = 8.0
margin_after_pt = 8.0

[blockquote]
text_color = "#666666"
font_style = "italic"
"##;
