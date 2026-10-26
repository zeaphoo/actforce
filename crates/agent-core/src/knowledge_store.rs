use crate::types::{KnowledgeDoc, KnowledgeConfig, StoreEngine};
use anyhow::Result;
use chrono::Utc;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;



#[async_trait::async_trait]
pub trait KnowledgeStoreBackend: Send + Sync {
    async fn upsert(
        &self,
        title: &str,
        content: &str,
        tags: Option<Vec<String>>,
        doc_id: Option<String>,
        importance: Option<f64>,
    ) -> Result<KnowledgeDoc>;

    async fn get(&self, doc_id: &str) -> Result<Option<KnowledgeDoc>>;
    async fn delete(&self, doc_id: &str) -> Result<bool>;
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeDoc>>;

    /// 按标签组合查询：require_all 全部匹配（AND），require_any 至少匹配一个（OR）
    async fn query_by_tags(
        &self,
        require_all: &[String],
        require_any: &[String],
    ) -> Result<Vec<KnowledgeDoc>>;
}

pub struct FileSystemKnowledgeStore {
    _config: KnowledgeConfig,
    _base_dir: PathBuf,
}

impl FileSystemKnowledgeStore {
    pub fn new(config: KnowledgeConfig, base_dir: impl Into<PathBuf>) -> Result<Self> {
        let base_dir = base_dir.into();
        Ok(Self {
            _config: config,
            _base_dir: base_dir,
        })
    }
}

#[async_trait::async_trait]
impl KnowledgeStoreBackend for FileSystemKnowledgeStore {
    async fn upsert(
        &self,
        _title: &str,
        _content: &str,
        _tags: Option<Vec<String>>,
        _doc_id: Option<String>,
        _importance: Option<f64>,
    ) -> Result<KnowledgeDoc> {
        anyhow::bail!("FileSystemKnowledgeStore not fully implemented")
    }

    async fn get(&self, _doc_id: &str) -> Result<Option<KnowledgeDoc>> {
        Ok(None)
    }

