//! Cross-platform screen capture and info.
//!
//! - Linux: uses x11rb to capture from X11 display
//! - Windows/macOS: uses the `screenshots` crate

use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use image::{DynamicImage, RgbaImage};
use std::io::Cursor;
use std::path::Path;

// ──────────────────────────────────────────
// Linux (X11) implementation
// ──────────────────────────────────────────
#[cfg(target_os = "linux")]
fn capture() -> Result<DynamicImage> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::rust_connection::RustConnection;

    let (conn, screen_num) = RustConnection::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;
    let width = screen.width_in_pixels;
    let height = screen.height_in_pixels;

    let image = conn.get_image(
        ImageFormat::Z_PIXMAP,
        root,
        0,
        0,
        width,
        height,
        !0u32,
    )?.reply()?;

    // X11 Z_PIXMAP is typically BGRX (32-bit) on modern systems
    let data = image.data;
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    let depth = image.depth;

    if depth == 24 || depth == 32 {
        // 4 bytes per pixel: B G R X or B G R A
        for chunk in data.chunks(4) {
            if chunk.len() < 4 {
                break;
            }
            let b = chunk[0];
            let g = chunk[1];
            let r = chunk[2];
            let a = if depth == 32 { chunk[3] } else { 255 };
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }
    } else {
        return Err(anyhow!("Unsupported X11 image depth: {depth}"));
    }

    let img = RgbaImage::from_raw(width as u32, height as u32, rgba)
        .ok_or_else(|| anyhow!("Failed to create image from X11 data"))?;
    Ok(DynamicImage::ImageRgba8(img))
}

// ──────────────────────────────────────────
// Windows / macOS implementation
// ──────────────────────────────────────────
#[cfg(not(target_os = "linux"))]
fn capture() -> Result<DynamicImage> {
    use screenshots::Screen;
    let screens = Screen::all()?;
    let screen = screens.into_iter().next()
        .ok_or_else(|| anyhow!("No screen found"))?;
    let img = screen.capture()?;
    let rgba = RgbaImage::from_raw(img.width(), img.height(), img.rgba().to_vec())
        .ok_or_else(|| anyhow!("Failed to create image"))?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

// ──────────────────────────────────────────
// Public API
// ──────────────────────────────────────────

/// Take a screenshot and return a DynamicImage.
pub fn screenshot() -> Result<DynamicImage> {
    capture()
}

/// Take a screenshot and return it as PNG bytes.
pub fn screenshot_bytes() -> Result<Vec<u8>> {
    let img = screenshot()?;
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(buf.into_inner())
}

/// Take a screenshot and return it as base64-encoded PNG string.
pub fn screenshot_b64() -> Result<String> {
    let bytes = screenshot_bytes()?;
    Ok(STANDARD.encode(&bytes))
}

/// Save a screenshot to the given file path.
pub fn screenshot_to_file(path: &Path) -> Result<()> {
    let img = screenshot()?;
    img.save(path)?;
    Ok(())
}

/// Return the screen size (width, height) in pixels.
pub fn screen_size() -> Result<(u32, u32)> {
    let img = screenshot()?;
    Ok((img.width(), img.height()))
}

/// Get the display scale factor (from environment on Linux).
pub fn get_display_scale() -> f64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(val) = std::env::var("GDK_SCALE") {
            if let Ok(f) = val.parse::<f64>() {
                return f;
            }
        }
        if let Ok(val) = std::env::var("QT_SCALE_FACTOR") {
            if let Ok(f) = val.parse::<f64>() {
                return f;
            }
        }
    }
    1.0
}
