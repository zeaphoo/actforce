//! agent-image CLI 入口
//!
//! 用法：
//!   agent-image split   --input <PATH> [--method blank|ratio|size] [--output-dir <DIR>]
//!   agent-image transform --input <PATH> [--rotate 0|90|180|270] [--short-side <N>] [--crop-white] [--output <PATH>]
//!   agent-image pdf2img --input <PATH> [--dpi 288] [--pages 1-10] [--crop-white] [--output-dir <DIR>]
//!   agent-image ofd2img --input <PATH> [--dpi 150] [--pages 1-10] [--output-dir <DIR>]
//!   agent-image info    --input <PATH>

use anyhow::Result;
use clap::{Parser, Subcommand};
use image::GenericImageView;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "agent-image", version, about = "图像处理 CLI 工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 切分图片
    Split {
        /// 输入图片路径
        #[arg(short, long)]
        input: PathBuf,

        /// 切分方式：blank（空白区域）、ratio（按宽高比）、size（按字节大小）
        #[arg(short, long, default_value = "blank")]
        method: String,

        /// 输出目录（默认为输入图片同目录下的 split_output/）
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// [blank] 最小空白区域高度（像素）
        #[arg(long, default_value_t = 30)]
        min_gap_height: u32,

        /// [blank] 白色阈值（0-255）
        #[arg(long, default_value_t = 235)]
        white_thresh: u8,

        /// [blank] 空白行占比阈值
        #[arg(long, default_value_t = 0.95)]
        blank_row_ratio: f64,

        /// [ratio] 宽高比阈值
        #[arg(long, default_value_t = 2.5)]
        aspect_threshold: f64,

        /// [size] 每份最大字节数
        #[arg(long, default_value_t = 10 * 1024 * 1024)]
        max_bytes: usize,

        /// 裁白边
        #[arg(long, default_value_t = true)]
        crop_white: bool,

        /// 裁白边白色阈值
        #[arg(long, default_value_t = 235)]
        crop_white_thresh: u8,

        /// 裁白边 padding
        #[arg(long, default_value_t = 2)]
        crop_padding: u32,

        /// 最大像素数，超过时等比例缩放（默认 1600，0 表示不缩放）
        #[arg(long, default_value_t = 1600)]
        max_pixels: u32,
    },

    /// 图片变换（旋转/压缩/裁白边）
    Transform {
        /// 输入图片路径
        #[arg(short, long)]
        input: PathBuf,

        /// 旋转角度 (0/90/180/270)
        #[arg(long, default_value_t = 0)]
        rotate: u32,

        /// 按短边压缩到的像素值（0 表示不压缩）
        #[arg(long, default_value_t = 0)]
        short_side: u32,

        /// 裁白边
        #[arg(long, default_value_t = true)]
        crop_white: bool,

        /// 裁白边白色阈值
        #[arg(long, default_value_t = 240)]
        crop_white_thresh: u8,

        /// 裁白边 padding
        #[arg(long, default_value_t = 2)]
        crop_padding: u32,

        /// 输出路径（默认覆盖原图）
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// 最大像素数，超过时等比例缩放（默认 1600，0 表示不缩放）
        #[arg(long, default_value_t = 1600)]
        max_pixels: u32,
    },

    /// PDF 转图片
    #[command(name = "pdf2img")]
    Pdf2Img {
        /// 输入 PDF 路径
        #[arg(short, long)]
        input: PathBuf,

        /// DPI
        #[arg(long, default_value_t = 288)]
        dpi: u32,

        /// 页码范围，如 "1-5"
        #[arg(long)]
        pages: Option<String>,

        /// 裁白边
        #[arg(long, default_value_t = true)]
        crop_white: bool,

        /// 裁白边白色阈值
        #[arg(long, default_value_t = 240)]
        crop_white_thresh: u8,

        /// 裁白边 padding
        #[arg(long, default_value_t = 2)]
        crop_padding: u32,

        /// 输出目录
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// 使用的后端：auto（默认）、poppler、mupdf
        #[arg(long, default_value = "auto")]
        backend: String,

        /// 最大像素数，超过时等比例缩放（默认 1600，0 表示不缩放）
        #[arg(long, default_value_t = 1600)]
        max_pixels: u32,
    },

    /// OFD 转图片
    #[command(name = "ofd2img")]
    Ofd2Img {
        /// 输入 OFD 路径
        #[arg(short, long)]
        input: PathBuf,

        /// DPI
        #[arg(long, default_value_t = 150)]
        dpi: u32,

        /// 页码范围，如 "1-5"
        #[arg(long)]
        pages: Option<String>,

        /// 输出目录
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// 裁白边
        #[arg(long, default_value_t = true)]
        crop_white: bool,

        /// 裁白边白色阈值
        #[arg(long, default_value_t = 240)]
        crop_white_thresh: u8,

        /// 裁白边 padding
        #[arg(long, default_value_t = 2)]
        crop_padding: u32,

        /// 最大像素数，超过时等比例缩放（默认 1600，0 表示不缩放）
        #[arg(long, default_value_t = 1600)]
        max_pixels: u32,
    },

    /// 获取图片信息
    Info {
        /// 输入图片路径
        #[arg(short, long)]
        input: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Split {
            input,
            method,
            output_dir,
            min_gap_height,
            white_thresh,
            blank_row_ratio,
            aspect_threshold,
            max_bytes,
            crop_white,
            crop_white_thresh,
            crop_padding,
            max_pixels,
        } => cmd_split(
            &input,
            &method,
            output_dir,
            min_gap_height,
            white_thresh,
            blank_row_ratio,
            aspect_threshold,
            max_bytes,
            crop_white,
            crop_white_thresh,
            crop_padding,
            max_pixels,
        ),

        Commands::Transform {
            input,
            rotate,
            short_side,
            crop_white,
            crop_white_thresh,
            crop_padding,
            output,
            max_pixels,
        } => cmd_transform(
            &input,
            rotate,
            short_side,
            crop_white,
            crop_white_thresh,
            crop_padding,
            output,
            max_pixels,
        ),

        Commands::Pdf2Img {
            input,
            dpi,
            pages,
            crop_white,
            crop_white_thresh,
            crop_padding,
            output_dir,
            backend,
            max_pixels,
        } => cmd_pdf2img(
            &input,
            dpi,
            pages.as_deref(),
            crop_white,
            crop_white_thresh,
            crop_padding,
            output_dir,
            &backend,
            max_pixels,
        ),

        Commands::Ofd2Img {
            input,
            dpi,
            pages,
            output_dir,
            crop_white,
            crop_white_thresh,
            crop_padding,
            max_pixels,
        } => cmd_ofd2img(&input, dpi, pages.as_deref(), output_dir, crop_white, crop_white_thresh, crop_padding, max_pixels),

        Commands::Info { input } => cmd_info(&input),
    }
}

