use crate::core::{AgentLoop, LLMConfig};
use crate::knowledge_store::MemoryKnowledgeStore;
use crate::skill_registry::SkillDef;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::Duration;
use tracing::{debug, info};

// ============================================================================
// Re-exports from core.rs for backward compatibility
// ============================================================================
pub use crate::core::{
    AgentRuntime,
    Artifact,
    Attachment,
    ContextKind,
    DynamicSemaphore,
    EnvManager,
    StepContext,
    ThinkResult,
};

// ============================================================================
// ChatLoopManager / ChatScope
// ============================================================================

struct ManagedChatLoop {
    loop_: AgentLoop,
    last_used: Instant,
    in_flight: usize,
}

pub struct ChatScope {
    pub subject: String,
    pub identity_id: Option<String>,
    pub user_id: Option<String>,
}

impl ChatScope {
    pub fn cache_key(&self) -> &str {
        &self.subject
    }
}

/// Chat loop 管理器，负责：
/// 1. Loop 复用（同一个 subject 的多次 chat 共享 history 和 env）
/// 2. TTL 回收（空闲 loop 自动清理）
/// 3. 并发安全（in_flight 计数防止回收正在使用的 loop）
pub struct ChatLoopManager {
    items: Arc<RwLock<HashMap<String, ManagedChatLoop>>>,
    idle_ttl: Duration,
    #[allow(dead_code)]
    max_concurrent_sessions: Option<usize>,
    builtin_skills: RwLock<Vec<SkillDef>>,
    builtin_knowledge: RwLock<Option<Arc<MemoryKnowledgeStore>>>,
}

impl ChatLoopManager {
    /// 创建新的管理器
    ///
    /// 参数：
    /// - `idle_ttl_secs`: 空闲 loop 的 TTL（秒），默认 30 分钟
    /// - `sweep_interval_secs`: 扫描间隔（秒），默认 60 秒
    /// - `max_concurrent_sessions`: 全局最大并发会话数
    pub fn new(
        idle_ttl_secs: u64,
        sweep_interval_secs: u64,
        max_concurrent_sessions: Option<usize>,
    ) -> Self {
        let mgr = Self {
            items: Arc::new(RwLock::new(HashMap::new())),
            idle_ttl: Duration::from_secs(idle_ttl_secs),
            max_concurrent_sessions,
            builtin_skills: RwLock::new(Vec::new()),
            builtin_knowledge: RwLock::new(None),
        };

        // 初始化全局会话信号量
        if let Some(v) = max_concurrent_sessions {
            if v > 0 {
                let rt = tokio::runtime::Handle::current();
                rt.spawn(async move {
                    crate::core::set_global_session_limit(Some(v)).await;
                });
            }
        }

        // 启动 janitor 后台任务
        let items_clone = mgr.items.clone();
        let ttl = mgr.idle_ttl;
        let sweep_interval = Duration::from_secs(sweep_interval_secs);
        tokio::spawn(async move {
            Self::janitor_loop(items_clone, ttl, sweep_interval).await;
        });

        mgr
    }

    /// 更新最大并发会话数（全局生效）
    pub fn update_max_concurrent_sessions(&self, value: Option<usize>) {
        let rt = tokio::runtime::Handle::current();
        rt.spawn(async move {
            crate::core::set_global_session_limit(value).await;
        });
        info!("最大并发会话数已更新: {:?}", value);
    }

    /// 获取或创建 chat loop
    ///
    /// 如果缓存中已有对应 key 的 loop，更新 last_used 并返回克隆。
    /// 否则创建新 loop 并缓存。
    pub async fn get_loop(
        &self,
        scope: ChatScope,
        llm_config: Option<LLMConfig>,
    ) -> AgentLoop {
        let key = scope.cache_key().to_string();
        let now = Instant::now();

        let mut items = self.items.write().await;
        if let Some(item) = items.get_mut(&key) {
            item.last_used = now;
            return item.loop_.clone();
        }

        let mut loop_ = AgentLoop::new(
            scope.subject,
            scope.identity_id,
            scope.user_id,
            llm_config,
        );

        // Inject builtin skills into new loop
        {
            let builtins = self.builtin_skills.read().await;
            if !builtins.is_empty() {
                let mut reg = loop_.skill_registry.write().await;
                for skill in builtins.iter() {
                    reg.register(skill.clone());
                }
            }
        }

        // Inject builtin knowledge store into new loop
        {
            let kb = self.builtin_knowledge.read().await;
            if let Some(ref store) = *kb {
                loop_.knowledge_store = Some(store.clone());
            }
        }

        items.insert(key, ManagedChatLoop {
            loop_: loop_.clone(),
            last_used: now,
            in_flight: 0,
        });

        loop_
    }

    /// Register a builtin skill into existing and future loops (actforce startup)
    pub async fn register_builtin_skill(&self, skill: SkillDef) {
        let mut builtins = self.builtin_skills.write().await;
        builtins.push(skill.clone());

        let items = self.items.read().await;
        for item in items.values() {
            item.loop_.skill_registry.write().await.register(skill.clone());
        }
    }

    /// Set the builtin knowledge store and inject into existing and future loops
    pub async fn set_builtin_knowledge_store(&self, store: Arc<MemoryKnowledgeStore>) {
        // Update future loops
        *self.builtin_knowledge.write().await = Some(store.clone());

        // Inject into existing loops
        let mut items = self.items.write().await;
        for item in items.values_mut() {
            item.loop_.knowledge_store = Some(store.clone());
        }
    }

    /// 更新 loop 的最后使用时间（心跳）
    pub async fn touch(&self, key: &str) {
        if let Some(item) = self.items.write().await.get_mut(key) {
            item.last_used = Instant::now();
        }
    }

    /// 查询指定 subject 的 loop 是否正在运行
    /// agentlite parity: cache keys are "conv_{id}", so callers passing a bare
    /// conversation id used to miss the cache and always report not-running.
    pub async fn is_running_by_subject(&self, subject: &str) -> bool {
        let key = if subject.starts_with("conv_") {
            subject.to_string()
        } else {
            format!("conv_{}", subject)
        };
        let items = self.items.read().await;
        if let Some(item) = items.get(&key) {
            *item.loop_.running.read().await
        } else {
            false
        }
    }

    /// 标记 loop 正在使用/使用完毕
    ///
    /// delta > 0 表示新增一个请求，delta < 0 表示一个请求完成。
    pub async fn mark_in_flight(&self, key: &str, delta: isize) {
        if let Some(item) = self.items.write().await.get_mut(key) {
            item.in_flight = (item.in_flight as isize + delta).max(0) as usize;
            item.last_used = Instant::now();
        }
    }

    /// Janitor 后台任务：定期扫描并回收空闲 loop
    async fn janitor_loop(
        items: Arc<RwLock<HashMap<String, ManagedChatLoop>>>,
        ttl: Duration,
        sweep_interval: Duration,
    ) {
        let mut interval = tokio::time::interval(sweep_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;
            let now = Instant::now();
            let mut guard = items.write().await;

            guard.retain(|key, item| {
                if item.in_flight > 0 {
                    true // 正在使用中，保留
                } else if now.duration_since(item.last_used) > ttl {
                    debug!("Recycling idle chat loop: {}", key);
                    false // TTL 超时，回收
                } else {
                    true
                }
            });
        }
    }
}
