//! PDF 转图片模块
//!
//! 支持多种后端进行 PDF 转图片：
//! - `poppler`: 使用 pdftoppm（默认优先）
//! - `mupdf`: 使用 mutool draw
//! - `auto`: 自动降级策略 poppler → mutool

use anyhow::{anyhow, Context, Result};
use image::GenericImageView;
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

/// PDF 转图片后端
#[derive(Debug, Clone, Default)]
pub enum PdfBackend {
    /// 自动降级策略：poppler → mutool
    #[default]
    Auto,
    /// 使用 poppler 系统工具（pdftoppm）
    Poppler,
    /// 使用 MuPDF 系统工具（mutool draw）
    MuPDF,
}

/// PDF 转图片选项
#[derive(Debug, Clone)]
pub struct PdfToImageOptions {
    /// 渲染 DPI，默认 288
    pub dpi: u32,
    /// 起始页码（从 1 开始）
    pub first_page: Option<u32>,
    /// 结束页码
    pub last_page: Option<u32>,
    /// 是否裁白边
    pub crop_border: bool,
    /// 裁白边参数
    pub white_thresh: u8,
    /// 裁白边 padding
    pub padding: u32,
    /// 使用的后端
    pub backend: PdfBackend,
    /// 输出目录
    pub output_dir: Option<PathBuf>,
    /// 最大像素数，超过时等比例缩放（默认 1600）
    pub max_pixels: u32,
}

impl Default for PdfToImageOptions {
    fn default() -> Self {
        Self {
            dpi: 288,
            first_page: None,
            last_page: None,
            crop_border: true,
            white_thresh: 240,
            padding: 2,
            backend: PdfBackend::default(),
            output_dir: None,
            max_pixels: 1600,
        }
    }
}

/// PDF 转图片的结果
pub struct PdfToImageResult {
    /// 生成的图片路径列表
    pub paths: Vec<PathBuf>,
}

/// PDF 转图片的主入口
/// 根据后端选择调用不同的实现，直接输出到指定目录
pub fn pdf_to_images(pdf_path: &Path, opts: &PdfToImageOptions) -> Result<PdfToImageResult> {
    if !pdf_path.exists() {
        return Err(anyhow!("PDF 文件不存在: {}", pdf_path.display()));
    }

    let out_dir = opts.output_dir.as_ref()
        .ok_or_else(|| anyhow!("必须指定输出目录"))?;
    std::fs::create_dir_all(out_dir)?;

    match &opts.backend {
        PdfBackend::Auto => {
            info!("使用 auto 后端转换 PDF");

            // 1. 尝试 poppler
            if is_poppler_available() {
                match pdf_to_images_poppler(pdf_path, opts, out_dir) {
                    Ok(result) => {
                        info!("auto 后端选择了 poppler");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("poppler 后端失败: {}, 尝试下一个", e);
                    }
                }
            }

            // 2. 尝试 mutool
            if is_mutool_available() {
                match pdf_to_images_mutool(pdf_path, opts, out_dir) {
                    Ok(result) => {
                        info!("auto 后端选择了 mutool");
                        return Ok(result);
                    }
                    Err(e) => {
                        warn!("mutool 后端失败: {}", e);
                    }
                }
            }

            Err(anyhow!("所有 PDF 转图片后端均不可用 (poppler, mutool)"))
        }
        PdfBackend::Poppler => {
            info!("使用 poppler 后端转换 PDF");
            pdf_to_images_poppler(pdf_path, opts, out_dir)
        }
        PdfBackend::MuPDF => {
            info!("使用 mupdf 后端转换 PDF");
            pdf_to_images_mutool(pdf_path, opts, out_dir)
        }
    }
}

/// 使用 mutool (MuPDF) 进行 PDF 转图片
fn pdf_to_images_mutool(pdf_path: &Path, opts: &PdfToImageOptions, out_dir: &Path) -> Result<PdfToImageResult> {
    if !is_mutool_available() {
        return Err(anyhow!("mutool (MuPDF) 未安装"));
    }

    let start = opts.first_page.unwrap_or(1);
    let end = opts.last_page.unwrap_or(999999);

    let base_name = pdf_path.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let output_pattern = out_dir.join(format!("{}_%d.png", base_name));

    let mut cmd = Command::new("mutool");
    cmd.arg("draw")
        .arg("-o")
        .arg(&output_pattern)
        .arg("-r")
        .arg(opts.dpi.to_string())
        .arg("-F")
        .arg("png")
        .arg(pdf_path)
        .arg(format!("{}-{}", start, end));

    debug!("执行 mutool 命令: {:?}", cmd);
    debug!("输出模式: {}", output_pattern.display());

    let output = cmd.output().with_context(|| "执行 mutool 失败")?;

    debug!("mutool 退出状态: {}", output.status);
    debug!("mutool stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("mutool stderr: {}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("mutool 错误: {}", stderr));
    }

    // 扫描输出目录下所有 PNG 文件
    let mut paths: Vec<_> = std::fs::read_dir(out_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
        .map(|e| e.path())
        .collect();
    paths.sort();

    if paths.is_empty() {
        return Err(anyhow!("mutool 未生成任何图片"));
    }

    // 如果需要裁白边，处理每个文件
    if opts.crop_border {
        for path in &paths {
            let data = std::fs::read(path)?;
            if let Ok(img) = image::load_from_memory(&data) {
                use crate::transform::{crop_white_border, encode_image, CropWhiteParams, OutputFormat};
                let cropped = crop_white_border(&img, &CropWhiteParams {
                    white_thresh: opts.white_thresh,
                    padding: opts.padding,
                });
                if let Ok(bytes) = encode_image(&cropped, OutputFormat::Png) {
                    std::fs::write(path, bytes)?;
                }
            }
        }
    }

    // 如果设置了 max_pixels，对超过尺寸的图片进行等比例缩放
    if opts.max_pixels > 0 {
        for path in &paths {
            scale_image_to_max_pixels(path, opts.max_pixels)?;
        }
    }

    Ok(PdfToImageResult { paths })
}

