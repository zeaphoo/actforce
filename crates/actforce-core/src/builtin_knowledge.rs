use agent_core::knowledge_store::{KnowledgeStoreBackend, MemoryKnowledgeStore};
use serde::Deserialize;
use tracing::info;

/// content 字段支持字符串或字符串数组两种格式
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ContentField {
    String(String),
    Array(Vec<String>),
}

impl ContentField {
    fn into_string(self) -> String {
        match self {
            ContentField::String(s) => s,
            ContentField::Array(arr) => arr.join("\n"),
        }
    }
}

/// 单条知识条目（从 .knowledge.json 解析）
#[derive(Debug, Deserialize)]
struct KnowledgeEntry {
    #[serde(default)]
    id: Option<String>,
    title: String,
    #[serde(default)]
    tags: Vec<String>,
    content: ContentField,
}

/// 嵌入式知识文件清单（编译时使用 include_str! 嵌入）
const KNOWLEDGE_FILES: &[(&str, &str)] = &[
    (
        "agent_mindset.knowledge.json",
        include_str!("knowledge/agent_mindset.knowledge.json"),
    ),
];

/// 加载所有嵌入知识文件中的条目。
fn load_knowledge_entries() -> Vec<KnowledgeEntry> {
    let mut entries = Vec::new();

    for (filename, content) in KNOWLEDGE_FILES {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 尝试解析为 list 格式
        if let Ok(list) = serde_json::from_str::<Vec<KnowledgeEntry>>(trimmed) {
            for entry in list {
                entries.push(entry);
            }
            continue;
        }

        // 尝试解析为单条 dict 格式
        match serde_json::from_str::<KnowledgeEntry>(trimmed) {
            Ok(entry) => entries.push(entry),
            Err(e) => {
                tracing::warn!("Failed to parse knowledge file {}: {}", filename, e);
            }
        }
    }

    entries
}

/// 注册内置知识到知识存储。
/// 从 compile-time embedded `knowledge/*.knowledge.json` 文件加载。
pub async fn register_builtin_knowledge(store: &MemoryKnowledgeStore) {
    let entries = load_knowledge_entries();
    info!("Registering {} builtin knowledge entries", entries.len());
    for entry in entries {
        let doc_id = entry.id.unwrap_or_else(|| {
            // 若无 id，从标题生成
            entry.title.chars().take(32).collect::<String>()
        });
        let content = entry.content.into_string();
        if let Err(e) = store
            .upsert(
                &entry.title,
                &content,
                Some(entry.tags),
                Some(doc_id),
                Some(0.8),
            )
            .await
        {
            tracing::error!("Failed to upsert builtin knowledge: {}", e);
        }
    }
}
