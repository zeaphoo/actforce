//! 文档提取模块 - 支持多后端
//!
//! 支持三种后端：
//! - `auto`: 自动选择最佳后端（默认）
//! - `kreuzberg`: 使用 Kreuzberg 库，支持 91+ 种格式
//! - `native`: 使用本地轻量级库，适合简单格式

mod kreuzberg_backend;
mod native;

pub use kreuzberg_backend::KreuzbergExtractor;
pub use native::NativeExtractor;

use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use tracing::{info, warn};

/// 提取后端枚举
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractBackend {
    /// 自动选择（默认）
    Auto,
    /// Kreuzberg 后端
    Kreuzberg,
    /// Native 后端
    Native,
}

impl ExtractBackend {
    /// 从字符串解析后端
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "kreuzberg" | "kb" => Some(Self::Kreuzberg),
            "native" | "nat" => Some(Self::Native),
            _ => None,
        }
    }

    /// 获取后端名称
    pub fn name(&self) -> &str {
        match self {
            Self::Auto => "auto",
            Self::Kreuzberg => "kreuzberg",
            Self::Native => "native",
        }
    }
}

impl Default for ExtractBackend {
    fn default() -> Self {
        Self::Auto
    }
}

/// 输出格式枚举
#[derive(Debug, Clone, PartialEq)]
pub enum ExtractOutputFormat {
    /// Markdown 格式
    Markdown,
    /// CSV 格式（仅表格）
    Csv,
    /// 纯文本格式
    Txt,
}

impl ExtractOutputFormat {
    /// 从字符串解析输出格式
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "md" | "markdown" => Some(Self::Markdown),
            "csv" => Some(Self::Csv),
            "txt" | "text" => Some(Self::Txt),
            _ => None,
        }
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> &str {
        match self {
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Txt => "txt",
        }
    }
}

/// 统一的提取结果
#[derive(Debug, Serialize)]
pub struct ExtractResult {
    /// 提取的文本内容
    pub content: String,
    /// 源文件路径
    pub source_file: String,
    /// MIME 类型
    pub mime_type: String,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 使用的后端
    pub backend: String,
}

/// 文档提取器 - 统一接口
pub struct DocumentExtractor {
    backend: ExtractBackend,
}

impl DocumentExtractor {
    /// 创建新的提取器
    pub fn new(backend: ExtractBackend) -> Self {
        Self { backend }
    }

    /// 获取当前后端
    pub fn backend(&self) -> &ExtractBackend {
        &self.backend
    }

    /// 提取单个文档文件
    pub fn extract_file<P: AsRef<Path>>(&self, source: P) -> Result<ExtractResult> {
        let source = source.as_ref();

        if !source.exists() {
            anyhow::bail!("文件不存在: {:?}", source);
        }

        let file_size = source
            .metadata()
            .with_context(|| format!("无法读取文件元数据: {:?}", source))?
            .len();

        // 预处理：.doc / .xls / .ppt 若为真正的旧版二进制则调用 LibreOffice
        // 转换为 OOXML；若扩展名弄错实际已是 OOXML 则改名即可。
        let upgraded = crate::doc_preprocess::preprocess_doc(source)?;
        let working_path = upgraded.path();

        // 解析实际使用的后端（基于可能已升级后的路径）
        let actual_backend = self.resolve_backend(working_path);

        info!("正在提取文件 [{}]: {:?}", actual_backend.name(), source);

        let content = match actual_backend {
            ExtractBackend::Kreuzberg => {
                // docx 在 Auto 模式下：Kreuzberg 失败时回退到 native
                if self.backend == ExtractBackend::Auto {
                    match KreuzbergExtractor::extract_file(working_path) {
                        Ok(extraction) => extraction.content,
                        Err(e) => {
                            warn!("docx 文件 Kreuzberg 提取失败 ({}), 回退到 native", e);
                            let native_result = NativeExtractor::extract_file(working_path)
                                .with_context(|| format!("native 后端无法转换 DOCX: {:?}", working_path))?;
                            native_result.content
                        }
                    }
                } else {
                    // 显式指定 Kreuzberg 时不回退
                    let extraction = KreuzbergExtractor::extract_file(working_path)?;
                    extraction.content
                }
            }
            ExtractBackend::Native => {
                let extraction = NativeExtractor::extract_file(working_path)?;
                extraction.content
            }
            ExtractBackend::Auto => {
                // 不应该到达这里，因为 resolve_backend 会返回具体后端
                anyhow::bail!("无法确定合适的后端");
            }
        };

        info!(
            "提取完成 [{}]: {:?} ({} 字节)",
            actual_backend.name(),
            source,
            content.len()
        );

        Ok(ExtractResult {
            content,
            source_file: source.display().to_string(),
            mime_type: "text/plain".to_string(),
            file_size,
            backend: actual_backend.name().to_string(),
        })
    }

