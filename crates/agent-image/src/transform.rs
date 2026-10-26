//! 图像变换模块
//!
//! 提供旋转、按短边压缩、裁白边、格式转换等功能。

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, ImageFormat, Rgba};
use std::path::Path;

// ---------------------------------------------------------------------------
// EXIF 方向
// ---------------------------------------------------------------------------

/// 读取图片文件的 EXIF Orientation 并返回对应的旋转角度
/// 
/// 使用 image crate 内置的 EXIF 解析，支持 JPEG 等格式。
pub fn read_exif_orientation(path: &Path) -> Option<RotationAngle> {
    use image::metadata::Orientation;
    use image::ImageDecoder;
    use image::ImageReader;

    let reader = ImageReader::open(path).ok()?;
    let reader = reader.with_guessed_format().ok()?;
    let mut decoder = reader.into_decoder().ok()?;
    let orientation = decoder.orientation().ok()?;

    match orientation {
        Orientation::NoTransforms => Some(RotationAngle::Deg0),
        Orientation::Rotate90 => Some(RotationAngle::Deg90),
        Orientation::Rotate180 => Some(RotationAngle::Deg180),
        Orientation::Rotate270 => Some(RotationAngle::Deg270),
        _ => Some(RotationAngle::Deg0),
    }
}

// ---------------------------------------------------------------------------
// 旋转
// ---------------------------------------------------------------------------

/// 旋转角度（0/90/180/270）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationAngle {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

impl RotationAngle {
    pub fn from_degrees(deg: u32) -> Option<Self> {
        match deg % 360 {
            0 => Some(Self::Deg0),
            90 => Some(Self::Deg90),
            180 => Some(Self::Deg180),
            270 => Some(Self::Deg270),
            _ => None,
        }
    }

    pub fn degrees(&self) -> u32 {
        match self {
            Self::Deg0 => 0,
            Self::Deg90 => 90,
            Self::Deg180 => 180,
            Self::Deg270 => 270,
        }
    }
}

/// 旋转图片（顺时针）
pub fn rotate(img: &DynamicImage, angle: RotationAngle) -> DynamicImage {
    match angle {
        RotationAngle::Deg0 => img.clone(),
        RotationAngle::Deg90 => img.rotate90(),
        RotationAngle::Deg180 => img.rotate180(),
        RotationAngle::Deg270 => img.rotate270(),
    }
}

// ---------------------------------------------------------------------------
// 按短边压缩
// ---------------------------------------------------------------------------

/// 按较短边压缩图片，保持宽高比。
///
/// `target_short_side` 为 0 或超过当前短边则不压缩。
pub fn resize_by_short_side(img: &DynamicImage, target_short_side: u32) -> DynamicImage {
    if target_short_side == 0 {
        return img.clone();
    }

    let (w, h) = img.dimensions();
    let short_side = w.min(h);
    if short_side <= target_short_side {
        return img.clone();
    }

    let ratio = target_short_side as f64 / short_side as f64;
    let new_w = (w as f64 * ratio).round() as u32;
    let new_h = (h as f64 * ratio).round() as u32;

    img.resize_exact(new_w, new_h, image::imageops::FilterType::Lanczos3)
}

// ---------------------------------------------------------------------------
// 裁白边
// ---------------------------------------------------------------------------

/// 裁剪参数
#[derive(Debug, Clone)]
pub struct CropWhiteParams {
    /// 灰度值 >= 此阈值视为白色像素（0-255）
    pub white_thresh: u8,
    /// 裁剪后四周保留的像素边距
    pub padding: u32,
}

impl Default for CropWhiteParams {
    fn default() -> Self {
        Self {
            white_thresh: 240,
            padding: 2,
        }
    }
}

