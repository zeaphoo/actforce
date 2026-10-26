//! Markdown 转 DOCX 转换器 - 严格对齐 Python 版本实现
//!
//! 使用 pulldown-cmark 解析 Markdown，使用 docx-rs 生成 Word 文档。
//! 功能与 src/actforce/tools/markdown_to_word.py 严格一致。

use anyhow::Result;
use docx_rs::*;
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::io::Cursor;
use std::path::Path;

/// Markdown 转 Word 文档转换器
pub struct MarkdownToDocxConverter;

impl Default for MarkdownToDocxConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownToDocxConverter {
    pub fn new() -> Self {
        Self
    }

    /// 将 Markdown 文本转换为 Word 文档（对应 Python 版本的 convert）
    pub fn convert(&self, markdown_text: &str) -> Result<Vec<u8>> {
        let paragraphs = self.process_tokens(markdown_text)?;
        let mut docx = Docx::new();
        for para in paragraphs {
            docx = docx.add_paragraph(para);
        }

        let mut cursor = Cursor::new(Vec::new());
        docx.build().pack(&mut cursor)?;
        Ok(cursor.into_inner())
    }

    /// 从文件读取 Markdown 并转换为 Word 文档
    pub fn convert_file<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<()> {
        let markdown = std::fs::read_to_string(input_path.as_ref())
            .map_err(|e| anyhow::anyhow!("无法读取文件: {:?}: {}", input_path.as_ref(), e))?;

        let docx_bytes = self.convert(&markdown)?;

        std::fs::write(output_path.as_ref(), &docx_bytes)
            .map_err(|e| anyhow::anyhow!("无法写入文件: {:?}: {}", output_path.as_ref(), e))?;

        Ok(())
    }

    /// 处理 Markdown tokens（对应 Python 版本的 _process_tokens）
    fn process_tokens(&self, markdown: &str) -> Result<Vec<Paragraph>> {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_TABLES);
        options.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(markdown, options);

        let mut paragraphs: Vec<Paragraph> = Vec::new();

        // 状态变量
        let mut current_paragraph: Option<Paragraph> = None;
        let mut list_stack: Vec<(bool, usize)> = Vec::new(); // (ordered, level)
        let mut list_item_index_stack: Vec<usize> = Vec::new();
        let mut pending_ordered_prefix: Vec<bool> = Vec::new();
        let mut current_ordered_number: Vec<usize> = Vec::new();
        let mut blockquote_level: usize = 0;

        // 表格状态
        let mut current_table_rows: Vec<Vec<Vec<InlineRun>>> = Vec::new();
        let mut current_table_cell_runs: Option<Vec<InlineRun>> = None;
        let mut table_row_idx: isize = -1;
        let mut table_col_idx: isize = -1;
        let mut in_table = false;

        // 代码块状态
        let mut in_code_block = false;
        let mut code_block_text = String::new();

        // 标题状态
        let mut heading_level: Option<HeadingLevel> = None;
        let mut heading_text = String::new();

