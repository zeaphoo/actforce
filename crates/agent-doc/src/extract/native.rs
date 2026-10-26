//! Native 后端 - 使用本地库处理不同文件格式
//!
//! 参考 common crate 的实现，使用轻量级库处理不同格式：
//! - 纯文本文件：直接读取
//! - CSV：csv crate
//! - Excel (xlsx/xls)：calamine crate
//! - PDF：pdf-extract crate

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

/// Native 提取结果
#[derive(Debug, Serialize)]
pub struct NativeExtractResult {
    /// 提取的文本内容
    pub content: String,
    /// 源文件路径
    pub source_file: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 元数据
    pub metadata: HashMap<String, String>,
}

/// Native 文档提取器
pub struct NativeExtractor;

impl NativeExtractor {
    /// 提取单个文件
    pub fn extract_file<P: AsRef<Path>>(source: P) -> Result<NativeExtractResult> {
        let source = source.as_ref();

        if !source.exists() {
            anyhow::bail!("文件不存在: {:?}", source);
        }

        let file_size = source
            .metadata()
            .with_context(|| format!("无法读取文件元数据: {:?}", source))?
            .len();

        let ext = source
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        info!("正在提取文件 (native): {:?}", source);

        let (content, mime_type, metadata) = match ext.as_str() {
            // 纯文本文件
            "txt" | "md" | "markdown" | "rs" | "py" | "js" | "ts" | "json" | "yaml" | "yml"
            | "toml" | "xml" | "css" | "sql" | "sh" | "bash" | "zsh" | "c"
            | "cpp" | "h" | "hpp" | "go" | "java" | "rb" | "php" | "lua" | "r" | "tex"
            | "latex" | "bib" | "ini" | "cfg" | "conf" | "log" => {
                Self::extract_text_file(source, &ext)?
            }

            // CSV 文件
            "csv" | "tsv" => Self::extract_csv(source, &ext)?,

            // Excel 文件
            "xlsx" | "xls" | "xlsb" | "xlsm" => Self::extract_excel(source)?,

            // Word 文档
            "docx" => Self::extract_docx(source)?,

            // PowerPoint 演示文稿
            "pptx" => Self::extract_pptx(source)?,

            // HTML 文件
            "html" | "htm" => Self::extract_html(source)?,

            // PDF 文件
            "pdf" => Self::extract_pdf(source)?,

            // OFD 文件（中国国家版式文档）
            "ofd" => Self::extract_ofd(source)?,

            _ => {
                anyhow::bail!("Native 后端不支持的格式: .{}", ext);
            }
        };

        info!("提取完成 (native): {:?} ({} 字节)", source, content.len());

        Ok(NativeExtractResult {
            content,
            source_file: source.display().to_string(),
            mime_type,
            file_size,
            metadata,
        })
    }

    /// 提取纯文本文件
    fn extract_text_file(
        path: &Path,
        ext: &str,
    ) -> Result<(String, String, HashMap<String, String>)> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取文本文件: {:?}", path))?;

        let mime_type = match ext {
            "md" | "markdown" => "text/markdown",
            "json" => "application/json",
            "xml" | "html" | "htm" => "text/html",
            "yaml" | "yml" => "text/yaml",
            "toml" => "text/toml",
            "css" => "text/css",
            "csv" | "tsv" => "text/csv",
            "sql" => "text/x-sql",
            "sh" | "bash" | "zsh" => "text/x-shellscript",
            "py" => "text/x-python",
            "js" | "mjs" => "text/javascript",
            "ts" | "tsx" => "text/typescript",
            "c" | "cpp" | "h" | "hpp" => "text/x-c",
            "go" => "text/x-go",
            "java" => "text/x-java",
            "rb" => "text/x-ruby",
            "php" => "text/x-php",
            _ => "text/plain",
        }
        .to_string();

        let mut metadata = HashMap::new();
        metadata.insert(
            "line_count".to_string(),
            content.lines().count().to_string(),
        );
        metadata.insert(
            "char_count".to_string(),
            content.chars().count().to_string(),
        );

