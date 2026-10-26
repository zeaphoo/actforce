//! Font discovery utilities for OFD rendering.
//!
//! Since OFD documents frequently omit embedded FontFiles but reference CJK
//! fonts by name, we probe a small list of common system font paths so that
//! PDF / PNG output still shows the correct glyphs without the caller having
//! to supply one explicitly.

use std::path::{Path, PathBuf};

/// Return a TTF/OTF/TTC font path to use as fallback.
///
/// Precedence:
/// 1. `$OFD_FILE_CJK_FONT`
/// 2. A few well-known system paths (Noto CJK, DejaVu, WenQuanYi, Droid,
///    macOS PingFang, Windows Microsoft YaHei)
pub fn discover_fallback_font() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("OFD_FILE_CJK_FONT") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    for candidate in CANDIDATES {
        let p = Path::new(candidate);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

const CANDIDATES: &[&str] = &[
    // Linux – Noto
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto/NotoSansCJK-Regular.ttc",
    // Linux – WenQuanYi
    "/usr/share/fonts/wqy-zenhei/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    // Linux – Droid fallback (older distros)
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    "/system/fonts/DroidSansFallback.ttf",
    // Generic DejaVu – at least Latin works
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
    // macOS
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    // Windows
    "C:/Windows/Fonts/msyh.ttc",
    "C:/Windows/Fonts/simsun.ttc",
    "C:/Windows/Fonts/simhei.ttf",
];