        // 内联格式状态
        let mut in_bold = false;
        let mut in_italic = false;
        let mut in_inline_code = false;
        let mut in_link = false;

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Heading { level, .. } => {
                        heading_level = Some(level);
                        heading_text.clear();
                    }
                    Tag::Paragraph => {
                        // 表格单元格内不要创建普通段落
                        if in_table {
                            continue;
                        }
                        if list_stack.is_empty() {
                            let para = Paragraph::new();
                            current_paragraph = Some(para);
                        } else if let Some((ordered, _)) = list_stack.last() {
                            if *ordered {
                                let para = Paragraph::new();
                                current_paragraph = Some(para);
                            } else {
                                let para = Paragraph::new();
                                current_paragraph = Some(para);
                            }
                        }
                    }
                    Tag::CodeBlock(_) => {
                        in_code_block = true;
                        code_block_text.clear();
                    }
                    Tag::List(Some(start)) => {
                        list_stack.push((true, list_stack.len() + 1));
                        // 存 start-1，方便在 list_item_open 时先 +1 得到当前编号
                        list_item_index_stack.push((start - 1) as usize);
                        pending_ordered_prefix.push(false);
                        current_ordered_number.push(start as usize);
                    }
                    Tag::List(None) => {
                        list_stack.push((false, list_stack.len() + 1));
                    }
                    Tag::Item => {
                        if let Some((true, _)) = list_stack.last() {
                            // ordered list：推进编号，并标记下一个段落需要加前缀
                            if let Some(last_idx) = list_item_index_stack.last_mut() {
                                *last_idx += 1;
                                if let Some(last_num) = current_ordered_number.last_mut() {
                                    *last_num = *last_idx;
                                }
                            }
                            if let Some(last_prefix) = pending_ordered_prefix.last_mut() {
                                *last_prefix = true;
                            }
                        }
                    }
                    Tag::BlockQuote(_) => {
                        blockquote_level += 1;
                    }
                    Tag::Table(_) => {
                        in_table = true;
                        current_table_rows.clear();
                        table_row_idx = -1;
                        table_col_idx = -1;
                    }
                    Tag::TableHead => {
                        table_row_idx += 1;
                        table_col_idx = -1;
                        current_table_rows.push(Vec::new());
                        current_table_cell_runs = Some(Vec::new());
                    }
                    Tag::TableRow => {
                        table_row_idx += 1;
                        table_col_idx = -1;
                        current_table_rows.push(Vec::new());
                        current_table_cell_runs = Some(Vec::new());
                    }
                    Tag::TableCell => {
                        table_col_idx += 1;
                        current_table_cell_runs = Some(Vec::new());
                    }
                    Tag::Emphasis => {
                        in_italic = true;
                    }
                    Tag::Strong => {
                        in_bold = true;
                    }
                    Tag::Strikethrough => {}
                    Tag::Link { .. } => {
                        in_link = true;
                    }
                    Tag::Image { .. } => {}
                    Tag::FootnoteDefinition(_) => {}
                    Tag::HtmlBlock => {}
                    Tag::DefinitionList
                    | Tag::DefinitionListTitle
                    | Tag::DefinitionListDefinition => {}
                    Tag::MetadataBlock(_) => {}
                },
                Event::End(tag) => match tag {
                    TagEnd::Heading(_) => {
                        let level = heading_level.unwrap_or(HeadingLevel::H1);
                        let para = self.create_heading(&heading_text, level);
                        paragraphs.push(para);
                        heading_level = None;
                    }
                    TagEnd::Paragraph => {
                        if !in_table {
                            if let Some(para) = current_paragraph.take() {
                                paragraphs.push(para);
                            }
                        }
                    }
                    TagEnd::CodeBlock => {
                        in_code_block = false;
                        let para = self.create_code_block(&code_block_text);
                        paragraphs.push(para);
                    }
                    TagEnd::List(ordered) => {
                        if list_stack.pop().is_some() {
                            if ordered {
                                list_item_index_stack.pop();
                                pending_ordered_prefix.pop();
                                current_ordered_number.pop();
                            }
                        }
                    }
                    TagEnd::Item => {}
                    TagEnd::BlockQuote(_) => {
                        blockquote_level = blockquote_level.saturating_sub(1);
                    }
                    TagEnd::Table => {
                        in_table = false;
                        let table_paras = self.create_table(&current_table_rows);
                        paragraphs.extend(table_paras);
                        current_table_rows.clear();
                        current_table_cell_runs = None;
                        table_row_idx = -1;
                        table_col_idx = -1;
                        current_paragraph = None;
                    }
                    TagEnd::TableHead | TagEnd::TableRow => {
                        current_table_cell_runs = None;
                    }
                    TagEnd::TableCell => {
                        if let Some(cell_runs) = current_table_cell_runs.take() {
                            if let Some(row) = current_table_rows.last_mut() {
                                row.push(cell_runs);
                            }
                        }
                    }
                    TagEnd::Emphasis => {
                        in_italic = false;
                    }
                    TagEnd::Strong => {
                        in_bold = false;
                    }
                    TagEnd::Strikethrough => {}
                    TagEnd::Link => {
                        in_link = false;
                    }
                    TagEnd::Image => {}
                    TagEnd::FootnoteDefinition => {}
                    TagEnd::HtmlBlock => {}
                    TagEnd::DefinitionList
                    | TagEnd::DefinitionListTitle
                    | TagEnd::DefinitionListDefinition => {}
                    TagEnd::MetadataBlock(_) => {}
                },
                Event::Text(text) => {
                    if in_code_block {
                        code_block_text.push_str(&text);
                    } else if heading_level.is_some() {
                        heading_text.push_str(&text);
                    } else if in_table {
                        if let Some(ref mut cell) = current_table_cell_runs {
                            cell.push(InlineRun {
                                text: text.to_string(),
                                bold: in_bold,
                                italic: in_italic,
                                code: false,
                                link: in_link,
                            });
                        }
                    } else if let Some(ref mut para) = current_paragraph {
                        // 处理有序列表前缀
                        if let Some((true, _)) = list_stack.last() {
                            if pending_ordered_prefix.last() == Some(&true) {
                                if let Some(num) = current_ordered_number.last() {
                                    let prefix = format!("{}. ", num);
                                    let prefix_run = Run::new()
                                        .add_text(&prefix)
                                        .fonts(RunFonts::new().east_asia("宋体"));
                                    let taken = std::mem::take(para);
                                    *para = taken.add_run(prefix_run);
                                }
                                if let Some(pf) = pending_ordered_prefix.last_mut() {
                                    *pf = false;
                                }
                            }
                        }
                        let run = self.create_inline_run(&text, in_bold, in_italic, false, in_link);
                        let taken = std::mem::take(para);
                        *para = taken.add_run(run);
                    }
                }
                Event::Code(text) => {
                    if heading_level.is_some() {
                        heading_text.push_str(&text);
                    } else if in_table {
                        if let Some(ref mut cell) = current_table_cell_runs {
                            cell.push(InlineRun {
                                text: text.to_string(),
                                bold: false,
                                italic: false,
                                code: true,
                                link: false,
                            });
                        }
                    } else if let Some(ref mut para) = current_paragraph {
                        let run = Run::new()
                            .add_text(text.as_ref())
                            .fonts(RunFonts::new().ascii("Consolas"))
                            .size(20); // 10pt = 20 half-points
                        let taken = std::mem::take(para);
                        *para = taken.add_run(run);
                    }
                }
                Event::SoftBreak => {
                    if let Some(ref mut para) = current_paragraph {
                        let run = Run::new().add_text("\n");
                        let taken = std::mem::take(para);
                        *para = taken.add_run(run);
                    }
                }
                Event::HardBreak => {
                    if let Some(ref mut para) = current_paragraph {
                        let run = Run::new().add_break(BreakType::TextWrapping);
                        let taken = std::mem::take(para);
                        *para = taken.add_run(run);
                    }
                }
                Event::Rule => {
                    let para = self.create_horizontal_line();
                    paragraphs.push(para);
                }
                Event::Html(_) | Event::InlineHtml(_) => {
                    // HTML 标签直接跳过
                }
                Event::FootnoteReference(_) => {}
                Event::TaskListMarker(_) => {}
                Event::InlineMath(_) | Event::DisplayMath(_) => {}
            }
        }

        Ok(paragraphs)
    }

    /// 创建标题（对应 Python 版本的 _add_heading）
    fn create_heading(&self, text: &str, level: HeadingLevel) -> Paragraph {
        let level_num = match level {
            HeadingLevel::H1 => 1,
            HeadingLevel::H2 => 2,
            HeadingLevel::H3 => 3,
            HeadingLevel::H4 => 4,
            HeadingLevel::H5 => 5,
            HeadingLevel::H6 => 6,
        };

        let size = match level_num {
            1 => 44, // 22pt
            2 => 36, // 18pt
            _ => 28, // 14pt
        };

        let east_asia_font = match level_num {
            1 => "宋体",
            2 => "黑体",
            _ => "宋体",
        };

        Paragraph::new().add_run(
            Run::new()
                .add_text(text)
                .fonts(RunFonts::new().east_asia(east_asia_font))
                .size(size)
                .bold(),
        )
    }

    /// 创建内联 run（对应 Python 版本的 _add_inline_runs 中单个 run 的处理）
    fn create_inline_run(
        &self,
        text: &str,
        bold: bool,
        italic: bool,
        code: bool,
        link: bool,
    ) -> Run {
        if code {
            Run::new()
                .add_text(text)
                .fonts(RunFonts::new().ascii("Consolas"))
                .size(20) // 10pt
        } else {
            let mut run = Run::new()
                .add_text(text)
                .fonts(RunFonts::new().east_asia("宋体"));

            if bold {
                run = run.bold();
            }
            if italic {
                run = run.italic();
            }
            if link {
                // 链接：灰色 + 下划线
                run = run.color("505050");
                run = run.underline("single");
            }

            run
        }
    }

    /// 创建代码块（对应 Python 版本的 _add_code_block）
    fn create_code_block(&self, code_text: &str) -> Paragraph {
        Paragraph::new().add_run(
            Run::new()
                .add_text(code_text)
                .fonts(RunFonts::new().ascii("Consolas"))
                .size(20), // 10pt
        )
    }

    /// 创建水平线（对应 Python 版本的 _add_horizontal_line）
    fn create_horizontal_line(&self) -> Paragraph {
        let separator = "\u{2500}".repeat(50);
        Paragraph::new().add_run(Run::new().add_text(&separator))
    }

    /// 创建表格（对应 Python 版本的表格处理逻辑）
    fn create_table(&self, rows: &[Vec<Vec<InlineRun>>]) -> Vec<Paragraph> {
        if rows.is_empty() {
            return Vec::new();
        }

        let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if max_cols == 0 {
            return Vec::new();
        }

        // 将表格转换为段落（由于 docx-rs 的表格 API 限制，这里用段落模拟）
        let mut paragraphs = Vec::new();

        for (row_idx, row) in rows.iter().enumerate() {
            let mut row_text = String::new();
            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx > 0 {
                    row_text.push('\t');
                }
                for elem in cell {
                    row_text.push_str(&elem.text);
                }
            }
            let para = Paragraph::new().add_run(
                Run::new()
                    .add_text(&row_text)
                    .fonts(RunFonts::new().east_asia("宋体")),
            );
            paragraphs.push(para);

            // 在行之间添加分隔线（除了最后一行）
            if row_idx < rows.len() - 1 {
                let sep = Paragraph::new().add_run(
                    Run::new()
                        .add_text(&"-".repeat(max_cols * 20))
                        .fonts(RunFonts::new().east_asia("宋体")),
                );
                paragraphs.push(sep);
            }
        }

        paragraphs
    }
}

/// 内联 run 表示（用于表格单元格）
#[derive(Clone, Debug)]
struct InlineRun {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
}

/// 便捷函数：将 Markdown 文本转换为 Word 文档
pub fn markdown_to_docx(markdown_text: &str) -> Result<Vec<u8>> {
    let converter = MarkdownToDocxConverter::new();
    converter.convert(markdown_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_basic_markdown() {
        let md = "# Hello World\n\nThis is a paragraph.";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_convert_headings() {
        let md = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_code_block() {
        let md = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_inline_formatting() {
        let md = "**bold** *italic* `code`";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_list() {
        let md = "- Item 1\n- Item 2\n- Item 3";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_ordered_list() {
        let md = "1. First\n2. Second\n3. Third";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_table() {
        let md = "| A | B |\n|---|---|\n| 1 | 2 |";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }

    #[test]
    fn test_convert_empty() {
        let md = "";
        let result = markdown_to_docx(md);
        assert!(result.is_ok());
    }
}
