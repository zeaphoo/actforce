//! Cross-platform mouse control via enigo.

use anyhow::Result;
use enigo::{Button, Coordinate, Direction, Enigo, Mouse, Settings};

fn make_enigo() -> Result<Enigo> {
    Ok(Enigo::new(&Settings::default())?)
}

fn map_button(button: &str) -> Button {
    match button.to_lowercase().as_str() {
        "right" => Button::Right,
        "middle" => Button::Middle,
        _ => Button::Left,
    }
}

/// Single click at (x, y) with given button ("left", "right", "middle").
pub fn click(x: i32, y: i32, button: &str) -> Result<()> {
    let mut enigo = make_enigo()?;
    enigo.move_mouse(x, y, Coordinate::Abs)?;
    enigo.button(map_button(button), Direction::Click)?;
    Ok(())
}

/// Right-click at (x, y).
pub fn right_click(x: i32, y: i32) -> Result<()> {
    click(x, y, "right")
}

/// Double left-click at (x, y).
pub fn double_click(x: i32, y: i32) -> Result<()> {
    let mut enigo = make_enigo()?;
    enigo.move_mouse(x, y, Coordinate::Abs)?;
    enigo.button(Button::Left, Direction::Click)?;
    enigo.button(Button::Left, Direction::Click)?;
    Ok(())
}

/// Move cursor to (x, y) without clicking.
pub fn move_to(x: i32, y: i32) -> Result<()> {
    let mut enigo = make_enigo()?;
    enigo.move_mouse(x, y, Coordinate::Abs)?;
    Ok(())
}

/// Press and hold a mouse button, optionally moving first.
pub fn mouse_down(x: Option<i32>, y: Option<i32>, button: &str) -> Result<()> {
    let mut enigo = make_enigo()?;
    if let (Some(x), Some(y)) = (x, y) {
        enigo.move_mouse(x, y, Coordinate::Abs)?;
    }
    enigo.button(map_button(button), Direction::Press)?;
    Ok(())
}

/// Release a mouse button, optionally moving first.
pub fn mouse_up(x: Option<i32>, y: Option<i32>, button: &str) -> Result<()> {
    let mut enigo = make_enigo()?;
    if let (Some(x), Some(y)) = (x, y) {
        enigo.move_mouse(x, y, Coordinate::Abs)?;
    }
    enigo.button(map_button(button), Direction::Release)?;
    Ok(())
}

/// Drag from (start_x, start_y) to (end_x, end_y).
pub fn drag(start_x: i32, start_y: i32, end_x: i32, end_y: i32, button: &str) -> Result<()> {
    let btn = map_button(button);
    let mut enigo = make_enigo()?;
    enigo.move_mouse(start_x, start_y, Coordinate::Abs)?;
    enigo.button(btn, Direction::Press)?;
    enigo.move_mouse(end_x, end_y, Coordinate::Abs)?;
    enigo.button(btn, Direction::Release)?;
    Ok(())
}

/// Drag from current position to (x, y).
pub fn drag_to(x: i32, y: i32, button: &str) -> Result<()> {
    let btn = map_button(button);
    let mut enigo = make_enigo()?;
    enigo.button(btn, Direction::Press)?;
    enigo.move_mouse(x, y, Coordinate::Abs)?;
    enigo.button(btn, Direction::Release)?;
    Ok(())
}

/// Scroll by (dx, dy). Positive dy = scroll up.
pub fn scroll(dx: i32, dy: i32) -> Result<()> {
    let mut enigo = make_enigo()?;
    if dx != 0 {
        enigo.scroll(dx, enigo::Axis::Horizontal)?;
    }
    if dy != 0 {
        enigo.scroll(dy, enigo::Axis::Vertical)?;
    }
    Ok(())
}

/// Scroll up by clicks notches.
pub fn scroll_up(clicks: i32) -> Result<()> {
    scroll(0, clicks.abs())
}

/// Scroll down by clicks notches.
pub fn scroll_down(clicks: i32) -> Result<()> {
    scroll(0, -clicks.abs())
}

/// Return the current cursor position as (x, y).
pub fn position() -> Result<(i32, i32)> {
    let enigo = make_enigo()?;
    let (x, y) = enigo.location()?;
    Ok((x, y))
}
