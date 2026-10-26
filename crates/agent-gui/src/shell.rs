//! Shell command execution module.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub returncode: i32,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.returncode == 0
    }
}

/// Decode bytes trying multiple encodings, falling back to lossy UTF-8.
fn decode_bytes(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    // Try UTF-8 first
    if let Ok(s) = std::str::from_utf8(data) {
        return s.to_string();
    }
    // Fallback: lossy UTF-8
    String::from_utf8_lossy(data).into_owned()
}

/// Run a shell command with a timeout (seconds).
pub fn run(command: &str, timeout: u64) -> Result<CommandResult> {
    // For timeout we use a thread-based approach
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();
    let command_owned = command.to_string();
    let _handle = thread::spawn(move || {
        #[cfg(unix)]
        let output = Command::new("sh").arg("-c").arg(&command_owned).output();
        #[cfg(windows)]
        let output = Command::new("cmd").arg("/C").arg(&command_owned).output();
        let _ = tx.send(output);
    });

    match rx.recv_timeout(Duration::from_secs(timeout)) {
        Ok(Ok(output)) => {
            let returncode = output.status.code().unwrap_or(-1);
            Ok(CommandResult {
                stdout: decode_bytes(&output.stdout),
                stderr: decode_bytes(&output.stderr),
                returncode,
            })
        }
        Ok(Err(e)) => Err(e.into()),
        Err(_) => {
            // Timeout expired
            Ok(CommandResult {
                stdout: String::new(),
                stderr: "command timed out".to_string(),
                returncode: -1,
            })
        }
    }
}
