//! Kreuzberg 后端 - 使用 Kreuzberg 库提取文档内容

use anyhow::{Context, Result};
use kreuzberg::{extract_file_sync, ExtractionConfig, ExtractionResult};
use serde::Serialize;
use std::path::Path;
use tracing::info;

/// Kreuzberg 提取结果
#[derive(Debug, Serialize)]
pub struct KreuzbergExtractResult {
    /// 提取的文本内容
    pub content: String,
    /// 源文件路径
    pub source_file: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 表格数量
    pub table_count: usize,
}

/// Kreuzberg 文档提取器
pub struct KreuzbergExtractor;

impl KreuzbergExtractor {
    /// 提取单个文档文件
    pub fn extract_file<P: AsRef<Path>>(source: P) -> Result<KreuzbergExtractResult> {
        let source = source.as_ref();

        if !source.exists() {
            anyhow::bail!("文件不存在: {:?}", source);
        }

        let file_size = source
            .metadata()
            .with_context(|| format!("无法读取文件元数据: {:?}", source))?
            .len();

        info!("正在提取文件 (kreuzberg): {:?}", source);

        // 使用默认配置执行提取
        let config = ExtractionConfig::default();
        let extraction = extract_file_sync(source, None, &config)
            .with_context(|| format!("提取文件内容失败: {:?}", source))?;

        let content = extraction.content.clone();

        info!(
            "提取完成 (kreuzberg): {:?} ({} 字节)",
            source,
            content.len()
        );

        Ok(KreuzbergExtractResult {
            content,
            source_file: source.display().to_string(),
            mime_type: extraction.mime_type.to_string(),
            file_size,
            table_count: extraction.tables.len(),
        })
    }

    /// 将 Kreuzberg 提取结果转换为 Markdown 格式
    pub fn to_markdown(extraction: &ExtractionResult) -> String {
        let mut output = String::new();

        // 添加主内容
        output.push_str(&extraction.content);

        // 如果有表格，添加到末尾
        if !extraction.tables.is_empty() {
            output.push_str("\n\n---\n\n## Tables\n");
            for (i, table) in extraction.tables.iter().enumerate() {
                output.push_str(&format!("\n### Table {}\n\n", i + 1));
                output.push_str(&table.markdown);
            }
        }

        output
    }

    /// 将 Kreuzberg 提取结果转换为 CSV 格式
    pub fn to_csv(extraction: &ExtractionResult) -> String {
        let mut output = String::new();

        if extraction.tables.is_empty() {
            // 如果没有表格，返回纯文本内容
            return extraction.content.clone();
        }

        for (i, table) in extraction.tables.iter().enumerate() {
            if i > 0 {
                output.push_str("\n---\n\n");
            }

            output.push_str(&format!("# Table {}\n\n", i + 1));

            // 输出 CSV 表头
            if let Some(first_row) = table.cells.first() {
                output.push_str(&first_row.join(","));
                output.push('\n');
            }

            // 输出 CSV 数据行
            for row in &table.cells {
                output.push_str(&row.join(","));
                output.push('\n');
            }
        }

        output
    }

    /// 将 Kreuzberg 提取结果转换为纯文本格式
    pub fn to_plain_text(extraction: &ExtractionResult) -> String {
        let mut output = extraction.content.clone();

        // 如果有表格，以纯文本形式添加
        if !extraction.tables.is_empty() {
            output.push_str("\n\n---\n\nTables:\n");
            for (i, table) in extraction.tables.iter().enumerate() {
                output.push_str(&format!("\nTable {}:\n", i + 1));
                for row in &table.cells {
                    output.push_str(&row.join(" | "));
                    output.push('\n');
                }
            }
        }

        output
    }

    /// 检测文件是否受 Kreuzberg 支持
    pub fn is_supported_format<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Self::is_supported_format_by_ext(&ext)
    }

    /// 根据扩展名检测是否受 Kreuzberg 支持
    /// 
    /// 注意：Kreuzberg 的 PDF 支持需要系统安装 libpdfium.so，
    /// 这里不声明 PDF 支持，让 Auto 模式 fallback 到 native 的 pdf-extract crate。
    pub fn is_supported_format_by_ext(ext: &str) -> bool {
        // Kreuzberg 支持的格式（不含 PDF，PDF 使用 native 后端处理）
        let supported = vec![
            // MS Office
            "docx", "xlsx", "pptx", "doc", "xls", "ppt",
            // Apple iWork
            "pages", "numbers", "key",
            // OpenDocument Format (ODF)
            "odt", "ods", "odp",
            // Chinese UOF format
            "uot", "uos", "uop", "uof",
            // Web and data
            "html", "htm", "json", "csv", "xml", "yaml", "yml",
            // Text and documents
            "md", "txt", "rtf",
            // Email and archives
            "eml", "msg", "zip", "tar", "gz",
            // Images (OCR) - Kreuzberg 需要 Tesseract
            // "png", "jpg", "jpeg", "svg", "gif", "bmp", "tiff", "tif", "webp",
            // Academic
            "bib", "ipynb", "tex", "latex",
            // E-books
            "epub", "mobi", "azw3",
            // Source code
            "sql", "py", "js", "ts", "rs", "go", "java", "c", "cpp", "h", "hpp",
        ];

        supported.contains(&ext)
    }
}

/// 获取文件扩展名的显示名称（Kreuzberg 后端）
pub fn format_description(ext: &str) -> &str {
    match ext.to_lowercase().as_str() {
        "docx" => "Microsoft Word 文档",
        "doc" => "Microsoft Word 97-2003 文档",
        "xlsx" => "Microsoft Excel 工作簿",
        "xls" => "Microsoft Excel 97-2003 工作簿",
        "pptx" => "Microsoft PowerPoint 演示文稿",
        "ppt" => "Microsoft PowerPoint 97-2003 演示文稿",
        "pdf" => "PDF 文档",
        "html" | "htm" => "HTML 网页",
        "csv" => "CSV 数据文件",
        "json" => "JSON 数据文件",
        "xml" => "XML 数据文件",
        "md" => "Markdown 文档",
        "txt" => "纯文本文件",
        "png" | "jpg" | "jpeg" | "svg" | "gif" | "bmp" | "tiff" | "tif" => "图片文件 (OCR)",
        "eml" | "msg" => "电子邮件",
        "epub" => "EPUB 电子书",
        _ => "未知格式",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_format() {
        assert!(KreuzbergExtractor::is_supported_format("test.docx"));
        assert!(KreuzbergExtractor::is_supported_format("test.pdf"));
        assert!(KreuzbergExtractor::is_supported_format("test.html"));
        assert!(KreuzbergExtractor::is_supported_format("test.png"));
        assert!(!KreuzbergExtractor::is_supported_format("test.xyz"));
    }

    #[test]
    fn test_format_description() {
        assert_eq!(format_description("docx"), "Microsoft Word 文档");
        assert_eq!(format_description("pdf"), "PDF 文档");
        assert_eq!(format_description("png"), "图片文件 (OCR)");
    }
}
