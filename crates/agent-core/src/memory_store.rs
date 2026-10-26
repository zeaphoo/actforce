use crate::types::MemoryEntry;
use anyhow::{Context, Result};
use chrono::Utc;
use regex::Regex;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// SQLite FTS5 内存存储
pub struct MemoryStore {
    max_entries: usize,
    persist_path: Option<String>,
    conn: Arc<Mutex<Connection>>,
}

impl MemoryStore {
    pub fn new(max_entries: usize, persist_path: Option<String>) -> Result<Self> {
        let conn = Connection::open(":memory:")?;

        let store = Self {
            max_entries,
            persist_path,
            conn: Arc::new(Mutex::new(conn)),
        };

        store.check_fts5()?;
        store.init_tables()?;

        // 加载持久化数据
        if store.persist_path.is_some() {
            store.load_from_jsonl()?;
        }

        Ok(store)
    }

    /// 检查 FTS5 可用性
    fn check_fts5(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("CREATE VIRTUAL TABLE IF NOT EXISTS _fts5_test USING fts5(x)", [])?;
        conn.execute("DROP TABLE IF EXISTS _fts5_test", [])?;
        Ok(())
    }

    /// 初始化表结构
    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        // 主表
        conn.execute(
            "CREATE TABLE IF NOT EXISTS entries (
                rowid INTEGER PRIMARY KEY AUTOINCREMENT,
                id TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL,
                content_tokens TEXT,
                tags TEXT,
                source TEXT,
                created_at TEXT,
                importance REAL DEFAULT 0.5,
                meta TEXT
            )",
            [],
        )?;

        // FTS5 虚拟表 - 使用独立表，不依赖外部内容
        conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
                content_tokens,
                tags
            )",
            [],
        )?;

        Ok(())
    }

    /// 列出所有 entries
    pub fn list(&self) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, content, tags, source, created_at, importance, meta 
             FROM entries ORDER BY rowid DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let meta_json: Option<String> = row.get(6)?;
            let meta: Option<HashMap<String, serde_json::Value>> = meta_json
                .and_then(|s| serde_json::from_str(&s).ok());

            Ok(MemoryEntry {
                id: row.get(0)?,
                content: row.get(1)?,
                tags,
                source: row.get(3)?,
                created_at: row.get(4)?,
                importance: row.get(5)?,
                meta,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }

    /// 添加 entry
    pub fn append(
        &self,
        content: &str,
        tags: Option<Vec<String>>,
        source: &str,
        importance: f64,
        meta: Option<HashMap<String, serde_json::Value>>,
        _dedup: bool,
    ) -> Result<MemoryEntry> {
        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("content is required");
        }

        let content_tokens = tokenize_for_fts(content);
        let entry = MemoryEntry {
            id: format!("mem_{}", Uuid::new_v4().to_string()[..8].to_string()),
            content: content.to_string(),
            tags: tags.unwrap_or_default(),
            source: source.to_string(),
            created_at: Utc::now().to_rfc3339(),
            importance,
            meta,
        };

        let meta_str: Option<String> = entry.meta.as_ref().map(|m| serde_json::to_string(m).unwrap());
        let tags_str = serde_json::to_string(&entry.tags)?;
        let conn = self.conn.lock().unwrap();
        
        // 插入主表
        conn.execute(
            "INSERT INTO entries (id, content, content_tokens, tags, source, created_at, importance, meta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            [
                &entry.id,
                &entry.content,
                &content_tokens,
                &tags_str,
                &entry.source,
                &entry.created_at,
                &importance.to_string(),
                meta_str.as_deref().unwrap_or(""),
            ],
        )?;
        
        // 获取刚插入的 rowid
        let rowid: i64 = conn.last_insert_rowid();
        
        // 手动插入到 FTS 表
        conn.execute(
            "INSERT INTO entries_fts(rowid, content_tokens, tags) VALUES (?1, ?2, ?3)",
            [&rowid.to_string(), &content_tokens, &tags_str],
        )?;

        drop(conn);
        
        // 同步写入 JSONL
        if self.persist_path.is_some() {
            self.append_to_jsonl(&entry)?;
        }

        // 维护最大条目数
        if self.max_entries > 0 {
            self.enforce_max_entries()?;
        }

        Ok(entry)
    }

    /// 维护最大条目数
    fn enforce_max_entries(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))?;

        let to_delete = count - self.max_entries as i64;
        if to_delete > 0 {
            // 先删除 FTS 表中的记录
            conn.execute(
                "DELETE FROM entries_fts WHERE rowid IN (
                    SELECT rowid FROM entries ORDER BY rowid ASC LIMIT ?1
                )",
                [to_delete],
            )?;
            
            // 再删除主表中的记录
            conn.execute(
                "DELETE FROM entries WHERE rowid IN (
                    SELECT rowid FROM entries ORDER BY rowid ASC LIMIT ?1
                )",
                [to_delete],
            )?;

            // 重写 JSONL
            drop(conn);
            if self.persist_path.is_some() {
                self.rewrite_jsonl()?;
            }
        }

        Ok(())
    }

    /// 搜索 entries
    pub fn search(
        &self,
        query: &str,
        top_k: usize,
        min_score: f64,
    ) -> Result<Vec<(f64, MemoryEntry)>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let query_tokens = tokenize_for_fts(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let safe_query = escape_fts5(&query_tokens);

        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT e.id, e.content, e.tags, e.source, e.created_at,
                    e.importance, e.meta, bm25(entries_fts) as bm25_score
             FROM entries_fts
             JOIN entries e ON e.rowid = entries_fts.rowid
             WHERE entries_fts MATCH ?1
             ORDER BY bm25(entries_fts) ASC
             LIMIT ?2"
        )?;

        let rows = stmt.query_map([&safe_query, &(top_k as i64).to_string()], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            let meta_json: Option<String> = row.get(6)?;
            let meta: Option<HashMap<String, serde_json::Value>> = meta_json
                .and_then(|s| serde_json::from_str(&s).ok());
            let bm25_score: f64 = row.get(7)?;
            let importance: f64 = row.get(5)?;

            // BM25 分数越小越好，转换为正分
            let score = (10.0 - bm25_score).max(0.0) * (0.5 + importance.min(1.0).max(0.0));

            Ok((
                score,
                MemoryEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    tags,
                    source: row.get(3)?,
                    created_at: row.get(4)?,
                    importance,
                    meta,
                },
            ))
        })?;

        let mut results = Vec::new();
        for row in rows {
            let (score, entry) = row?;
            if score >= min_score {
                results.push((score, entry));
            }
        }

        Ok(results)
    }

    /// 刷新到 JSONL
    pub fn flush(&self) -> Result<()> {
        if self.persist_path.is_some() {
            self.rewrite_jsonl()?;
        }
        Ok(())
    }

    /// 关闭连接
    pub fn close(&self) -> Result<()> {
        self.flush()?;
        Ok(())
    }

    /// 从 JSONL 加载
    fn load_from_jsonl(&self) -> Result<()> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };

        if !std::path::Path::new(path).exists() {
            return Ok(());
        }

        let content = std::fs::read_to_string(path)?;
        let conn = self.conn.lock().unwrap();

        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
                let entry = MemoryEntry {
                    id: obj.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    content: obj.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    tags: obj
                        .get("tags")
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    source: obj.get("source").and_then(|v| v.as_str()).unwrap_or("step_result").to_string(),
                    created_at: obj
                        .get("created_at")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&Utc::now().to_rfc3339())
                        .to_string(),
                    importance: obj.get("importance").and_then(|v| v.as_f64()).unwrap_or(0.5),
                    meta: obj.get("meta").and_then(|v| v.as_object()).map(|o| {
                        o.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect()
                    }),
                };

                let content_tokens = tokenize_for_fts(&entry.content);
                let meta_str: Option<String> = entry.meta.as_ref().map(|m| serde_json::to_string(m).unwrap());
                let tags_str = serde_json::to_string(&entry.tags).unwrap_or_default();
                
                // 插入主表
                let _ = conn.execute(
                    "INSERT INTO entries (id, content, content_tokens, tags, source, created_at, importance, meta)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    [
                        &entry.id,
                        &entry.content,
                        &content_tokens,
                        &tags_str,
                        &entry.source,
                        &entry.created_at,
                        &entry.importance.to_string(),
                        meta_str.as_deref().unwrap_or(""),
                    ],
                );
                
                // 获取 rowid 并插入到 FTS 表
                let rowid = conn.last_insert_rowid();
                let _ = conn.execute(
                    "INSERT INTO entries_fts(rowid, content_tokens, tags) VALUES (?1, ?2, ?3)",
                    [&rowid.to_string(), &content_tokens, &tags_str],
                );
            }
        }

        Ok(())
    }

    /// 追加到 JSONL
    fn append_to_jsonl(&self, entry: &MemoryEntry) -> Result<()> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let parent = std::path::Path::new(path).parent();
        if let Some(p) = parent {
            std::fs::create_dir_all(p)?;
        }

        let obj = serde_json::json!({
            "id": entry.id,
            "content": entry.content,
            "tags": entry.tags,
            "source": entry.source,
            "created_at": entry.created_at,
            "importance": entry.importance,
            "meta": entry.meta,
        });

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{}", serde_json::to_string(&obj)?)?;

        Ok(())
    }

    /// 重写 JSONL
    fn rewrite_jsonl(&self) -> Result<()> {
        let path = match &self.persist_path {
            Some(p) => p,
            None => return Ok(()),
        };

        let entries = self.list()?;
        let tmp_path = format!("{}.tmp.{}", path, Uuid::new_v4().to_string()[..8].to_string());

        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp_path)?;
            for entry in entries {
                let obj = serde_json::json!({
                    "id": entry.id,
                    "content": entry.content,
                    "tags": entry.tags,
                    "source": entry.source,
                    "created_at": entry.created_at,
                    "importance": entry.importance,
                    "meta": entry.meta,
                });
                writeln!(file, "{}", serde_json::to_string(&obj)?)?;
            }
        }

        std::fs::rename(tmp_path, path)?;
        Ok(())
    }
}

/// 使用 jieba 分词
fn tokenize_for_fts(text: &str) -> String {
    // 简化实现：按字符分割中文，保留英文单词
    let re = Regex::new(r"[A-Za-z0-9_]+|[\u4e00-\u9fff]").unwrap();
    re.find_iter(text)
        .map(|m| m.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 转义 FTS5 查询
fn escape_fts5(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_append_and_search() {
        let store = MemoryStore::new(100, None).unwrap();

        // 添加条目
        store
            .append(
                "这是一个测试内容",
                Some(vec!["test".to_string()]),
                "step_result",
                0.8,
                None,
                false,
            )
            .unwrap();

        store
            .append(
                "另一个测试",
                Some(vec!["other".to_string()]),
                "user",
                0.5,
                None,
                false,
            )
            .unwrap();

        // 搜索
        let results = store.search("测试", 10, 0.0).unwrap();
        assert!(!results.is_empty());

        // 列出
        let entries = store.list().unwrap();
        assert_eq!(entries.len(), 2);
    }
}
