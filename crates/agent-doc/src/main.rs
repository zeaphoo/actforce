mod chm_extractor;
mod convert;
mod extract;
mod doc_preprocess;

use anyhow::Result;
use chm_extractor::{default_output_dir, ChmExtractor};
use clap::{Parser, Subcommand};
use convert::{ConvertBackend, DocumentConverter, PandocConverter};
use extract::{
    backend_description, supported_backends, DocumentExtractor, ExtractBackend, ExtractOutputFormat,
};
use std::path::PathBuf;
use tracing::info;

#[derive(Parser)]
#[command(name = "agent-doc")]
#[command(author, version, about = "AgentDoc - 文档格式转换工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 启用详细日志输出
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 提取 CHM 文件内容到目录（目录不能已存在）
    ExtractChm {
        /// CHM 文件路径
        #[arg(value_name = "CHM_FILE")]
        chm_file: PathBuf,

        /// 输出目录路径（默认：<文件名>_html）
        #[arg(short, long, value_name = "OUTPUT_DIR")]
        output: Option<PathBuf>,
    },

    /// 列出 CHM 文件中的所有条目
    List {
        /// CHM 文件路径
        #[arg(value_name = "CHM_FILE")]
        chm_file: PathBuf,
    },

    /// 提取文档内容为纯文本格式（支持多种格式）
    Extract {
        /// 输入文件或目录路径
        #[arg(value_name = "INPUT", required = false)]
        input: Option<PathBuf>,

        /// 输出文件或目录路径（默认：输入文件同目录或 ./extracted/）
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,

        /// 输出格式：md, csv, txt（默认：md）
        #[arg(short, long, value_name = "FORMAT", default_value = "md")]
        format: String,

        /// 提取后端：auto（默认）, kreuzberg, native
        #[arg(long, value_name = "BACKEND", default_value = "auto")]
        backend: String,

        /// 列出所有支持的后端
        #[arg(long)]
        list_backends: bool,

        /// 递归处理子目录
        #[arg(long, default_value = "false")]
        recursive: bool,
    },

    /// 检测文件格式是否支持提取
    CheckFormat {
        /// 文件路径
        #[arg(value_name = "FILE")]
        file: PathBuf,

        /// 检查指定后端（kreuzberg 或 native）
        #[arg(long, value_name = "BACKEND", default_value = "kreuzberg")]
        backend: String,
    },

    /// 转换文档格式（DOCX <-> Markdown）
    Convert {
        /// 输入文件路径
        #[arg(value_name = "INPUT")]
        input: Option<PathBuf>,

        /// 输出文件路径
        #[arg(short, long, value_name = "OUTPUT")]
        output: Option<PathBuf>,

        /// 转换后端：auto（默认）, native, pandoc
        #[arg(long, value_name = "BACKEND", default_value = "auto")]
        backend: String,

        /// 列出支持的转换格式
        #[arg(long)]
        list_formats: bool,

        /// 列出可用的后端
        #[arg(long)]
        list_backends: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // 初始化日志
    let level = if cli.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };
    tracing_subscriber::fmt().with_max_level(level).init();

    match cli.command {
        Commands::ExtractChm { chm_file, output } => extract_chm(&chm_file, output),
        Commands::List { chm_file } => list_chm_contents(&chm_file),
        Commands::Extract {
            input,
            output,
            format,
            backend,
            list_backends,
            recursive,
        } => {
            if list_backends {
                return list_supported_backends();
            }
            extract_document(&input, output, &format, &backend, recursive)
        }
        Commands::CheckFormat { file, backend } => check_format(&file, &backend),
        Commands::Convert {
            input,
            output,
            backend,
            list_formats,
            list_backends,
        } => {
            if list_formats {
                return list_supported_formats();
            }
            if list_backends {
                return list_convert_backends();
            }
            convert_document(&input, output, &backend)
        }
    }
}

