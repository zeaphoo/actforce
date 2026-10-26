//! Cross-platform PTY (pseudo-terminal) manager via portable-pty.
//!
//! On Unix this uses the system pty, on Windows it uses ConPTY.

use anyhow::{anyhow, Result};
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtySession {
    pub pid: u32,
    pub cols: u16,
    pub rows: u16,
}

struct SessionHolder {
    master: Box<dyn portable_pty::MasterPty + Send>,
    writer: Box<dyn std::io::Write + Send>,
    exit_code: Arc<Mutex<Option<i32>>>,
}

pub struct Terminal {
    sessions: Arc<Mutex<HashMap<u32, SessionHolder>>>,
}

impl Terminal {
    pub fn new() -> Self {
        Terminal {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new PTY session.
    pub fn create(
        &self,
        command: Option<&str>,
        cols: u16,
        rows: u16,
        cwd: Option<&str>,
        envs: Option<HashMap<String, String>>,
        on_data: Option<Box<dyn Fn(Vec<u8>) + Send + 'static>>,
    ) -> Result<PtySession> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let cmd_str = command.unwrap_or(if cfg!(windows) { "powershell" } else { "bash" });

        #[cfg(unix)]
        let mut cmd = {
            let mut c = CommandBuilder::new("sh");
            c.arg("-c");
            c.arg(cmd_str);
            c
        };
        #[cfg(windows)]
        let mut cmd = {
            let mut c = CommandBuilder::new("cmd");
            c.arg("/C");
            c.arg(cmd_str);
            c
        };

        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }
        if let Some(env_map) = envs {
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        let child = pair.slave.spawn_command(cmd)?;
        let pid = child.process_id().unwrap_or(0);

        let exit_code: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
        let exit_code_clone = exit_code.clone();

        // Spawn a thread to wait for child exit
        let mut child = child;
        thread::spawn(move || {
            let code = child
                .wait()
                .ok()
                .and_then(|s| if s.success() { Some(0) } else { Some(1) });
            *exit_code_clone.lock().unwrap() = code;
        });

        // Spawn a reader thread for PTY output
        let mut reader = pair.master.try_clone_reader()?;
        let sessions_clone = self.sessions.clone();
        let pid_clone = pid;
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        if let Some(cb) = &on_data {
                            cb(data);
                        }
                    }
                }
            }
            // Clean up session
            sessions_clone.lock().unwrap().remove(&pid_clone);
        });

        let writer = pair.master.take_writer()?;
        let holder = SessionHolder {
            master: pair.master,
            writer,
            exit_code,
        };

        self.sessions.lock().unwrap().insert(pid, holder);

        Ok(PtySession { pid, cols, rows })
    }

    /// Write data to the PTY stdin.
    pub fn send_stdin(&self, pid: u32, data: &[u8]) -> Result<()> {
        let mut sessions = self.sessions.lock().unwrap();
        let holder = sessions
            .get_mut(&pid)
            .ok_or_else(|| anyhow!("No PTY session with pid {pid}"))?;
        std::io::Write::write_all(&mut holder.writer, data)?;
        Ok(())
    }

    /// Resize the PTY.
    pub fn resize(&self, pid: u32, cols: u16, rows: u16) -> Result<()> {
        let sessions = self.sessions.lock().unwrap();
        let holder = sessions
            .get(&pid)
            .ok_or_else(|| anyhow!("No PTY session with pid {pid}"))?;
        holder.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    /// Check whether a session exists.
    pub fn has_session(&self, pid: u32) -> bool {
        self.sessions.lock().unwrap().contains_key(&pid)
    }

    /// Get the exit code of a session (None if still running or not found).
    pub fn exit_code(&self, pid: u32) -> Option<i32> {
        let sessions = self.sessions.lock().unwrap();
        sessions
            .get(&pid)
            .and_then(|h| *h.exit_code.lock().unwrap())
    }
}

impl Default for Terminal {
    fn default() -> Self {
        Self::new()
    }
}
