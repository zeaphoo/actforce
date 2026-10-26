//! 文档格式转换模块
//!
//! 支持 DOCX 与 Markdown 之间的双向转换
//! 支持三种后端：
//! - `auto`: 自动选择最佳后端（默认）
//! - `native`: 使用本地库
//! - `pandoc`: 调用 pandoc 二进制程序

mod docx_to_markdown;
mod markdown_to_docx;
mod markdown_to_pdf;
mod pandoc;

pub use docx_to_markdown::DocxToMarkdownConverter;
pub use markdown_to_docx::MarkdownToDocxConverter;
pub use markdown_to_pdf::MarkdownToPdfConverter;
pub use pandoc::{PandocConverter, PandocPdfConverter};

use anyhow::{Context, Result};
use std::path::Path;

/// 转换后端
#[derive(Debug, Clone, PartialEq)]
pub enum ConvertBackend {
    /// 自动选择（默认）
    Auto,
    /// 本地库后端
    Native,
    /// Pandoc 后端
    Pandoc,
}

impl ConvertBackend {
    /// 从字符串解析后端
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "native" | "nat" => Some(Self::Native),
            "pandoc" | "pan" => Some(Self::Pandoc),
            _ => None,
        }
    }

    /// 获取后端名称
    pub fn name(&self) -> &str {
        match self {
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Pandoc => "pandoc",
        }
    }
}

impl Default for ConvertBackend {
    fn default() -> Self {
        Self::Auto
    }
}

/// 文档转换器 - 统一接口
pub struct DocumentConverter {
    backend: ConvertBackend,
}

impl DocumentConverter {
    /// 创建新的转换器
    pub fn new(backend: ConvertBackend) -> Self {
        Self { backend }
    }

    /// 获取当前后端
    pub fn backend(&self) -> &ConvertBackend {
        &self.backend
    }

    /// 解析实际使用的后端
    fn resolve_backend(&self) -> &ConvertBackend {
        if self.backend == ConvertBackend::Auto {
            if PandocConverter::is_available() {
                &ConvertBackend::Pandoc
            } else {
                &ConvertBackend::Native
            }
        } else {
            &self.backend
        }
    }

    /// DOCX 转 Markdown
    pub fn docx_to_markdown<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        match self.resolve_backend() {
            ConvertBackend::Native => {
                let converter = DocxToMarkdownConverter::new();
                converter.convert_file(path)
            }
            ConvertBackend::Pandoc => PandocConverter::docx_to_markdown(path),
            ConvertBackend::Auto => {
                if PandocConverter::is_available() {
                    PandocConverter::docx_to_markdown(path)
                } else {
                    let converter = DocxToMarkdownConverter::new();
                    converter.convert_file(path)
                }
            }
        }
    }

    /// Markdown 转 DOCX
    pub fn markdown_to_docx<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        markdown: &str,
        output_path: Q,
    ) -> Result<()> {
        match self.resolve_backend() {
            ConvertBackend::Native => {
                let converter = MarkdownToDocxConverter::new();
                let docx_bytes = converter.convert(markdown)?;
                std::fs::write(output_path.as_ref(), &docx_bytes)
                    .with_context(|| format!("无法写入文件: {:?}", output_path.as_ref()))?;
                Ok(())
            }
            ConvertBackend::Pandoc => {
                let temp_dir = std::env::temp_dir();
                let temp_md = temp_dir.join(format!("pandoc_input_{}.md", std::process::id()));
                std::fs::write(&temp_md, markdown).context("无法创建临时文件")?;
                let result = PandocConverter::markdown_to_docx(&temp_md, output_path);
                std::fs::remove_file(&temp_md).ok();
                result
            }
            ConvertBackend::Auto => {
                if PandocConverter::is_available() {
                    let temp_dir = std::env::temp_dir();
                    let temp_md = temp_dir.join(format!("pandoc_input_{}.md", std::process::id()));
                    std::fs::write(&temp_md, markdown).context("无法创建临时文件")?;
                    let result = PandocConverter::markdown_to_docx(&temp_md, output_path);
                    std::fs::remove_file(&temp_md).ok();
                    result
                } else {
                    let converter = MarkdownToDocxConverter::new();
                    let docx_bytes = converter.convert(markdown)?;
                    std::fs::write(output_path.as_ref(), &docx_bytes)
                        .with_context(|| format!("无法写入文件: {:?}", output_path.as_ref()))?;
                    Ok(())
                }
            }
        }
    }

    /// Markdown 转 PDF
    pub fn markdown_to_pdf<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        markdown: &str,
        output_path: Q,
    ) -> Result<()> {
        let output_path = output_path.as_ref();
        // PDF 转换的 auto 逻辑：优先 native（纯 Rust，无外部依赖），失败后 fallback 到 pandoc
        // 这与 resolve_backend 的行为不同（resolve_backend 优先 pandoc，适用于 DOCX）
        match &self.backend {
            ConvertBackend::Auto | ConvertBackend::Native => {
                let converter = MarkdownToPdfConverter::new();
                match converter.convert_to_file(markdown, output_path) {
                    Ok(()) => Ok(()),
                    Err(_) if self.backend == ConvertBackend::Auto && PandocPdfConverter::is_available() => {
                        PandocPdfConverter::convert_to_file(markdown, output_path)
                    }
                    Err(e) => Err(e),
                }
            }
            ConvertBackend::Pandoc => {
                PandocPdfConverter::convert_to_file(markdown, output_path)
            }
        }
    }

    /// 从文件转换（自动检测方向）
    pub fn convert_file<P: AsRef<Path>, Q: AsRef<Path>>(
        &self,
        input_path: P,
        output_path: Q,
    ) -> Result<()> {
        let input_path = input_path.as_ref();
        let output_path = output_path.as_ref();

        // 预处理：若输入是 .doc（旧版二进制或扩展名弄错的 docx），先转换为 OOXML。
        let upgraded = crate::doc_preprocess::preprocess_doc(input_path)?;
        let working_input = upgraded.path();

        let input_ext = working_input
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let output_ext = output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match (input_ext.as_str(), output_ext.as_str()) {
            ("docx", "md" | "markdown") => {
                let markdown = self.docx_to_markdown(working_input)?;
                std::fs::write(output_path, &markdown)
                    .with_context(|| format!("无法写入文件: {:?}", output_path))?;
                Ok(())
            }
            ("md" | "markdown", "docx") => {
                let markdown = std::fs::read_to_string(working_input)
                    .with_context(|| format!("无法读取文件: {:?}", working_input))?;
                self.markdown_to_docx::<&Path, _>(&markdown, output_path)
            }
            ("md" | "markdown", "pdf") => {
                let markdown = std::fs::read_to_string(working_input)
                    .with_context(|| format!("无法读取文件: {:?}", working_input))?;
                self.markdown_to_pdf::<&Path, _>(&markdown, output_path)
            }
            _ => {
                anyhow::bail!(
                    "不支持的转换: .{} -> .{} (支持: docx->md, md->docx, md->pdf)",
                    input_ext,
                    output_ext
                )
            }
        }
    }

    /// 获取支持的转换格式列表
    pub fn supported_conversions() -> Vec<(&'static str, &'static str)> {
        vec![
            ("docx", "md"),
            ("docx", "markdown"),
            ("md", "docx"),
            ("markdown", "docx"),
            ("md", "pdf"),
            ("markdown", "pdf"),
        ]
    }

    /// 检查是否支持指定的转换
    pub fn is_conversion_supported(input_ext: &str, output_ext: &str) -> bool {
        let supported = Self::supported_conversions();
        supported.iter().any(|&(i, o)| {
            i.to_lowercase() == input_ext.to_lowercase()
                && o.to_lowercase() == output_ext.to_lowercase()
        })
    }
}

