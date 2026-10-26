//! Markdown 解析工具
//!
//! 使用 pulldown-cmark 解析 Markdown AST，提供准确的文本提取、代码块提取等功能。

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// 提取 Markdown 中的纯文本（移除所有格式标记）
pub fn extract_text(content: &str) -> String {
    let options = Options::all();
    let parser = Parser::new_ext(content, options);

    let mut text = String::with_capacity(content.len());
    let mut in_code_block = false;

    for event in parser {
        match event {
            // 代码块：保留内容但标记
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                text.push('\n');
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                text.push('\n');
            }
            Event::Text(t) if in_code_block => {
                text.push_str(&t);
            }

            // 标题
            Event::Start(Tag::Heading { .. }) => {
                if !text.is_empty() && !text.ends_with("\n\n") {
                    text.push_str("\n\n");
                }
            }
            Event::End(TagEnd::Heading { .. }) => {
                text.push_str("\n\n");
            }

            // 段落
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                text.push_str("\n\n");
            }

            // 列表
            Event::Start(Tag::List(_)) => {}
            Event::End(TagEnd::List(_)) => {
                text.push('\n');
            }
            Event::Start(Tag::Item) => {
                text.push_str("- ");
            }
            Event::End(TagEnd::Item) => {}

            // 引用块
            Event::Start(Tag::BlockQuote(_)) => {}
            Event::End(TagEnd::BlockQuote(_)) => {
                text.push('\n');
            }

            // 行内代码：保留内容
            Event::Code(c) => {
                text.push_str(&c);
            }

            // 链接：保留链接文本
            Event::Start(Tag::Link { .. }) => {}
            Event::End(TagEnd::Link) => {}

            // 图片：跳过
            Event::Start(Tag::Image { .. }) => {}
            Event::End(TagEnd::Image) => {}

            // 粗体/斜体/删除线：保留文本
            Event::Start(Tag::Strong) | Event::End(TagEnd::Strong) => {}
            Event::Start(Tag::Emphasis) | Event::End(TagEnd::Emphasis) => {}
            Event::Start(Tag::Strikethrough) | Event::End(TagEnd::Strikethrough) => {}

            // HTML：跳过
            Event::Html(_html) => {}
            Event::Start(Tag::HtmlBlock) => {}
            Event::End(TagEnd::HtmlBlock) => {}

            // 文本：直接添加
            Event::Text(t) => {
                text.push_str(&t);
            }

            // 软/硬换行
            Event::SoftBreak => {
                if !text.ends_with(' ') && !text.ends_with('\n') {
                    text.push(' ');
                }
            }
            Event::HardBreak => {
                text.push('\n');
            }

            // 其他：忽略
            _ => {}
        }
    }

    // 清理多余空行（最多保留两个连续换行）
    let mut result = String::with_capacity(text.len());
    let mut consecutive_newlines = 0;
    for ch in text.chars() {
        if ch == '\n' {
            consecutive_newlines += 1;
            if consecutive_newlines <= 2 {
                result.push(ch);
            }
        } else {
            consecutive_newlines = 0;
            result.push(ch);
        }
    }

    result.trim().to_string()
}

/// 代码块信息
#[derive(Debug, Clone)]
pub struct CodeBlock {
    /// 语言标识（如 "rust", "python"）
    pub language: Option<String>,
    /// 代码内容
    pub code: String,
}

/// 提取所有代码块
pub fn extract_code_blocks(content: &str, language: Option<&str>) -> Vec<CodeBlock> {
    let options = Options::all();
    let parser = Parser::new_ext(content, options);

    let mut blocks = Vec::new();
    let mut in_code_block = false;
    let mut current_lang: Option<String> = None;
    let mut current_code = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_code.clear();
                current_lang = match kind {
                    CodeBlockKind::Fenced(lang) => {
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang.to_string())
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let lang_match = language.map(|l| {
                    current_lang
                        .as_ref()
                        .map(|cl| cl.to_lowercase() == l.to_lowercase())
                        .unwrap_or(false)
                });

                if language.is_none() || lang_match.unwrap_or(false) {
                    blocks.push(CodeBlock {
                        language: current_lang.clone(),
                        code: current_code.trim_end_matches('\n').to_string(),
                    });
                }
            }
            Event::Text(t) if in_code_block => {
                current_code.push_str(&t);
            }
            _ => {}
        }
    }

    blocks
}

/// 标题信息
#[derive(Debug, Clone)]
pub struct Heading {
    /// 标题级别（1-6）
    pub level: u8,
    /// 标题文本
    pub text: String,
}