    async fn delete(&self, _doc_id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn search(&self, _query: &str, _top_k: usize) -> Result<Vec<KnowledgeDoc>> {
        Ok(Vec::new())
    }

    async fn query_by_tags(
        &self,
        _require_all: &[String],
        _require_any: &[String],
    ) -> Result<Vec<KnowledgeDoc>> {
        Ok(Vec::new())
    }
}

#[derive(Debug)]
pub struct MemoryKnowledgeStore {
    config: KnowledgeConfig,
    data: Arc<Mutex<HashMap<String, KnowledgeDoc>>>,
}

impl MemoryKnowledgeStore {
    pub fn new(config: KnowledgeConfig) -> Self {
        Self {
            config,
            data: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl KnowledgeStoreBackend for MemoryKnowledgeStore {
    async fn upsert(
        &self,
        title: &str,
        content: &str,
        tags: Option<Vec<String>>,
        doc_id: Option<String>,
        importance: Option<f64>,
    ) -> Result<KnowledgeDoc> {
        if self.config.read_only {
            anyhow::bail!("MemoryKnowledgeStore is read-only");
        }

        let title = title.trim();
        if title.is_empty() {
            anyhow::bail!("title is required");
        }

        let content = content.trim();
        if content.is_empty() {
            anyhow::bail!("content is required");
        }

        let now = Utc::now().to_rfc3339();
        let doc_id = doc_id.unwrap_or_else(|| format!("kb_{}", Uuid::new_v4().to_string()[..8].to_string()));

        let mut data = self.data.lock().unwrap();
        let created_at = data
            .get(&doc_id)
            .map(|d| d.created_at.clone())
            .unwrap_or_else(|| now.clone());

        let doc = KnowledgeDoc {
            id: doc_id.clone(),
            title: title.to_string(),
            content: content.to_string(),
            tags: tags.unwrap_or_default(),
            created_at,
            updated_at: now,
            importance,
        };

        data.insert(doc_id, doc.clone());
        Ok(doc)
    }

    async fn get(&self, doc_id: &str) -> Result<Option<KnowledgeDoc>> {
        let data = self.data.lock().unwrap();
        Ok(data.get(doc_id).cloned())
    }

    async fn delete(&self, doc_id: &str) -> Result<bool> {
        if self.config.read_only {
            anyhow::bail!("MemoryKnowledgeStore is read-only");
        }

        let mut data = self.data.lock().unwrap();
        Ok(data.remove(doc_id).is_some())
    }

    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeDoc>> {
        let toks = tokenize(query);
        if toks.is_empty() {
            return Ok(Vec::new());
        }

        let data = self.data.lock().unwrap();
        let mut scored: Vec<(f64, KnowledgeDoc)> = Vec::new();

        for doc in data.values() {
            let tags_s = doc.tags.join(" ");
            let mut score = 0.0;
            for t in &toks {
                score += 5.0 * count_occurrences(&doc.title, t) as f64;
                score += 3.0 * count_occurrences(&tags_s, t) as f64;
                score += 1.0 * count_occurrences(&doc.content, t) as f64;
            }

            if score > 0.0 {
                scored.push((score, doc.clone()));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        Ok(scored.into_iter().take(top_k).map(|(_, d)| d).collect())
    }

    async fn query_by_tags(
        &self,
        require_all: &[String],
        require_any: &[String],
    ) -> Result<Vec<KnowledgeDoc>> {
        Ok(self.query_by_tags_sync(require_all, require_any))
    }
}

impl MemoryKnowledgeStore {
    /// Synchronous search across all documents.
    pub fn search_sync(&self, query: &str, top_k: usize) -> Vec<KnowledgeDoc> {
        let toks = tokenize(query);
        if toks.is_empty() {
            return Vec::new();
        }

        let data = self.data.lock().unwrap();
        let mut scored: Vec<(f64, KnowledgeDoc)> = Vec::new();

        for doc in data.values() {
            let tags_s = doc.tags.join(" ");
            let mut score = 0.0;
            for t in &toks {
                score += 5.0 * count_occurrences(&doc.title, t) as f64;
                score += 3.0 * count_occurrences(&tags_s, t) as f64;
                score += 1.0 * count_occurrences(&doc.content, t) as f64;
            }

            if score > 0.0 {
                scored.push((score, doc.clone()));
            }
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        scored.into_iter().take(top_k).map(|(_, d)| d).collect()
    }

    /// List all documents, sorted by updated_at descending.
    pub fn list_all(&self) -> Vec<KnowledgeDoc> {
        let data = self.data.lock().unwrap();
        let mut docs: Vec<KnowledgeDoc> = data.values().cloned().collect();
        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        docs
    }

    /// Total number of documents in the store.
    pub fn count(&self) -> usize {
        self.data.lock().unwrap().len()
    }

    /// 按标签组合查询：require_all 全部匹配（AND），require_any 至少匹配一个（OR）。
    pub fn query_by_tags_sync(
        &self,
        require_all: &[String],
        require_any: &[String],
    ) -> Vec<KnowledgeDoc> {
        let data = self.data.lock().unwrap();
        let mut docs: Vec<KnowledgeDoc> = data
            .values()
            .filter(|d| {
                let tags_joined = d.tags.join(" ");
                require_all
                    .iter()
                    .all(|tag| tags_joined.contains(tag.as_str()))
                    && (require_any.is_empty()
                        || require_any
                            .iter()
                            .any(|tag| tags_joined.contains(tag.as_str())))
            })
            .cloned()
            .collect();
        docs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        docs
    }
}

pub struct KnowledgeStore {
    backend: Box<dyn KnowledgeStoreBackend>,
}

impl KnowledgeStore {
    pub fn new(config: KnowledgeConfig, base_dir: impl Into<PathBuf>) -> Result<Self> {
        let base_dir = base_dir.into();
        let backend: Box<dyn KnowledgeStoreBackend> = match config.store_engine {
            StoreEngine::Filesystem => Box::new(FileSystemKnowledgeStore::new(config, base_dir)?),
            StoreEngine::Memory => Box::new(MemoryKnowledgeStore::new(config)),
        };

        Ok(Self { backend })
    }

    pub async fn upsert(
        &self,
        title: &str,
        content: &str,
        tags: Option<Vec<String>>,
        doc_id: Option<String>,
        importance: Option<f64>,
    ) -> Result<KnowledgeDoc> {
        self.backend
            .upsert(title, content, tags, doc_id, importance)
            .await
    }

    pub async fn get(&self, doc_id: &str) -> Result<Option<KnowledgeDoc>> {
        self.backend.get(doc_id).await
    }

    pub async fn delete(&self, doc_id: &str) -> Result<bool> {
        self.backend.delete(doc_id).await
    }

    pub async fn search(&self, query: &str, top_k: usize) -> Result<Vec<KnowledgeDoc>> {
        self.backend.search(query, top_k).await
    }

    pub async fn query_by_tags(
        &self,
        require_all: &[String],
        require_any: &[String],
    ) -> Result<Vec<KnowledgeDoc>> {
        self.backend.query_by_tags(require_all, require_any).await
    }
}

fn tokenize(q: &str) -> Vec<String> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return Vec::new();
    }

    let re = Regex::new(r"[A-Za-z0-9_]+|[\u4e00-\u9fff]+").unwrap();
    re.find_iter(&q)
        .map(|m| m.as_str().to_string())
        .collect()
}

fn count_occurrences(text: &str, token: &str) -> usize {
    if text.is_empty() || token.is_empty() {
        return 0;
    }
    text.to_lowercase().matches(&token.to_lowercase()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_knowledge_store() {
        let config = KnowledgeConfig {
            tenant_key: "test".to_string(),
            doc_dir: None,
            index_dir: None,
            store_engine: StoreEngine::Memory,
            read_only: false,
        };

        let store = MemoryKnowledgeStore::new(config);

        let doc = store
            .upsert("Test Title", "Test Content", Some(vec!["tag1".to_string()]), None, Some(0.8))
            .await
            .unwrap();

        assert_eq!(doc.title, "Test Title");
        assert_eq!(doc.content, "Test Content");

        let fetched = store.get(&doc.id).await.unwrap();
        assert!(fetched.is_some());

        let results = store.search("test", 10).await.unwrap();
        assert!(!results.is_empty());

        let deleted = store.delete(&doc.id).await.unwrap();
        assert!(deleted);
    }
}
