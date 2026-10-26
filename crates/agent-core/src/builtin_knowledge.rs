use crate::knowledge_store::MemoryKnowledgeStore;
use crate::types::{KnowledgeConfig, StoreEngine};
use std::sync::Arc;

/// 内置知识存储配置
fn builtin_knowledge_config() -> KnowledgeConfig {
    KnowledgeConfig {
        tenant_key: "builtin_knowledge".to_string(),
        doc_dir: None,
        index_dir: None,
        store_engine: StoreEngine::Memory,
        read_only: false,
    }
}

/// 创建空的内置知识存储。由上层 (actforce) 注册具体条目。
pub async fn create_builtin_knowledge_store() -> Arc<MemoryKnowledgeStore> {
    let config = builtin_knowledge_config();
    Arc::new(MemoryKnowledgeStore::new(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_builtin_knowledge_store_empty() {
        let store = create_builtin_knowledge_store().await;
        assert_eq!(store.count(), 0);
    }
}