// ---------------------------------------------------------------------------
// 子命令实现
// ---------------------------------------------------------------------------

fn cmd_split(
    input: &PathBuf,
    method: &str,
    output_dir: Option<PathBuf>,
    min_gap_height: u32,
    white_thresh: u8,
    blank_row_ratio: f64,
    aspect_threshold: f64,
    max_bytes: usize,
    crop_white: bool,
    crop_white_thresh: u8,
    crop_padding: u32,
    max_pixels: u32,
) -> Result<()> {
    let img = agent_image::split::read_image(input)?;

    let parts = match method {
        "blank" => {
            let params = agent_image::split::BlankRegionParams {
                min_gap_height,
                white_thresh,
                blank_row_ratio,
            };
            agent_image::split::auto_split(&img, &params)
        }
        "ratio" => agent_image::split::split_by_ratio(&img, aspect_threshold),
        "size" => agent_image::split::split_by_byte_size(&img, max_bytes),
        _ => anyhow::bail!("未知切分方式: {}，支持 blank/ratio/size", method),
    };

    let out_dir = output_dir.unwrap_or_else(|| {
        input
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("split_output")
    });

    let paths = agent_image::split::save_parts(&parts, input, &out_dir)?;

    // 对分割后的图片进行裁白边和缩放
    use image::GenericImageView;
    for path in &paths {
        let data = std::fs::read(path)?;
        if let Ok(img) = image::load_from_memory(&data) {
            let mut processed = img;

            // 裁白边
            if crop_white {
                use agent_image::transform::{crop_white_border, CropWhiteParams};
                processed = crop_white_border(&processed, &CropWhiteParams {
                    white_thresh: crop_white_thresh,
                    padding: crop_padding,
                });
            }

            // 缩放
            if max_pixels > 0 {
                let (w, h) = processed.dimensions();
                if w > max_pixels || h > max_pixels {
                    let scale = max_pixels as f32 / (w.max(h) as f32);
                    let new_w = (w as f32 * scale).round() as u32;
                    let new_h = (h as f32 * scale).round() as u32;
                    processed = processed.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
                }
            }

            // 写回文件
            if crop_white || max_pixels > 0 {
                use agent_image::transform::{encode_image, OutputFormat};
                if let Ok(bytes) = encode_image(&processed, OutputFormat::Png) {
                    std::fs::write(path, bytes)?;
                }
            }
        }
    }

    // 输出 JSON 结果（方便 Python 端解析）
    let result: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    println!("{}", serde_json::to_string(&result)?);

    tracing::info!("切分完成: {} → {} 份", input.display(), paths.len());
    Ok(())
}