/// 提取所有标题
pub fn extract_headings(content: &str) -> Vec<Heading> {
    let options = Options::all();
    let parser = Parser::new_ext(content, options);

    let mut headings = Vec::new();
    let mut in_heading = false;
    let mut current_level: u8 = 0;
    let mut current_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                current_level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                current_text.clear();
            }
            Event::End(TagEnd::Heading { .. }) => {
                in_heading = false;
                headings.push(Heading {
                    level: current_level,
                    text: current_text.trim().to_string(),
                });
            }
            Event::Text(t) if in_heading => {
                current_text.push_str(&t);
            }
            Event::Code(c) if in_heading => {
                current_text.push_str(&c);
            }
            _ => {}
        }
    }

    headings
}

/// 表格信息
#[derive(Debug, Clone)]
pub struct Table {
    /// 表头
    pub header: Vec<String>,
    /// 数据行
    pub rows: Vec<Vec<String>>,
}

/// 提取所有表格
pub fn extract_tables(content: &str) -> Vec<Table> {
    let options = Options::all();
    let parser = Parser::new_ext(content, options);

    let mut tables = Vec::new();
    let mut in_table = false;
    let mut in_header = false;
    let mut current_header: Vec<String> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut current_rows: Vec<Vec<String>> = Vec::new();
    let mut current_text = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Table(_)) => {
                in_table = true;
                current_header.clear();
                current_rows.clear();
            }
            Event::End(TagEnd::Table) => {
                in_table = false;
                if !current_header.is_empty() || !current_rows.is_empty() {
                    tables.push(Table {
                        header: current_header.clone(),
                        rows: current_rows.clone(),
                    });
                }
            }
            Event::Start(Tag::TableHead) => {
                in_header = true;
            }
            Event::End(TagEnd::TableHead) => {
                in_header = false;
                current_header = current_row.clone();
                current_row.clear();
            }
            Event::Start(Tag::TableRow) => {}
            Event::End(TagEnd::TableRow) => {
                if in_header {
                    current_header = current_row.clone();
                } else {
                    current_rows.push(current_row.clone());
                }
                current_row.clear();
            }
            Event::Start(Tag::TableCell) => {
                current_text.clear();
            }
            Event::End(TagEnd::TableCell) => {
                current_row.push(current_text.trim().to_string());
            }
            Event::Text(t) if in_table => {
                current_text.push_str(&t);
            }
            _ => {}
        }
    }

    tables
}

/// 将纯文本转换为基本 Markdown（添加段落和换行）
pub fn text_to_markdown(text: &str) -> String {
    let mut result = String::with_capacity(text.len() + text.len() / 10);

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            result.push_str("\n\n");
        } else {
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text() {
        let md = "# Title\n\nThis is **bold** and *italic* text.\n\nA [link](http://example.com).";
        let text = extract_text(md);
        assert!(text.contains("Title"));
        assert!(text.contains("bold"));
        assert!(text.contains("italic"));
        assert!(text.contains("link"));
        assert!(!text.contains("http://"));
    }

    #[test]
    fn test_extract_code_blocks() {
        let md = "```rust\nfn main() {}\n```\n\nSome text.\n\n```python\nprint('hello')\n```";
        let blocks = extract_code_blocks(md, None);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].language.as_deref(), Some("rust"));
        assert!(blocks[0].code.contains("fn main()"));
        assert_eq!(blocks[1].language.as_deref(), Some("python"));
    }

    #[test]
    fn test_extract_code_blocks_by_language() {
        let md = "```rust\nfn main() {}\n```\n```python\nprint('hello')\n```";
        let blocks = extract_code_blocks(md, Some("rust"));
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].language.as_deref(), Some("rust"));
    }

    #[test]
    fn test_extract_headings() {
        let md = "# H1\n## H2 with **bold**\n### H3";
        let headings = extract_headings(md);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "H1");
        assert_eq!(headings[1].text, "H2 with bold");
    }

    #[test]
    fn test_extract_tables() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
        let tables = extract_tables(md);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].header, vec!["A", "B"]);
        assert_eq!(tables[0].rows.len(), 2);
        assert_eq!(tables[0].rows[0], vec!["1", "2"]);
    }

    #[test]
    fn test_nested_formatting() {
        // 测试嵌套格式：**bold *italic* bold**
        let md = "This is **bold and *nested italic* and bold** text.";
        let text = extract_text(md);
        assert!(text.contains("bold and"));
        assert!(text.contains("nested italic"));
    }

    #[test]
    fn test_code_blocks_preserved() {
        let md = "Some text.\n\n```python\ndef hello():\n    print('world')\n```\n\nMore text.";
        let text = extract_text(md);
        // 代码块内容应该保留
        assert!(text.contains("def hello()"));
        assert!(text.contains("print('world')"));
    }
}
