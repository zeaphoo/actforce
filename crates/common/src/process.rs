use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: std::process::ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub fn which<S: AsRef<OsStr>>(exe: S) -> Option<PathBuf> {
    let exe = exe.as_ref();

    let p = Path::new(exe);
    if p.is_absolute() && p.is_file() {
        return Some(p.to_path_buf());
    }

    let s = exe.to_string_lossy();
    if s.contains('/') {
        let p = PathBuf::from(s.as_ref());
        if p.is_file() {
            return Some(p);
        }
        return None;
    }

    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let cand = dir.join(exe);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

pub async fn run_command(
    exe: &Path,
    args: &[String],
    cwd: Option<&Path>,
    stdin_bytes: Option<Vec<u8>>,
    timeout: Option<Duration>,
) -> Result<CommandOutput> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if stdin_bytes.is_some() {
        cmd.stdin(Stdio::piped());
    }

    let mut child = cmd.spawn().with_context(|| format!("spawn failed: {} {:?}", exe.display(), args))?;

    if let Some(input) = stdin_bytes {
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&input).await.context("write stdin failed")?;
        }
    }

    let wait_fut = child.wait_with_output();
    let output = if let Some(t) = timeout {
        tokio::time::timeout(t, wait_fut)
            .await
            .map_err(|_| anyhow!("command timeout"))??
    } else {
        wait_fut.await?
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    Ok(CommandOutput {
        status: output.status,
        stdout,
        stderr,
    })
}

pub fn exit_ok(out: &CommandOutput) -> Result<()> {
    if out.status.success() {
        return Ok(());
    }
    Err(anyhow!(
        "command failed: code={:?} stderr={} stdout={}",
        out.status.code(),
        out.stderr,
        out.stdout
    ))
}