/// 便捷函数：将 Markdown 文本直接导出为 PDF 或 DOCX 字节
///
/// # 参数
/// - `markdown_text`: Markdown 内容
/// - `format`: 输出格式，"pdf" 或 "docx"
///
/// # 返回
/// 转换后的文件字节（Vec<u8>）
pub fn export_markdown(markdown_text: &str, format: &str) -> Result<Vec<u8>> {
    let fmt = format.to_lowercase();
    match fmt.as_str() {
        "pdf" => {
            // Try native first, fallback to pandoc
            let converter = MarkdownToPdfConverter::new();
            match converter.convert(markdown_text) {
                Ok(bytes) => Ok(bytes),
                Err(_) if PandocPdfConverter::is_available() => {
                    let temp_dir = std::env::temp_dir();
                    let out_path = temp_dir.join(format!("export_{}.pdf", std::process::id()));
                    PandocPdfConverter::convert_to_file(markdown_text, &out_path)
                        .with_context(|| "PDF 转换失败")?;
                    let bytes = std::fs::read(&out_path)
                        .with_context(|| format!("无法读取输出文件: {:?}", out_path))?;
                    std::fs::remove_file(&out_path).ok();
                    Ok(bytes)
                }
                Err(e) => Err(e),
            }
        }
        "docx" => {
            let converter = DocumentConverter::new(ConvertBackend::Auto);
            let temp_dir = std::env::temp_dir();
            let out_path = temp_dir.join(format!("export_{}.docx", std::process::id()));
            converter.markdown_to_docx::<&str, _>(markdown_text, &out_path)
                .with_context(|| "DOCX 转换失败")?;
            let bytes = std::fs::read(&out_path)
                .with_context(|| format!("无法读取输出文件: {:?}", out_path))?;
            std::fs::remove_file(&out_path).ok();
            Ok(bytes)
        }
        _ => {
            anyhow::bail!("不支持的格式: {} (支持: pdf, docx)", format)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_str() {
        assert_eq!(ConvertBackend::from_str("auto"), Some(ConvertBackend::Auto));
        assert_eq!(
            ConvertBackend::from_str("native"),
            Some(ConvertBackend::Native)
        );
        assert_eq!(
            ConvertBackend::from_str("pandoc"),
            Some(ConvertBackend::Pandoc)
        );
        assert_eq!(ConvertBackend::from_str("invalid"), None);
    }

    #[test]
    fn test_supported_conversions() {
        let conversions = DocumentConverter::supported_conversions();
        assert!(!conversions.is_empty());
    }
}
