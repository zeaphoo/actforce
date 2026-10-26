//! Cross-platform window management.
//!
//! - Linux: uses x11rb to enumerate and inspect windows via EWMH
//! - Windows/macOS: limited support via process listing

use anyhow::Result;

// ──────────────────────────────────────────
// Linux (X11/EWMH) implementation
// ──────────────────────────────────────────
#[cfg(target_os = "linux")]
mod platform {
    use anyhow::{anyhow, Result};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::*;
    use x11rb::rust_connection::RustConnection;
    use x11rb::atom_manager;

    atom_manager! {
        pub Atoms: AtomsCookie {
            _NET_ACTIVE_WINDOW,
            _NET_WM_NAME,
            UTF8_STRING,
            WM_NAME,
            _NET_CLIENT_LIST,
        }
    }

    fn connect() -> Result<(RustConnection, usize)> {
        Ok(RustConnection::connect(None)?)
    }

    fn get_root_window_property_u32(
        conn: &RustConnection,
        root: u32,
        atom: u32,
    ) -> Result<Option<u32>> {
        let reply = conn
            .get_property(false, root, atom, AtomEnum::WINDOW, 0, 1)?
            .reply()?;
        if reply.value32().map(|mut i| i.next()).flatten().is_none() {
            return Ok(None);
        }
        let mut iter = reply.value32().ok_or_else(|| anyhow!("Expected u32 property"))?;
        Ok(iter.next())
    }

    fn get_window_text(
        conn: &RustConnection,
        atoms: &Atoms,
        window: u32,
    ) -> Result<String> {
        // Try _NET_WM_NAME (UTF-8) first
        let reply = conn
            .get_property(false, window, atoms._NET_WM_NAME, atoms.UTF8_STRING, 0, 1024)?
            .reply()?;
        if !reply.value.is_empty() {
            return Ok(String::from_utf8_lossy(&reply.value).into_owned());
        }
        // Fallback to WM_NAME
        let reply = conn
            .get_property(false, window, atoms.WM_NAME, AtomEnum::STRING, 0, 1024)?
            .reply()?;
        Ok(String::from_utf8_lossy(&reply.value).into_owned())
    }

    pub fn get_active_window_title() -> Result<String> {
        let (conn, screen_num) = connect()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let root = conn.setup().roots[screen_num].root;
        if let Some(win) = get_root_window_property_u32(&conn, root, atoms._NET_ACTIVE_WINDOW)? {
            if win != 0 {
                return Ok(get_window_text(&conn, &atoms, win)?);
            }
        }
        Ok("Desktop".to_string())
    }

    pub fn get_active_window_handle() -> Result<Option<String>> {
        let (conn, screen_num) = connect()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let root = conn.setup().roots[screen_num].root;
        if let Some(win) = get_root_window_property_u32(&conn, root, atoms._NET_ACTIVE_WINDOW)? {
            if win != 0 {
                return Ok(Some(win.to_string()));
            }
        }
        Ok(None)
    }

    pub fn get_windows_with_title(title: &str) -> Result<Vec<String>> {
        let title_lower = title.to_lowercase();
        let (conn, screen_num) = connect()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        let root = conn.setup().roots[screen_num].root;

        let reply = conn
            .get_property(false, root, atoms._NET_CLIENT_LIST, AtomEnum::WINDOW, 0, 65536)?
            .reply()?;

        let mut result = Vec::new();
        if let Some(iter) = reply.value32() {
            for win in iter {
                if let Ok(name) = get_window_text(&conn, &atoms, win) {
                    if name.to_lowercase().contains(&title_lower) {
                        result.push(win.to_string());
                    }
                }
            }
        }
        Ok(result)
    }

    pub fn activate_window(_handle: &str) -> Result<bool> {
        // Activating windows via X11 requires sending _NET_ACTIVE_WINDOW message
        // This is complex; return false (not supported in this minimal implementation)
        Ok(false)
    }

    pub fn get_window_title(handle: &str) -> Result<Option<String>> {
        let win_id: u32 = handle.parse().map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let (conn, _) = connect()?;
        let atoms = Atoms::new(&conn)?.reply()?;
        match get_window_text(&conn, &atoms, win_id) {
            Ok(title) => Ok(Some(title)),
            Err(_) => Ok(None),
        }
    }

    pub fn get_window_size(handle: &str) -> Result<Option<(u32, u32)>> {
        let win_id: u32 = handle.parse().map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let (conn, _) = connect()?;
        let reply = conn.get_geometry(win_id)?.reply();
        match reply {
            Ok(geom) => Ok(Some((geom.width as u32, geom.height as u32))),
            Err(_) => Ok(None),
        }
    }

    pub fn get_window_position(handle: &str) -> Result<Option<(i32, i32)>> {
        let win_id: u32 = handle.parse().map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let (conn, _) = connect()?;
        let reply = conn.get_geometry(win_id)?.reply();
        match reply {
            Ok(geom) => Ok(Some((geom.x as i32, geom.y as i32))),
            Err(_) => Ok(None),
        }
    }
}