    /// 提取单个文档文件并保存到输出文件
    pub fn extract_file_to_output<P: AsRef<Path>, O: AsRef<Path>>(
        &self,
        source: P,
        output: O,
        output_format: ExtractOutputFormat,
    ) -> Result<ExtractResult> {
        let source = source.as_ref();
        let file_size = source
            .metadata()
            .with_context(|| format!("无法读取文件元数据: {:?}", source))?
            .len();

        // 预处理：旧版 Office 文件先转换为 OOXML，以便后续统一按 docx/xlsx/pptx 处理。
        let upgraded = crate::doc_preprocess::preprocess_doc(source)?;
        let working_path = upgraded.path();

        // 统一走 extract_file 的路由逻辑（包括 docx 的 Kreuzberg → native 回退）
        let content = self.extract_file(working_path)?.content;

        let content = Self::format_output(&content, output_format);

        let output = output.as_ref();

        // 确保父目录存在
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("无法创建目录: {:?}", parent))?;
        }

        std::fs::write(output, &content)
            .with_context(|| format!("无法写入输出文件: {:?}", output))?;

        info!("输出已保存到: {:?}", output);

        Ok(ExtractResult {
            content,
            source_file: source.display().to_string(),
            mime_type: "text/plain".to_string(),
            file_size,
            backend: self.backend.name().to_string(),
        })
    }

    /// 批量提取目录中的所有文档文件
    pub fn extract_directory<P: AsRef<Path>>(
        &self,
        input_dir: P,
        output_dir: P,
        output_format: ExtractOutputFormat,
        supported_extensions: Option<Vec<String>>,
    ) -> Result<Vec<ExtractResult>> {
        let input_dir = input_dir.as_ref();
        let output_dir = output_dir.as_ref();

        if !input_dir.exists() {
            anyhow::bail!("输入目录不存在: {:?}", input_dir);
        }

        if !input_dir.is_dir() {
            anyhow::bail!("输入路径不是目录: {:?}", input_dir);
        }

        // 创建输出目录
        std::fs::create_dir_all(output_dir)
            .with_context(|| format!("无法创建输出目录: {:?}", output_dir))?;

        info!("正在扫描目录: {:?}", input_dir);

        // 根据后端选择支持的扩展名
        let extensions = supported_extensions.unwrap_or_else(|| {
            match &self.backend {
                ExtractBackend::Auto | ExtractBackend::Kreuzberg => {
                    // Auto 和 Kreuzberg 支持更多格式
                    vec![
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
                        // Images (OCR)
                        "png", "jpg", "jpeg", "svg", "gif", "bmp", "tiff", "tif", "webp",
                        // Academic
                        "bib", "ipynb", "tex", "latex",
                        // E-books
                        "epub", "mobi", "azw3",
                        // Source code
                        "sql", "py", "js", "ts", "rs", "go", "java", "c", "cpp", "h", "hpp",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect()
                }
                ExtractBackend::Native => {
                    // Native 支持较少的格式
                    vec![
                        "txt", "md", "markdown", "csv", "tsv", "xlsx", "xls", "xlsb", "xlsm",
                        "pdf", "json", "yaml", "yml", "toml", "xml", "html", "htm", "py", "js",
                        "ts", "rs", "go", "java", "c", "cpp", "h", "hpp", "sql", "sh", "bash",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect()
                }
            }
        });

        let mut results = Vec::new();
        let mut file_count = 0u32;
        let mut error_count = 0u32;

        // 遍历输入目录
        for entry in std::fs::read_dir(input_dir)
            .with_context(|| format!("无法读取目录: {:?}", input_dir))?
        {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // 检查文件扩展名
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !extensions.contains(&ext) {
                continue;
            }

            file_count += 1;

            // 生成输出文件路径
            let output_filename = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");

            let output_path =
                output_dir.join(format!("{}.{}", output_filename, output_format.extension()));

            info!("处理文件 [{}]: {:?}", self.backend.name(), path);

            match self.extract_file_to_output(&path, &output_path, output_format.clone()) {
                Ok(result) => {
                    results.push(result);
                }
                Err(e) => {
                    error_count += 1;
                    warn!("提取失败 {:?}: {}", path, e);
                }
            }
        }

        info!(
            "批量提取完成 [{}]: 成功 {} 个, 失败 {} 个, 总计 {} 个",
            self.backend.name(),
            results.len(),
            error_count,
            file_count
        );

        Ok(results)
    }

    /// 格式化输出内容
    fn format_output(content: &str, output_format: ExtractOutputFormat) -> String {
        match output_format {
            ExtractOutputFormat::Markdown => content.to_string(),
            ExtractOutputFormat::Csv => {
                // CSV 格式尝试提取表格（简化实现）
                content.to_string()
            }
            ExtractOutputFormat::Txt => {
                // 纯文本格式：移除 Markdown 标记
                content
                    .lines()
                    .map(|line| {
                        // 移除标题标记
                        let line = line.trim_start_matches('#').trim();
                        // 移除粗体/斜体标记
                        let line = line.replace("**", "").replace("*", "");
                        line
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }

    /// 检测文件是否受当前后端支持
    pub fn is_supported_format<P: AsRef<Path>>(&self, path: P) -> bool {
        let path = path.as_ref();
        match &self.backend {
            ExtractBackend::Auto => {
                // Auto 模式下，只要任一后端支持就算支持
                KreuzbergExtractor::is_supported_format(path)
                    || NativeExtractor::is_supported_format(path)
            }
            ExtractBackend::Kreuzberg => KreuzbergExtractor::is_supported_format(path),
            ExtractBackend::Native => NativeExtractor::is_supported_format(path),
        }
    }

    /// 获取文件类型描述
    pub fn format_description<P: AsRef<Path>>(&self, path: P) -> String {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match &self.backend {
            ExtractBackend::Auto => {
                // 优先使用 Kreuzberg 的描述
                if KreuzbergExtractor::is_supported_format_by_ext(&ext) {
                    kreuzberg_backend::format_description(&ext).to_string()
                } else {
                    NativeExtractor::format_description(&ext).to_string()
                }
            }
            ExtractBackend::Kreuzberg => kreuzberg_backend::format_description(&ext).to_string(),
            ExtractBackend::Native => NativeExtractor::format_description(&ext).to_string(),
        }
    }

    /// 解析实际使用的后端
    fn resolve_backend<P: AsRef<Path>>(&self, path: P) -> &ExtractBackend {
        let path = path.as_ref();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if self.backend == ExtractBackend::Auto {
            // Auto 模式：根据文件格式选择最合适的后端
            if KreuzbergExtractor::is_supported_format_by_ext(&ext) {
                // Kreuzberg 支持更多格式，优先使用
                &ExtractBackend::Kreuzberg
            } else if NativeExtractor::is_supported_format_by_ext(&ext) {
                &ExtractBackend::Native
            } else {
                // 都不支持，默认 Kreuzberg（会报错）
                &ExtractBackend::Kreuzberg
            }
        } else {
            &self.backend
        }
    }
}

/// 获取支持的后端列表
pub fn supported_backends() -> Vec<&'static str> {
    vec!["auto", "kreuzberg", "native"]
}

/// 获取后端描述
pub fn backend_description(backend: &str) -> &str {
    match backend.to_lowercase().as_str() {
        "auto" => "Auto - 自动选择最佳后端（默认）",
        "kreuzberg" => "Kreuzberg - 高性能文档智能库，支持 91+ 种格式（Office、PDF、图片 OCR 等）",
        "native" => "Native - 本地轻量级库，支持常见格式（文本、CSV、Excel、PDF）",
        _ => "未知后端",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_from_str() {
        assert_eq!(
            ExtractBackend::from_str("kreuzberg"),
            Some(ExtractBackend::Kreuzberg)
        );
        assert_eq!(
            ExtractBackend::from_str("native"),
            Some(ExtractBackend::Native)
        );
        assert_eq!(ExtractBackend::from_str("invalid"), None);
    }

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(
            ExtractOutputFormat::from_str("md").unwrap().extension(),
            "md"
        );
        assert_eq!(
            ExtractOutputFormat::from_str("csv").unwrap().extension(),
            "csv"
        );
        assert_eq!(ExtractOutputFormat::from_str("invalid"), None);
    }

    #[test]
    fn test_supported_backends() {
        let backends = supported_backends();
        assert!(backends.contains(&"kreuzberg"));
        assert!(backends.contains(&"native"));
    }
}
