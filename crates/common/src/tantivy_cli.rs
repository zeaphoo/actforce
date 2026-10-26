use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::process::{exit_ok, run_command, which, CommandOutput};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TantivyFieldSpec {
    pub name: String,
    #[serde(default = "default_field_type")]
    pub field_type: String,
    #[serde(default = "default_true")]
    pub stored: bool,
    #[serde(default = "default_true")]
    pub indexed: bool,
    #[serde(default)]
    pub fast: bool,
    #[serde(default = "default_true")]
    pub tokenized: bool,
    #[serde(default = "default_true")]
    pub term_frequencies: bool,
    #[serde(default = "default_true")]
    pub term_positions: bool,
    #[serde(default)]
    pub tokenizer: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_field_type() -> String {
    "Text".to_string()
}

#[derive(Debug, Clone)]
pub struct Tansearch {
    exe: PathBuf,
}

impl Tansearch {
    pub fn discover() -> Result<Self> {
        let exe = which("tansearch").ok_or_else(|| anyhow!("tansearch not found in PATH"))?;
        Ok(Self { exe })
    }

    pub fn with_exe(exe: PathBuf) -> Self {
        Self { exe }
    }

    pub async fn run_checked(
        &self,
        args: &[String],
        cwd: Option<&Path>,
        stdin_bytes: Option<Vec<u8>>,
        timeout: Option<Duration>,
    ) -> Result<CommandOutput> {
        let out = run_command(&self.exe, args, cwd, stdin_bytes, timeout).await?;
        exit_ok(&out)?;
        Ok(out)
    }

    pub fn init_index_from_fields(
        &self,
        index_dir: &Path,
        fields: Vec<TantivyFieldSpec>,
        opstamp: i64,
        overwrite: bool,
        tokenizer: &str,
    ) -> Result<PathBuf> {
        let index_dir = index_dir.to_path_buf();
        std::fs::create_dir_all(&index_dir)?;
        let meta_json = index_dir.join("meta.json");

        if meta_json.exists() && !overwrite {
            return Ok(index_dir);
        }

        let mut schema: Vec<serde_json::Value> = Vec::new();
        for f in fields {
            let f_type = f.field_type.to_lowercase();
            if matches!(f_type.as_str(), "text" | "str" | "string") {
                let mut options = serde_json::json!({
                    "stored": f.stored,
                    "fast": f.fast,
                });

                if f.indexed {
                    let record = if f.term_positions {
                        "position"
                    } else if f.term_frequencies {
                        "freq"
                    } else {
                        "basic"
                    };
                    let chosen = f.tokenizer.clone().unwrap_or_else(|| {
                        if f.tokenized {
                            tokenizer.to_string()
                        } else {
                            "raw".to_string()
                        }
                    });
                    options["indexing"] = serde_json::json!({
                        "tokenizer": chosen,
                        "record": record,
                        "fieldnorms": true,
                    });
                }

                schema.push(serde_json::json!({
                    "name": f.name,
                    "type": "text",
                    "options": options,
                }));
                continue;
            }

            schema.push(serde_json::json!({
                "name": f.name,
                "type": f_type,
                "options": {
                    "stored": f.stored,
                    "indexed": f.indexed,
                    "fieldnorms": false,
                    "fast": f.fast,
                }
            }));
        }

        let meta = serde_json::json!({
            "segments": [],
            "schema": schema,
            "opstamp": opstamp,
        });

        std::fs::write(&meta_json, serde_json::to_vec(&meta)?)?;
        Ok(index_dir)
    }

    pub async fn index_jsonl(&self, index_dir: &Path, jsonl_path: &Path) -> Result<()> {
        let meta_json = index_dir.join("meta.json");
        if !meta_json.exists() {
            return Err(anyhow!("missing meta.json: {}", meta_json.display()));
        }
        let data = tokio::fs::read(jsonl_path).await?;
        let args = vec![
            "index".to_string(),
            "-i".to_string(),
            index_dir.display().to_string(),
        ];
        let _ = self
            .run_checked(&args, None, Some(data), Some(Duration::from_secs(60)))
            .await?;
        Ok(())
    }

    pub async fn delete_by_term(&self, index_dir: &Path, field: &str, value: &str) -> Result<()> {
        let args = vec![
            "delete".to_string(),
            "-i".to_string(),
            index_dir.display().to_string(),
            "--field".to_string(),
            field.to_string(),
            "--value".to_string(),
            value.to_string(),
        ];
        let _ = self
            .run_checked(&args, None, None, Some(Duration::from_secs(30)))
            .await?;
        Ok(())
    }

    pub async fn search(
        &self,
        index_dir: &Path,
        query: &str,
        limit: usize,
        offset: usize,
        sort_by: Option<&str>,
        desc: bool,
    ) -> Result<Vec<String>> {
        let mut args = vec![
            "search".to_string(),
            "-i".to_string(),
            index_dir.display().to_string(),
            "-q".to_string(),
            query.to_string(),
            "--limit".to_string(),
            format!("{}", limit),
            "--offset".to_string(),
            format!("{}", offset),
        ];
        if let Some(f) = sort_by {
            args.push("--sort-by".to_string());
            args.push(f.to_string());
            if desc {
                args.push("--desc".to_string());
            }
        }

        let out = self
            .run_checked(&args, None, None, Some(Duration::from_secs(30)))
            .await?;
        Ok(out
            .stdout
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }
}
