use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};

use crate::process::{exit_ok, run_command, which, CommandOutput};

#[derive(Debug, Clone)]
pub struct Qsv {
    exe: PathBuf,
}

impl Qsv {
    pub fn discover() -> Result<Self> {
        let exe = which("qsv").ok_or_else(|| anyhow!("qsv not found in PATH"))?;
        Ok(Self { exe })
    }

    pub fn with_exe(exe: PathBuf) -> Self {
        Self { exe }
    }

    pub async fn run(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        stdin_bytes: Option<Vec<u8>>,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput> {
        let out = run_command(&self.exe, args, cwd, stdin_bytes, timeout).await?;
        Ok(out)
    }

    pub async fn run_checked(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        stdin_bytes: Option<Vec<u8>>,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput> {
        let out = self.run(args, cwd, stdin_bytes, timeout).await?;
        exit_ok(&out)?;
        Ok(out)
    }
}