/// 切除图片四周白色边框
pub fn crop_white_border(img: &DynamicImage, params: &CropWhiteParams) -> DynamicImage {
    let (w, h) = img.dimensions();
    let thresh = params.white_thresh;

    // 找出非白色像素的边界
    let mut x_min = w;
    let mut x_max = 0u32;
    let mut y_min = h;
    let mut y_max = 0u32;

    // 采样扫描以提高速度（步长根据图片大小调整）
    let step = std::cmp::max(1, w.min(h) / 500) as u32;

    for y in (0..h).step_by(step as usize) {
        for x in (0..w).step_by(step as usize) {
            let pixel = img.get_pixel(x, y);
            let Rgba([r, g, b, _]) = pixel;
            if r < thresh || g < thresh || b < thresh {
                x_min = x_min.min(x);
                x_max = x_max.max(x);
                y_min = y_min.min(y);
                y_max = y_max.max(y);
            }
        }
    }

    // 如果没有找到非白色像素，返回原图
    if x_min >= x_max || y_min >= y_max {
        return img.clone();
    }

    // 精确扫描边界（在采样边界附近细化）
    // 细化 x_min：在 y_min..=y_max 行中找最小的 x
    let mut exact_x_min = x_min;
    'outer_x_min: for x in 0..x_min {
        for y in (y_min..=y_max.min(h - 1)).step_by(step as usize) {
            let pixel = img.get_pixel(x, y);
            let Rgba([r, g, b, _]) = pixel;
            if r < thresh || g < thresh || b < thresh {
                exact_x_min = x;
                break 'outer_x_min;
            }
        }
    }

    let mut exact_x_max = x_max;
    'outer_x_max: for x in (x_max + 1..w).rev() {
        for y in (y_min..=y_max.min(h - 1)).step_by(step as usize) {
            let pixel = img.get_pixel(x, y);
            let Rgba([r, g, b, _]) = pixel;
            if r < thresh || g < thresh || b < thresh {
                exact_x_max = x;
                break 'outer_x_max;
            }
        }
    }

    let mut exact_y_min = y_min;
    'outer_y_min: for y in 0..y_min {
        for x in (exact_x_min..=exact_x_max.min(w - 1)).step_by(step as usize) {
            let pixel = img.get_pixel(x, y);
            let Rgba([r, g, b, _]) = pixel;
            if r < thresh || g < thresh || b < thresh {
                exact_y_min = y;
                break 'outer_y_min;
            }
        }
    }

    let mut exact_y_max = y_max;
    'outer_y_max: for y in (y_max + 1..h).rev() {
        for x in (exact_x_min..=exact_x_max.min(w - 1)).step_by(step as usize) {
            let pixel = img.get_pixel(x, y);
            let Rgba([r, g, b, _]) = pixel;
            if r < thresh || g < thresh || b < thresh {
                exact_y_max = y;
                break 'outer_y_max;
            }
        }
    }

    // 应用 padding
    let pad = params.padding;
    let crop_x = exact_x_min.saturating_sub(pad);
    let crop_y = exact_y_min.saturating_sub(pad);
    let crop_x_end = (exact_x_max + 1 + pad).min(w);
    let crop_y_end = (exact_y_max + 1 + pad).min(h);

    let crop_w = crop_x_end - crop_x;
    let crop_h = crop_y_end - crop_y;

    if crop_w == 0 || crop_h == 0 {
        return img.clone();
    }

    img.crop_imm(crop_x, crop_y, crop_w, crop_h)
}

// ---------------------------------------------------------------------------
// 格式转换
// ---------------------------------------------------------------------------

/// 输出图片格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Png,
    Jpeg,
}

impl OutputFormat {
    pub fn from_ext(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => Self::Jpeg,
            _ => Self::Png,
        }
    }

    pub fn to_image_format(&self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
        }
    }
}

/// 将图片编码为字节数据
pub fn encode_image(img: &DynamicImage, format: OutputFormat) -> Result<Vec<u8>> {
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, format.to_image_format())
        .context("编码图片失败")?;
    Ok(buf.into_inner())
}

