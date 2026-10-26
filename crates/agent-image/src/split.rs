//! 图像切分模块
//!
//! 支持三种切分策略：
//! - 按空白区域切分（适合长截图/长表格）
//! - 按宽高比等比例切分
//! - 按字节大小切分

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// 切分参数
// ---------------------------------------------------------------------------

/// 空白区域检测参数
#[derive(Debug, Clone)]
pub struct BlankRegionParams {
    /// 最小空白区域高度（像素），低于此高度的空白行组不被视为分隔区
    pub min_gap_height: u32,
    /// 灰度值 >= 此阈值视为白色像素（0-255）
    pub white_thresh: u8,
    /// 一行中白色像素占比超过此值则该行视为空白行（0.0 - 1.0）
    pub blank_row_ratio: f64,
}

impl Default for BlankRegionParams {
    fn default() -> Self {
        Self {
            min_gap_height: 30,
            white_thresh: 250,
            blank_row_ratio: 0.95,
        }
    }
}

// ---------------------------------------------------------------------------
// 核心逻辑
// ---------------------------------------------------------------------------

/// 根据图片形态自动切分。
///
/// - 当图片高度/宽度比 > 3 时，尝试基于空白区域切分；
/// - 如未能找到空白区域则回退为不切分。
pub fn auto_split(img: &DynamicImage, params: &BlankRegionParams) -> Vec<DynamicImage> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return vec![img.clone()];
    }
    if (h as f64 / w as f64) <= 3.0 {
        return vec![img.clone()];
    }

    match split_by_blank_regions(img, params) {
        parts if parts.len() > 1 => parts,
        _ => vec![img.clone()],
    }
}

/// 基于空白区域切分图片
pub fn split_by_blank_regions(
    img: &DynamicImage,
    params: &BlankRegionParams,
) -> Vec<DynamicImage> {
    let regions = find_blank_regions(img, params);
    if regions.is_empty() {
        return vec![img.clone()];
    }

    let (_, h) = img.dimensions();

    // 计算切分点：取每个空白区域的中间行
    let mut cut_points: Vec<u32> = regions
        .iter()
        .map(|(start, end)| (start + end) / 2)
        .collect();
    cut_points.insert(0, 0);
    cut_points.push(h);

    let mut parts = Vec::with_capacity(cut_points.len() - 1);
    for i in 0..cut_points.len() - 1 {
        let start = cut_points[i];
        let end = cut_points[i + 1];
        if end <= start {
            continue;
        }
        let (w, _) = img.dimensions();
        let sub = img.crop_imm(0, start, w, end - start);
        parts.push(sub);
    }
    parts
}

/// 按宽高比等比例切分
///
/// 当高/宽 > `aspect_threshold` 时，按比例分成 N 等份。
pub fn split_by_ratio(img: &DynamicImage, aspect_threshold: f64) -> Vec<DynamicImage> {
    let (w, h) = img.dimensions();
    let ratio = h as f64 / w as f64;
    if ratio <= aspect_threshold {
        return vec![img.clone()];
    }

    let num_parts = std::cmp::max(2, ratio as usize);
    let part_height = h / num_parts as u32;
    let (w, _) = img.dimensions();

    let mut parts = Vec::with_capacity(num_parts);
    for i in 0..num_parts {
        let start = i as u32 * part_height;
        let end = if i < num_parts - 1 {
            (i as u32 + 1) * part_height
        } else {
            h
        };
        let sub = img.crop_imm(0, start, w, end - start);
        parts.push(sub);
    }
    parts
}

