use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type ChatRole = String; // "system", "user", "assistant", "tool"

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepKind {
    #[serde(rename = "question")]
    Question,
    #[serde(rename = "answer")]
    Answer,
    #[serde(rename = "find_more_context")]
    FindMoreContext,
    // 新增步骤类型（与 Python 版本对齐）
    #[serde(rename = "fast_file_read")]
    FastFileRead,
    #[serde(rename = "file_process")]
    FileProcess,
    #[serde(rename = "generate_command")]
    GenerateCommand,
    #[serde(rename = "call_llm")]
    CallLlm,
    #[serde(rename = "check_validation")]
    CheckValidation,
    #[serde(rename = "bash_command")]
    BashCommand,
    #[serde(rename = "content_assemble")]
    ContentAssemble,
    #[serde(rename = "follow_guide")]
    FollowGuide,
    #[serde(rename = "request_user_input")]
    RequestUserInput,
    #[serde(rename = "task")]
    Task,
    #[serde(rename = "think_and_generate")]
    ThinkAndGenerate,
    // 控制流步骤类型
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "loop_block")]
    LoopBlock,
    // 内部控制流标记（由 chat_core 自动插入，不写在 skill 中）
    #[serde(rename = "enter_block")]
    EnterBlock,
    #[serde(rename = "exit_block")]
    ExitBlock,
}

impl Default for StepKind {
    fn default() -> Self {
        StepKind::Question
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "failed")]
    Failed,
}

