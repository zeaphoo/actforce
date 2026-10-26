//! OFD basic value types: Box, Matrix, Color, unit helpers.

use crate::error::{OfdError, Result};

/// OFD 默认使用毫米 (mm) 作为页面坐标单位。
/// 将 mm 转换为像素（按给定 DPI）。
pub fn mm_to_px(mm: f64, dpi: f64) -> f64 {
    mm * dpi / 25.4
}

/// 将 mm 转换为 PDF points（1 pt = 1/72 inch, 1 inch = 25.4 mm）。
pub fn mm_to_pt(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

/// 4-元组矩形，单位 mm。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Boundary {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Boundary {
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<f64> = s
            .split_whitespace()
            .map(|p| p.parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| OfdError::Structure(format!("invalid boundary `{s}`: {e}")))?;
        if parts.len() != 4 {
            return Err(OfdError::Structure(format!(
                "boundary `{s}` must have 4 values"
            )));
        }
        Ok(Self {
            x: parts[0],
            y: parts[1],
            width: parts[2],
            height: parts[3],
        })
    }
}

/// 2x3 仿射变换矩阵：[a b; c d; e f]。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Matrix {
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<f64> = s
            .split_whitespace()
            .map(|p| p.parse::<f64>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| OfdError::Structure(format!("invalid CTM `{s}`: {e}")))?;
        if parts.len() != 6 {
            return Err(OfdError::Structure(format!("CTM `{s}` must have 6 values")));
        }
        Ok(Self {
            a: parts[0], b: parts[1], c: parts[2],
            d: parts[3], e: parts[4], f: parts[5],
        })
    }
}

impl Default for Matrix {
    fn default() -> Self { Self::identity() }
}

/// 颜色（RGB，0-255）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub alpha: u8,
}

impl Default for RgbColor {
    fn default() -> Self {
        Self { r: 0, g: 0, b: 0, alpha: 255 }
    }
}

impl RgbColor {
    pub fn parse(value: &str, alpha: Option<f64>) -> Result<Self> {
        let parts: Vec<u8> = value
            .split_whitespace()
            .map(|p| p.parse::<u8>())
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| OfdError::Structure(format!("invalid color `{value}`: {e}")))?;
        let (r, g, b) = match parts.len() {
            3 => (parts[0], parts[1], parts[2]),
            1 => (parts[0], parts[0], parts[0]),
            _ => return Err(OfdError::Structure(format!("unsupported color `{value}`"))),
        };
        Ok(Self {
            r, g, b,
            alpha: alpha.map(|a| a.clamp(0.0, 255.0) as u8).unwrap_or(255),
        })
    }
}