/// 列出支持的后端
fn list_supported_backends() -> Result<()> {
    println!("\n支持的提取后端:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for name in supported_backends() {
        println!("  {:<12} - {}", name, backend_description(name));
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n默认后端: auto");
    println!("使用 --backend <name> 切换后端");

    Ok(())
}

/// 提取 CHM 文件
fn extract_chm(chm_file: &PathBuf, output_dir: Option<PathBuf>) -> Result<()> {
    if !chm_file.exists() {
        anyhow::bail!("CHM 文件不存在: {:?}", chm_file);
    }

    // 与 archmage file2dir 逻辑一致：默认输出到 <stem>_html 目录
    let output_dir = output_dir.unwrap_or_else(|| default_output_dir(chm_file));

    info!("正在提取 CHM 文件: {:?}", chm_file);
    info!("输出目录: {:?}", output_dir);

    let mut extractor = ChmExtractor::open(chm_file)?;
    let stats = extractor.extract(&output_dir)?;

    println!("\n提取完成！");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("HTML 文件:    {}", stats.html_files);
    println!("其他文件:     {}", stats.other_files);
    if stats.skipped_files > 0 {
        println!("跳过文件:     {} (路径不安全)", stats.skipped_files);
    }
    println!("总文件数:     {}", stats.total_files);
    println!(
        "总大小:       {:.2} MB",
        stats.total_size as f64 / 1024.0 / 1024.0
    );
    println!("输出目录:     {:?}", output_dir);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

/// 列出 CHM 文件内容
fn list_chm_contents(chm_file: &PathBuf) -> Result<()> {
    if !chm_file.exists() {
        anyhow::bail!("CHM 文件不存在: {:?}", chm_file);
    }

    info!("正在列出 CHM 文件内容: {:?}", chm_file);

    let mut extractor = ChmExtractor::open(chm_file)?;
    let contents = extractor.list_contents()?;

    println!("\nCHM 文件内容:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for path in &contents {
        println!("{}", path);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("总计 {} 个条目", contents.len());

    Ok(())
}

/// 提取文档内容
fn extract_document(
    input: &Option<PathBuf>,
    output: Option<PathBuf>,
    format: &str,
    backend_str: &str,
    _recursive: bool,
) -> Result<()> {
    let input = input.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "缺少输入路径参数。请提供文件/目录路径，或使用 --list-backends 查看支持的后端"
        )
    })?;

    if !input.exists() {
        anyhow::bail!("输入路径不存在: {:?}", input);
    }

    // 解析后端
    let backend = ExtractBackend::from_str(backend_str).ok_or_else(|| {
        anyhow::anyhow!(
            "不支持的后端: {} (支持: {})",
            backend_str,
            supported_backends().join(", ")
        )
    })?;

    // 解析输出格式
    let output_format = ExtractOutputFormat::from_str(format)
        .ok_or_else(|| anyhow::anyhow!("不支持的输出格式: {} (支持: md, csv, txt)", format))?;

    // 创建提取器
    let extractor = DocumentExtractor::new(backend.clone());

    if input.is_file() {
        // 单文件提取
        let output_path = output.unwrap_or_else(|| {
            let stem = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            input
                .parent()
                .unwrap_or(PathBuf::from(".").as_ref())
                .join(format!("{}.{}", stem, output_format.extension()))
        });

        info!("正在提取文件 [{}]: {:?}", backend.name(), input);
        info!("输出格式: {}", format);
        info!("输出路径: {:?}", output_path);

        let result =
            extractor.extract_file_to_output(input, &output_path, output_format.clone())?;

        println!("\n提取完成！");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("源文件:       {}", result.source_file);
        println!("后端:         {}", result.backend);
        println!("文件大小:     {:.2} KB", result.file_size as f64 / 1024.0);
        println!(
            "内容大小:     {:.2} KB",
            result.content.len() as f64 / 1024.0
        );
        println!("输出路径:     {:?}", output_path);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    } else if input.is_dir() {
        // 目录批量提取
        let output_dir = output.unwrap_or_else(|| input.join("extracted"));

        info!("正在批量提取目录 [{}]: {:?}", backend.name(), input);
        info!("输出目录: {:?}", output_dir);

        let results =
            extractor.extract_directory(input, &output_dir, output_format.clone(), None)?;

        println!("\n批量提取完成！");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("后端:         {}", backend.name());
        println!("成功文件数:   {}", results.len());
        println!("输出格式:     {}", format);
        println!("输出目录:     {:?}", output_dir);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    } else {
        anyhow::bail!("输入路径既不是文件也不是目录: {:?}", input);
    }

    Ok(())
}

