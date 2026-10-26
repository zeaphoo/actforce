use crate::types::{Event, EventRef};
use anyhow::Result;
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use uuid::Uuid;

/// 文件事件存储
pub struct FileEventStore {
    #[allow(dead_code)]
    base_dir: PathBuf,
    events_root: PathBuf,
}

impl FileEventStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        let events_root = base_dir.join("events").join("streams");
        Self {
            base_dir,
            events_root,
        }
    }

    /// 确保目录存在
    async fn ensure_dir(&self) -> Result<()> {
        fs::create_dir_all(&self.events_root).await?;
        Ok(())
    }

    /// 获取 stream 目录
    fn stream_dir(&self, stream_id: &str) -> PathBuf {
        self.events_root.join(safe_part(stream_id))
    }

    /// 获取日期文件路径
    fn day_file(&self, stream_id: &str, dt: DateTime<Utc>) -> PathBuf {
        let day_key = dt.format("%Y-%m-%d").to_string();
        self.stream_dir(stream_id).join(format!("{}.jsonl", day_key))
    }

    /// 追加事件
    pub async fn append(
        &self,
        stream_id: &str,
        event_type: &str,
        payload: serde_json::Value,
        ts: Option<DateTime<Utc>>,
        event_id: Option<String>,
        seq: Option<i64>,
    ) -> Result<(Event, String, i64)> {
        self.ensure_dir().await?;

        let sid = stream_id.trim();
        if sid.is_empty() {
            anyhow::bail!("stream_id is required");
        }

        let et = event_type.trim();
        let et = if et.is_empty() { "event" } else { et };
        let eid = event_id.unwrap_or_else(|| format!("evt_{}", Uuid::new_v4().to_string()[..8].to_string()));
        let dt = ts.unwrap_or_else(Utc::now);
        let ts_ms = dt.timestamp_millis();

        let event = Event {
            event_id: eid,
            stream_id: sid.to_string(),
            ts: dt.to_rfc3339(),
            ts_ms,
            event_type: et.to_string(),
            payload,
            seq,
        };

        let fp = self.day_file(stream_id, dt);
        fs::create_dir_all(fp.parent().unwrap()).await?;

        // 追加并捕获 offset
        let line = serde_json::to_string(&event)?;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&fp)
            .await?;

        let offset = file.metadata().await?.len() as i64;
        file.write_all(format!("{}\n", line).as_bytes()).await?;
        file.flush().await?;

        Ok((event, fp.to_string_lossy().to_string(), offset))
    }

    /// 在指定 offset 读取事件
    pub async fn read_at(&self, file_path: &str, offset: i64) -> Result<Option<Event>> {
        let fp = Path::new(file_path);
        if !fp.exists() {
            return Ok(None);
        }

        let file = fs::File::open(fp).await?;
        let mut reader = BufReader::new(file);
        reader.seek(std::io::SeekFrom::Start(offset as u64)).await?;

        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(None);
        }

        match serde_json::from_str::<Event>(&line) {
            Ok(event) => Ok(Some(event)),
            Err(_) => Ok(None),
        }
    }
}

/// Tantivy 事件索引 (简化实现)
pub struct TansearchEventIndex {
    index_dir: PathBuf,
    #[allow(dead_code)]
    tokenizer: String,
}

impl TansearchEventIndex {
    pub fn new(index_dir: impl Into<PathBuf>, tokenizer: Option<String>) -> Self {
        Self {
            index_dir: index_dir.into(),
            tokenizer: tokenizer.unwrap_or_else(|| "jieba".to_string()),
        }
    }

    /// 添加事件到索引 (简化实现)
    pub async fn add(&self, _ref: &EventRef, _text: &str) -> Result<()> {
        // TODO: 实际实现应使用 tantivy 进行索引
        fs::create_dir_all(&self.index_dir).await?;
        Ok(())
    }

    /// 搜索事件引用 (简化实现)
    pub async fn search_refs(
        &self,
        _query: &str,
        _stream_id: Option<&str>,
        _event_type: Option<&str>,
        _start_ts_ms: Option<i64>,
        _end_ts_ms: Option<i64>,
        _limit: usize,
    ) -> Result<Vec<EventRef>> {
        // TODO: 实际实现应使用 tantivy 进行搜索
        Ok(Vec::new())
    }
}

/// 事件存储 (组合文件存储和索引)
pub struct EventStore {
    file_store: FileEventStore,
    index: TansearchEventIndex,
}

impl EventStore {
    pub fn new(base_dir: impl Into<PathBuf>, index_dir: Option<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        let index_dir = index_dir.unwrap_or_else(|| base_dir.join("events_index"));
        Self {
            file_store: FileEventStore::new(&base_dir),
            index: TansearchEventIndex::new(index_dir, None),
        }
    }

    /// 追加事件
    pub async fn append(
        &self,
        stream_id: &str,
        event_type: &str,
        payload: serde_json::Value,
        ts: Option<DateTime<Utc>>,
        event_id: Option<String>,
        seq: Option<i64>,
        text: Option<String>,
    ) -> Result<Event> {
        let (event, file, offset) = self
            .file_store
            .append(stream_id, event_type, payload, ts, event_id, seq)
            .await?;

        // 构建索引文本
        let index_text = text.unwrap_or_else(|| {
            format!("{} {}", event_type, serde_json::to_string(&event.payload).unwrap_or_default())
        });

        let event_ref = EventRef {
            event_id: event.event_id.clone(),
            stream_id: event.stream_id.clone(),
            ts_ms: event.ts_ms,
            event_type: event.event_type.clone(),
            file,
            offset,
        };

        self.index.add(&event_ref, &index_text).await?;

        Ok(event)
    }

    /// 搜索事件
    pub async fn search(
        &self,
        query: &str,
        stream_id: Option<&str>,
        event_type: Option<&str>,
        start_ts_ms: Option<i64>,
        end_ts_ms: Option<i64>,
        limit: usize,
    ) -> Result<Vec<Event>> {
        let refs = self
            .index
            .search_refs(query, stream_id, event_type, start_ts_ms, end_ts_ms, limit)
            .await?;

        let mut events = Vec::new();
        for event_ref in refs {
            if let Some(event) = self.file_store.read_at(&event_ref.file, event_ref.offset).await? {
                events.push(event);
                if events.len() >= limit {
                    break;
                }
            }
        }

        Ok(events)
    }
}

/// 安全文件名处理
fn safe_part(s: &str) -> String {
    let s2: String = s
        .trim()
        .replace('/', "_")
        .replace('\\', "_");
    if s2.is_empty() {
        "_".to_string()
    } else {
        s2[..s2.len().min(200)].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_event_store_append_and_read() {
        let temp_dir = TempDir::new().unwrap();
        let store = FileEventStore::new(temp_dir.path());

        // 追加事件
        let payload = serde_json::json!({"key": "value"});
        let (event, file, offset) = store
            .append("test_stream", "test_event", payload, None, None, None)
            .await
            .unwrap();

        assert_eq!(event.stream_id, "test_stream");
        assert_eq!(event.event_type, "test_event");

        // 读取事件
        let read_event = store.read_at(&file, offset).await.unwrap().unwrap();
        assert_eq!(read_event.event_id, event.event_id);
    }
}