// ──────────────────────────────────────────
// Windows (Win32 API) implementation
// Mirrors pywinctl functionality via Win32
// ──────────────────────────────────────────
#[cfg(target_os = "windows")]
mod platform {
    use anyhow::{anyhow, Result};
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetForegroundWindow, GetWindowRect, GetWindowTextW, IsWindowVisible,
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    /// Read the title of a HWND as a String.
    fn window_title(hwnd: HWND) -> String {
        let mut buf = [0u16; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if len == 0 {
            return String::new();
        }
        OsString::from_wide(&buf[..len as usize])
            .to_string_lossy()
            .into_owned()
    }

    /// Collect all visible top-level windows as (hwnd_usize, title) pairs.
    fn all_windows() -> Vec<(usize, String)> {
        let mut windows: Vec<(usize, String)> = Vec::new();
        let ptr = &mut windows as *mut Vec<(usize, String)> as isize;

        unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let list = &mut *(lparam.0 as *mut Vec<(usize, String)>);
            if IsWindowVisible(hwnd).as_bool() {
                let title = {
                    let mut buf = [0u16; 512];
                    let len = GetWindowTextW(hwnd, &mut buf);
                    if len > 0 {
                        OsString::from_wide(&buf[..len as usize])
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        String::new()
                    }
                };
                if !title.is_empty() {
                    list.push((hwnd.0 as usize, title));
                }
            }
            BOOL(1) // continue enumeration
        }

        unsafe {
            let _ = EnumWindows(Some(enum_cb), LPARAM(ptr));
        }
        windows
    }

    pub fn get_active_window_title() -> Result<String> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0 == 0 {
            return Ok("Desktop".to_string());
        }
        Ok(window_title(hwnd))
    }

    pub fn get_active_window_handle() -> Result<Option<String>> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.0 == 0 {
            Ok(None)
        } else {
            Ok(Some((hwnd.0 as usize).to_string()))
        }
    }

    pub fn get_windows_with_title(title: &str) -> Result<Vec<String>> {
        let title_lower = title.to_lowercase();
        Ok(all_windows()
            .into_iter()
            .filter(|(_, t)| t.to_lowercase().contains(&title_lower))
            .map(|(h, _)| h.to_string())
            .collect())
    }

    pub fn activate_window(handle: &str) -> Result<bool> {
        let h: usize = handle
            .parse()
            .map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let hwnd = HWND(h as isize);
        unsafe {
            // Restore if minimized, then bring to foreground
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd).ok()?;
        }
        Ok(true)
    }

    pub fn get_window_title(handle: &str) -> Result<Option<String>> {
        let h: usize = handle
            .parse()
            .map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let hwnd = HWND(h as isize);
        let title = window_title(hwnd);
        Ok(if title.is_empty() { None } else { Some(title) })
    }

    pub fn get_window_size(handle: &str) -> Result<Option<(u32, u32)>> {
        let h: usize = handle
            .parse()
            .map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let hwnd = HWND(h as isize);
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut rect)
                .map_err(|e| anyhow!("GetWindowRect failed: {e}"))?;
        }
        let w = (rect.right - rect.left).max(0) as u32;
        let h_size = (rect.bottom - rect.top).max(0) as u32;
        Ok(Some((w, h_size)))
    }

    pub fn get_window_position(handle: &str) -> Result<Option<(i32, i32)>> {
        let h: usize = handle
            .parse()
            .map_err(|_| anyhow!("Invalid window handle: {handle}"))?;
        let hwnd = HWND(h as isize);
        let mut rect = RECT::default();
        unsafe {
            GetWindowRect(hwnd, &mut rect)
                .map_err(|e| anyhow!("GetWindowRect failed: {e}"))?;
        }
        Ok(Some((rect.left, rect.top)))
    }
}

// ──────────────────────────────────────────
// macOS stub (AppleScript / Accessibility API needed for full support)
// ──────────────────────────────────────────
#[cfg(target_os = "macos")]
mod platform {
    use anyhow::Result;

    pub fn get_active_window_title() -> Result<String> {
        Ok("Desktop".to_string())
    }

    pub fn get_active_window_handle() -> Result<Option<String>> {
        Ok(None)
    }

    pub fn get_windows_with_title(_title: &str) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    pub fn activate_window(_handle: &str) -> Result<bool> {
        Ok(false)
    }

    pub fn get_window_title(_handle: &str) -> Result<Option<String>> {
        Ok(None)
    }

    pub fn get_window_size(_handle: &str) -> Result<Option<(u32, u32)>> {
        Ok(None)
    }

    pub fn get_window_position(_handle: &str) -> Result<Option<(i32, i32)>> {
        Ok(None)
    }
}


// ──────────────────────────────────────────
// Public API (delegating to platform module)
// ──────────────────────────────────────────

/// Get the title of the currently active window.
pub fn get_active_window_title() -> Result<String> {
    platform::get_active_window_title()
}

/// Get the handle (id) of the currently active window as a string.
pub fn get_active_window_handle() -> Result<Option<String>> {
    platform::get_active_window_handle()
}

/// Get all windows whose title contains the given string (case-insensitive).
pub fn get_windows_with_title(title: &str) -> Result<Vec<String>> {
    platform::get_windows_with_title(title)
}

/// Activate (focus) a window by handle.
pub fn activate_window(handle: &str) -> Result<bool> {
    platform::activate_window(handle)
}

/// Get the title of a window by handle.
pub fn get_window_title(handle: &str) -> Result<Option<String>> {
    platform::get_window_title(handle)
}

/// Get the (width, height) of a window by handle.
pub fn get_window_size(handle: &str) -> Result<Option<(u32, u32)>> {
    platform::get_window_size(handle)
}

/// Get the (x, y) position of a window by handle.
pub fn get_window_position(handle: &str) -> Result<Option<(i32, i32)>> {
    platform::get_window_position(handle)
}
