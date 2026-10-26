use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::fs::{self, OpenOptions};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::types::ChatRole;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredChatMessage {
    pub sequence: i64,
    pub role: ChatRole,
    pub content: String,
    pub ts: DateTime<Utc>,
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,
    pub attachments: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MessageRecord {
    sequence: i64,
    role: String,
    content: String,
    ts: String,
    ts_ms: i64,
    #[serde(default)]
    meta: HashMap<String, serde_json::Value>,
    attachments: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Clone)]
pub struct ChatStore {
    pub identity_id: Option<String>,
    pub user_id: Option<String>,
    pub data_root: PathBuf,
    /// Per-subject mutex so concurrent appends get unique sequences
    /// (agentlite parity: per-subject asyncio.Lock in chat_store.py).
    locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl ChatStore {
    pub fn new(identity_id: Option<String>, user_id: Option<String>, data_root: impl Into<PathBuf>) -> Self {
        Self {
            identity_id,
            user_id,
            data_root: data_root.into(),
            locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    fn safe_part(s: &str) -> String {
        let s2: String = s
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '_' })
            .collect();
        if s2.len() > 120 {
            s2[..120].to_string()
        } else if s2.is_empty() {
            "_".to_string()
        } else {
            s2
        }
    }

    fn subject_dir(&self, subject: &str) -> PathBuf {
        let sub_part = Self::safe_part(subject);

        let base = if let Some(ref user_id) = self.user_id {
            self.data_root.join("users").join(Self::safe_part(user_id)).join("chat")
        } else if let Some(ref identity_id) = self.identity_id {
            self.data_root.join("identities").join(Self::safe_part(identity_id)).join("chat")
        } else {
            self.data_root.join("chat").join("anonymous")
        };

        let subject_dir = base.join(&sub_part);
        std::fs::create_dir_all(&subject_dir).ok();
        subject_dir
    }

    fn messages_file_path(&self, subject: &str) -> PathBuf {
        self.subject_dir(subject).join("session.jsonl")
    }

    fn contexts_file_path(&self, subject: &str) -> PathBuf {
        self.subject_dir(subject).join("contexts.jsonl")
    }

    async fn read_last_sequence(&self, path: &PathBuf) -> i64 {
        if !path.exists() {
            return 0;
        }

        match fs::read(path).await {
            Ok(data) => {
                let text = String::from_utf8_lossy(&data);
                let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                if let Some(last) = lines.last() {
                    if let Ok(record) = serde_json::from_str::<MessageRecord>(last) {
                        return record.sequence;
                    }
                }
                0
            }
            Err(_) => 0,
        }
    }

    pub async fn append_message(
        &self,
        subject: &str,
        role: &str,
        content: &str,
        meta: Option<HashMap<String, serde_json::Value>>,
        attachments: Option<HashMap<String, serde_json::Value>>,
    ) -> anyhow::Result<StoredChatMessage> {
        // Serialize appends per subject (agentlite parity: per-subject lock)
        let subject_lock = {
            let mut map = self.locks.lock().unwrap();
            map.entry(subject.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = subject_lock.lock().await;

        let path = self.messages_file_path(subject);
        let last_seq = self.read_last_sequence(&path).await;
        let seq = last_seq + 1;
        let now = Utc::now();
        let ts_ms = now.timestamp_millis();

        let record = MessageRecord {
            sequence: seq,
            role: role.to_string(),
            content: content.to_string(),
            ts: now.to_rfc3339(),
            ts_ms,
            meta: meta.clone().unwrap_or_default(),
            attachments: attachments.clone(),
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;

        let line = serde_json::to_string(&record)? + "\n";
        file.write_all(line.as_bytes()).await?;

        Ok(StoredChatMessage {
            sequence: seq,
            role: role.to_string(),
            content: content.to_string(),
            ts: now,
            meta: meta.unwrap_or_default(),
            attachments,
        })
    }

    pub async fn save_contexts(
        &self,
        subject: &str,
        contexts: &[serde_json::Value],
    ) -> anyhow::Result<()> {
        let path = self.contexts_file_path(subject);
        let last_seq = self.read_last_sequence(&path).await;
        let seq = last_seq + 1;
        let now = Utc::now();
        let record = serde_json::json!({
            "sequence": seq,
            "ts": now.to_rfc3339(),
            "contexts": contexts,
        });
        let line = serde_json::to_string(&record)? + "\n";
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await?;
        file.write_all(line.as_bytes()).await?;
        Ok(())
    }

    pub async fn load_latest_contexts(
        &self,
        subject: &str,
    ) -> Option<Vec<serde_json::Value>> {
        let path = self.contexts_file_path(subject);
        if !path.exists() {
            return None;
        }
        let size = match tokio::fs::metadata(&path).await {
            Ok(m) => m.len(),
            Err(_) => return None,
        };
        let read_size = (size as usize).min(64 * 1024);
        let start = (size as usize).saturating_sub(read_size);
        let data = match fs::read(&path).await {
            Ok(d) => d,
            Err(_) => return None,
        };
        let text = String::from_utf8_lossy(&data[start..]);
        let last_line = text.lines().filter(|l| !l.trim().is_empty()).last()?;
        let obj: serde_json::Value = serde_json::from_str(last_line).ok()?;
        obj.get("contexts")?.as_array().cloned()
    }

    /// 读取所有历史 contexts 记录（从所有 runs 中提取 contexts 条目）
    pub async fn load_all_contexts(
        &self,
        subject: &str,
    ) -> Vec<serde_json::Value> {
        let path = self.contexts_file_path(subject);
        if !path.exists() {
            return vec![];
        }
        let data = match fs::read(&path).await {
            Ok(d) => d,
            Err(_) => return vec![],
        };
        let text = String::from_utf8_lossy(&data);
        let mut all: Vec<serde_json::Value> = Vec::new();
        for line in text.lines() {
            let ln = line.trim();
            if ln.is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<serde_json::Value>(ln) {
                if let Some(ctxs) = record.get("contexts").and_then(|v| v.as_array()) {
                    for ctx in ctxs {
                        all.push(ctx.clone());
                    }
                }
            }
        }
        all
    }

    pub async fn list_messages(&self, subject: &str, limit: usize) -> anyhow::Result<Vec<StoredChatMessage>> {
        let path = self.messages_file_path(subject);
        if !path.exists() {
            return Ok(vec![]);
        }

        let file = fs::File::open(&path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut msgs = Vec::new();

        while let Some(line) = lines.next_line().await? {
            let ln = line.trim();
            if ln.is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<MessageRecord>(ln) {
                // Cross-read compatibility: agentlite writes naive isoformat
                // timestamps (no timezone). RFC3339 parsing fails on those, so
                // fall back to parsing the naive form (interpreted as UTC, the
                // same wall-clock value agentlite stored locally) before giving
                // up. Write side stays RFC3339 (correct, timezone-aware).
                let ts = DateTime::parse_from_rfc3339(&record.ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .or_else(|_| {
                        chrono::NaiveDateTime::parse_from_str(&record.ts, "%Y-%m-%dT%H:%M:%S%.f")
                            .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
                    })
                    .unwrap_or_else(|_| Utc::now());

                msgs.push(StoredChatMessage {
                    sequence: record.sequence,
                    role: record.role,
                    content: record.content,
                    ts,
                    meta: record.meta,
                    attachments: record.attachments,
                });
            }
        }

        // Sort by sequence and return last N
        msgs.sort_by_key(|m| m.sequence);
        let start = if msgs.len() > limit { msgs.len() - limit } else { 0 };
        Ok(msgs[start..].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_chat_store_append_and_list() {
        let temp_dir = TempDir::new().unwrap();
        let store = ChatStore::new(None, None, temp_dir.path());

        // 追加消息
        store
            .append_message("test_subject", "user", "Hello", None, None)
            .await
            .unwrap();
        store
            .append_message("test_subject", "assistant", "Hi there!", None, None)
            .await
            .unwrap();

        // 列出消息
        let messages = store.list_messages("test_subject", 10).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Hi there!");
    }
}