/// 使用系统 poppler 进程进行 PDF 转图片
fn pdf_to_images_poppler(pdf_path: &Path, opts: &PdfToImageOptions, out_dir: &Path) -> Result<PdfToImageResult> {
    if !is_poppler_available() {
        return Err(anyhow!("poppler 工具 (pdftoppm) 未安装"));
    }

    let prefix = out_dir.join("page");

    // 构建 pdftoppm 命令
    let mut cmd = Command::new("pdftoppm");
    cmd.arg("-png")
        .arg("-r")
        .arg(opts.dpi.to_string())
        .arg("-f")
        .arg(opts.first_page.unwrap_or(1).to_string())
        .arg("-l")
        .arg(opts.last_page.unwrap_or(999999).to_string())
        .arg(pdf_path)
        .arg(&prefix);

    debug!("执行 pdftoppm 命令: {:?}", cmd);
    debug!("PDF 路径: {}", pdf_path.display());
    debug!("输出前缀: {}", prefix.display());
    debug!("DPI: {}", opts.dpi);
    debug!("页码范围: {}-{}", opts.first_page.unwrap_or(1), opts.last_page.unwrap_or(999999));

    let output = cmd.output().with_context(|| "执行 pdftoppm 失败")?;

    debug!("pdftoppm 退出状态: {}", output.status);
    debug!("pdftoppm stdout: {}", String::from_utf8_lossy(&output.stdout));
    debug!("pdftoppm stderr: {}", String::from_utf8_lossy(&output.stderr));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("pdftoppm 错误: {}", stderr));
    }

    // 扫描输出目录下所有 PNG 文件
    let mut paths: Vec<_> = std::fs::read_dir(out_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "png"))
        .map(|e| e.path())
        .collect();
    paths.sort();

    if paths.is_empty() {
        return Err(anyhow!("poppler 未生成任何图片"));
    }

    // 如果需要裁白边，处理每个文件
    if opts.crop_border {
        for path in &paths {
            let data = std::fs::read(path)?;
            if let Ok(img) = image::load_from_memory(&data) {
                use crate::transform::{crop_white_border, encode_image, CropWhiteParams, OutputFormat};
                let cropped = crop_white_border(&img, &CropWhiteParams {
                    white_thresh: opts.white_thresh,
                    padding: opts.padding,
                });
                if let Ok(bytes) = encode_image(&cropped, OutputFormat::Png) {
                    std::fs::write(path, bytes)?;
                }
            }
        }
    }

    // 如果设置了 max_pixels，对超过尺寸的图片进行等比例缩放
    if opts.max_pixels > 0 {
        for path in &paths {
            scale_image_to_max_pixels(path, opts.max_pixels)?;
        }
    }

    Ok(PdfToImageResult { paths })
}

/// 检查系统是否安装了 poppler
pub fn is_poppler_available() -> bool {
    // pdftoppm 不支持 --version，使用 -h 检测
    Command::new("pdftoppm")
        .arg("-h")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 检查系统是否安装了 MuPDF (mutool)
pub fn is_mutool_available() -> bool {
    Command::new("mutool")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// 获取 PDF 页数
pub fn get_pdf_page_count(pdf_path: &Path) -> Result<u32> {
    if !pdf_path.exists() {
        return Err(anyhow!("PDF 文件不存在: {}", pdf_path.display()));
    }

    // 尝试使用 poppler 的 pdfinfo
    if is_poppler_available() {
        let output = Command::new("pdfinfo").arg(pdf_path).output();

        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if line.starts_with("Pages:") {
                        if let Some(pages_str) = line.split(':').nth(1) {
                            if let Ok(pages) = pages_str.trim().parse() {
                                return Ok(pages);
                            }
                        }
                    }
                }
            }
        }
    }

    // 尝试使用 mutool
    if is_mutool_available() {
        let output = Command::new("mutool")
            .arg("info")
            .arg(pdf_path)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                for line in text.lines() {
                    if line.starts_with("Pages:") || line.starts_with("  ") && line.contains("page") {
                        // mutool info 输出格式可能不同，尝试解析
                        if let Ok(pages) = text.trim().parse() {
                            return Ok(pages);
                        }
                    }
                }
            }
        }
    }

    Err(anyhow!("无法获取 PDF 页数"))
}

/// 等比例缩放图片以不超过 max_pixels
fn scale_image_to_max_pixels(path: &Path, max_pixels: u32) -> Result<()> {
    let data = std::fs::read(path)?;
    if let Ok(img) = image::load_from_memory(&data) {
        let (w, h) = img.dimensions();
        if w > max_pixels || h > max_pixels {
            let scale = max_pixels as f32 / (w.max(h) as f32);
            let new_w = (w as f32 * scale).round() as u32;
            let new_h = (h as f32 * scale).round() as u32;
            let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
            if let Ok(bytes) = crate::transform::encode_image(&resized, crate::transform::OutputFormat::Png) {
                std::fs::write(path, bytes)?;
                debug!("图片 {} 从 {}x{} 缩放到 {}x{}", path.display(), w, h, new_w, new_h);
            }
        }
    }
    Ok(())
}