fn cmd_transform(
    input: &PathBuf,
    rotate_deg: u32,
    short_side: u32,
    crop_white: bool,
    crop_white_thresh: u8,
    crop_padding: u32,
    output: Option<PathBuf>,
    max_pixels: u32,
) -> Result<()> {
    let img = agent_image::split::read_image(input)?;

    let rotation = agent_image::transform::RotationAngle::from_degrees(rotate_deg)
        .unwrap_or(agent_image::transform::RotationAngle::Deg0);

    // 当用户未指定旋转角度时，读取 EXIF Orientation
    let exif_rotation = if rotate_deg == 0 {
        agent_image::transform::read_exif_orientation(input)
    } else {
        None
    };

    let params = agent_image::transform::PreprocessParams {
        rotation,
        target_short_side: short_side,
        crop_white,
        crop_white_params: agent_image::transform::CropWhiteParams {
            white_thresh: crop_white_thresh,
            padding: crop_padding,
        },
    };

    let mut result = agent_image::transform::preprocess(&img, &params, exif_rotation);

    // 缩放
    if max_pixels > 0 {
        use image::GenericImageView;
        let (w, h) = result.dimensions();
        if w > max_pixels || h > max_pixels {
            let scale = max_pixels as f32 / (w.max(h) as f32);
            let new_w = (w as f32 * scale).round() as u32;
            let new_h = (h as f32 * scale).round() as u32;
            result = result.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
        }
    }

    let out_path = output.unwrap_or_else(|| input.clone());
    let fmt = agent_image::transform::image_type_from_path(&out_path);
    agent_image::transform::save_image(&result, &out_path, fmt)?;

    tracing::info!("变换完成: {} → {}", input.display(), out_path.display());
    Ok(())
}

fn cmd_pdf2img(
    input: &PathBuf,
    dpi: u32,
    pages: Option<&str>,
    crop_white: bool,
    crop_white_thresh: u8,
    crop_padding: u32,
    output_dir: Option<PathBuf>,
    backend: &str,
    max_pixels: u32,
) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("Input file not found: {}", input.display());
    }
    if !input.is_file() {
        anyhow::bail!("Input path is not a file: {}", input.display());
    }

    let (first_page, last_page) = parse_page_range(pages);

    let pdf_backend = match backend.to_lowercase().as_str() {
        "auto" => agent_image::pdf2img::PdfBackend::Auto,
        "poppler" => agent_image::pdf2img::PdfBackend::Poppler,
        "mupdf" | "mutool" => agent_image::pdf2img::PdfBackend::MuPDF,
        _ => {
            eprintln!("警告: 未知后端 '{}', 使用 auto", backend);
            agent_image::pdf2img::PdfBackend::Auto
        }
    };

    let opts = agent_image::pdf2img::PdfToImageOptions {
        dpi,
        first_page,
        last_page,
        crop_border: crop_white,
        white_thresh: crop_white_thresh,
        padding: crop_padding,
        backend: pdf_backend,
        output_dir: output_dir.clone(),
        max_pixels,
    };

    let result = agent_image::pdf2img::pdf_to_images(input, &opts)?;

    let output_paths: Vec<String> = result.paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    println!("{}", serde_json::to_string(&output_paths)?);
    tracing::info!("PDF 转图片完成: {} → {} 页", input.display(), result.paths.len());
    Ok(())
}

fn cmd_ofd2img(
    input: &PathBuf,
    dpi: u32,
    pages: Option<&str>,
    output_dir: Option<PathBuf>,
    crop_white: bool,
    crop_white_thresh: u8,
    crop_padding: u32,
    max_pixels: u32,
) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("Input file not found: {}", input.display());
    }
    if !input.is_file() {
        anyhow::bail!("Input path is not a file: {}", input.display());
    }

    let (first_page, last_page) = parse_page_range(pages);

    let opts = agent_image::ofd2img::OfdToImgOptions {
        dpi: dpi as f32,
        first_page,
        last_page,
        crop_border: crop_white,
        white_thresh: crop_white_thresh,
        padding: crop_padding,
        output_dir,
        max_pixels,
    };

    let output_paths = agent_image::ofd2img::ofd_to_images(input, &opts)?;

    let paths_str: Vec<String> = output_paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();

    println!("{}", serde_json::to_string(&paths_str)?);
    tracing::info!("OFD 转图片完成: {} → {} 页", input.display(), paths_str.len());
    Ok(())
}

fn cmd_info(input: &PathBuf) -> Result<()> {
    if !input.exists() {
        anyhow::bail!("Input file not found: {}", input.display());
    }
    let img = agent_image::split::read_image(input)?;
    let (w, h) = img.dimensions();
    let fmt = input
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("unknown");

    let info = serde_json::json!({
        "path": input.to_string_lossy(),
        "width": w,
        "height": h,
        "format": fmt,
    });

    println!("{}", serde_json::to_string_pretty(&info)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// 辅助
// ---------------------------------------------------------------------------

fn parse_page_range(pages: Option<&str>) -> (Option<u32>, Option<u32>) {
    pages
        .and_then(|s| {
            let parts: Vec<&str> = s.split('-').collect();
            match parts.len() {
                1 => {
                    let n = parts[0].parse().ok()?;
                    Some((Some(n), Some(n)))
                }
                2 => {
                    let a = parts[0].parse().ok()?;
                    let b = parts[1].parse().ok()?;
                    Some((Some(a), Some(b)))
                }
                _ => None,
            }
        })
        .unwrap_or((None, None))
}