impl Default for StepStatus {
    fn default() -> Self {
        StepStatus::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmPrompt {
    #[serde(default)]
    pub system: String,
    #[serde(default)]
    pub user: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputTarget {
    #[serde(default = "default_output_type")]
    pub output_type: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub target_block: Option<String>,
    pub identify_artifacts: Option<serde_json::Value>,
}

fn default_output_type() -> String {
    "context".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StepParams {
    pub file_paths: Option<String>,
    pub file_path: Option<String>,
    pub file_name: Option<String>,
    pub file_type: Option<String>,
    #[serde(default)]
    pub skip_if_none: bool,
    pub content_format: Option<String>,  // "json" | "text"
    pub items: Option<String>,
    pub help_message: Option<String>,
    pub skill_name: Option<String>,
    pub skill_content: Option<String>,
    pub exit_condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationFailureHandling {
    #[serde(default)]
    pub can_ignore: bool,
    #[serde(default)]
    pub fail_message: String,
}

/// 步骤的静态定义 — 来自 skill.json，创建后只读
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StepDef {
    #[serde(default)]
    pub step_name: String,
    pub step_type: Option<String>,
    #[serde(default)]
    pub action: String,
    pub llm_prompt: Option<LlmPrompt>,
    #[serde(default)]
    pub context_query: Vec<String>,
    #[serde(default)]
    pub validation: String,
    pub validation_failure_handling: Option<serde_json::Value>,
    pub output_target: Option<OutputTarget>,
    #[serde(default)]
    pub children: Vec<serde_json::Value>,
    pub params: Option<StepParams>,
    pub max_iterations: Option<i32>,
    pub max_retries: Option<i32>,
    /// 兜底未声明字段
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl StepDef {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 HashMap 构建 StepDef（通过 JSON 序列化转换）
    pub fn from_map(map: HashMap<String, serde_json::Value>) -> Self {
        serde_json::from_value(serde_json::Value::Object(
            map.into_iter().collect()
        ))
        .unwrap_or_default()
    }

    /// 获取字段值（支持已声明字段和 extra 字段）
    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.extra.get(key)
    }

    /// 获取字段值（已声明字段 + extra，返回 owned Value）
    pub fn get_owned(&self, key: &str) -> Option<serde_json::Value> {
        let full = serde_json::to_value(self).ok()?;
        full.get(key).cloned()
    }

    /// 获取字段字符串值
    pub fn get_str(&self, key: &str) -> &str {
        self.extra
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    /// 插入字段值（写入 extra HashMap）
    pub fn insert(&mut self, key: String, value: serde_json::Value) {
        self.extra.insert(key, value);
    }

    /// 检查字段是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        self.extra.contains_key(key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepState {
    pub step_id: String,
    #[serde(default)]
    pub step_goal: String,
    pub kind: StepKind,
    #[serde(default = "default_block_id")]
    pub block_id: String,
    /// 祖先链: [parent_block_id, grandparent_block_id, ...]
    #[serde(default)]
    pub block_ancestors: Vec<String>,
    #[serde(default)]
    pub step_def: StepDef,
    /// 运行时状态（执行过程中动态写入）
    #[serde(default)]
    pub runtime: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub children: Vec<StepState>,
    #[serde(default)]
    pub status: StepStatus,
    #[serde(default)]
    pub attempt: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

fn default_block_id() -> String {
    "root".to_string()
}

impl StepState {
    pub fn new(step_id: String, kind: StepKind, step_goal: String) -> Self {
        Self {
            step_id,
            step_goal,
            kind,
            block_id: "root".to_string(),
            block_ancestors: Vec::new(),
            step_def: StepDef::new(),
            runtime: HashMap::new(),
            children: Vec::new(),
            status: StepStatus::Pending,
            attempt: 0,
            started_at: None,
            ended_at: None,
        }
    }

    pub fn start_running(&mut self) {
        self.status = StepStatus::Running;
        self.attempt += 1;
        self.started_at = Some(Utc::now());
    }

    pub fn mark_completed(&mut self) {
        self.status = StepStatus::Completed;
        self.ended_at = Some(Utc::now());
    }

    pub fn mark_failed(&mut self) {
        self.status = StepStatus::Failed;
        self.ended_at = Some(Utc::now());
    }

    /// 解析 target_block 为目标 block_id
    ///
    /// - `"current"`: 当前 block（默认）
    /// - `"parent"`: 父 block（block_ancestors[0]，无则 "root"）
    /// - `"root"`: 根 block（始终为 "root"）
    pub fn resolve_target_block(&self, target_block: &str) -> String {
        match target_block {
            "root" => "root".to_string(),
            "parent" => {
                self.block_ancestors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "root".to_string())
            }
            _ => self.block_id.clone(), // "current" 或其他未知值
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default = "Utc::now")]
    pub ts: DateTime<Utc>,
    #[serde(default)]
    pub meta: HashMap<String, serde_json::Value>,
}

impl ChatMessage {
    pub fn new(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            image_url: None,
            ts: Utc::now(),
            meta: HashMap::new(),
        }
    }

    pub fn system(content: &str) -> Self {
        Self::new("system", content)
    }

    pub fn user(content: &str) -> Self {
        Self::new("user", content)
    }

    pub fn user_with_image(content: &str, image_url: &str) -> Self {
        let mut msg = Self::new("user", content);
        msg.image_url = Some(image_url.to_string());
        msg
    }

    pub fn assistant(content: &str) -> Self {
        Self::new("assistant", content)
    }
}

/// 聊天事件类型（单一事件源架构）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ChatEvent {
    #[serde(rename = "init")]
    Init { user_message_id: String, streaming_message_id: String },
    #[serde(rename = "step_status")]
    StepStatus { status: StepStatusInfo },
    #[serde(rename = "token")]
    Token { text: String },
    #[serde(rename = "attachments")]
    Attachments { attachments: Vec<serde_json::Value> },
    #[serde(rename = "queued")]
    Queued { position: usize },
    #[serde(rename = "processing")]
    Processing,
    #[serde(rename = "done")]
    Done { content: String, elapsed_seconds: f64, elapsed_display: String, completed_at: String },
    #[serde(rename = "error")]
    Error { message: String },
    #[serde(rename = "snapshot")]
    Snapshot { content: String, step_statuses: HashMap<String, StepStatusInfo>, attachments: Vec<serde_json::Value> },
}

/// 步骤状态信息
#[derive(Debug, Clone, Serialize)]
pub struct StepStatusInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub status_type: String, // progress, info, success, warning, error
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_hide: Option<bool>,
}

/// Action 规范（用于 Executor）
#[derive(Debug, Clone, Deserialize)]
pub struct ActionSpec {
    pub kind: ActionKind,
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

/// Action 类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Http,
    Python,
    Bash,
    Sql,
}

/// 执行结果（与 actforce ExecuteResult 对齐）
#[derive(Debug, Clone, Serialize)]
pub struct ExecuteResult {
    #[serde(default = "default_true")]
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[allow(dead_code)]
fn default_true() -> bool {
    true
}

impl Default for ExecuteResult {
    fn default() -> Self {
        Self {
            ok: true,
            stdout: None,
            stderr: None,
            exit_code: None,
            status_code: None,
            data: None,
        }
    }
}

impl ExecuteResult {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self {
            ok: true,
            stdout: Some(stdout.into()),
            ..Default::default()
        }
    }

    pub fn error(stderr: impl Into<String>) -> Self {
        Self {
            ok: false,
            stderr: Some(stderr.into()),
            ..Default::default()
        }
    }
}

// ============================================================================
// 以下为 core.rs / actforce-cli 保留的类型（actforce 中不使用）
// ============================================================================

/// 知识库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    pub tenant_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_dir: Option<String>,
    #[serde(default = "default_store_engine")]
    pub store_engine: StoreEngine,
    #[serde(default)]
    pub read_only: bool,
}

fn default_store_engine() -> StoreEngine {
    StoreEngine::Filesystem
}

/// 存储引擎类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreEngine {
    Filesystem,
    Memory,
}

/// 知识库文档
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDoc {
    pub id: String,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance: Option<f64>,
}

/// 向后兼容别名
pub type ToolResult = ExecuteResult;

// ============================================================================
// Event Store 类型 (event_store.rs 使用)
// ============================================================================

/// 事件引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRef {
    pub event_id: String,
    pub stream_id: String,
    pub ts_ms: i64,
    pub event_type: String,
    pub file: String,
    pub offset: i64,
}

/// 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub stream_id: String,
    pub ts: String,
    pub ts_ms: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<i64>,
}
