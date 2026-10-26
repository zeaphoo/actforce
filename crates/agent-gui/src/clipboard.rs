//! Cross-platform clipboard access via arboard.

use anyhow::Result;
use arboard::Clipboard;

/// Get the current clipboard text content.
pub fn get() -> Result<String> {
    let mut clipboard = Clipboard::new()?;
    Ok(clipboard.get_text()?)
}

/// Set the clipboard text content.
pub fn set(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