/// 保存图片到文件
pub fn save_image(img: &DynamicImage, path: &Path, format: OutputFormat) -> Result<()> {
    img.save_with_format(path, format.to_image_format())
        .with_context(|| format!("保存图片失败: {}", path.display()))?;
    Ok(())
}

/// 根据文件扩展名推断图片类型
pub fn image_type_from_path(path: &Path) -> OutputFormat {
    path.extension()
        .and_then(|e| e.to_str())
        .map(OutputFormat::from_ext)
        .unwrap_or(OutputFormat::Png)
}

// ---------------------------------------------------------------------------
// 组合预处理
// ---------------------------------------------------------------------------

/// 预处理参数
#[derive(Debug, Clone)]
pub struct PreprocessParams {
    pub rotation: RotationAngle,
    pub target_short_side: u32,
    pub crop_white: bool,
    pub crop_white_params: CropWhiteParams,
}

impl Default for PreprocessParams {
    fn default() -> Self {
        Self {
            rotation: RotationAngle::Deg0,
            target_short_side: 0,
            crop_white: false,
            crop_white_params: CropWhiteParams::default(),
        }
    }
}

/// 对图片执行预处理流水线：旋转 → 裁白边 → 压缩
pub fn preprocess(img: &DynamicImage, params: &PreprocessParams, exif_rotation: Option<RotationAngle>) -> DynamicImage {
    let mut result = img.clone();

    // 1. EXIF 旋转（当用户未指定旋转角度时生效）
    if let Some(angle) = exif_rotation {
        result = rotate(&result, angle);
    }

    // 2. 用户指定的旋转
    if params.rotation != RotationAngle::Deg0 {
        result = rotate(&result, params.rotation);
    }

    // 3. 裁白边
    if params.crop_white {
        result = crop_white_border(&result, &params.crop_white_params);
    }

    // 4. 按短边压缩
    if params.target_short_side > 0 {
        result = resize_by_short_side(&result, params.target_short_side);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    fn white_image(w: u32, h: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        DynamicImage::ImageRgba8(img)
    }

    fn image_with_black_center(w: u32, h: u32) -> DynamicImage {
        let mut img = RgbaImage::from_pixel(w, h, Rgba([255, 255, 255, 255]));
        // 中心画一个黑色方块
        for y in (h / 4)..(3 * h / 4) {
            for x in (w / 4)..(3 * w / 4) {
                img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        DynamicImage::ImageRgba8(img)
    }

    #[test]
    fn test_rotate_0() {
        let img = DynamicImage::new_rgba8(100, 50);
        let rotated = rotate(&img, RotationAngle::Deg0);
        assert_eq!(rotated.dimensions(), (100, 50));
    }

    #[test]
    fn test_rotate_90() {
        let img = DynamicImage::new_rgba8(100, 50);
        let rotated = rotate(&img, RotationAngle::Deg90);
        assert_eq!(rotated.dimensions(), (50, 100));
    }

    #[test]
    fn test_resize_no_shrink() {
        let img = DynamicImage::new_rgba8(100, 50);
        let resized = resize_by_short_side(&img, 200);
        assert_eq!(resized.dimensions(), (100, 50));
    }

    #[test]
    fn test_resize_shrink() {
        let img = DynamicImage::new_rgba8(2000, 1000);
        let resized = resize_by_short_side(&img, 500);
        let (nw, nh) = resized.dimensions();
        assert_eq!(nw.min(nh), 500);
    }

    #[test]
    fn test_crop_white_border() {
        let img = image_with_black_center(100, 100);
        let cropped = crop_white_border(&img, &CropWhiteParams::default());
        let (cw, ch) = cropped.dimensions();
        // 裁剪后应比原图小
        assert!(cw < 100 || ch < 100);
    }

    #[test]
    fn test_crop_white_all_white() {
        let img = white_image(100, 100);
        let cropped = crop_white_border(&img, &CropWhiteParams::default());
        assert_eq!(cropped.dimensions(), (100, 100)); // 全白返回原图
    }
}
