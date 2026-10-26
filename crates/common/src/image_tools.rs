//! 图像处理工具 (简化版)

use anyhow::Result;
use image::{GenericImageView, ImageFormat};
use std::path::Path;

/// 图像分割器
pub struct ImageSplitter;

impl ImageSplitter {
    /// 将图像分割成网格
    pub fn split_grid<P: AsRef<Path>>(
        input_path: P,
        rows: u32,
        cols: u32,
        output_dir: P,
    ) -> Result<Vec<String>> {
        let img = image::open(&input_path)?;
        let (width, height) = img.dimensions();
        
        let cell_width = width / cols;
        let cell_height = height / rows;
        
        let mut output_paths = Vec::new();
        let base_name = input_path.as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        
        std::fs::create_dir_all(&output_dir)?;
        
        for row in 0..rows {
            for col in 0..cols {
                let x = col * cell_width;
                let y = row * cell_height;
                
                let cropped = img.crop_imm(x, y, cell_width, cell_height);
                
                let output_name = format!("{}_{}_{}.png", base_name, row, col);
                let output_path = output_dir.as_ref().join(&output_name);
                
                cropped.save_with_format(&output_path, ImageFormat::Png)?;
                output_paths.push(output_path.to_string_lossy().to_string());
            }
        }
        
        Ok(output_paths)
    }

    /// 按固定大小分割图像
    pub fn split_by_size<P: AsRef<Path>>(
        input_path: P,
        chunk_width: u32,
        chunk_height: u32,
        output_dir: P,
    ) -> Result<Vec<String>> {
        let img = image::open(&input_path)?;
        let (width, height) = img.dimensions();
        
        let cols = (width + chunk_width - 1) / chunk_width;
        let rows = (height + chunk_height - 1) / chunk_height;
        
        let mut output_paths = Vec::new();
        let base_name = input_path.as_ref()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        
        std::fs::create_dir_all(&output_dir)?;
        
        for row in 0..rows {
            for col in 0..cols {
                let x = col * chunk_width;
                let y = row * chunk_height;
                let w = (chunk_width).min(width - x);
                let h = (chunk_height).min(height - y);
                
                let cropped = img.crop_imm(x, y, w, h);
                
                let output_name = format!("{}_{}_{}.png", base_name, row, col);
                let output_path = output_dir.as_ref().join(&output_name);
                
                cropped.save_with_format(&output_path, ImageFormat::Png)?;
                output_paths.push(output_path.to_string_lossy().to_string());
            }
        }
        
        Ok(output_paths)
    }
}

/// 图像信息
pub struct ImageInfo;

impl ImageInfo {
    /// 获取图像基本信息
    pub fn get_info<P: AsRef<Path>>(path: P) -> Result<ImageMetadata> {
        let img = image::open(&path)?;
        let (width, height) = img.dimensions();
        
        Ok(ImageMetadata {
            width,
            height,
            format: Self::detect_format(&path)?,
        })
    }

    fn detect_format<P: AsRef<Path>>(path: P) -> Result<String> {
        let ext = path.as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown")
            .to_lowercase();
        
        Ok(match ext.as_str() {
            "jpg" | "jpeg" => "JPEG",
            "png" => "PNG",
            "gif" => "GIF",
            "bmp" => "BMP",
            "webp" => "WebP",
            "tiff" | "tif" => "TIFF",
            _ => "Unknown",
        }.to_string())
    }

    /// 调整图像大小
    pub fn resize<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let img = image::open(&input_path)?;
        let resized = img.resize(width, height, image::imageops::FilterType::Lanczos3);
        resized.save(output_path)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
    pub format: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_split_grid() {
        let img = image::DynamicImage::new_rgba8(100, 100);
        let temp_dir = TempDir::new().unwrap();
        let input_path = temp_dir.path().join("test.png");
        img.save(&input_path).unwrap();
        
        let output_dir = temp_dir.path().join("output");
        let result = ImageSplitter::split_grid(&input_path, 2, 2, &output_dir).unwrap();
        
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_get_info() {
        let img = image::DynamicImage::new_rgba8(100, 200);
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("test.png");
        img.save(&path).unwrap();
        
        let info = ImageInfo::get_info(&path).unwrap();
        assert_eq!(info.width, 100);
        assert_eq!(info.height, 200);
        assert_eq!(info.format, "PNG");
    }
}