        Ok((content, mime_type, metadata))
    }

    /// 提取 CSV 文件
    fn extract_csv(path: &Path, ext: &str) -> Result<(String, String, HashMap<String, String>)> {
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .from_path(path)
            .with_context(|| format!("无法读取 CSV 文件: {:?}", path))?;

        let headers = reader.headers().map(|h| h.clone()).unwrap_or_default();

        let mut content = String::new();
        let mut row_count = 0;

        // 输出表头为 Markdown 表格
        if !headers.is_empty() {
            content.push_str("| ");
            for (i, header) in headers.iter().enumerate() {
                if i > 0 {
                    content.push_str(" | ");
                }
                content.push_str(&Self::escape_markdown_cell(header));
            }
            content.push_str(" |\n");

            // 分隔符
            content.push_str("|");
            for _ in 0..headers.len() {
                content.push_str(" --- |");
            }
            content.push('\n');
        }

        // 输出数据行
        for result in reader.records() {
            let record = result?;
            content.push_str("| ");
            for (i, field) in record.iter().enumerate() {
                if i > 0 {
                    content.push_str(" | ");
                }
                content.push_str(&Self::escape_markdown_cell(field));
            }
            // 补齐缺失的列
            for _ in record.len()..headers.len() {
                content.push_str(" | ");
            }
            content.push_str(" |\n");
            row_count += 1;
        }

        let mut metadata = HashMap::new();
        metadata.insert("row_count".to_string(), row_count.to_string());
        metadata.insert("column_count".to_string(), headers.len().to_string());

        let mime_type = if ext == "tsv" {
            "text/tab-separated-values"
        } else {
            "text/csv"
        }
        .to_string();

        Ok((content, mime_type, metadata))
    }

    /// 提取 Excel 文件
    fn extract_excel(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use calamine::{open_workbook, Data, Reader, Xlsx};

        let mut workbook: Xlsx<_> =
            open_workbook(path).with_context(|| format!("无法打开 Excel 文件: {:?}", path))?;

        let mut content = String::new();
        let mut total_rows = 0;
        let mut sheet_count = 0;

        let sheet_names = workbook.sheet_names().to_vec();

        for sheet_name in &sheet_names {
            sheet_count += 1;

            if sheet_count > 1 {
                content.push_str("\n\n");
            }

            content.push_str(&format!("## Sheet: {}\n\n", sheet_name));

            let range = workbook
                .worksheet_range(sheet_name)
                .with_context(|| format!("无法读取工作表: {}", sheet_name))?;

            if range.is_empty() {
                content.push_str("*(空表)*\n");
                continue;
            }

            let mut rows: Vec<Vec<String>> = Vec::new();
            let mut max_cols = 0;

            for row in range.rows() {
                let row_data: Vec<String> = row
                    .iter()
                    .map(|cell| -> String {
                        match cell {
                            Data::Empty => String::new(),
                            Data::String(s) => s.clone(),
                            Data::Float(x) => {
                                if x.fract() == 0.0 {
                                    format!("{}", *x as i64)
                                } else {
                                    format!("{}", x)
                                }
                            }
                            Data::Int(i) => format!("{}", i),
                            Data::Bool(b) => format!("{}", b),
                            Data::DateTime(x) => format!("{}", x),
                            Data::DateTimeIso(s) => s.clone(),
                            Data::DurationIso(s) => s.clone(),
                            Data::Error(e) => format!("{:?}", e),
                        }
                    })
                    .collect();
                max_cols = max_cols.max(row_data.len());
                rows.push(row_data);
                total_rows += 1;
            }

            // 输出为 Markdown 表格
            if let Some(first_row) = rows.first() {
                // 表头
                content.push_str("| ");
                for (i, cell) in first_row.iter().enumerate() {
                    if i > 0 {
                        content.push_str(" | ");
                    }
                    content.push_str(&Self::escape_markdown_cell(cell));
                }
                for _ in first_row.len()..max_cols {
                    content.push_str(" | ");
                }
                content.push_str(" |\n");

                // 分隔符
                content.push_str("|");
                for _ in 0..max_cols {
                    content.push_str(" --- |");
                }
                content.push('\n');

                // 数据行
                for row in rows.iter().skip(1) {
                    content.push_str("| ");
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            content.push_str(" | ");
                        }
                        content.push_str(&Self::escape_markdown_cell(cell));
                    }
                    for _ in row.len()..max_cols {
                        content.push_str(" | ");
                    }
                    content.push_str(" |\n");
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("sheet_count".to_string(), sheet_count.to_string());
        metadata.insert("total_rows".to_string(), total_rows.to_string());

        Ok((
            content,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
            metadata,
        ))
    }

    /// 提取 HTML 文件
    ///
    /// 使用 html-to-markdown-rs 将 HTML 转换为 Markdown
    fn extract_html(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use html_to_markdown_rs::{convert, ConversionOptions};

        let html_content = std::fs::read_to_string(path)
            .with_context(|| format!("无法读取 HTML 文件: {:?}", path))?;

        let options = ConversionOptions::default();
        let result = convert(&html_content, Some(options))
            .with_context(|| format!("无法将 HTML 转换为 Markdown: {:?}", path))?;

        let markdown = result.content.unwrap_or_default();
        let mut metadata = HashMap::new();
        metadata.insert(
            "char_count".to_string(),
            markdown.chars().count().to_string(),
        );

        Ok((markdown, "text/html".to_string(), metadata))
    }

    /// 提取 PDF 文件
    ///
    /// 优先使用 pdf-oxide 进行提取（更快、更现代），失败后回退到 pdf-extract
    fn extract_pdf(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        // 优先尝试 pdf-oxide
        match Self::extract_pdf_oxide(path) {
            Ok(result) => {
                info!("使用 pdf-oxide 提取 PDF 成功");
                return Ok(result);
            }
            Err(e) => {
                info!("pdf-oxide 提取失败，回退到 pdf-extract: {}", e);
            }
        }

        // 回退到 pdf-extract
        let text = pdf_extract::extract_text(path)
            .with_context(|| format!("无法提取 PDF 文本: {:?}", path))?;

        // 简单的页数估计
        let page_count = if let Ok(content) = std::fs::read(path) {
            content.windows(10).filter(|w| w == b"/Type/Page").count()
        } else {
            0
        };

        let mut metadata = HashMap::new();
        if page_count > 0 {
            metadata.insert("page_count".to_string(), page_count.to_string());
        }

        Ok((text, "application/pdf".to_string(), metadata))
    }

    /// 使用 pdf-oxide 提取 PDF（转换为 Markdown）
    fn extract_pdf_oxide(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use pdf_oxide::PdfDocument;

        let mut doc = PdfDocument::open(path)
            .with_context(|| format!("pdf-oxide 无法打开 PDF: {:?}", path))?;

        let page_count = doc
            .page_count()
            .with_context(|| "pdf-oxide 无法获取页数")?;

        // 使用 to_markdown_all 直接提取整个文档的 Markdown
        let content = doc
            .to_markdown_all(&Default::default())
            .with_context(|| "pdf-oxide 无法将 PDF 转换为 Markdown")?;

        let mut metadata = HashMap::new();
        metadata.insert("page_count".to_string(), page_count.to_string());

        Ok((content, "application/pdf".to_string(), metadata))
    }

    /// 提取 OFD 文件（中国国家版式文档）
    ///
    /// 使用 ofd-file 库将 OFD 转换为 Markdown 文本
    fn extract_ofd(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use ofd_file::OfdDocument;

        let doc = OfdDocument::open(path)
            .with_context(|| format!("无法打开 OFD 文件: {:?}", path))?;

        let page_count = doc.page_count();
        let markdown = doc
            .to_markdown()
            .with_context(|| format!("无法提取 OFD 文本: {:?}", path))?;

        let mut metadata = HashMap::new();
        metadata.insert("page_count".to_string(), page_count.to_string());

        // 尝试获取文档标题
        let doc_info = doc.doc_info();
        if !doc_info.title.is_empty() {
            metadata.insert("title".to_string(), doc_info.title.clone());
        }
        if !doc_info.author.is_empty() {
            metadata.insert("author".to_string(), doc_info.author.clone());
        }

        Ok((markdown, "application/ofd".to_string(), metadata))
    }

    /// 提取 DOCX 文件（Word 文档）
    ///
    /// 使用 anytomd 库将 DOCX 转换为 Markdown，保留完整格式（标题、粗体、斜体、表格等）
    fn extract_docx(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use anytomd::{convert_file, ConversionOptions};

        let options = ConversionOptions::default();
        let conversion_result = convert_file(path, &options)
            .with_context(|| format!("无法转换 DOCX 文件: {:?}", path))?;

        // 统计段落数量
        let paragraph_count = conversion_result.markdown.matches('\n').count();
        let mut metadata = HashMap::new();
        metadata.insert("paragraph_count".to_string(), paragraph_count.to_string());

        Ok((
            conversion_result.markdown,
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document".to_string(),
            metadata,
        ))
    }

    /// 提取 PPTX 文件（PowerPoint 演示文稿）
    ///
    /// PPTX 是 ZIP 格式的 XML 文件，幻灯片在 ppt/slides/ 目录下
    fn extract_pptx(path: &Path) -> Result<(String, String, HashMap<String, String>)> {
        use std::io::Read;
        use zip::ZipArchive;

        let file =
            std::fs::File::open(path).with_context(|| format!("无法打开 PPTX 文件: {:?}", path))?;

        let mut archive =
            ZipArchive::new(file).with_context(|| format!("无法解析 ZIP 文件: {:?}", path))?;

        let mut content = String::new();
        let mut slide_count = 0;

        // 收集所有幻灯片文件并按数字排序
        let mut slide_files: Vec<usize> = Vec::new();
        let mut slide_names: Vec<String> = Vec::new();
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();

            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                // 提取幻灯片编号 (slide1.xml -> 1)
                if let Some(num_str) = name
                    .strip_prefix("ppt/slides/slide")
                    .and_then(|s| s.strip_suffix(".xml"))
                {
                    if let Ok(num) = num_str.parse::<usize>() {
                        slide_files.push(num);
                        slide_names.push(name);
                    }
                }
            }
        }

        // 创建索引映射并排序
        let mut indexed: Vec<_> = slide_files
            .into_iter()
            .zip(slide_names.into_iter())
            .collect();
        indexed.sort_by_key(|(num, _)| *num);

        // 按顺序提取每个幻灯片
        for (_, slide_name) in &indexed {
            slide_count += 1;
            content.push_str(&format!("\n--- Slide {} ---\n", slide_count));

            // 重新打开 archive 以读取新文件
            let new_file = std::fs::File::open(path)
                .with_context(|| format!("无法打开 PPTX 文件: {:?}", path))?;
            let mut archive2 = ZipArchive::new(new_file)
                .with_context(|| format!("无法解析 ZIP 文件: {:?}", path))?;

            if let Ok(mut slide_file) = archive2.by_name(slide_name.as_str()) {
                let mut xml_content = String::new();
                slide_file.read_to_string(&mut xml_content)?;

                // 解析 XML 提取文本
                let slide_text = Self::extract_text_from_xml(&xml_content);
                content.push_str(&slide_text);
            };
        }

        if slide_count == 0 {
            anyhow::bail!("无法找到幻灯片文件，可能不是有效的 PPTX 文件");
        }

        let mut metadata = HashMap::new();
        metadata.insert("slide_count".to_string(), slide_count.to_string());

        Ok((
            content.trim_start().to_string(),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation".to_string(),
            metadata,
        ))
    }

    /// 从 XML 中提取纯文本
    ///
    /// 使用 quick-xml 解析 OpenXML 格式，提取所有文本内容
    fn extract_text_from_xml(xml: &str) -> String {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);

        let mut text_parts: Vec<String> = Vec::new();
        let mut current_text = String::new();
        let mut in_text_element = false;

        loop {
            match reader.read_event() {
                Ok(Event::Text(e)) => {
                    if in_text_element {
                        if let Ok(text) = e.unescape() {
                            if !text.is_empty() {
                                current_text.push_str(&text);
                            }
                        }
                    }
                }
                Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                    let tag = e.name();
                    let tag_name = std::str::from_utf8(tag.as_ref()).unwrap_or("");

                    // OpenXML 文本元素：t (文本), del (删除的文本), ins (插入的文本)
                    // 标签名可能包含命名空间前缀，如 w:t
                    if tag_name.ends_with(":t")
                        || tag_name == "t"
                        || tag_name.ends_with(":del")
                        || tag_name == "del"
                        || tag_name.ends_with(":ins")
                        || tag_name == "ins"
                    {
                        in_text_element = true;
                    }
                }
                Ok(Event::End(e)) => {
                    let tag = e.name();
                    let tag_name = std::str::from_utf8(tag.as_ref()).unwrap_or("");

                    // OpenXML 文本元素
                    if tag_name.ends_with(":t")
                        || tag_name == "t"
                        || tag_name.ends_with(":del")
                        || tag_name == "del"
                        || tag_name.ends_with(":ins")
                        || tag_name == "ins"
                    {
                        in_text_element = false;
                        if !current_text.is_empty() {
                            text_parts.push(current_text.clone());
                            current_text.clear();
                        }
                    }

                    // 块级元素添加换行
                    if tag_name.ends_with(":p")
                        || tag_name == "p"
                        || tag_name.ends_with(":txBody")
                        || tag_name == "txBody"
                    {
                        if !text_parts.is_empty() {
                            if let Some(last) = text_parts.last() {
                                if !last.ends_with('\n') {
                                    // 不添加额外的换行
                                }
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                _ => {}
            }
        }

        // 合并所有文本部分
        let mut result = String::new();
        let mut last_was_empty = false;

        for part in &text_parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                if !last_was_empty {
                    result.push('\n');
                    last_was_empty = true;
                }
            } else {
                if !result.is_empty() && !result.ends_with('\n') {
                    // 如果前一行不是空的，添加空格而不是换行
                    result.push(' ');
                }
                result.push_str(trimmed);
                last_was_empty = false;
            }
        }

        result
    }

    /// 转义 Markdown 表格单元格中的特殊字符
    fn escape_markdown_cell(text: &str) -> String {
        text.replace('|', "\\|")
            .replace('\n', " ")
            .replace('\r', "")
    }

    /// 检查文件格式是否受 Native 后端支持
    pub fn is_supported_format<P: AsRef<Path>>(path: P) -> bool {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        Self::is_supported_format_by_ext(&ext)
    }

    /// 根据扩展名检查文件格式是否受 Native 后端支持
    pub fn is_supported_format_by_ext(ext: &str) -> bool {
        matches!(
            ext,
            // 纯文本
            "txt" | "md" | "markdown" | "rs" | "py" | "js" | "ts" | "json"
            | "yaml" | "yml" | "toml" | "xml" | "css" | "sql"
            | "sh" | "bash" | "zsh" | "c" | "cpp" | "h" | "hpp" | "go" | "java"
            | "rb" | "php" | "lua" | "r" | "tex" | "latex" | "bib" | "ini" | "cfg"
            | "conf" | "log"
            // HTML
            | "html" | "htm"
            // CSV
            | "csv" | "tsv"
            // Excel
            | "xlsx" | "xls" | "xlsb" | "xlsm"
            // Word
            | "docx"
            // PowerPoint
            | "pptx"
            // PDF
            | "pdf"
            // OFD
            | "ofd"
        )
    }

    /// 获取文件类型描述
    pub fn format_description(ext: &str) -> &str {
        match ext.to_lowercase().as_str() {
            "txt" => "纯文本文件",
            "md" | "markdown" => "Markdown 文档",
            "csv" | "tsv" => "CSV/TSV 数据文件",
            "xlsx" | "xls" | "xlsb" | "xlsm" => "Excel 工作簿",
            "docx" => "Word 文档",
            "pptx" => "PowerPoint 演示文稿",
            "pdf" => "PDF 文档",
            "json" => "JSON 数据文件",
            "xml" => "XML 文件",
            "html" | "htm" => "HTML 网页",
            "yaml" | "yml" => "YAML 配置文件",
            "toml" => "TOML 配置文件",
            "py" => "Python 脚本",
            "js" | "mjs" => "JavaScript 脚本",
            "ts" | "tsx" => "TypeScript 脚本",
            "rs" => "Rust 源代码",
            "go" => "Go 源代码",
            "java" => "Java 源代码",
            "c" | "cpp" | "h" | "hpp" => "C/C++ 源代码",
            "sql" => "SQL 脚本",
            "sh" | "bash" | "zsh" => "Shell 脚本",
            "tex" | "latex" => "LaTeX 文档",
            "bib" => "BibTeX 参考文献",
            "ofd" => "OFD 版式文档（中国国家标准的固定版面文档）",
            _ => "未知格式",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_supported_format() {
        assert!(NativeExtractor::is_supported_format("test.txt"));
        assert!(NativeExtractor::is_supported_format("test.pdf"));
        assert!(NativeExtractor::is_supported_format("test.xlsx"));
        assert!(NativeExtractor::is_supported_format("test.csv"));
        assert!(NativeExtractor::is_supported_format("test.docx"));
        assert!(NativeExtractor::is_supported_format("test.pptx"));
        assert!(!NativeExtractor::is_supported_format("test.xyz"));
    }

    #[test]
    fn test_format_description() {
        assert_eq!(NativeExtractor::format_description("txt"), "纯文本文件");
        assert_eq!(NativeExtractor::format_description("pdf"), "PDF 文档");
        assert_eq!(NativeExtractor::format_description("xlsx"), "Excel 工作簿");
        assert_eq!(NativeExtractor::format_description("docx"), "Word 文档");
        assert_eq!(
            NativeExtractor::format_description("pptx"),
            "PowerPoint 演示文稿"
        );
    }

    #[test]
    fn test_escape_markdown_cell() {
        assert_eq!(
            NativeExtractor::escape_markdown_cell("hello | world"),
            "hello \\| world"
        );
        assert_eq!(
            NativeExtractor::escape_markdown_cell("line1\nline2"),
            "line1 line2"
        );
    }

    #[test]
    fn test_extract_text_file() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("agent_doc_test_native.txt");
        std::fs::write(&test_file, "Hello World\n").unwrap();

        let result = NativeExtractor::extract_text_file(&test_file, "txt").unwrap();
        assert_eq!(result.0.trim(), "Hello World");
        assert_eq!(result.1, "text/plain");

        std::fs::remove_file(&test_file).ok();
    }
}
