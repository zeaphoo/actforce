//! DOCX 转 Markdown 转换器

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// DOCX 转 Markdown 转换器
pub struct DocxToMarkdownConverter;

impl Default for DocxToMarkdownConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl DocxToMarkdownConverter {
    pub fn new() -> Self {
        Self
    }

    pub fn convert_file<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let path = path.as_ref();
        let file =
            std::fs::File::open(path).with_context(|| format!("无法打开 DOCX 文件: {:?}", path))?;

        let mut archive =
            ZipArchive::new(file).with_context(|| format!("无法解析 ZIP 文件: {:?}", path))?;

        self.convert_from_archive(&mut archive)
    }

    fn convert_from_archive<R: Read + std::io::Seek>(
        &self,
        archive: &mut ZipArchive<R>,
    ) -> Result<String> {
        let mut file = archive
            .by_name("word/document.xml")
            .context("无法找到 word/document.xml")?;

        let mut xml_content = String::new();
        file.read_to_string(&mut xml_content)?;

        Ok(Self::parse_docx_xml(&xml_content))
    }

    fn parse_docx_xml(xml: &str) -> String {
        let mut reader = Reader::from_str(xml);
        let mut buf = Vec::new();

        let mut result = String::new();
        let mut current_text = String::new();
        let mut paragraph_style: Option<String> = None;
        let mut elements: Vec<ParagraphElement> = Vec::new();
        let mut is_bold = false;
        let mut is_italic = false;

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name_local = e.name();
                    let name = String::from_utf8_lossy(name_local.as_ref());

                    match name.as_ref() {
                        "w:p" | "p" => {
                            if !elements.is_empty() {
                                result.push_str(&Self::render_paragraph(
                                    &elements,
                                    paragraph_style.clone(),
                                ));
                                result.push_str("\n\n");
                            }
                            elements.clear();
                            paragraph_style = None;
                        }
                        "w:r" | "r" => {
                            is_bold = false;
                            is_italic = false;
                        }
                        "w:b" | "b" => is_bold = true,
                        "w:i" | "i" => is_italic = true,
                        "w:t" | "t" => current_text.clear(),
                        _ => {}
                    }
                }
                Ok(Event::Text(e)) => {
                    if let Ok(text) = e.unescape() {
                        current_text.push_str(&text);
                    }
                }
                Ok(Event::End(e)) => {
                    let name_local = e.name();
                    let name = String::from_utf8_lossy(name_local.as_ref());

                    match name.as_ref() {
                        "w:p" | "p" => {
                            if !elements.is_empty() {
                                result.push_str(&Self::render_paragraph(
                                    &elements,
                                    paragraph_style.clone(),
                                ));
                                result.push_str("\n\n");
                            }
                            elements.clear();
                        }
                        "w:t" | "t" | "w:r" | "r" => {
                            if !current_text.is_empty() {
                                elements.push(ParagraphElement {
                                    text: current_text.clone(),
                                    bold: is_bold,
                                    italic: is_italic,
                                });
                                current_text.clear();
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        Self::clean_markdown(&result)
    }

    fn render_paragraph(elements: &[ParagraphElement], style: Option<String>) -> String {
        let mut result = String::new();

        if let Some(ref s) = style {
            let s_lower = s.to_lowercase();
            if let Some(level) = Self::parse_heading_style(&s_lower) {
                result.push_str(&"#".repeat(level));
                result.push(' ');
            }
        }

        for elem in elements {
            let mut text = elem.text.clone();
            if elem.bold && elem.italic {
                text = format!("***{}***", text);
            } else if elem.bold {
                text = format!("**{}**", text);
            } else if elem.italic {
                text = format!("*{}*", text);
            }
            result.push_str(&text);
        }

        result.trim().to_string()
    }

    fn parse_heading_style(style: &str) -> Option<usize> {
        if style.contains("heading1") || (style.contains("title") && !style.contains("subtitle")) {
            Some(1)
        } else if style.contains("heading2") {
            Some(2)
        } else if style.contains("heading3") {
            Some(3)
        } else if style.contains("heading4") {
            Some(4)
        } else if style.contains("heading5") {
            Some(5)
        } else if style.contains("heading6") {
            Some(6)
        } else {
            None
        }
    }

    fn clean_markdown(input: &str) -> String {
        let mut result = String::new();
        let mut last_was_empty = false;

        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                if !last_was_empty {
                    result.push('\n');
                    last_was_empty = true;
                }
            } else {
                result.push_str(trimmed);
                result.push('\n');
                last_was_empty = false;
            }
        }

        result.trim().to_string()
    }
}

#[derive(Debug, Clone)]
struct ParagraphElement {
    text: String,
    bold: bool,
    italic: bool,
}
