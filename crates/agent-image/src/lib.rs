//! agent-image: 图像处理工具库
//!
//! 提供图像切分、变换（旋转/压缩/裁白边）、PDF 转图片和 OFD 转图片功能。
//! 既可作为库被 Rust 端直接引用，也可通过 CLI 二进制被 Python 端 subprocess 调用。

pub mod ofd2img;
pub mod pdf2img;
pub mod split;
pub mod transform;
