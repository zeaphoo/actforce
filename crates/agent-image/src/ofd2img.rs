//! OFD (Open Fixed-layout Document) 转图片模块
//!
//! 使用 ofd-file crate 将 OFD 文件的每一页渲染为 PNG 图片。
//! 注意：ofd-file 的 PNG 渲染不包含文本（OFD 使用显式字形定位），
//! 如果需要文本在图片中，可考虑先转 PDF 再光栅化。

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView};
use ofd_file::OfdDocument;
use ofd_file::PngOptions;
use std::path::{Path, PathBuf};

/// OFD 转图片选项
#[derive(Debug, Clone)]
pub struct OfdToImgOptions {
    /// DPI（默认 150）
    pub dpi: f32,
    /// 页码范围，如 "1-5"（默认全部）
    pub first_page: Option<u32>,
    pub last_page: Option<u32>,
    /// 是否裁白边
    pub crop_border: bool,
    /// 裁白边白色阈值
    pub white_thresh: u8,
    /// 裁白边 padding
    pub padding: u32,
    /// 输出目录
    pub output_dir: Option<PathBuf>,
    /// 最大像素数，超过时等比例缩放（默认 1600）
    pub max_pixels: u32,
}

impl Default for OfdToImgOptions {
    fn default() -> Self {
        Self {
            dpi: 150.0,
            first_page: None,
            last_page: None,
            crop_border: true,
            white_thresh: 240,
            padding: 2,
            output_dir: None,
            max_pixels: 1600,
        }
    }
}

/// 将 OFD 文件转为 PNG 图片，返回输出文件路径列表
pub fn ofd_to_images(input: &Path, opts: &OfdToImgOptions) -> Result<Vec<PathBuf>> {
    let png_opts = PngOptions::new().with_dpi(opts.dpi);

    let mut doc = OfdDocument::open(input)
        .with_context(|| format!("无法打开 OFD 文件: {}", input.display()))?;

    let page_count = doc.page_count();
    let (first, last) = resolve_page_range(opts.first_page, opts.last_page, page_count);

    // 创建输出目录
    let out_dir = opts.output_dir.clone().unwrap_or_else(|| {
        input
            .parent()
            .unwrap_or(Path::new("."))
            .join("ofd_output")
    });
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("无法创建输出目录: {}", out_dir.display()))?;

    let base_name = input.file_stem().and_then(|s| s.to_str()).unwrap_or("page");
    let mut output_paths = Vec::new();

    for page_idx in first..=last {
        // ofd-file 使用 0-based 索引
        let zero_idx = (page_idx - 1) as usize;
        let out_path = out_dir.join(format!("{}_page_{:04}.png", base_name, page_idx));

        doc.to_png_file(zero_idx, &out_path, &png_opts)
            .with_context(|| format!("渲染第 {} 页失败", page_idx))?;

        // 如果需要裁白边，处理图片
        if opts.crop_border {
            let data = std::fs::read(&out_path)?;
            if let Ok(img) = image::load_from_memory(&data) {
                use crate::transform::{crop_white_border, encode_image, CropWhiteParams, OutputFormat};
                let cropped = crop_white_border(&img, &CropWhiteParams {
                    white_thresh: opts.white_thresh,
                    padding: opts.padding,
                });
                if let Ok(bytes) = encode_image(&cropped, OutputFormat::Png) {
                    std::fs::write(&out_path, bytes)
                        .with_context(|| format!("裁白边第 {} 页失败", page_idx))?;
                }
            }
        }

        // 如果设置了 max_pixels，对超过尺寸的图片进行等比例缩放
        if opts.max_pixels > 0 {
            let data = std::fs::read(&out_path)?;
            if let Ok(img) = image::load_from_memory(&data) {
                let (w, h) = img.dimensions();
                if w > opts.max_pixels || h > opts.max_pixels {
                    let scale = opts.max_pixels as f32 / (w.max(h) as f32);
                    let new_w = (w as f32 * scale).round() as u32;
                    let new_h = (h as f32 * scale).round() as u32;
                    let resized = img.resize(new_w, new_h, image::imageops::FilterType::Lanczos3);
                    let bytes = image_to_png_bytes(&resized)
                        .with_context(|| format!("编码第 {} 页图片失败", page_idx))?;
                    std::fs::write(&out_path, bytes)
                        .with_context(|| format!("写入第 {} 页图片失败", page_idx))?;
                }
            }
        }

        output_paths.push(out_path);
    }

    Ok(output_paths)
}

/// 解析页码范围
fn resolve_page_range(
    first: Option<u32>,
    last: Option<u32>,
    total_pages: usize,
) -> (u32, u32) {
    let total = total_pages as u32;
    let first = first.unwrap_or(1).clamp(1, total);
    let last = last.unwrap_or(total).clamp(first, total);
    (first, last)
}

/// 将 DynamicImage 编码为 PNG 字节
fn image_to_png_bytes(img: &DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
        .with_context(|| "编码图片为 PNG 失败")?;
    Ok(buf)
}