/// 检查文件格式是否支持
fn check_format(file: &PathBuf, backend_str: &str) -> Result<()> {
    if !file.exists() {
        anyhow::bail!("文件不存在: {:?}", file);
    }

    // 解析后端
    let backend = ExtractBackend::from_str(backend_str).ok_or_else(|| {
        anyhow::anyhow!(
            "不支持的后端: {} (支持: {})",
            backend_str,
            supported_backends().join(", ")
        )
    })?;

    let extractor = DocumentExtractor::new(backend.clone());
    let is_supported = extractor.is_supported_format(file);
    let ext = file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let description = extractor.format_description(file);

    println!("\n文件检测结果 [{}]:", backend.name());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("文件路径:     {:?}", file);
    println!("扩展名:       .{}", ext);
    println!("类型描述:     {}", description);
    println!("支持提取:     {}", if is_supported { "是" } else { "否" });
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if is_supported {
        println!(
            "提示: 可以使用 'agent-doc extract --backend {}' 命令提取此文件内容",
            backend.name()
        );
    } else {
        println!("提示: 此格式可能不受支持，建议尝试其他工具");
    }

    Ok(())
}

/// 列出支持的转换格式
fn list_supported_formats() -> Result<()> {
    println!("\n支持的转换格式:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    for (input, output) in DocumentConverter::supported_conversions() {
        println!("  .{:<12} -> .{}", input, output);
    }
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n使用方法:");
    println!("  agent-doc convert input.docx -o output.md");
    println!("  agent-doc convert input.md -o output.docx");
    println!("\n使用 --list-backends 查看可用的转换后端");

    Ok(())
}

/// 列出转换后端
fn list_convert_backends() -> Result<()> {
    println!("\n可用的转换后端:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  {:<12} - {}", "auto", "自动选择最佳后端（默认）");
    println!("  {:<12} - {}", "native", "本地库实现");
    println!("  {:<12} - {}", "pandoc", "Pandoc 二进制程序（需安装）");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 检查 pandoc 是否可用
    if PandocConverter::is_available() {
        if let Ok(version) = PandocConverter::version() {
            println!("\nPandoc 状态: 已安装 ({})", version);
        }
    } else {
        println!("\nPandoc 状态: 未安装");
        println!("安装方法:");
        println!("  Ubuntu/Debian: sudo apt install pandoc");
        println!("  macOS:         brew install pandoc");
        println!("  其他:          https://pandoc.org/installing.html");
    }

    Ok(())
}

/// 转换文档
fn convert_document(
    input: &Option<PathBuf>,
    output: Option<PathBuf>,
    backend_str: &str,
) -> Result<()> {
    // 解析后端
    let backend = ConvertBackend::from_str(backend_str)
        .ok_or_else(|| anyhow::anyhow!("不支持的后端: {} (支持: native, pandoc)", backend_str))?;

    let input = input.as_ref().ok_or_else(|| {
        anyhow::anyhow!("缺少输入文件路径。请提供文件路径或使用 --list-formats 查看支持的格式")
    })?;

    if !input.exists() {
        anyhow::bail!("输入文件不存在: {:?}", input);
    }

    if !input.is_file() {
        anyhow::bail!("输入路径不是文件: {:?}", input);
    }

    let input_ext = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 确定输出路径
    let output_path = output.unwrap_or_else(|| {
        let stem = input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");

        let output_ext = match input_ext.as_str() {
            "docx" => "md",
            "md" | "markdown" => "pdf",  // default to pdf for markdown input
            _ => "txt",
        };

        input
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{}.{}", stem, output_ext))
    });

    let output_ext = output_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // 检查是否支持转换
    if !DocumentConverter::is_conversion_supported(&input_ext, &output_ext) {
        anyhow::bail!(
            "不支持的转换: .{} -> .{} (使用 --list-formats 查看支持的格式)",
            input_ext,
            output_ext
        );
    }

    info!("正在转换文件 [{}]: {:?}", backend.name(), input);
    info!("输出路径: {:?}", output_path);

    // 创建转换器
    let converter = DocumentConverter::new(backend.clone());

    // 执行转换
    converter.convert_file(input, &output_path)?;

    let file_size = std::fs::metadata(input).map(|m| m.len()).unwrap_or(0);
    let output_size = std::fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    println!("\n转换完成！");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("源文件:       {:?}", input);
    println!("源文件大小:   {:.2} KB", file_size as f64 / 1024.0);
    println!("输出文件:     {:?}", output_path);
    println!("输出文件大小: {:.2} KB", output_size as f64 / 1024.0);
    println!("后端:         {}", backend.name());
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}
