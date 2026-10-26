//! Cross-platform keyboard control via enigo.

use anyhow::{anyhow, Result};
use enigo::{Direction, Enigo, Key, Keyboard, Settings};

fn make_enigo() -> Result<Enigo> {
    Ok(Enigo::new(&Settings::default())?)
}

/// Resolve a key name string to an enigo Key.
fn resolve_key(key: &str) -> Result<Key> {
    let lk = key.to_lowercase();
    let k = match lk.as_str() {
        "enter" | "return" => Key::Return,
        "esc" | "escape" => Key::Escape,
        "space" => Key::Space,
        "tab" => Key::Tab,
        "backspace" => Key::Backspace,
        "delete" => Key::Delete,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "page_up" => Key::PageUp,
        "pagedown" | "page_down" => Key::PageDown,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "shift" | "shift_l" => Key::Shift,
        "shift_r" => Key::RShift,
        "ctrl" | "ctrl_l" | "control" => Key::Control,
        "ctrl_r" => Key::RControl,
        "alt" | "alt_l" => Key::Alt,
        "alt_r" => Key::Alt,
        "cmd" | "command" | "win" | "super" | "meta" => Key::Meta,
        "option" | "option_l" => Key::Alt,
        "option_r" => Key::Alt,
        "capslock" | "caps_lock" => Key::CapsLock,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "insert" => Key::Insert,
        "print_screen" => Key::PrintScr,
        "num_lock" => Key::Numlock,
        "scroll_lock" => Key::ScrollLock,
        "pause" => Key::Pause,
        s if s.chars().count() == 1 => {
            let ch = s.chars().next().unwrap();
            Key::Unicode(ch)
        }
        _ => return Err(anyhow!("Unknown key: {key:?}")),
    };
    Ok(k)
}

/// Press and hold a key.
pub fn key_down(key: &str) -> Result<()> {
    let k = resolve_key(key)?;
    let mut enigo = make_enigo()?;
    enigo.key(k, Direction::Press)?;
    Ok(())
}

/// Release a key.
pub fn key_up(key: &str) -> Result<()> {
    let k = resolve_key(key)?;
    let mut enigo = make_enigo()?;
    enigo.key(k, Direction::Release)?;
    Ok(())
}

/// Press and immediately release a key.
pub fn press_key(key: &str) -> Result<()> {
    let k = resolve_key(key)?;
    let mut enigo = make_enigo()?;
    enigo.key(k, Direction::Click)?;
    Ok(())
}

/// Type a string of text (supports Unicode).
pub fn type_text(text: &str) -> Result<()> {
    let mut enigo = make_enigo()?;
    enigo.text(text)?;
    Ok(())
}

/// Press a combination of keys, e.g. ["ctrl", "c"].
pub fn hotkey(keys: &[&str]) -> Result<()> {
    if keys.is_empty() {
        return Ok(());
    }
    let resolved: Result<Vec<Key>> = keys.iter().map(|k| resolve_key(k)).collect();
    let resolved = resolved?;

    let mut enigo = make_enigo()?;

    // Press all modifier keys except the last
    for k in resolved.iter().take(resolved.len() - 1) {
        enigo.key(*k, Direction::Press)?;
    }
    // Press+release the last key
    if let Some(last) = resolved.last() {
        enigo.key(*last, Direction::Click)?;
    }
    // Release modifiers in reverse
    for k in resolved.iter().take(resolved.len() - 1).rev() {
        enigo.key(*k, Direction::Release)?;
    }
    Ok(())
}