/// 按字节大小切分
///
/// 将图片切成若干份，每份编码后的字节大小不超过 `max_bytes`。
pub fn split_by_byte_size(img: &DynamicImage, max_bytes: usize) -> Vec<DynamicImage> {
    let encoded = encode_png(img);
    if encoded.len() <= max_bytes {
        return vec![img.clone()];
    }

    let num_parts = std::cmp::max(2, encoded.len() / max_bytes + 1);
    let (w, h) = img.dimensions();
    let part_height = h / num_parts as u32;

    let mut parts = Vec::with_capacity(num_parts);
    for i in 0..num_parts {
        let start = i as u32 * part_height;
        let end = if i < num_parts - 1 {
            (i as u32 + 1) * part_height
        } else {
            h
        };
        let sub = img.crop_imm(0, start, w, end - start);
        parts.push(sub);
    }
    parts
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 查找图片中的连续空白行区域
fn find_blank_regions(
    img: &DynamicImage,
    params: &BlankRegionParams,
) -> Vec<(u32, u32)> {
    let (w, h) = img.dimensions();
    let mut blank_rows: Vec<u32> = Vec::new();

    for y in 0..h {
        let row = img.crop_imm(0, y, w, 1);
        if is_blank_row(&row, params.white_thresh, params.blank_row_ratio) {
            blank_rows.push(y);
        }
    }

    // 将连续空白行分组
    let mut groups: Vec<Vec<u32>> = Vec::new();
    let mut current: Vec<u32> = Vec::new();

    for &row in &blank_rows {
        if let Some(&last) = current.last() {
            if row > last + 1 {
                if current.len() >= params.min_gap_height as usize {
                    groups.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
        current.push(row);
    }
    if current.len() >= params.min_gap_height as usize {
        groups.push(current);
    }

    // 对每个足够大的空白区域，取中间部分作为切分区域
    groups
        .iter()
        .filter(|g| g.len() >= params.min_gap_height as usize)
        .map(|g| {
            let mid = g.len() / 2;
            let start_idx = mid.saturating_sub(5);
            let end_idx = std::cmp::min(g.len() - 1, mid + 5);
            (g[start_idx], g[end_idx])
        })
        .collect()
}

/// 判断一整行是否为空白行
fn is_blank_row(row: &DynamicImage, white_thresh: u8, blank_row_ratio: f64) -> bool {
    let (w, _) = row.dimensions();

    // 简化计算：采样检测
    let step = std::cmp::max(1, w / 200); // 采样步长
    let mut sample_count: usize = 0;
    let mut sample_white: usize = 0;

    for x in (0..w).step_by(step as usize) {
        let pixel = row.get_pixel(x, 0);
        let Rgba([r, g, b, _]) = pixel;
        sample_count += 3;
        if r >= white_thresh && g >= white_thresh && b >= white_thresh {
            sample_white += 3;
        }
    }

    if sample_count == 0 {
        return true;
    }
    (sample_white as f64 / sample_count as f64) >= blank_row_ratio
}

/// 将 DynamicImage 编码为 PNG 字节
fn encode_png(img: &DynamicImage) -> Vec<u8> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, ImageFormat::Png).unwrap_or_default();
    buf.into_inner()
}

/// 读取图片文件
pub fn read_image<P: AsRef<Path>>(path: P) -> Result<DynamicImage> {
    let path = path.as_ref();
    image::open(path).with_context(|| format!("无法读取图片: {}", path.display()))
}

/// 将切分结果保存到输出目录
pub fn save_parts(
    parts: &[DynamicImage],
    input_path: &Path,
    output_dir: &Path,
) -> Result<Vec<PathBuf>> {
    std::fs::create_dir_all(output_dir)
        .with_context(|| format!("创建输出目录失败: {}", output_dir.display()))?;

    let base_name = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");

    let mut output_paths = Vec::with_capacity(parts.len());
    for (i, part) in parts.iter().enumerate() {
        let name = format!("{}_part_{:04}.png", base_name, i + 1);
        let out_path = output_dir.join(&name);
        part.save_with_format(&out_path, ImageFormat::Png)
            .with_context(|| format!("保存切分图片失败: {}", out_path.display()))?;
        output_paths.push(out_path);
    }
    Ok(output_paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    #[test]
    fn test_split_by_ratio_no_split() {
        let img = DynamicImage::new_rgba8(100, 100);
        let parts = split_by_ratio(&img, 2.5);
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn test_split_by_ratio_tall() {
        let img = DynamicImage::new_rgba8(100, 600);
        let parts = split_by_ratio(&img, 2.5);
        assert!(parts.len() >= 2);
    }

    #[test]
    fn test_split_by_byte_size_small() {
        let img = DynamicImage::new_rgba8(10, 10);
        let parts = split_by_byte_size(&img, 10 * 1024 * 1024);
        assert_eq!(parts.len(), 1);
    }
}
