//! Pandoc 文档转换器
//!
//! 通过调用 pandoc 二进制程序进行文件格式转换

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

/// Pandoc 转换器
pub struct PandocConverter;

impl Default for PandocConverter {
    fn default() -> Self {
        Self::new()
    }
}

impl PandocConverter {
    pub fn new() -> Self {
        Self
    }

    /// 检查 pandoc 是否可用
    pub fn is_available() -> bool {
        Command::new("pandoc")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// DOCX 转 Markdown
    pub fn docx_to_markdown<P: AsRef<Path>>(input: P) -> Result<String> {
        let input = input.as_ref();

        let output = Command::new("pandoc")
            .arg(input)
            .arg("-f")
            .arg("docx")
            .arg("-t")
            .arg("markdown")
            .arg("--wrap=none")
            .output()
            .with_context(|| "无法执行 pandoc，请确保已安装 pandoc")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("pandoc 转换失败: {}", stderr);
        }

        String::from_utf8(output.stdout).context("pandoc 输出包含无效 UTF-8")
    }

    /// Markdown 转 DOCX
    pub fn markdown_to_docx<P: AsRef<Path>, Q: AsRef<Path>>(input: P, output: Q) -> Result<()> {
        let input = input.as_ref();
        let output = output.as_ref();

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("无法创建目录: {:?}", parent))?;
        }

        let status = Command::new("pandoc")
            .arg(input)
            .arg("-f")
            .arg("markdown")
            .arg("-t")
            .arg("docx")
            .arg("-o")
            .arg(output)
            .status()
            .with_context(|| "无法执行 pandoc")?;

        if !status.success() {
            anyhow::bail!("pandoc 转换失败");
        }

        Ok(())
    }

    /// 获取 pandoc 版本信息
    pub fn version() -> Result<String> {
        let output = Command::new("pandoc")
            .arg("--version")
            .output()
            .context("无法执行 pandoc")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let first_line = stdout.lines().next().unwrap_or("unknown");
        Ok(first_line.to_string())
    }
}

/// Pandoc-based PDF converter (requires pandoc + xelatex installed).
pub struct PandocPdfConverter;

impl PandocPdfConverter {
    pub fn new() -> Self {
        Self
    }

    /// Check if pandoc is available.
    pub fn is_available() -> bool {
        PandocConverter::is_available()
    }

    /// Convert markdown text to PDF file using pandoc + xelatex.
    pub fn convert_to_file<P: AsRef<Path>>(markdown: &str, output: P) -> Result<()> {
        let output = output.as_ref();
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("无法创建目录: {:?}", parent))?;
        }

        let temp_dir = std::env::temp_dir();
        let temp_md = temp_dir.join(format!("agent_doc_md_{}.md", std::process::id()));
        std::fs::write(&temp_md, markdown).context("无法创建临时文件")?;

        let status = Command::new("pandoc")
            .arg(&temp_md)
            .arg("-f")
            .arg("markdown")
            .arg("-t")
            .arg("pdf")
            .arg("--pdf-engine=xelatex")
            .arg("-V")
            .arg("CJKmainfont=Noto Sans CJK SC")
            .arg("-o")
            .arg(output)
            .status()
            .with_context(|| "无法执行 pandoc，请确保已安装 pandoc 和 xelatex")?;

        std::fs::remove_file(&temp_md).ok();

        if !status.success() {
            anyhow::bail!("pandoc 转换失败");
        }

        Ok(())
    }

    /// Convert markdown file to PDF file using pandoc + xelatex.
    pub fn convert_file<P: AsRef<Path>, Q: AsRef<Path>>(input: P, output: Q) -> Result<()> {
        let markdown = std::fs::read_to_string(input.as_ref())
            .with_context(|| format!("无法读取文件: {:?}", input.as_ref()))?;
        Self::convert_to_file(&markdown, output)
    }
}
