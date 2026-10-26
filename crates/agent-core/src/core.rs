use crate::chat_store::ChatStore;
use crate::skill_registry::SkillRegistry;
use crate::types::{
    ChatEvent,
    ChatMessage,
    StepDef,
    StepKind,
    StepState,
    StepStatus,
    StepStatusInfo,
};
use anyhow::Result;
use chrono::{ DateTime, Utc };
use minijinja::Environment;
use serde::{ Deserialize, Serialize };
use std::collections::HashMap;
use std::path::{ Path, PathBuf };
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::{ mpsc, Notify, RwLock };
use tokio::time::Duration;
use tracing::{ debug, error, info, warn };
use uuid::Uuid;

// ============================================================================
// 工具函数
// ============================================================================

/// 将秒数格式化为人类可读的耗时字符串。
/// - < 60s → "12.5s"
/// - 60s ~ 3599s → "2m 30s"
/// - >= 3600s → "1h 5m 12s"
fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{:.1}s", seconds)
    } else if seconds < 3600.0 {
        let minutes = seconds as u64 / 60;
        let secs = seconds as u64 % 60;
        format!("{}m {}s", minutes, secs)
    } else {
        let hours = seconds as u64 / 3600;
        let minutes = (seconds as u64 % 3600) / 60;
        let secs = seconds as u64 % 60;
        format!("{}h {}m {}s", hours, minutes, secs)
    }
}

fn format_step_result_for_context(result: &str) -> String {
    let stripped = result.trim();
    if stripped.is_empty() {
        return result.to_string();
    }
    format!("```\n{}\n```", stripped)
}

fn save_to_file(
    conversation_id: &str,
    step_id: &str,
    content: &str,
    name: &str,
    ext: &str,
    chat_files_dir: &Path
) -> Result<String> {
    use std::io::Write;
    let safe_name: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_' )
        .collect::<String>()
        .trim()
        .to_string();
    let safe_name = if safe_name.is_empty() { format!("output_{}", step_id) } else { safe_name };
    let output_dir = chat_files_dir.join(conversation_id).join("outputs");
    std::fs::create_dir_all(&output_dir)?;
    let mut file_path = output_dir.join(format!("{}.{}", safe_name, ext));
    let mut counter = 1;
    while file_path.exists() {
        file_path = output_dir.join(format!("{}_{}.{}", safe_name, counter, ext));
        counter += 1;
    }
    let mut file = std::fs::File::create(&file_path)?;
    file.write_all(content.as_bytes())?;
    Ok(file_path.canonicalize()?.to_string_lossy().to_string())
}

/// 从大模型返回的文本中健壮地提取 JSON
fn extract_json_from_text(text: &str) -> Option<serde_json::Value> {
    let text = text.trim();
    // 尝试直接解析
    if let Ok(value) = serde_json::from_str(text) {
        return Some(value);
    }
    // 尝试从 markdown 代码块中提取
    let patterns = [
        r"```json\s*\n(.*?)\n```",
        r"```\s*\n(.*?)\n```",
        r"```json\s*(.*?)```",
        r"```\s*(.*?)```",
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(text) {
                if let Some(json_str) = captures.get(1) {
                    let json_str = json_str.as_str().trim();
                    if let Ok(value) = serde_json::from_str(json_str) {
                        return Some(value);
                    }
                }
            }
        }
    }
    // 尝试找到最大的有效 JSON 对象
    let mut max_json = None;
    let mut max_len = 0;
    if let Ok(re) = regex::Regex::new(r"\{[^{}]+\}") {
        for captures in re.find_iter(text) {
            let candidate = captures.as_str();
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate) {
                if candidate.len() > max_len {
                    max_json = Some(value);
                    max_len = candidate.len();
                }
            }
        }
    }
    // Fallback: 用正则提取关键字段
    if max_json.is_none() {
        let matched_m = regex::Regex
            ::new(r#""matched"\s*:\s*(true|false)"#)
            .ok()
            .and_then(|re| re.captures(text))
            .and_then(|c| c.get(1).map(|m| m.as_str() == "true"));
        let skill_id_m = regex::Regex
            ::new(r#""skill_id"\s*:\s*"([^"]+)""#)
            .ok()
            .and_then(|re| re.captures(text))
            .and_then(|c| c.get(1).map(|m| m.as_str().to_string()));
        if let (Some(matched), Some(skill_id)) = (matched_m, skill_id_m) {
            let reason = regex::Regex
                ::new(r#""reason"\s*:\s*"([^"]*)""#)
                .ok()
                .and_then(|re| re.captures(text))
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
                .unwrap_or_default();
            return Some(
                serde_json::json!({"matched": matched, "skill_id": skill_id, "reason": reason})
            );
        }
    }
    max_json
}

/// 检查文本是否包含有效的 JSON 结果
#[allow(dead_code)]
fn check_json_result(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    // 尝试直接解析
    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
        return true;
    }
    // 检查是否包含代码块中的 JSON
    if text.contains("```json") || text.contains("```") {
        if extract_json_from_text(text).is_some() {
            return true;
        }
    }
    false
}

/// 尝试将文本解析为 JSON 对象
fn try_text_to_json(text: &str) -> serde_json::Value {
    let stripped = text.trim();
    if stripped.is_empty() {
        return serde_json::Value::String(text.to_string());
    }
    // 1. 尝试直接解析
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(stripped) {
        if data.is_string() {
            return serde_json::Value::String(text.to_string());
        }
        return data;
    }
    // 2. 尝试从 markdown 代码块中提取 JSON
    let patterns = [
        r"```json\s*\n(.*?)\n```",
        r"```\s*\n(.*?)\n```",
        r"```json\s*(.*?)```",
        r"```\s*(.*?)```",
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(stripped) {
                if let (Some(m), Some(full)) = (captures.get(1), captures.get(0)) {
                    let json_str = m.as_str().trim();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        let outside = stripped.replace(full.as_str(), "");
                        let outside_trimmed = outside.trim();
                        if outside_trimmed.len() < (stripped.len() * 3) / 10 {
                            return data;
                        }
                    }
                }
            }
        }
    }
    serde_json::Value::String(text.to_string())
}

/// 无条件从文本中提取 JSON（dict/list），失败返回 None
fn ensure_json(text: &str) -> Option<serde_json::Value> {
    let stripped = text.trim();
    if stripped.is_empty() {
        return None;
    }
    // 1. 尝试通过 try_text_to_json（处理干净 JSON / 少量外部内容 markdown）
    let v = try_text_to_json(stripped);
    if v.is_object() || v.is_array() {
        return Some(v);
    }
    // 2. 直接从 markdown 代码块提取（不计较外部内容占比）
    let patterns = [
        r"```json\s*\n(.*?)\n```",
        r"```\s*\n(.*?)\n```",
        r"```json\s*(.*?)```",
        r"```\s*(.*?)```",
    ];
    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(captures) = re.captures(stripped) {
                if let Some(m) = captures.get(1) {
                    let json_str = m.as_str().trim();
                    if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                        if data.is_object() || data.is_array() {
                            return Some(data);
                        }
                    }
                }
            }
        }
    }
    // 3. 最后尝试直接 parse
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(stripped) {
        if data.is_object() || data.is_array() {
            return Some(data);
        }
    }
    None
}

fn new_attachment_id() -> String {
    format!("att_{}", &Uuid::new_v4().to_string()[..8])
}

/// 从文本中提取 [[path]] 标记并验证文件是否存在
fn extract_path_attachments(
    text: &str,
    conversation_id: &str,
    chat_files_dir: &Path,
    disks_dir: Option<&Path>
) -> Vec<Attachment> {
    let re = match regex::Regex::new(r"\[\[(.+?)\]\]") {
        Ok(re) => re,
        Err(_) => {
            return vec![];
        }
    };
    let mut attachments = Vec::new();
    let chat_output_dir = chat_files_dir.join(conversation_id).join("outputs");
    for cap in re.captures_iter(text) {
        if let Some(path_match) = cap.get(1) {
            let path_str = path_match.as_str().trim().to_string();
            if path_str.is_empty() {
                continue;
            }
            // 检查聊天 output 目录
            let file_path_in_chat = chat_output_dir.join(&path_str);
            if file_path_in_chat.exists() && file_path_in_chat.is_file() {
                if let Ok(metadata) = std::fs::metadata(&file_path_in_chat) {
                    let file_type = guess_mime_type_from_path(&file_path_in_chat);
                    let mut meta = HashMap::new();
                    meta.insert(
                        "url".to_string(),
                        serde_json::json!(
                            format!("/api/chat/files/{}/outputs/{}", conversation_id, path_str)
                        )
                    );
                    meta.insert("source".to_string(), serde_json::json!("chat_output"));
                    attachments.push(Attachment {
                        attachment_id: new_attachment_id(),
                        file_name: file_path_in_chat
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        file_path: path_str.clone(),
                        file_type,
                        file_size: metadata.len() as i64,
                        metadata: meta,
                        uploaded_at: Utc::now(),
                    });
                }
                continue;
            }
            // 检查 disks_default raw 目录
            if let Some(dd) = disks_dir {
                let file_path_in_disks = dd.join("raw").join(&path_str);
                if file_path_in_disks.exists() && file_path_in_disks.is_file() {
                    if let Ok(metadata) = std::fs::metadata(&file_path_in_disks) {
                        let file_type = guess_mime_type_from_path(&file_path_in_disks);
                        let mut meta = HashMap::new();
                        meta.insert(
                            "url".to_string(),
                            serde_json::json!(format!("/api/disks/default/raw/{}", path_str))
                        );
                        meta.insert("source".to_string(), serde_json::json!("disks_default"));
                        attachments.push(Attachment {
                            attachment_id: new_attachment_id(),
                            file_name: file_path_in_disks
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string(),
                            file_path: path_str.clone(),
                            file_type,
                            file_size: metadata.len() as i64,
                            metadata: meta,
                            uploaded_at: Utc::now(),
                        });
                    }
                }
            }
        }
    }
    if !attachments.is_empty() {
        debug!("Extracted {} path attachments from text", attachments.len());
    }
    attachments
}

/// 根据文件路径猜测 MIME 类型
fn guess_mime_type_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "gif" => "image/gif".into(),
        "webp" => "image/webp".into(),
        "svg" => "image/svg+xml".into(),
        "pdf" => "application/pdf".into(),
        "txt" => "text/plain".into(),
        "md" | "markdown" => "text/markdown".into(),
        "json" => "application/json".into(),
        "csv" => "text/csv".into(),
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".into(),
        "xls" => "application/vnd.ms-excel".into(),
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
        "doc" => "application/msword".into(),
        "html" | "htm" => "text/html".into(),
        _ => "application/octet-stream".into(),
    }
}

fn new_chat_run_id() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    format!("chat_{}_{}", now.as_millis(), now.as_nanos() % 1_000_000)
}

fn new_chat_step_id() -> String {
    format!("chat_step_{}", &Uuid::new_v4().to_string()[..8])
}
fn new_context_id(index: usize) -> String {
    format!("ctx_{}", index + 1)
}

const ALLOWED_BASH_COMMANDS: &[&str] = &[
    "cat",
    "head",
    "tail",
    "grep",
    "jq",
    "wc",
    "cut",
    "sort",
    "uniq",
    "awk",
    "sed",
    "find",
    "ls",
    "file",
    "less",
    "more",
    "zcat",
    "xzcat",
];
const DANGER_KEYWORDS: &[&str] = &[
    "rm ",
    "rmdir ",
    "del ",
    "chmod ",
    "chown ",
    "chgrp ",
    "mv ",
    "cp ",
    "sudo ",
    "su ",
    "mkfs",
    "dd ",
    "wget ",
    "curl ",
    "nc ",
    "ssh ",
    "scp ",
    "ftp ",
    "kill ",
    "pkill ",
    "killall ",
    "shutdown",
    "reboot",
    "export ",
    "source ",
    "eval ",
    "exec ",
];

fn is_safe_command(cmd: &str) -> bool {
    let cmd_lower = cmd.to_lowercase();
    for &kw in DANGER_KEYWORDS {
        if cmd_lower.contains(kw) {
            return false;
        }
    }
    if cmd.contains(">") || cmd.contains(">>") || cmd.contains("`") || cmd.contains("$(") {
        return false;
    }
    for part in cmd.split('|') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = part.split_whitespace().collect();
        if let Some(cmd_name) = tokens.first() {
            let base = cmd_name.rsplit('/').next().unwrap_or(cmd_name);
            if !ALLOWED_BASH_COMMANDS.contains(&base) {
                return false;
            }
        }
    }
    true
}

// ============================================================================
// EnvManager
// ============================================================================

pub struct EnvScope<'a> {
    manager: &'a EnvManager,
}
impl<'a> Drop for EnvScope<'a> {
    fn drop(&mut self) {
        self.manager._scopes.lock().unwrap().pop();
    }
}

#[derive(Debug, Clone)]
pub struct EnvManager {
    _base_env: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    _scopes: Arc<Mutex<Vec<HashMap<String, serde_json::Value>>>>,
}

impl EnvManager {
    pub fn new(base_env: HashMap<String, serde_json::Value>) -> Self {
        Self {
            _base_env: Arc::new(Mutex::new(base_env)),
            _scopes: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn get(&self, key: &str) -> Option<serde_json::Value> {
        let scopes = self._scopes.lock().unwrap();
        for scope in scopes.iter().rev() {
            if let Some(v) = scope.get(key) {
                return Some(v.clone());
            }
        }
        self._base_env.lock().unwrap().get(key).cloned()
    }
    pub fn set(&self, key: &str, value: serde_json::Value) {
        let mut scopes = self._scopes.lock().unwrap();
        if let Some(scope) = scopes.last_mut() {
            scope.insert(key.to_string(), value);
        } else {
            self._base_env.lock().unwrap().insert(key.to_string(), value);
        }
    }
    pub fn to_dict(&self) -> HashMap<String, serde_json::Value> {
        let mut result = self._base_env.lock().unwrap().clone();
        for scope in self._scopes.lock().unwrap().iter() {
            result.extend(scope.clone());
        }
        result
    }
    pub fn scope(&self, overrides: HashMap<String, serde_json::Value>) -> EnvScope<'_> {
        self._scopes.lock().unwrap().push(overrides);
        EnvScope { manager: self }
    }
    pub fn update(&self, vars: HashMap<String, serde_json::Value>) {
        let mut scopes = self._scopes.lock().unwrap();
        if let Some(scope) = scopes.last_mut() {
            scope.extend(vars);
        } else {
            self._base_env.lock().unwrap().extend(vars);
        }
    }
    pub fn copy(&self) -> Self {
        Self { _base_env: self._base_env.clone(), _scopes: self._scopes.clone() }
    }
}

// ============================================================================
// Context / Artifact / Attachment
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContextKind {
    #[serde(rename = "context")] Context,
    #[serde(rename = "artifact")] Artifact,
}
impl std::fmt::Display for ContextKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextKind::Context => write!(f, "context"),
            ContextKind::Artifact => write!(f, "artifact"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepContext {
    pub context_id: String,
    // agentlite parity: contexts.jsonl / API use "block_id" (Python field name);
    // read both spellings for backward compatibility.
    #[serde(rename = "block_id", alias = "phase_id")]
    pub phase_id: String,
    pub step_id: String,
    pub step_name: String,
    pub kind: ContextKind,
    pub tags: Vec<String>,
    pub content: serde_json::Value,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}
impl StepContext {
    pub fn new(
        context_id: String,
        phase_id: String,
        step_id: String,
        step_name: String,
        kind: ContextKind,
        content: serde_json::Value,
        tags: Vec<String>
    ) -> Self {
        Self {
            context_id,
            phase_id,
            step_id,
            step_name,
            kind,
            tags,
            content,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }
    pub fn is_context(&self) -> bool {
        matches!(self.kind, ContextKind::Context)
    }
    pub fn is_artifact(&self) -> bool {
        matches!(self.kind, ContextKind::Artifact)
    }
    pub fn get_files_info(&self) -> Vec<serde_json::Value> {
        if let Some(obj) = self.content.as_object() {
            if let Some(relevant_files) = obj.get("relevant_files") {
                if let Some(arr) = relevant_files.as_array() {
                    return arr.clone();
                }
            }
        }
        if let Some(arr) = self.content.as_array() {
            return arr
                .iter()
                .filter(|item|
                    item
                        .as_object()
                        .map_or(false, |obj| obj.contains_key("path") && obj.contains_key("name"))
                )
                .cloned()
                .collect();
        }
        Vec::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub attachment_id: String,
    pub file_path: String,
    pub file_name: String,
    #[serde(default)] pub file_type: String,
    #[serde(default)] pub file_size: i64,
    #[serde(default)] pub metadata: HashMap<String, serde_json::Value>,
    #[serde(default = "Utc::now")] pub uploaded_at: DateTime<Utc>,
}

impl Attachment {
    pub fn from_dict(data: &serde_json::Value) -> Self {
        let obj = data.as_object().cloned().unwrap_or_default();
        let mut metadata: HashMap<String, serde_json::Value> = obj
            .get("metadata")
            .and_then(|v| v.as_object())
            .map(|m|
                m
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect()
            )
            .unwrap_or_default();
        if let Some(url) = obj.get("url") {
            metadata.insert("url".to_string(), url.clone());
        }
        Self {
            attachment_id: obj
                .get("attachment_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&new_attachment_id())
                .to_string(),
            file_path: obj
                .get("path")
                .or_else(|| obj.get("file_path"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            file_name: obj
                .get("name")
                .or_else(|| obj.get("file_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            file_type: obj
                .get("type")
                .or_else(|| obj.get("file_type"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            file_size: obj
                .get("size")
                .or_else(|| obj.get("file_size"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            metadata,
            uploaded_at: obj
                .get("uploaded_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        }
    }
    pub fn to_dict(&self) -> serde_json::Value {
        // Frontend-compatible shape ({name,type,size,path,url} + extra fields),
        // matching agentlite's attachment dict so WS events, DB content_json
        // and history messages all render attachments correctly.
        let url = self.metadata.get("url").cloned().unwrap_or(serde_json::Value::Null);
        serde_json::json!({"attachment_id": self.attachment_id, "path": self.file_path, "name": self.file_name, "type": self.file_type, "size": self.file_size, "url": url, "metadata": self.metadata, "uploaded_at": self.uploaded_at.to_rfc3339()})
    }
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub artifact_id: String,
    pub phase_id: String,
    pub step_id: String,
    pub step_name: String,
    pub artifact_type: String,
    pub path: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

// ============================================================================
// AgentRuntime
// ============================================================================

#[derive(Debug, Clone)]
pub struct AgentRuntime {
    pub run_id: String,
    pub subject: String,
    pub phase_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_content: String,
    pub system_prompt: Option<String>,
    pub assistant_text: String,
    pub steps: Vec<StepState>,
    pub completed_step_ids: Vec<String>,
    pub contexts: Vec<StepContext>,
    pub artifacts: Vec<Artifact>,
    pub attachments: Vec<Attachment>,
    pub cancelled: bool,
    pub params: HashMap<String, serde_json::Value>,
    pub env: EnvManager,
    pub knowledge_store: Option<Arc<crate::knowledge_store::MemoryKnowledgeStore>>,
}

impl AgentRuntime {
    pub fn new(subject: String, user_content: String, system_prompt: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            run_id: new_chat_run_id(),
            subject,
            phase_id: String::new(),
            created_at: now,
            updated_at: now,
            user_content,
            system_prompt,
            assistant_text: String::new(),
            steps: Vec::new(),
            completed_step_ids: Vec::new(),
            contexts: Vec::new(),
            artifacts: Vec::new(),
            attachments: Vec::new(),
            cancelled: false,
            params: HashMap::new(),
            env: EnvManager::new(HashMap::new()),
            knowledge_store: None,
        }
    }
    pub fn push_step(&mut self, st: StepState) {
        self.steps.push(st);
    }
    pub fn pop_step(&mut self) -> Option<StepState> {
        self.steps.pop()
    }
    pub fn mark_step_completed(&mut self, step_id: &str) {
        if !step_id.is_empty() && !self.completed_step_ids.contains(&step_id.to_string()) {
            self.completed_step_ids.push(step_id.to_string());
        }
    }
    pub fn add_context(
        &mut self,
        phase_id: &str,
        step_id: &str,
        step_name: &str,
        content: serde_json::Value,
        tags: Vec<String>
    ) -> StepContext {
        let ctx = StepContext::new(
            new_context_id(self.contexts.len()),
            phase_id.to_string(),
            step_id.to_string(),
            step_name.to_string(),
            ContextKind::Context,
            content,
            tags
        );
        self.contexts.push(ctx.clone());
        ctx
    }
    pub fn add_artifact(
        &mut self,
        phase_id: &str,
        step_id: &str,
        step_name: &str,
        artifact_type: &str,
        path: &str,
        metadata: HashMap<String, serde_json::Value>
    ) -> Artifact {
        let art = Artifact {
            artifact_id: format!("art_{}", self.artifacts.len() + 1),
            phase_id: phase_id.to_string(),
            step_id: step_id.to_string(),
            step_name: step_name.to_string(),
            artifact_type: artifact_type.to_string(),
            path: path.to_string(),
            metadata,
            created_at: Utc::now(),
        };
        self.artifacts.push(art.clone());
        art
    }
    pub fn get_all_artifacts(&self) -> Vec<&Artifact> {
        self.artifacts.iter().collect()
    }

    /// 按 context_id 获取 content
    pub fn get_context(&self, key: &str) -> Option<serde_json::Value> {
        self.contexts
            .iter()
            .find(|ctx| ctx.context_id == key)
            .map(|ctx| ctx.content.clone())
    }
    /// 按步骤 ID 查找
    pub fn find_contexts_by_step_id(&self, step_id: &str) -> Vec<&StepContext> {
        self.contexts
            .iter()
            .filter(|ctx| ctx.step_id == step_id)
            .collect()
    }
    /// 按步骤名称查找
    pub fn find_contexts_by_step_name(&self, step_name: &str) -> Vec<&StepContext> {
        self.contexts
            .iter()
            .filter(|ctx| ctx.step_name == step_name)
            .collect()
    }
    /// 按类型查找
    pub fn find_contexts_by_kind(&self, kind: &ContextKind) -> Vec<&StepContext> {
        self.contexts
            .iter()
            .filter(|ctx| std::mem::discriminant(&ctx.kind) == std::mem::discriminant(kind))
            .collect()
    }
    /// 按阶段 ID 查找
    pub fn find_contexts_by_phase_id(&self, phase_id: &str) -> Vec<&StepContext> {
        if phase_id.is_empty() {
            return Vec::new();
        }
        self.contexts
            .iter()
            .filter(|ctx| ctx.phase_id == phase_id)
            .collect()
    }
    /// 按标签查找
    pub fn find_contexts_by_tags(&self, tags: &[String]) -> Vec<&StepContext> {
        let tag_set: std::collections::HashSet<&str> = tags
            .iter()
            .map(|s| s.as_str())
            .collect();
        self.contexts
            .iter()
            .filter(|ctx| {
                let ctx_tags: std::collections::HashSet<&str> = ctx.tags
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                !tag_set.is_disjoint(&ctx_tags)
            })
            .collect()
    }
    /// 按步骤名称查找，返回第一个匹配的 content
    pub fn find_context_by_step_name(&self, step_name: &str) -> Option<serde_json::Value> {
        self.contexts
            .iter()
            .find(|ctx| ctx.step_name == step_name)
            .map(|ctx| ctx.content.clone())
    }
    /// 获取所有 context 条目
    pub fn get_all_contexts(&self) -> Vec<&StepContext> {
        self.contexts.iter().collect()
    }
    /// 获取所有文件信息（从 artifacts 中提取）
    pub fn get_all_files_info(&self) -> Vec<serde_json::Value> {
        self.artifacts
            .iter()
            .map(|art| {
                let name = art.metadata
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                serde_json::json!({"path": art.path, "name": name, "type": art.artifact_type, "tags": []})
            })
            .collect()
    }

    pub fn execute_context_query(&self, context_query: &[String]) -> Vec<serde_json::Value> {
        if context_query.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::new();
        for query in context_query {
            if query.starts_with("@llm:") {
                continue;
            }
            if query.starts_with('#') {
                let tag = &query[1..];
                let matched = self.find_contexts_by_tags(&[tag.to_string()]);
                for ctx in matched {
                    result.push(ctx.content.clone());
                }
            } else if query.starts_with('$') {
                let keyword = &query[1..];
                if keyword == "all" {
                    for ctx in &self.contexts {
                        result.push(ctx.content.clone());
                    }
                } else if keyword == "builtin_skills" {
                    let skills_text = self.load_builtin_skills_text();
                    if !skills_text.is_empty() {
                        result.push(serde_json::Value::String(skills_text));
                    }
                } else if keyword.starts_with("builtin_knowledge") {
                    let search_kw = if let Some((_, kw)) = keyword.split_once(':') {
                        kw.trim().to_string()
                    } else {
                        String::new()
                    };
                    if let Some(ref store) = self.knowledge_store {
                        if search_kw.is_empty() || search_kw.eq_ignore_ascii_case("all") {
                            let all_docs = store.list_all();
                            if all_docs.is_empty() {
                                result.push(serde_json::Value::String(
                                    "内置知识库暂无内容。".to_string()
                                ));
                            } else {
                                let mut lines = vec![format!(
                                    "## 内置知识库（共 {} 条）",
                                    all_docs.len()
                                )];
                                for d in &all_docs {
                                    let tags_str = d.tags.join(", ");
                                    lines.push(format!(
                                        "- [{}] **{}**{}",
                                        d.id,
                                        d.title,
                                        if tags_str.is_empty() { String::new() } else { format!(" `{}`", tags_str) }
                                    ));
                                }
                                result.push(serde_json::Value::String(lines.join("\n")));
                            }
                        } else {
                            let all_results = store.search_sync(&search_kw, 20);
                            if all_results.is_empty() {
                                result.push(serde_json::Value::String(format!(
                                    "未在内置知识库中找到与 '{}' 相关的内容。",
                                    search_kw
                                )));
                            } else {
                                let parts: Vec<String> = all_results.iter().map(|d| {
                                    let preview = if d.content.len() > 500 {
                                        format!("{}...", &d.content[..500])
                                    } else {
                                        d.content.clone()
                                    };
                                    format!("### [{}] {}\n{}", d.id, d.title, preview)
                                }).collect();
                                result.push(serde_json::Value::String(parts.join("\n\n")));
                            }
                        }
                    }
                } else {
                    let matched = self.find_contexts_by_tags(&[query.clone()]);
                    for ctx in matched {
                        result.push(ctx.content.clone());
                    }
                }
            } else {
                for ctx in &self.contexts {
                    if ctx.content.to_string().contains(query.as_str()) {
                        result.push(ctx.content.clone());
                    }
                }
            }
        }
        // 去重（保持顺序）
        let mut seen = std::collections::HashSet::new();
        let mut unique_result = Vec::new();
        for item in result {
            let item_str = item.to_string();
            if seen.insert(item_str) {
                unique_result.push(item);
            }
        }
        if !unique_result.is_empty() {
            debug!(
                "context_query matched {} contexts with queries: {:?}",
                unique_result.len(),
                context_query
            );
        }
        unique_result
    }

    /// 加载所有启用的内置技能列表（文本格式）
    fn load_builtin_skills_text(&self) -> String {
        // agent-core 简化实现，实际实现可根据项目结构调整
        String::new()
    }
}

// ============================================================================
// LLMConfig / DynamicSemaphore
// ============================================================================

#[derive(Clone)]
pub struct LLMConfig {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub debug: bool,
    pub max_retries: usize,
    pub retry_min_wait_secs: u64,
    pub retry_max_wait_secs: u64,
    pub max_seq_len: usize,
}
impl Default for LLMConfig {
    fn default() -> Self {
        let auto_start = std::env::var("INFERENCE_AUTO_START")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let base_url = if auto_start {
            let host = std::env::var("INFERENCE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
            let port = std::env::var("INFERENCE_PORT").unwrap_or_else(|_| "8080".to_string());
            format!("http://{host}:{port}/v1")
        } else {
            std::env::var("LLM_BASE_URL").unwrap_or_else(|_| {
                let host = std::env::var("INFERENCE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
                let port = std::env::var("INFERENCE_PORT").unwrap_or_else(|_| "8080".to_string());
                format!("http://{host}:{port}/v1")
            })
        };

        let api_key = std::env::var("LLM_API_KEY").ok();
        let model = if auto_start {
            std::env::var("INFERENCE_MODEL_NAME").ok()
        } else {
            std::env::var("LLM_MODEL").ok().or_else(|| std::env::var("INFERENCE_MODEL_NAME").ok())
        };
        let debug = std::env::var("LLM_DEBUG")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        let max_retries = std::env::var("LLM_MAX_RETRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let retry_min_wait_secs = std::env::var("LLM_RETRY_MIN_WAIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1);
        let retry_max_wait_secs = std::env::var("LLM_RETRY_MAX_WAIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        let max_seq_len = std::env::var("LLM_MAX_SEQ_LEN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32768);

        Self {
            base_url,
            api_key,
            model,
            debug,
            max_retries,
            retry_min_wait_secs,
            retry_max_wait_secs,
            max_seq_len,
        }
    }
}

pub struct DynamicSemaphore {
    limit: tokio::sync::Mutex<usize>,
    used: AtomicUsize,
    waiting: AtomicUsize,
    notify: Notify,
}
impl DynamicSemaphore {
    pub fn new(v: usize) -> Self {
        Self {
            limit: tokio::sync::Mutex::new(v),
            used: AtomicUsize::new(0),
            waiting: AtomicUsize::new(0),
            notify: Notify::new(),
        }
    }
    pub async fn acquire(&self) {
        loop {
            let current = self.used.load(Ordering::Acquire);
            let limit = *self.limit.lock().await;
            if current < limit {
                if
                    self.used.compare_exchange(
                        current,
                        current + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire
                    ).is_ok()
                {
                    return;
                }
            } else {
                self.notify.notified().await;
            }
        }
    }
    /// Acquire and send `Queued` / `Processing` events atomically with position
    /// reporting. agentlite parity: the reported position includes the caller
    /// itself plus already-used slots (position = waiting + used), a `Queued`
    /// event is emitted on entry, and `Processing` only after the slot is won.
    pub async fn acquire_with_events(&self, tx: &mpsc::Sender<ChatEvent>) {
        // Enter the queue: report position = waiting + used (includes self)
        let waiting = self.waiting.fetch_add(1, Ordering::AcqRel);
        let used = self.used.load(Ordering::Acquire);
        let _ = tx.send(ChatEvent::Queued { position: waiting + used }).await;
        loop {
            let current = self.used.load(Ordering::Acquire);
            let limit = *self.limit.lock().await;
            if current < limit {
                if
                    self.used.compare_exchange(
                        current,
                        current + 1,
                        Ordering::AcqRel,
                        Ordering::Acquire
                    ).is_ok()
                {
                    break;
                }
            } else {
                // Re-push the current queue position while waiting
                // (agentlite pushes position updates on release)
                let w = self.waiting.load(Ordering::Acquire);
                let _ = tx.send(ChatEvent::Queued { position: w.saturating_sub(1) + current }).await;
                self.notify.notified().await;
            }
        }
        self.waiting.fetch_sub(1, Ordering::AcqRel);
        let _ = tx.send(ChatEvent::Processing).await;
    }
    pub fn release(&self) {
        self.used.fetch_sub(1, Ordering::Release);
        self.notify.notify_waiters();
    }
    pub async fn set_limit(&self, n: usize) {
        *self.limit.lock().await = n;
        self.notify.notify_waiters();
    }
    pub async fn waiting_count(&self) -> usize {
        let used = self.used.load(Ordering::Acquire);
        let limit = *self.limit.lock().await;
        used.saturating_sub(limit)
    }
}
/// Storage paths injected by the host application (actforce) so agent-core
/// does not hardcode /tmp/actforce. No environment variables are introduced;
/// the host calls configure_paths() at startup.
static CORE_PATHS: std::sync::OnceLock<CorePaths> = std::sync::OnceLock::new();

#[derive(Clone)]
pub struct CorePaths {
    pub data_root: PathBuf,
    pub chat_files_dir: PathBuf,
    pub disks_dir: PathBuf,
}

/// Host-side injection of storage paths (called by actforce at startup).
pub fn configure_paths(data_root: PathBuf, chat_files_dir: PathBuf, disks_dir: PathBuf) {
    let _ = CORE_PATHS.set(CorePaths { data_root, chat_files_dir, disks_dir });
}

pub(crate) fn core_paths() -> Option<&'static CorePaths> {
    CORE_PATHS.get()
}

static GLOBAL_LLM_SEMAPHORE: std::sync::LazyLock<Arc<DynamicSemaphore>> = std::sync::LazyLock::new(
    || Arc::new(DynamicSemaphore::new(24))
);
pub fn get_global_llm_semaphore() -> Arc<DynamicSemaphore> {
    GLOBAL_LLM_SEMAPHORE.clone()
}
pub async fn set_global_llm_limit(n: usize) {
    get_global_llm_semaphore().set_limit(n).await;
}

// Global semaphore for concurrent sessions (across ALL loops)
static GLOBAL_SESSION_SEMAPHORE: std::sync::LazyLock<tokio::sync::RwLock<Option<Arc<DynamicSemaphore>>>> =
    std::sync::LazyLock::new(|| tokio::sync::RwLock::new(None));

pub async fn get_global_session_semaphore() -> Option<Arc<DynamicSemaphore>> {
    GLOBAL_SESSION_SEMAPHORE.read().await.clone()
}

pub async fn set_global_session_limit(value: Option<usize>) {
    let mut guard = GLOBAL_SESSION_SEMAPHORE.write().await;
    match value {
        Some(v) if v > 0 => {
            if let Some(ref sem) = *guard {
                sem.set_limit(v).await;
            } else {
                *guard = Some(Arc::new(DynamicSemaphore::new(v)));
            }
        }
        _ => *guard = None,
    }
}
struct SemaphoreGuard(Arc<DynamicSemaphore>);
impl Drop for SemaphoreGuard {
    fn drop(&mut self) {
        self.0.release();
    }
}

async fn emit_event(event: ChatEvent, tx: &mpsc::Sender<ChatEvent>) {
    let _ = tx.send(event).await;
}
async fn emit_step_status(
    step_id: &str,
    status_type: &str,
    content: &str,
    progress: Option<u8>,
    tx: &mpsc::Sender<ChatEvent>
) {
    emit_event(
        ChatEvent::StepStatus {
            status: StepStatusInfo {
                id: format!("status_{}", step_id),
                status_type: status_type.to_string(),
                content: content.to_string(),
                progress,
                visible: None,
                auto_hide: None,
            },
        },
        tx
    ).await;
}

fn truncate_image_urls(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut r = serde_json::Map::new();
            for (k, v) in map {
                if k == "url" {
                    if let Some(s) = v.as_str() {
                        if s.starts_with("data:") {
                            r.insert(
                                k.clone(),
                                serde_json::Value::String(
                                    if s.len() > 50 {
                                        format!("{}...", &s[..50])
                                    } else {
                                        s.to_string()
                                    }
                                )
                            );
                        } else {
                            r.insert(k.clone(), v.clone());
                        }
                    } else {
                        r.insert(k.clone(), v.clone());
                    }
                } else {
                    r.insert(k.clone(), truncate_image_urls(v));
                }
            }
            serde_json::Value::Object(r)
        }
        serde_json::Value::Array(a) =>
            serde_json::Value::Array(a.iter().map(truncate_image_urls).collect()),
        o => o.clone(),
    }
}

// ============================================================================
// AgentLoop
// ============================================================================

#[derive(Clone)]
pub struct AgentLoop {
    pub subject: String,
    pub llm_config: LLMConfig,
    pub store: ChatStore,
    pub chat_files_dir: PathBuf,
    pub running: Arc<RwLock<bool>>,
    pub started_at: Arc<RwLock<Option<i64>>>,
    pub streaming_message_id: Arc<RwLock<Option<String>>>,
    pub event_buffer: Arc<RwLock<Vec<ChatEvent>>>,
    pub is_queued: Arc<RwLock<bool>>,
    pub queue_position: Arc<RwLock<usize>>,
    history_loaded: Arc<RwLock<bool>>,
    history: Arc<RwLock<Vec<ChatMessage>>>,
    pub skill_registry: Arc<RwLock<SkillRegistry>>,
    pub knowledge_store: Option<Arc<crate::knowledge_store::MemoryKnowledgeStore>>,
}

/// 思考分析结果 — LLM 深度分析用户需求后产出的结构化结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkResult {
    pub analysis: String,
    pub next_steps: Vec<String>,
    pub missing_info: Vec<String>,
    pub expected_output: String,
}

impl AgentLoop {
    pub fn new(
        subject: String,
        identity_id: Option<String>,
        user_id: Option<String>,
        llm_config: Option<LLMConfig>,
    ) -> Self {
        let sub = subject.trim().to_string();
        if sub.is_empty() {
            panic!("subject is required");
        }
        let _user_id_clone = user_id.clone();
        // Storage paths are injected by the host via configure_paths();
        // fall back to a temp dir when running standalone.
        let data_root = core_paths()
            .map(|p| p.data_root.clone())
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp/actforce"));
        let store = ChatStore::new(identity_id, user_id, &data_root);
        let chat_files_dir = core_paths()
            .map(|p| p.chat_files_dir.clone())
            .unwrap_or_else(|| data_root.join("chat_files"));
        Self {
            subject: sub,
            llm_config: llm_config.unwrap_or_default(),
            store,
            chat_files_dir,
            running: Arc::new(RwLock::new(false)),
            started_at: Arc::new(RwLock::new(None)),
            streaming_message_id: Arc::new(RwLock::new(None)),
            event_buffer: Arc::new(RwLock::new(Vec::new())),
            is_queued: Arc::new(RwLock::new(false)),
            queue_position: Arc::new(RwLock::new(0)),
            history_loaded: Arc::new(RwLock::new(false)),
            history: Arc::new(RwLock::new(Vec::new())),
            skill_registry: Arc::new(RwLock::new(SkillRegistry::new())),
            knowledge_store: None,
        }
    }

    async fn ensure_history_loaded(&self) -> Result<()> {
        if *self.history_loaded.read().await {
            return Ok(());
        }
        let stored = self.store.list_messages(&self.subject, 200).await?;
        let mut h = self.history.write().await;
        *h = stored
            .into_iter()
            .map(|m| ChatMessage {
                role: m.role,
                content: m.content,
                ts: m.ts,
                meta: m.meta,
                image_url: None,
            })
            .collect();
        *self.history_loaded.write().await = true;
        Ok(())
    }

    fn to_llm_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .map(|m| {
                if let Some(ref img) = m.image_url {
                    serde_json::json!({"role": m.role.to_string(), "content": [{"type":"text","text":m.content},{"type":"image_url","image_url":{"url":img}}]})
                } else {
                    serde_json::json!({"role": m.role.to_string(), "content": m.content})
                }
            })
            .collect()
    }

    pub async fn complete_text(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        top_p: f32
    ) -> Result<String> {
        // agentlite parity: 按 max_seq_len 预算截断消息（max_seq_len - max(2048, max_seq_len/5)）
        let output_budget = std::cmp::max(2048, self.llm_config.max_seq_len / 5);
        let estimated_tokens: usize = messages.iter().map(Self::estimate_message_tokens).sum();
        let messages: Vec<ChatMessage> = if
            estimated_tokens > self.llm_config.max_seq_len.saturating_sub(output_budget)
        {
            Self::truncate_messages(messages, self.llm_config.max_seq_len, output_budget)
        } else {
            messages.to_vec()
        };
        let sem = get_global_llm_semaphore();
        sem.acquire().await;
        let _g = SemaphoreGuard(sem);
        // agentlite parity: httpx client timeout=300s
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let url = format!("{}/chat/completions", self.llm_config.base_url);
        let mut req = client.post(&url);
        if let Some(ref k) = self.llm_config.api_key {
            req = req.header("Authorization", format!("Bearer {}", k));
        }
        let body =
            serde_json::json!({"model": self.llm_config.model.as_deref().unwrap_or("default"), "messages": Self::to_llm_messages(&messages), "temperature": temperature, "top_p": top_p, "max_tokens": 2048, "stop": ["<|im_end|>", "<|endoftext|>"], "stream": false, "extra_body": {"enable_thinking": false}, "chat_template_kwargs": {"enable_thinking": false}});
        if self.llm_config.debug {
            debug!(
                "LLM 请求: {}",
                serde_json::to_string_pretty(&truncate_image_urls(&body)).unwrap_or_default()
            );
        }
        // agentlite parity: stop_after_attempt(max_retries) + wait_exponential（min_wait * 2^attempt，上限 max_wait）
        let max_retries = self.llm_config.max_retries.max(1);
        for attempt in 0..max_retries {
            let wait_secs = self
                .llm_config
                .retry_min_wait_secs
                .saturating_mul(2u64.saturating_pow(attempt as u32))
                .min(self.llm_config.retry_max_wait_secs);
            match
                req
                    .try_clone()
                    .ok_or_else(|| anyhow::anyhow!("无法克隆请求体"))?
                    .json(&body)
                    .send().await
            {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let s = resp.status();
                        let b = resp.text().await.unwrap_or_default();
                        if attempt + 1 < max_retries {
                            warn!(
                                "LLM 请求失败 HTTP {} (尝试 {}/{}), {}s 后重试: {}",
                                s,
                                attempt + 1,
                                max_retries,
                                wait_secs,
                                b
                            );
                            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                            continue;
                        }
                        return Err(anyhow::anyhow!("LLM HTTP {} - {}", s, b));
                    }
                    let d: serde_json::Value = resp.json().await?;
                    let msg = &d["choices"][0]["message"];
                    return Ok(
                        msg["content"].as_str()
                            .or_else(|| msg["reasoning"].as_str())
                            .unwrap_or("").to_string()
                    );
                }
                Err(e) => {
                    if attempt + 1 < max_retries {
                        warn!(
                            "LLM 请求失败 (尝试 {}/{}) {}s 后重试: {}",
                            attempt + 1,
                            max_retries,
                            wait_secs,
                            e
                        );
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                    } else {
                        return Err(anyhow::anyhow!(e));
                    }
                }
            }
        }
        unreachable!()
    }

    pub async fn stream_text(
        &self,
        messages: &[ChatMessage],
        temperature: f32,
        top_p: f32,
        tx: mpsc::Sender<String>
    ) -> Result<String> {
        // agentlite parity: 按 max_seq_len 预算截断消息（max_seq_len - max(2048, max_seq_len/5)）
        let output_budget = std::cmp::max(2048, self.llm_config.max_seq_len / 5);
        let estimated_tokens: usize = messages.iter().map(Self::estimate_message_tokens).sum();
        let messages: Vec<ChatMessage> = if
            estimated_tokens > self.llm_config.max_seq_len.saturating_sub(output_budget)
        {
            Self::truncate_messages(messages, self.llm_config.max_seq_len, output_budget)
        } else {
            messages.to_vec()
        };
        let sem = get_global_llm_semaphore();
        sem.acquire().await;
        let _g = SemaphoreGuard(sem);
        // agentlite parity: httpx client timeout=300s
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let url = format!("{}/chat/completions", self.llm_config.base_url);
        let mut req = client.post(&url);
        if let Some(ref k) = self.llm_config.api_key {
            req = req.header("Authorization", format!("Bearer {}", k));
        }
        let body =
            serde_json::json!({"model": self.llm_config.model.as_deref().unwrap_or("default"), "messages": Self::to_llm_messages(&messages), "temperature": temperature, "top_p": top_p, "max_tokens": 2048, "stop": ["<|im_end|>", "<|endoftext|>"], "stream": true, "extra_body": {"enable_thinking": false}, "chat_template_kwargs": {"enable_thinking": false}});
        // agentlite parity: stop_after_attempt(max_retries) + wait_exponential（min_wait * 2^attempt，上限 max_wait）
        let max_retries = self.llm_config.max_retries.max(1);
        for attempt in 0..max_retries {
            let wait_secs = self
                .llm_config
                .retry_min_wait_secs
                .saturating_mul(2u64.saturating_pow(attempt as u32))
                .min(self.llm_config.retry_max_wait_secs);
            match
                req
                    .try_clone()
                    .ok_or_else(|| anyhow::anyhow!("无法克隆请求体"))?
                    .json(&body)
                    .send().await
            {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        let s = resp.status();
                        let b = resp.text().await.unwrap_or_default();
                        if attempt + 1 < max_retries {
                            warn!(
                                "LLM Stream 请求失败 HTTP {} (尝试 {}/{}), {}s 后重试: {}",
                                s,
                                attempt + 1,
                                max_retries,
                                wait_secs,
                                b
                            );
                            tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                            continue;
                        }
                        return Err(anyhow::anyhow!("LLM Stream HTTP {} - {}", s, b));
                    }
                    let mut stream = resp.bytes_stream();
                    let mut full = String::new();
                    let mut buf = String::new();
                    use futures::StreamExt;
                    while let Some(chunk) = stream.next().await {
                        buf.push_str(&String::from_utf8_lossy(&chunk?));
                        while let Some(pos) = buf.find('\n') {
                            let line = buf[..pos].trim().to_string();
                            buf = buf[pos + 1..].to_string();
                            if !line.starts_with("data: ") {
                                continue;
                            }
                            let d = &line[6..];
                            if d == "[DONE]" {
                                break;
                            }
                            if let Ok(data) = serde_json::from_str::<serde_json::Value>(d) {
                                let delta = &data["choices"][0]["delta"];
                                if let Some(c) = delta["content"].as_str() {
                                    if !c.is_empty() {
                                        full.push_str(c);
                                        let _ = tx.send(c.to_string()).await;
                                    }
                                } else if let Some(c) = delta["reasoning"].as_str() {
                                    full.push_str(c);
                                    let _ = tx.send(c.to_string()).await;
                                }
                            }
                        }
                    }
                    return Ok(full);
                }
                Err(e) => {
                    if attempt + 1 < max_retries {
                        warn!(
                            "LLM Stream 请求失败 (尝试 {}/{}) {}s 后重试: {}",
                            attempt + 1,
                            max_retries,
                            wait_secs,
                            e
                        );
                        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
                    } else {
                        return Err(anyhow::anyhow!(e));
                    }
                }
            }
        }
        unreachable!()
    }

    async fn persist_assistant_message(
        &self,
        subject: &str,
        text: &str,
        attachments: Option<Vec<serde_json::Value>>,
        meta: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<()> {
        let atts = attachments.and_then(|items| if items.is_empty() {
                None
            } else {
                let mut m = HashMap::new();
                m.insert("items".to_string(), serde_json::Value::Array(items));
                Some(m)
            } );
        self.store.append_message(subject, "assistant", text, meta, atts).await?;
        Ok(())
    }

    async fn append_and_cache(
        &self,
        role: &str,
        content: &str,
        attachments: Option<HashMap<String, serde_json::Value>>
    ) -> Result<()> {
        self.store.append_message(&self.subject, role, content, None, attachments).await?;
        if *self.history_loaded.read().await {
            let mut h = self.history.write().await;
            h.push(ChatMessage::new(role, content));
        }
        Ok(())
    }

    fn build_cmd_context(&self, rt: &AgentRuntime) -> minijinja::Value {
        let mut m = HashMap::new();
        for (k, v) in rt.env.to_dict() {
            let mv = match v {
                serde_json::Value::String(s) => minijinja::Value::from(s),
                serde_json::Value::Bool(b) => minijinja::Value::from(b),
                serde_json::Value::Number(n) => minijinja::Value::from(n.as_i64().unwrap_or(0)),
                _ => minijinja::Value::from(format!("{}", v)),
            };
            m.insert(k, mv);
        }
        minijinja::Value::from_serialize(&m)
    }

    /// 让 LLM 深度思考用户需求，返回分析结果和下一步计划
    pub async fn think(
        &self,
        user_content: &str,
        context_text: &str,
    ) -> Result<ThinkResult> {
        let context_block = if context_text.is_empty() {
            String::new()
        } else {
            format!("\n\n## 已有上下文\n{}", context_text)
        };

        let messages = vec![
            ChatMessage::system(
                "你是一个任务分析专家。在行动之前，请先深入理解用户需求并规划下一步。\n\n\
                 ## 分析要求\n\
                 1. **理解需求含义**：挖掘用户问题的核心意图，识别显性和隐性需求，明确任务边界和约束条件\n\
                 2. **评估上下文**：筛选已有上下文中的关键信息，判断信息充足性\n\
                 3. **识别信息缺口**：列出还需要获取的信息或数据\n\
                 4. **规划下一步**：给出具体、可执行的下一步行动计划，按优先级排序\n\
                 5. **定义预期输出**：明确最终应该产出的结果是什么\n\n\
                 请严格按以下 JSON 格式输出（只返回 JSON，不要其他内容）：\n\
                 ```json\n\
                 {\n\
                   \"analysis\": \"对需求含义的深入分析\",\n\
                   \"next_steps\": [\"步骤1\", \"步骤2\", ...],\n\
                   \"missing_info\": [\"缺少的信息1\", ...],\n\
                   \"expected_output\": \"预期最终输出描述\"\n\
                 }\n\
                 ```"
            ),
            ChatMessage::user(
                &format!(
                    "## 用户问题\n{}{}\n\n请进行分析并规划下一步。",
                    user_content, context_block
                )
            ),
        ];

        let response = self.complete_text(&messages, 0.4, 0.9).await?;
        let extracted = extract_json_from_text(&response)
            .unwrap_or_else(|| serde_json::json!({
                "analysis": response,
                "next_steps": [],
                "missing_info": [],
                "expected_output": ""
            }));

        Ok(ThinkResult {
            analysis: extracted.get("analysis")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            next_steps: extracted.get("next_steps")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            missing_info: extracted.get("missing_info")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            expected_output: extracted.get("expected_output")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
    }

    /// 保存本次 run 的 contexts 到持久存储（含用户问题和最终回答）
    pub async fn save_run_contexts(&self, rt: &AgentRuntime) {
        let mut contexts_data: Vec<serde_json::Value> = Vec::new();
        // 始终保存用户问题
        if !rt.user_content.is_empty() {
            contexts_data.push(serde_json::json!({
                "context_id": "user_question",
                "phase_id": rt.phase_id,
                "step_id": "user",
                "step_name": "user_question",
                "kind": "user",
                "tags": ["$question"],
                "content": rt.user_content,
                "metadata": {},
            }));
        }
        // 保存各步骤产生的 contexts
        for ctx in &rt.contexts {
            contexts_data.push(serde_json::json!({
                "context_id": ctx.context_id,
                "phase_id": ctx.phase_id,
                "step_id": ctx.step_id,
                "step_name": ctx.step_name,
                "kind": ctx.kind.to_string(),
                "tags": ctx.tags,
                "content": ctx.content,
                "metadata": ctx.metadata,
            }));
        }
        // 始终保存最终回答
        if !rt.assistant_text.is_empty() {
            contexts_data.push(serde_json::json!({
                "context_id": "assistant_answer",
                "phase_id": rt.phase_id,
                "step_id": "answer",
                "step_name": "assistant_answer",
                "kind": "assistant",
                "tags": ["$answer"],
                "content": rt.assistant_text,
                "metadata": {},
            }));
        }
        if contexts_data.is_empty() {
            return;
        }
        if let Err(e) = self.store.save_contexts(&rt.subject, &contexts_data).await {
            warn!("保存 contexts 失败: {}", e);
        }
    }

    /// 匹配用户问题到技能（agentlite parity: 恒走 LLM 判断，
    /// 注入 $mindset knowledge 与 recent_questions，返回含 program/reason）
    async fn match_skill_to_question(
        &self,
        rt: &AgentRuntime,
        user_content: &str,
        recent_questions: &[String],
    ) -> Option<serde_json::Value> {
        let registry = self.skill_registry.read().await;
        let enabled: Vec<_> = registry.list_enabled();
        if enabled.is_empty() {
            return None;
        }
        // 构建 skills 列表用于 prompt
        let skill_list: Vec<serde_json::Value> = enabled.iter().map(|s| {
            serde_json::json!({"id": s.id, "name": s.name, "description": s.description})
        }).collect();
        let skills_text = serde_json::to_string_pretty(&skill_list).unwrap_or_default();
        let questions_text = if recent_questions.is_empty() {
            "无历史问题".to_string()
        } else {
            recent_questions.iter().map(|q| format!("- {}", q)).collect::<Vec<_>>().join("\n")
        };
        // 查询 builtin knowledge 获取 skill 匹配相关的知识作为上下文
        // 要求: 必须包含 $mindset，且至少包含 $match_skill 或 skill匹配 之一
        let mut knowledge_context = String::new();
        if let Some(ref store) = rt.knowledge_store {
            let rows = store.query_by_tags_sync(
                &["$mindset".to_string()],
                &["$match_skill".to_string(), "skill匹配".to_string()],
            );
            if !rows.is_empty() {
                let parts: Vec<String> = rows.into_iter().take(3).map(|d| {
                    let preview = if d.content.len() > 500 {
                        format!("{}...", &d.content[..500])
                    } else {
                        d.content.clone()
                    };
                    format!("### [{}] {}\n{}", d.id, d.title, preview)
                }).collect();
                knowledge_context = format!("\n\n相关知识库参考：\n{}", parts.join("\n\n"));
            }
        }
        let decision_system =
            "下面给出所有可用的builtin skills（包含名称和描述），以及用户最近的提问。\
             根据用户的问题，判断用户目标得到的东西，是一个答案、一份报告、还是一些数据文件、分析报告？\
             然后匹配一个skill的处理能力和产出物。\
             如果是希望得到一个答案，尽量返回检索和回答相关的skill。\
             如果匹配，返回匹配skill的信息；如果不匹配任何skill，返回null。\
             输出必须且只能是严格有效的JSON格式：\
             {\"matched\": true/false, \"skill_id\": \"匹配的skill_id或null\", \"reason\": \"匹配或不匹配的原因\"}";
        let decision_user = format!(
            "可用skills：\n{}\n\n用户最近的提问（按时间顺序）：\n{}\n\n当前问题：{}\n\n{}看看调用哪个技能合适？",
            skills_text, questions_text, user_content, knowledge_context
        );
        match self.complete_text(
            &[ChatMessage::system(decision_system), ChatMessage::user(&decision_user)],
            0.0,
            0.9,
        ).await {
            Ok(response) => {
                let value = extract_json_from_text(&response);
                match value {
                    Some(ref v) if v.get("matched").and_then(|m| m.as_bool()).unwrap_or(false) => {
                        if let Some(skill_id) = v.get("skill_id").and_then(|id| id.as_str()) {
                            if let Some(skill) = registry.get(skill_id) {
                                return Some(serde_json::json!({
                                    "id": skill.id,
                                    "name": skill.name,
                                    "description": skill.description,
                                    "content": skill.content,
                                    "program": skill.program,
                                    "generated_json": skill.generated_json,
                                    "reason": v.get("reason").and_then(|r| r.as_str()).unwrap_or(""),
                                }));
                            }
                        }
                        None
                    }
                    _ => None,
                }
            }
            Err(e) => {
                warn!("Skill matching LLM call failed: {}", e);
                None
            }
        }
    }

    // ── Public skill API ──

    /// Register a skill into the agent's skill registry
    pub async fn register_skill(&self, skill: crate::skill_registry::SkillDef) {
        self.skill_registry.write().await.register(skill);
    }

    /// Unregister a user skill. Returns error for builtins.
    pub async fn remove_skill(&self, id: &str) -> Result<(), String> {
        self.skill_registry.write().await.unregister(id)
    }

    /// Get a skill by ID
    pub async fn get_skill(&self, id: &str) -> Option<crate::skill_registry::SkillDef> {
        self.skill_registry.read().await.get(id).cloned()
    }

    /// List all skills sorted by updated_at desc
    pub async fn list_skills(&self) -> Vec<crate::skill_registry::SkillDef> {
        self.skill_registry.read().await.list_all().into_iter().cloned().collect()
    }

    /// Search skills by keyword in name + description
    pub async fn search_skills(&self, query: &str) -> Vec<crate::skill_registry::SkillDef> {
        self.skill_registry.read().await.search(query).into_iter().cloned().collect()
    }

    /// Toggle a skill's enabled state
    pub async fn set_skill_enabled(&self, id: &str, enabled: bool) -> Option<bool> {
        self.skill_registry.write().await.set_enabled(id, enabled)
    }

    /// Save user skills to a JSON file (builtins excluded)
    pub async fn save_user_skills(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.skill_registry.read().await.save_user_skills(path)
    }

    /// Load user skills from a JSON file
    pub async fn load_user_skills(&self, path: impl AsRef<Path>) -> anyhow::Result<usize> {
        self.skill_registry.write().await.load_user_skills(path)
    }

    fn estimate_tokens(text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        std::cmp::max(1, text.chars().count() * 2 / 5)
    }

    fn estimate_message_tokens(msg: &ChatMessage) -> usize {
        4 + Self::estimate_tokens(&msg.content)
    }

    /// Truncate oldest non-system messages so total estimated tokens
    /// fit within (max_seq_len - output_budget). System messages are always preserved.
    fn truncate_messages(
        messages: &[ChatMessage],
        max_seq_len: usize,
        output_budget: usize,
    ) -> Vec<ChatMessage> {
        let budget = max_seq_len.saturating_sub(output_budget);
        let total: usize = messages.iter().map(Self::estimate_message_tokens).sum();

        if total <= budget {
            return messages.to_vec();
        }

        let system_tokens: usize = messages
            .iter()
            .filter(|m| m.role == "system")
            .map(Self::estimate_message_tokens)
            .sum();

        let available = budget.saturating_sub(system_tokens);

        let non_system: Vec<&ChatMessage> = messages
            .iter()
            .filter(|m| m.role != "system")
            .collect();
        let mut kept: Vec<&ChatMessage> = Vec::new();
        let mut kept_tokens = 0usize;

        for msg in non_system.iter().rev() {
            let msg_tokens = Self::estimate_message_tokens(msg);
            if kept_tokens + msg_tokens <= available {
                kept.push(msg);
                kept_tokens += msg_tokens;
            } else {
                break;
            }
        }
        kept.reverse();

        let dropped = non_system.len() - kept.len();
        if dropped > 0 {
            warn!(
                "Truncated {} messages ({} -> {} estimated tokens) to fit within max_seq_len={}",
                dropped,
                total,
                system_tokens + kept_tokens,
                max_seq_len
            );
        }

        let mut result: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| m.role == "system")
            .cloned()
            .collect();
        result.extend(kept.into_iter().cloned());
        result
    }

    // -- placeholder for step methods and run_steps, continued below --
}

// Step execution methods are implemented as inherent methods on AgentLoop
// in a separate impl block below to keep the file within size limits.
// See the continuation in the second half of this file.

// ============================================================================
// Step execution impl
// ============================================================================

impl AgentLoop {
    fn init_steps(rt: &mut AgentRuntime) {
        if !rt.steps.is_empty() {
            return;
        }
        let mut generated_steps = Vec::new();
        // 如果有附件，先 push 文件快速读取步骤
        if !rt.attachments.is_empty() {
            for att in &rt.attachments {
                if !att.file_path.is_empty() {
                    let mut step = StepState::new(
                        new_chat_step_id(),
                        StepKind::FastFileRead,
                        format!("读取附件: {}", att.file_name)
                    );
                    step.runtime.insert(
                        "step_name".to_string(),
                        serde_json::json!(format!("读取文件: {}", att.file_name))
                    );
                    step.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
                    step.runtime.insert(
                        "user_content".to_string(),
                        serde_json::json!(rt.user_content)
                    );
                    step.runtime.insert("file_path".to_string(), serde_json::json!(att.file_path));
                    step.runtime.insert("file_name".to_string(), serde_json::json!(att.file_name));
                    step.runtime.insert("file_type".to_string(), serde_json::json!(att.file_type));
                    step.runtime.insert(
                        "output_target".to_string(),
                        serde_json::json!({
                        "output_type": "context",
                        "name": format!("文件内容: {}", att.file_name),
                        "tags": ["file_content", att.file_type]
                    })
                    );
                    generated_steps.push(step);
                }
            }
        }
        let mut step = StepState::new(
            new_chat_step_id(),
            StepKind::Question,
            rt.user_content.clone()
        );
        step.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
        step.runtime.insert("user_content".to_string(), serde_json::json!(rt.user_content));
        generated_steps.push(step);
        // 逆序压入栈中（后进先出）
        for s in generated_steps.into_iter().rev() {
            rt.push_step(s);
        }
    }

    fn parse_step_kind(s: &str) -> StepKind {
        match s {
            "question" => StepKind::Question,
            "answer" => StepKind::Answer,
            "find_more_context" => StepKind::FindMoreContext,
            "fast_file_read" => StepKind::FastFileRead,
            "file_process" => StepKind::FileProcess,
            "generate_command" => StepKind::GenerateCommand,
            "call_llm" => StepKind::CallLlm,
            "check_validation" => StepKind::CheckValidation,
            "bash_command" => StepKind::BashCommand,
            "content_assemble" => StepKind::ContentAssemble,
            "follow_guide" => StepKind::FollowGuide,
            "request_user_input" => StepKind::RequestUserInput,
            "task" => StepKind::Task,
            "think_and_generate" => StepKind::ThinkAndGenerate,
            "block" => StepKind::Block,
            "loop_block" => StepKind::LoopBlock,
            "enter_block" => StepKind::EnterBlock,
            "exit_block" => StepKind::ExitBlock,
            _ => StepKind::CallLlm,
        }
    }

    fn evaluate_condition(rt: &AgentRuntime, expr: &str, iteration: i64) -> bool {
        let env = Environment::new();
        let stripped = expr.trim();
        let inner = if stripped.starts_with("{{") && stripped.ends_with("}}") {
            stripped[2..stripped.len() - 2].trim()
        } else {
            stripped
        };
        let template = format!("{{% if {} %}}true{{% endif %}}", inner);
        let mut ctx = rt.env.to_dict();
        ctx.insert("loop_index".to_string(), serde_json::json!(iteration - 1));
        ctx.insert("loop_iteration".to_string(), serde_json::json!(iteration));
        match env.render_str(&template, &minijinja::Value::from_serialize(&ctx)) {
            Ok(r) => r == "true",
            Err(e) => {
                warn!("条件求值失败: {}", e);
                false
            }
        }
    }

    fn build_child_step(
        _rt: &AgentRuntime,
        step_def_value: &serde_json::Value,
        loop_index: i64,
        block_id: &str
    ) -> StepState {
        let step_type = step_def_value
            .get("step_type")
            .and_then(|v| v.as_str())
            .unwrap_or("call_llm");
        let step_goal = step_def_value
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let kind = Self::parse_step_kind(step_type);
        let mut params = HashMap::new();
        let mut runtime = HashMap::new();
        for key in &[
            "step_name",
            "step_category",
            "llm_prompt",
            "output_target",
            "validation",
            "validation_failure_handling",
            "context_query",
            "subject",
            "bash_command",
            "condition",
            "max_iterations",
            "children",
        ] {
            if let Some(v) = step_def_value.get(*key) {
                params.insert(key.to_string(), v.clone());
            }
        }
        if let Some(obj) = step_def_value.as_object() {
            let skip = [
                "step_name",
                "step_type",
                "children",
                "llm_prompt",
                "output_target",
                "validation",
                "validation_failure_handling",
                "context_query",
                "action",
            ];
            for (k, v) in obj {
                if !skip.contains(&k.as_str()) {
                    params.insert(k.clone(), v.clone());
                }
            }
        }
        if loop_index >= 0 {
            runtime.insert("_loop_iteration".to_string(), serde_json::json!(loop_index));
        }
        StepState {
            step_id: new_chat_step_id(),
            step_goal,
            kind,
            block_id: block_id.to_string(),
            block_ancestors: Vec::new(),
            step_def: StepDef::from_map(params),
            runtime,
            children: Vec::new(),
            status: StepStatus::Pending,
            attempt: 0,
            started_at: None,
            ended_at: None,
        }
    }

    fn make_enter_block(
        parent: &StepState,
        child_count: usize,
        loop_iter: i64,
        block_id: &str
    ) -> StepState {
        let name = parent.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("未知块")
            .to_string();
        let mut p = HashMap::new();
        p.insert("_block_name".to_string(), serde_json::json!(name));
        p.insert("_block_parent_id".to_string(), serde_json::json!(parent.step_id));
        p.insert("_block_id".to_string(), serde_json::json!(block_id));
        p.insert("_block_child_count".to_string(), serde_json::json!(child_count));
        p.insert("_block_loop_iteration".to_string(), serde_json::json!(loop_iter));
        p.insert("_block_start_time".to_string(), serde_json::json!(Utc::now().to_rfc3339()));
        StepState {
            step_id: new_chat_step_id(),
            step_goal: format!("进入块: {}", name),
            kind: StepKind::EnterBlock,
            block_id: block_id.to_string(),
            block_ancestors: Vec::new(),
            step_def: StepDef::new(),
            runtime: p,
            children: Vec::new(),
            status: StepStatus::Pending,
            attempt: 0,
            started_at: None,
            ended_at: None,
        }
    }

    fn make_exit_block(
        parent: &StepState,
        child_count: usize,
        loop_iter: i64,
        block_id: &str
    ) -> StepState {
        let name = parent.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("未知块")
            .to_string();
        let mut p = HashMap::new();
        p.insert("_block_name".to_string(), serde_json::json!(name));
        p.insert("_block_parent_id".to_string(), serde_json::json!(parent.step_id));
        p.insert("_block_id".to_string(), serde_json::json!(block_id));
        p.insert("_block_child_count".to_string(), serde_json::json!(child_count));
        p.insert("_block_loop_iteration".to_string(), serde_json::json!(loop_iter));
        StepState {
            step_id: new_chat_step_id(),
            step_goal: format!("退出块: {}", name),
            kind: StepKind::ExitBlock,
            block_id: block_id.to_string(),
            block_ancestors: Vec::new(),
            step_def: StepDef::new(),
            runtime: p,
            children: Vec::new(),
            status: StepStatus::Pending,
            attempt: 0,
            started_at: None,
            ended_at: None,
        }
    }

    async fn execute_question_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        llm_messages: &mut Vec<ChatMessage>,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("问题处理");
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在分析问题: {}...", step_name),
            Some(0),
            event_tx
        ).await;
        let user_content = st.step_def
            .get("user_content")
            .and_then(|v| v.as_str())
            .unwrap_or(&st.step_goal)
            .trim()
            .to_string();
        if !user_content.is_empty() {
            llm_messages.push(ChatMessage::user(&user_content));
        }
        // 获取最近的用户问题用于 skill 匹配
        let all_questions: Vec<String> = llm_messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.clone())
            .collect();
        let recent_questions: Vec<String> = all_questions.into_iter().rev().take(5).rev().collect();
        // 尝试匹配 builtin skills
        let matched_skill = self.match_skill_to_question(rt, &user_content, &recent_questions).await;
        if let Some(ref skill) = matched_skill {
            let skill_name = skill
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            emit_step_status(
                &st.step_id,
                "info",
                &format!("匹配到技能: {}", skill_name),
                Some(0),
                event_tx
            ).await;
        }
        // agentlite parity: 匹配到 skill → answer + follow_guide；
        // 未匹配到 skill → push think_and_generate 动态生成处理步骤（不再默认 push answer）
        if let Some(skill) = matched_skill {
            // Push answer step
            let mut answer = StepState::new(
                new_chat_step_id(),
                StepKind::Answer,
                user_content.clone()
            );
            answer.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
            answer.runtime.insert("user_content".to_string(), serde_json::json!(user_content));
            answer.runtime.insert("matched_skill".to_string(), skill.clone());
            answer.runtime.insert("context_query".to_string(), serde_json::json!(["$all"]));
            rt.push_step(answer);
            // 插入 follow_guide step（在 answer 之前执行）
            let skill_name = skill
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut follow_guide_step = StepState::new(
                new_chat_step_id(),
                StepKind::FollowGuide,
                format!("Follow guide for skill: {}", skill_name)
            );
            follow_guide_step.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
            follow_guide_step.runtime.insert(
                "user_content".to_string(),
                serde_json::json!(user_content)
            );
            follow_guide_step.runtime.insert("matched_skill".to_string(), skill);
            rt.push_step(follow_guide_step);
            info!("Push follow_guide step for skill: {}", skill_name);
        } else {
            // 未匹配到技能，动态生成处理步骤（对齐 agentlite _execute_question_step else 分支）
            emit_step_status(
                &st.step_id,
                "info",
                "未匹配到技能，动态生成处理步骤",
                Some(0),
                event_tx
            ).await;
            let mut tg_step = StepState::new(
                new_chat_step_id(),
                StepKind::ThinkAndGenerate,
                user_content.clone()
            );
            tg_step.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
            tg_step.runtime.insert("user_content".to_string(), serde_json::json!(user_content));
            tg_step.runtime.insert(
                "step_name".to_string(),
                serde_json::json!("分析问题并生成处理步骤")
            );
            // 与 execute_think_and_generate_step 统一从 step_def 读取 skill_content/skill_name
            tg_step.step_def.insert(
                "skill_content".to_string(),
                serde_json::json!(
                    format!(
                        "用户提出的问题是：「{}」。\n没有匹配到内置技能，请根据问题类型动态生成合适的处理步骤，最终需要包含一个 answer 步骤来生成对用户的回复。",
                        user_content
                    )
                )
            );
            tg_step.step_def.insert("skill_name".to_string(), serde_json::json!(""));
            rt.push_step(tg_step);
            info!("No skill matched, pushing think_and_generate step");
        }
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        emit_step_status(&st.step_id, "success", "问题处理完成", Some(100), event_tx).await;
        Ok(String::new())
    }

    /// 按步骤的 context_query 注入上下文到消息列表（支持 @llm: 查询）。
    /// 对齐 agentlite _build_llm_context 的步骤级上下文注入逻辑。
    async fn inject_step_context(
        &self,
        rt: &AgentRuntime,
        st: &StepState,
        messages: &mut Vec<ChatMessage>
    ) {
        let cq_owned = st.step_def.get_owned("context_query");
        let cq: Vec<String> = st.runtime
            .get("context_query")
            .or(cq_owned.as_ref())
            .and_then(|v| v.as_array())
            .map(|arr|
                arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            )
            .unwrap_or_default();
        if cq.is_empty() {
            return;
        }
        let has_llm_query = cq.iter().any(|q| q.starts_with("@llm:"));
        let matched = if has_llm_query {
            let step_name_v = st.step_def.get_owned("step_name");
            let step_name = step_name_v
                .as_ref()
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown");
            self.execute_context_query_async(rt, &cq, step_name).await
        } else {
            rt.execute_context_query(&cq)
        };
        if matched.is_empty() {
            return;
        }
        let ctx_str = matched
            .iter()
            .map(|v|
                v
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| serde_json::to_string(v).unwrap_or_default())
            )
            .collect::<Vec<_>>()
            .join("\n\n");
        if ctx_str.is_empty() {
            return;
        }
        if let Some(first) = messages.iter_mut().find(|m| m.role == "system") {
            if !first.content.contains("## 上下文信息") {
                first.content.push_str("\n\n## 上下文信息\n");
                first.content.push_str(&ctx_str);
            }
        } else {
            messages.insert(0, ChatMessage::system(&ctx_str));
        }
    }

    async fn execute_answer_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        _llm_messages: &[ChatMessage],
        temperature: f32,
        top_p: f32,
        event_tx: Option<mpsc::Sender<ChatEvent>>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("生成回答");
        if let Some(ref tx) = event_tx {
            emit_step_status(
                &st.step_id,
                "progress",
                &format!("正在{}...", step_name),
                Some(0),
                tx
            ).await;
        }
        // agentlite parity: answer 不再拼接最近 10 条会话历史；
        // 使用固定模板 system+user（或自定义 llm_prompt）+ 上下文注入
        // 验证失败解释步骤：使用专门的失败解释 prompt（agentlite _execute_answer_step）
        let is_validation_failure = st.runtime
            .get("validation_failure")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut final_msgs: Vec<ChatMessage> = if is_validation_failure {
            let failure_user = st.runtime
                .get("user_content")
                .and_then(|v| v.as_str())
                .unwrap_or(&rt.user_content)
                .to_string();
            vec![
                ChatMessage::system(
                    "你是一个友好的助手。前面的流程因为验证失败而终止。你需要向用户解释清楚：\n\
                     1. 哪个步骤失败了\n\
                     2. 失败的具体原因\n\
                     3. 用户需要提供什么信息或做什么来继续\n\n\
                     请用友好、清晰的语气解释，并给出具体的操作建议。"
                ),
                ChatMessage::user(&failure_user),
            ]
        } else if let Some(lp) = st.step_def
            .get_owned("llm_prompt")
            .as_ref()
            .or_else(|| st.runtime.get("llm_prompt"))
        {
            // 使用 Skill Guide 中定义的 prompt 模板（agentlite parity）
            let sp = lp
                .get("system")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let up = lp
                .get("user")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut msgs = vec![ChatMessage::system(sp), ChatMessage::user(up)];
            self.inject_step_context(rt, st, &mut msgs).await;
            msgs
        } else {
            // 默认：根据上下文信息回答用户问题（固定模板，不含会话历史）
            let mut msgs = vec![
                ChatMessage::system("你是一个助手，请根据上下文信息回答用户的问题。"),
                ChatMessage::user("请根据上下文中的信息，回答用户的问题。"),
            ];
            self.inject_step_context(rt, st, &mut msgs).await;
            msgs
        };
        // agentlite _build_llm_context: 将用户原始问题追加到最后一条 user 消息
        // （验证失败分支不经过 _build_llm_context，不追加）
        if !is_validation_failure && !rt.user_content.trim().is_empty() {
            if let Some(last_user) = final_msgs.iter_mut().rev().find(|m| m.role == "user") {
                let uc = rt.user_content.trim();
                if !last_user.content.contains(uc) {
                    last_user.content.push_str("\n\n## 用户原始问题\n");
                    last_user.content.push_str(uc);
                }
            }
        }
        let assistant_text = if event_tx.is_some() {
            let (token_tx, mut token_rx) = mpsc::channel::<String>(100);
            let msgs = final_msgs.clone();
            let self_c = self.clone();
            tokio::spawn(async move {
                let _ = self_c.stream_text(&msgs, temperature, top_p, token_tx).await;
            });
            let mut full = String::new();
            while let Some(token) = token_rx.recv().await {
                full.push_str(&token);
                if let Some(ref tx) = event_tx {
                    emit_event(ChatEvent::Token { text: token.clone() }, tx).await;
                }
            }
            full
        } else {
            self.complete_text(&final_msgs, temperature, top_p).await?
        };
        rt.assistant_text = assistant_text.clone();
        // 优先从 artifact 构建附件列表，降级到 [[path]] 正则提取
        let mut attachments = self.build_attachments_from_artifacts(rt);
        if attachments.is_empty() {
            let conversation_id = if rt.subject.starts_with("conv_") {
                &rt.subject[5..]
            } else {
                "default"
            };
            let path_attachments = extract_path_attachments(
                &assistant_text,
                conversation_id,
                &self.chat_files_dir,
                None
            );
            attachments = path_attachments
                .iter()
                .map(|att| att.to_dict())
                .collect();
        }
        // 如果有附件，发送附件事件
        if !attachments.is_empty() && !rt.cancelled {
            if let Some(ref tx) = event_tx {
                emit_event(ChatEvent::Attachments { attachments: attachments.clone() }, tx).await;
            }
        }
        // Compute elapsed and completed_at for both persistence and SSE event
        let (elapsed, completed_at) = st.started_at.map(|start| {
            let dur = Utc::now() - start;
            let secs = dur.num_milliseconds() as f64 / 1000.0;
            let elapsed_display = format_duration(secs);
            let completed = Utc::now().to_rfc3339();
            ((secs, elapsed_display), completed)
        }).unwrap_or_else(|| ((0.0, "0.0s".to_string()), Utc::now().to_rfc3339()));

        if !rt.cancelled {
            let attachments_for_persist = if !attachments.is_empty() {
                Some(attachments.clone())
            } else {
                None
            };
            let mut meta = HashMap::new();
            meta.insert("elapsed_seconds".into(), serde_json::Value::Number(
                serde_json::Number::from_f64((elapsed.0 * 10.0).round() / 10.0).unwrap_or(serde_json::Number::from(0))
            ));
            meta.insert("elapsed_display".into(), serde_json::Value::String(elapsed.1.clone()));
            meta.insert("completed_at".into(), serde_json::Value::String(completed_at.clone()));
            self.persist_assistant_message(
                &rt.subject,
                &assistant_text,
                attachments_for_persist,
                Some(meta),
            ).await?;
            if *self.history_loaded.read().await {
                let mut h = self.history.write().await;
                h.push(ChatMessage::assistant(&assistant_text));
            }
        }
        self.save_run_contexts(rt).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        if let Some(tx) = event_tx {
            emit_event(ChatEvent::Done {
                content: assistant_text.clone(),
                elapsed_seconds: elapsed.0,
                elapsed_display: elapsed.1,
                completed_at,
            }, &tx).await;
        }
        Ok(assistant_text)
    }

    async fn execute_bash_command_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("bash命令");
        let cmd = st.step_def
            .get("cmd")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'cmd' parameter"))?;
        info!("Executing bash command: {}", cmd);
        let rendered = Environment::new()
            .render_str(cmd, self.build_cmd_context(rt))
            .unwrap_or_else(|_| cmd.to_string());
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在执行命令: {}", &rendered[..rendered.len().min(50)]),
            Some(0),
            event_tx
        ).await;
        let conv_id = if rt.subject.starts_with("conv_") { &rt.subject[5..] } else { "default" };
        let work_dir = self.chat_files_dir.join(conv_id);
        std::fs::create_dir_all(&work_dir).ok();
        let executor = crate::executor::Executor::new().with_root_dir(work_dir);
        let mut ap = HashMap::new();
        ap.insert("cmd".to_string(), serde_json::json!(rendered));
        let action = crate::types::ActionSpec {
            kind: crate::types::ActionKind::Bash,
            params: ap,

        };
        let result = executor.execute(action).await?;
        let st_type = if result.ok { "success" } else { "error" };
        emit_step_status(
            &st.step_id,
            st_type,
            &format!(
                "命令执行{} (exit code: {})",
                if result.ok {
                    "成功"
                } else {
                    "失败"
                },
                result.exit_code.unwrap_or(-1)
            ),
            Some(100),
            event_tx
        ).await;
        // 处理输出
        let output = if let Some(ref s) = result.stdout {
            if !s.is_empty() {
                s.clone()
            } else if let Some(ref e) = result.stderr {
                if !e.is_empty() { e.clone() } else { String::new() }
            } else {
                String::new()
            }
        } else if let Some(ref e) = result.stderr {
            if !e.is_empty() { e.clone() } else { String::new() }
        } else {
            String::new()
        };
        // 如果有文件变更信息，添加到 context
        if let Some(ref data) = result.data {
            if let Some(file_changes) = data.get("file_changes") {
                let created = file_changes
                    .get("created")
                    .and_then(|v| v.as_array())
                    .map(|a|
                        a
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    )
                    .unwrap_or_default();
                let modified = file_changes
                    .get("modified")
                    .and_then(|v| v.as_array())
                    .map(|a|
                        a
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    )
                    .unwrap_or_default();
                let deleted = file_changes
                    .get("deleted")
                    .and_then(|v| v.as_array())
                    .map(|a|
                        a
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<_>>()
                    )
                    .unwrap_or_default();
                if !created.is_empty() || !modified.is_empty() || !deleted.is_empty() {
                    let mut change_parts = Vec::new();
                    if !created.is_empty() {
                        change_parts.push(
                            format!("新建文件 ({}): {}", created.len(), created.join(", "))
                        );
                    }
                    if !modified.is_empty() {
                        change_parts.push(
                            format!("修改文件 ({}): {}", modified.len(), modified.join(", "))
                        );
                    }
                    if !deleted.is_empty() {
                        change_parts.push(
                            format!("删除文件 ({}): {}", deleted.len(), deleted.join(", "))
                        );
                    }
                    let change_summary = format!(
                        "命令执行的文件系统变更：\n{}",
                        change_parts.join("\n")
                    );
                    let phase_id = rt.phase_id.clone();
                    rt.add_context(
                        &phase_id,
                        &st.step_id,
                        step_name,
                        serde_json::json!(change_summary),
                        vec!["file_changes".to_string(), step_name.to_string()]
                    );
                }
            }
        }
        // 如果有 output_target 参数，根据配置保存结果
        if let Some(output_target) = st.step_def.get_owned("output_target").as_ref().and_then(|v| v.as_object()) {
            let output_type = output_target
                .get("output_type")
                .and_then(|v| v.as_str())
                .unwrap_or("context");
            let name = output_target
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(step_name);
            let tags: Vec<String> = output_target
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr|
                    arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                )
                .unwrap_or_else(|| vec![step_name.to_string()]);
            if output_type == "file" {
                let conversation_id = if rt.subject.starts_with("conv_") {
                    &rt.subject[5..]
                } else {
                    "default"
                };
                match
                    save_to_file(
                        conversation_id,
                        &st.step_id,
                        &output,
                        name,
                        "txt",
                        &self.chat_files_dir
                    )
                {
                    Ok(file_path) => {
                        let phase_id = rt.phase_id.clone();
                        rt.add_artifact(&phase_id, &st.step_id, name, "file", &file_path, {
                            let mut m = HashMap::new();
                            m.insert("name".to_string(), serde_json::json!(name));
                            m.insert("tags".to_string(), serde_json::json!(&tags));
                            m
                        });
                        rt.add_context(
                            &phase_id,
                            &st.step_id,
                            step_name,
                            serde_json::Value::String(
                                format!(
                                    "文件已保存至 {file_path}，文件名为 {name}，包含步骤 '{step_name}' 的完整输出内容"
                                )
                            ),
                            tags.clone()
                        );
                    }
                    Err(e) => {
                        error!("Failed to save bash command output to file: {}", e);
                    }
                }
            }
            if output_type == "context" {
                let phase_id = rt.phase_id.clone();
                rt.add_context(
                    &phase_id,
                    &st.step_id,
                    name,
                    serde_json::json!(format_step_result_for_context(&output)),
                    tags
                );
            }
        } else {
            let phase_id = rt.phase_id.clone();
            rt.add_context(
                &phase_id,
                &st.step_id,
                step_name,
                serde_json::json!(format_step_result_for_context(&output)),
                vec![step_name.to_string()]
            );
        }
        Ok(output)
    }

    async fn execute_call_llm_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        llm_messages: &[ChatMessage],
        temperature: f32,
        top_p: f32,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("LLM处理");
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在处理: {}...", step_name),
            Some(0),
            event_tx
        ).await;
        // agentlite parity (_execute_call_llm_step): 有自定义 llm_prompt 时用
        // [system, user] + 注入上下文；否则直接用 llm_messages（已含历史与上下文注入）
        let llm_call_messages = if let Some(lp) = st.step_def
            .get_owned("llm_prompt")
            .as_ref()
            .or_else(|| st.runtime.get("llm_prompt"))
        {
            let sp = lp
                .get("system")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let up = lp
                .get("user")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut msgs = vec![ChatMessage::system(sp), ChatMessage::user(up)];
            self.inject_step_context(rt, st, &mut msgs).await;
            msgs
        } else {
            llm_messages.to_vec()
        };
        // 温度/采样参数使用 run 传入值（agentlite parity，不再硬编码 1.0）
        let result = self.complete_text(&llm_call_messages, temperature, top_p).await?;
        emit_step_status(&st.step_id, "success", "处理完成", Some(100), event_tx).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(result)
    }

    async fn execute_find_more_context_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        emit_step_status(
            &st.step_id,
            "progress",
            "正在查找上下文...",
            Some(0),
            event_tx
        ).await;
        let terms: Vec<String> = st.step_def
            .get("search_terms")
            .and_then(|v| v.as_array())
            .map(|a|
                a
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            )
            .unwrap_or_default();
        let filter_tags: Vec<String> = st.step_def
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|a|
                a.iter().filter_map(|v| v.as_str().map(String::from)).collect()
            )
            .unwrap_or_default();
        let max_count: usize = st.step_def
            .get("max_count")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(50);
        let mut chunks = Vec::new();
        let historical_ctxs = self.store.load_all_contexts(&rt.subject).await;
        let lines: Vec<String> = historical_ctxs.iter()
            .rev()
            .filter_map(|ctx| {
                let content = ctx.get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if content.is_empty() {
                    return None;
                }
                if !terms.is_empty() &&
                   !terms.iter().any(|t| content.to_lowercase().contains(&t.to_lowercase())) {
                    return None;
                }
                if !filter_tags.is_empty() {
                    let ctx_tags: Vec<&str> = ctx.get("tags")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                        .unwrap_or_default();
                    if !filter_tags.iter().any(|t| ctx_tags.contains(&t.as_str())) {
                        return None;
                    }
                }
                let step_name = ctx.get("step_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let kind = ctx.get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let snippet = if content.len() > 400 { &content[..400] } else { content };
                Some(format!("- [{}::{}] {}", kind, step_name, snippet))
            })
            .take(max_count)
            .collect();
        if !lines.is_empty() {
            chunks.push(format!("historical_contexts:\n{}", lines.join("\n")));
        }
        let result = if chunks.is_empty() {
            "未找到相关上下文".to_string()
        } else {
            let c = chunks.join("\n\n");
            let pid = rt.phase_id.clone();
            rt.add_context(
                &pid,
                &st.step_id,
                "find_more_context",
                serde_json::json!(&c),
                vec!["context".to_string()]
            );
            c
        };
        emit_step_status(&st.step_id, "success", "上下文查找完成", Some(100), event_tx).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(result)
    }

    async fn execute_content_assemble_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        emit_step_status(&st.step_id, "progress", "正在组装内容...", Some(0), event_tx).await;
        let parts: Vec<String> = rt.contexts
            .iter()
            .filter_map(|c| c.content.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        let result = parts.join("\n\n");
        emit_step_status(&st.step_id, "success", "内容组装完成", Some(100), event_tx).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(result)
    }

    async fn execute_enter_block_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let name = st.step_def
            .get("_block_name")
            .and_then(|v| v.as_str())
            .unwrap_or("未知块");
        let iter = st.step_def
            .get("_block_loop_iteration")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let cc = st.step_def
            .get("_block_child_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let msg = if iter > 0 {
            format!("进入块: {} (第{}轮, {}个子步骤)", name, iter, cc)
        } else {
            format!("进入块: {} ({}个子步骤)", name, cc)
        };
        let pid = rt.phase_id.clone();
        rt.add_context(
            &pid,
            &st.step_id,
            &format!("enter:{}", name),
            serde_json::json!({"event":"enter_block","name":name}),
            vec![format!("block:{}", name)]
        );
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        emit_step_status(&st.step_id, "success", &msg, None, event_tx).await;
        Ok(String::new())
    }

    async fn execute_exit_block_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let name = st.step_def
            .get("_block_name")
            .and_then(|v| v.as_str())
            .unwrap_or("未知块");
        let bt = format!("block:{}", name);

        // 收集该 block 相关的 context 数量
        let collected = rt.contexts
            .iter()
            .filter(|c| c.tags.contains(&bt))
            .count();

        // 删除该 block 的所有 context（enter_block、子步骤产生的等）
        let before = rt.contexts.len();
        rt.contexts.retain(|c| !c.tags.contains(&bt));
        let removed = before - rt.contexts.len();

        let msg = format!("退出块: {} (收集{}个上下文, 清理{}个)", name, collected, removed);
        let pid = rt.phase_id.clone();
        rt.add_context(
            &pid,
            &st.step_id,
            &format!("exit:{}", name),
            serde_json::json!({"event":"exit_block","name":name,"context_collected":collected}),
            vec![bt, "_block_lifecycle".to_string()]
        );
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        emit_step_status(&st.step_id, "success", &msg, None, event_tx).await;
        Ok(String::new())
    }

    async fn execute_block_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let children = st.step_def
            .get_owned("children")
            .as_ref()
            .and_then(|v| v.as_array())
            .cloned();
        if children.as_ref().map_or(true, |c| c.is_empty()) {
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        let children = children.unwrap();
        let cc = children.len();
        let bid = format!("block_{}", &Uuid::new_v4().to_string()[..8]);
        rt.push_step(Self::make_exit_block(st, cc, 0, &bid));
        for cd in children.iter().rev() {
            rt.push_step(Self::build_child_step(rt, cd, -1, &bid));
        }
        rt.push_step(Self::make_enter_block(st, cc, 0, &bid));
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        emit_step_status(&st.step_id, "success", "步骤块展开完成", None, event_tx).await;
        Ok(String::new())
    }

    /// 解析 loop_block 的 items 参数（对齐 agentlite _resolve_loop_items）
    /// 支持：数组、@llm: 指令解析、JSON 数组字符串、其他作为单个元素
    async fn resolve_loop_items(
        &self,
        rt: &AgentRuntime,
        items_param: &serde_json::Value
    ) -> Vec<serde_json::Value> {
        match items_param {
            serde_json::Value::Array(arr) => arr.clone(),
            serde_json::Value::String(s) => {
                let s = s.trim();
                if s.is_empty() {
                    return Vec::new();
                }
                if let Some(rest) = s.strip_prefix("@llm:") {
                    // 调用 LLM 从上下文解析 items 列表
                    let instruction = rest.trim();
                    info!("loop_block: resolving items via @llm: {}", instruction);
                    let summaries: Vec<String> = rt.contexts
                        .iter()
                        .map(|ctx| {
                            let preview = ctx.content.to_string();
                            let preview = if preview.len() > 500 {
                                format!("{}...", &preview[..500])
                            } else {
                                preview
                            };
                            format!(
                                "[{}] 步骤: {} | 标签: {}\n  内容: {}",
                                ctx.context_id,
                                ctx.step_name,
                                ctx.tags
                                    .iter()
                                    .map(|t| t.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                preview
                            )
                        })
                        .collect();
                    let prompt = format!(
                        "你是一个数据提取助手。根据用户指令，从以下上下文信息中提取需要处理的列表数据。\n\n## 可用的上下文信息\n{}\n\n## 用户指令\n{}\n\n## 要求\n1. 从上下文中提取所有符合条件的项，组成列表\n2. 如果没有找到任何有效项，返回空数组\n\n## 输出格式\n返回 JSON（不要其他内容）：\n{{\"items\": [ ... ]}}",
                        summaries.join("\n\n"),
                        instruction
                    );
                    return match self.complete_text(&[ChatMessage::user(&prompt)], 0.2, 0.9).await
                    {
                        Ok(response) => {
                            if let Some(v) = extract_json_from_text(&response) {
                                if let Some(arr) = v.get("items").and_then(|x| x.as_array()) {
                                    info!("loop_block: @llm resolved {} items", arr.len());
                                    return arr.clone();
                                }
                            }
                            Vec::new()
                        }
                        Err(e) => {
                            warn!("loop_block: failed to resolve items via @llm: {}", e);
                            Vec::new()
                        }
                    };
                }
                if s.starts_with('[') {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
                        if let Some(arr) = v.as_array() {
                            return arr.clone();
                        }
                    }
                    // JSON 解析失败则作为单个元素
                }
                vec![serde_json::json!(s.to_string())]
            }
            _ => Vec::new(),
        }
    }

    async fn execute_loop_block_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name = st.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("循环块")
            .to_string();
        let cond = st.step_def
            .get("exit_condition")
            .and_then(|v| v.as_str())
            .or_else(|| st.step_def.get("condition").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();
        let mut max_iter = st.step_def
            .get_owned("max_iterations")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .min(100) as usize;
        let cur = st.step_def
            .get("_loop_current_iteration")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let children = st.step_def
            .get_owned("children")
            .as_ref()
            .and_then(|v| v.as_array())
            .cloned();
        if children.as_ref().map_or(true, |c| c.is_empty()) {
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        let children = children.unwrap();
        // ── items 模式（for-each）：首次进入时解析 items 并缓存到 runtime ──
        let items_param = st.step_def
            .get("items")
            .cloned()
            .or_else(||
                st.step_def
                    .params
                    .as_ref()
                    .and_then(|p| p.items.as_ref())
                    .map(|s| serde_json::json!(s))
            );
        let skip_if_none = st.step_def
            .get("skip_if_none")
            .and_then(|v| v.as_bool())
            .or_else(|| st.step_def.params.as_ref().map(|p| p.skip_if_none))
            .unwrap_or(false);
        let mut resolved_items: Option<Vec<serde_json::Value>> = st.runtime
            .get("_resolved_items")
            .and_then(|v| v.as_array())
            .map(|a| a.clone());
        if items_param.is_some() && resolved_items.is_none() {
            let resolved = self.resolve_loop_items(rt, items_param.as_ref().unwrap()).await;
            st.runtime.insert("_resolved_items".to_string(), serde_json::json!(resolved.clone()));
            info!("loop_block '{}' resolved {} items", step_name, resolved.len());
            // skip_if_none: items 为空时跳过整个循环
            if resolved.is_empty() && skip_if_none {
                info!("loop_block '{}' skipped: no items and skip_if_none is True", step_name);
                emit_step_status(
                    &st.step_id,
                    "info",
                    &format!("循环块 {} 跳过：无待处理项", step_name),
                    None,
                    event_tx
                ).await;
                st.mark_completed();
                rt.mark_step_completed(&st.step_id);
                return Ok(String::new());
            }
            resolved_items = Some(resolved);
        }
        // items 模式下 max_iterations 由 items 数量决定
        if let Some(ref items) = resolved_items {
            if !items.is_empty() {
                max_iter = items.len();
            }
        }
        let items_mode = resolved_items.is_some();
        // 条件循环模式：检查退出条件（items 模式不使用 exit_condition）
        if !items_mode && !cond.is_empty() && Self::evaluate_condition(rt, &cond, cur + 1) {
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        if (cur as usize) >= max_iter {
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        let next = cur + 1;
        let current_item = resolved_items
            .as_ref()
            .and_then(|items| items.get(cur as usize))
            .cloned();
        let cc = children.len();
        let bid = format!("block_{}_iter{}", &Uuid::new_v4().to_string()[..8], next);
        // 注入 loop env 变量（保持原 env.set 持久行为，兼容 build_cmd_context/evaluate_condition）
        rt.env.set("loop_index", serde_json::json!(cur));
        rt.env.set("loop_iteration", serde_json::json!(next));
        if let Some(ref item) = current_item {
            rt.env.set("loop_item", item.clone());
        }
        // items 模式：将当前 item 存入 context，供子步骤引用（agentlite parity）
        if let Some(ref item) = current_item {
            let item_content = if item.is_object() || item.is_array() {
                serde_json::to_string(item).unwrap_or_else(|_| item.to_string())
            } else {
                item
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| item.to_string())
            };
            let phase_id = rt.phase_id.clone();
            rt.add_context(
                &phase_id,
                &st.step_id,
                &format!("loop_item:{}", step_name),
                serde_json::json!(item_content),
                vec![
                    "loop_item".to_string(),
                    format!("loop_item:{}", step_name),
                    format!("iter:{}", next),
                ],
            );
        }
        // 将 loop_block 自身压入队列底部（所有子步骤执行完后再次触发），
        // 保留 runtime（_resolved_items 等）——agentlite parity
        let mut lp = st.step_def.clone();
        lp.insert("_loop_current_iteration".to_string(), serde_json::json!(next));
        rt.push_step(StepState {
            step_id: st.step_id.clone(),
            step_goal: st.step_goal.clone(),
            kind: StepKind::LoopBlock,
            block_id: bid.clone(),
            block_ancestors: Vec::new(),
            step_def: lp,
            runtime: st.runtime.clone(),
            children: Vec::new(),
            status: StepStatus::Pending,
            attempt: 0,
            started_at: None,
            ended_at: None,
        });
        rt.push_step(Self::make_exit_block(st, cc, next, &bid));
        for cd in children.iter().rev() {
            let mut child = Self::build_child_step(rt, cd, cur, &bid);
            // items 模式下注入 loop_item / loop_item_str 到子步骤 runtime（agentlite parity）
            if let Some(ref item) = current_item {
                child.runtime.insert("loop_item".to_string(), item.clone());
                let item_str = if item.is_object() || item.is_array() {
                    serde_json::to_string(item).unwrap_or_else(|_| item.to_string())
                } else {
                    item
                        .as_str()
                        .map(String::from)
                        .unwrap_or_else(|| item.to_string())
                };
                child.runtime.insert("loop_item_str".to_string(), serde_json::json!(item_str));
            }
            rt.push_step(child);
        }
        rt.push_step(Self::make_enter_block(st, cc, next, &bid));
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(String::new())
    }

    async fn execute_request_user_input_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let prompt = st.step_def
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("请输入:");
        emit_event(
            ChatEvent::StepStatus {
                status: StepStatusInfo {
                    id: format!("status_{}", st.step_id),
                    status_type: "info".to_string(),
                    content: format!("请求用户输入: {}", prompt),
                    progress: None,
                    visible: None,
                    auto_hide: None,
                },
            },
            event_tx
        ).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(String::new())
    }

    async fn execute_task_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let goal = st.step_def
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or(&st.step_goal);
        let result = format!("任务完成: {}", goal);
        emit_step_status(&st.step_id, "success", &result, Some(100), event_tx).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(result)
    }

    async fn execute_check_validation_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        llm_messages: &[ChatMessage],
        _temperature: f32,
        _top_p: f32,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name = st.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("验证检查")
            .to_string();
        let validation_rule = st.step_def
            .get_owned("validation")
            .as_ref()
            .or_else(|| st.runtime.get("validation"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let failure_handling = st.step_def
            .get_owned("validation_failure_handling")
            .as_ref()
            .or_else(|| st.runtime.get("validation_failure_handling"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在验证: {}...", step_name),
            Some(0),
            event_tx
        ).await;
        // agentlite parity (_execute_check_validation_step): 先做检查调用，
        // 有自定义 llm_prompt 时用 [system, user] + 注入上下文，否则直接用 llm_messages
        let check_result = if let Some(lp) = st.step_def
            .get_owned("llm_prompt")
            .as_ref()
            .or_else(|| st.runtime.get("llm_prompt"))
        {
            let sp = lp
                .get("system")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let up = lp
                .get("user")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let mut msgs = vec![ChatMessage::system(sp), ChatMessage::user(up)];
            self.inject_step_context(rt, st, &mut msgs).await;
            self.complete_text(&msgs, 0.2, 0.9).await?
        } else {
            self.complete_text(llm_messages, 0.2, 0.9).await?
        };
        // agentlite parity (_validate_step_result): 验证 prompt 注入验证规则/失败策略/步骤结果，
        // JSON 键使用 valid
        let result_preview = if check_result.len() > 2000 {
            format!("{}...", &check_result[..2000])
        } else {
            check_result.clone()
        };
        let validation_messages = vec![
            ChatMessage::system(
                "你是一个结果验证助手。根据提供的验证规则和步骤结果，判断结果是否符合要求。\n\n\
                 请输出严格的 JSON 格式：\n\
                 {\n\
                   \"valid\": true/false,\n\
                   \"reason\": \"验证通过或不通过的具体原因\",\n\
                   \"suggested_action\": \"continue或abort\"\n\
                 }\n\n\
                 说明：\n\
                 - valid: 结果是否有效\n\
                 - reason: 详细说明为什么有效或无效\n\
                 - suggested_action: 如果验证失败，建议的处理策略\n\
                   - \"continue\": 可以忽略此错误继续执行（如部分数据缺失但可后续补充）\n\
                   - \"abort\": 必须终止流程（如关键数据缺失无法继续）"
            ),
            ChatMessage::user(
                &format!(
                    "## 步骤名称\n{}\n\n## 验证规则\n{}\n\n## 验证失败处理策略\n{}\n\n## 步骤结果\n{}\n\n请根据验证规则判断步骤结果是否有效，并说明原因和建议的处理策略。",
                    step_name,
                    validation_rule,
                    if failure_handling.is_empty() {
                        "未指定，请根据验证规则自行判断".to_string()
                    } else {
                        failure_handling.clone()
                    },
                    result_preview
                )
            ),
        ];
        let result = self.complete_text(&validation_messages, 0.2, 0.9).await?;
        // 解析验证结果（JSON 键 valid）
        let (is_valid, reason, suggested_action) = Self::parse_validation_result(
            &result,
            &validation_rule,
            &step_name
        );
        if is_valid {
            info!("Step '{}' validation passed: {}", step_name, reason);
            emit_step_status(
                &st.step_id,
                "success",
                &format!("验证通过: {}", reason),
                Some(100),
                event_tx
            ).await;
            let formatted_result = format_step_result_for_context(&result);
            let phase_id = rt.phase_id.clone();
            rt.add_context(
                &phase_id,
                &st.step_id,
                &step_name,
                serde_json::json!(formatted_result),
                vec![step_name.clone()]
            );
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
        } else {
            warn!(
                "Step '{}' validation failed: {} (suggested_action: {})",
                step_name,
                reason,
                suggested_action
            );
            if suggested_action == "continue" || failure_handling == "ignore" {
                info!("Step '{}': validation failed but continuing (ignorable)", step_name);
                emit_step_status(
                    &st.step_id,
                    "warning",
                    &format!("验证未通过但继续: {}", reason),
                    None,
                    event_tx
                ).await;
                let formatted_result = format_step_result_for_context(&result);
                let phase_id = rt.phase_id.clone();
                rt.add_context(
                    &phase_id,
                    &st.step_id,
                    &step_name,
                    serde_json::json!(formatted_result),
                    vec![step_name.clone()]
                );
                st.mark_completed();
                rt.mark_step_completed(&st.step_id);
            } else {
                error!("Step '{}': validation failed, aborting flow", step_name);
                emit_step_status(
                    &st.step_id,
                    "error",
                    &format!("验证失败，流程终止: {}", reason),
                    None,
                    event_tx
                ).await;
                st.mark_failed();
                rt.mark_step_completed(&st.step_id);
                rt.steps.clear();
                // 推送 answer step 解释失败原因
                let failure_msg = format!(
                    "步骤 '{}' 验证失败。\n原因：{}\n\n{}",
                    step_name,
                    reason,
                    if !failure_handling.is_empty() {
                        failure_handling.clone()
                    } else {
                        "请检查输入信息后重试。".to_string()
                    }
                );
                let mut answer_step = StepState::new(
                    new_chat_step_id(),
                    StepKind::Answer,
                    "解释验证失败原因并说明需要什么".to_string()
                );
                answer_step.runtime.insert("subject".to_string(), serde_json::json!(&rt.subject));
                answer_step.runtime.insert(
                    "user_content".to_string(),
                    serde_json::json!(&failure_msg)
                );
                answer_step.runtime.insert(
                    "validation_failure".to_string(),
                    serde_json::json!(true)
                );
                answer_step.runtime.insert("failed_step".to_string(), serde_json::json!(step_name));
                answer_step.runtime.insert("failure_reason".to_string(), serde_json::json!(reason));
                rt.push_step(answer_step);
            }
        }
        Ok(result)
    }

    async fn execute_generate_command_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name = st.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("生成并执行命令")
            .to_string();
        let max_retries = st.step_def
            .get_owned("max_retries")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize;
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在生成命令: {}...", step_name),
            Some(0),
            event_tx
        ).await;
        let mut last_error: Option<String> = None;
        for attempt in 1..=max_retries {
            if attempt > 1 {
                info!("Retry attempt {}/{} for step '{}'", attempt, max_retries, step_name);
                emit_step_status(
                    &st.step_id,
                    "progress",
                    &format!(
                        "命令执行失败，正在重新生成命令 (尝试 {}/{})...",
                        attempt,
                        max_retries
                    ),
                    Some(20),
                    event_tx
                ).await;
            }
            // 使用自定义 prompt 或默认
            let (mut system_prompt, user_prompt) = if
                let Some(llm_prompt) = st.step_def.get_owned("llm_prompt")
            {
                let sys = llm_prompt
                    .get("system")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let usr = llm_prompt
                    .get("user")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                (sys, usr)
            } else {
                (
                    "你是一个 bash 命令生成专家。根据用户的任务目标和上下文，生成合适的 bash 命令。\n\n## 要求\n1. 只输出命令本身，不要包含其他解释\n2. 命令应该简洁明了，避免危险操作\n3. 如果需要多个命令，用 && 连接或分行列出\n4. 不要使用 ```bash 代码块标记，直接输出纯命令文本".to_string(),
                    "请根据上下文信息生成 bash 命令，只输出命令本身，不要其他内容。".to_string(),
                )
            };
            // 如果是重试，在 prompt 中添加失败信息
            if attempt > 1 {
                if let Some(ref err) = last_error {
                    system_prompt.push_str(
                        &format!("\n\n## 重要提示\n之前生成的命令执行失败了，错误信息：{}\n请分析错误原因，生成一个新的、正确的命令。\n确保命令语法正确，并且所有依赖都已安装。", err)
                    );
                }
            }
            let msgs = vec![ChatMessage::system(&system_prompt), ChatMessage::user(&user_prompt)];
            let cmd_template = self.complete_text(&msgs, 1.0, 1.0).await?;
            // 清理命令中的 markdown 标记
            let mut cmd = cmd_template.trim().to_string();
            if
                let Some(m) = regex::Regex
                    ::new(r"```(?:bash)?\s*\n?(.*?)\n?```")
                    .ok()
                    .and_then(|r| r.captures(&cmd))
            {
                cmd = m
                    .get(1)
                    .map(|m| m.as_str().trim().to_string())
                    .unwrap_or(cmd);
            }
            if cmd.is_empty() {
                warn!("Step '{}' generated empty command (attempt {})", step_name, attempt);
                last_error = Some("生成的命令为空".to_string());
                continue;
            }
            info!("Generated command template (attempt {}): {}", attempt, cmd);
            let rendered = Environment::new()
                .render_str(&cmd, self.build_cmd_context(rt))
                .unwrap_or_else(|_| cmd.clone());
            emit_step_status(
                &st.step_id,
                "progress",
                &format!("正在执行: {}", &rendered[..rendered.len().min(50)]),
                Some(60),
                event_tx
            ).await;
            let conv_id = if rt.subject.starts_with("conv_") {
                &rt.subject[5..]
            } else {
                "default"
            };
            let work_dir = self.chat_files_dir.join(conv_id);
            std::fs::create_dir_all(&work_dir).ok();
            let executor = crate::executor::Executor::new().with_root_dir(work_dir);
            let mut ap = HashMap::new();
            ap.insert("cmd".to_string(), serde_json::json!(rendered));
            let action = crate::types::ActionSpec {
                kind: crate::types::ActionKind::Bash,
                params: ap,
    
            };
            let result = executor.execute(action).await?;
            if result.ok {
                let output = result.stdout.clone().unwrap_or_default();
                let success_msg = if attempt > 1 {
                    format!("命令执行完成 (重试 {} 次)", attempt - 1)
                } else {
                    "命令执行完成".to_string()
                };
                emit_step_status(
                    &st.step_id,
                    "success",
                    &success_msg,
                    Some(100),
                    event_tx
                ).await;
                st.mark_completed();
                rt.mark_step_completed(&st.step_id);
                return Ok(output);
            } else {
                let error_msg = result.stderr.clone().unwrap_or_default();
                last_error = Some(error_msg.clone());
                warn!("Generated command failed (attempt {}): {}", attempt, error_msg);
                if attempt >= max_retries {
                    error!("Step '{}' failed after {} attempts", step_name, max_retries);
                    emit_step_status(
                        &st.step_id,
                        "error",
                        &format!("命令执行失败，已重试 {} 次: {}", max_retries, error_msg),
                        None,
                        event_tx
                    ).await;
                    st.mark_completed();
                    rt.mark_step_completed(&st.step_id);
                    return Err(
                        anyhow::anyhow!(
                            "Command failed after {} attempts: {}",
                            max_retries,
                            error_msg
                        )
                    );
                }
            }
        }
        error!("Step '{}' exited retry loop without result", step_name);
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(String::new())
    }

    async fn execute_follow_guide_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        _lm: &[ChatMessage],
        _t: f32,
        _tp: f32,
        event_tx: Option<mpsc::Sender<ChatEvent>>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("加载技能指导");
        if let Some(ref tx) = event_tx {
            emit_step_status(
                &st.step_id,
                "progress",
                &format!("正在加载技能: {}...", step_name),
                Some(0),
                tx
            ).await;
        }
        let matched_skill = st.runtime.get("matched_skill");
        if matched_skill.is_none() {
            warn!("follow_guide step '{}' 没有 matched_skill", step_name);
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        let matched_skill = matched_skill.unwrap();
        let skill_content = matched_skill
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let skill_name = matched_skill
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let generated_json = matched_skill.get("generated_json");
        let mut all_steps_plan: Vec<serde_json::Value> = Vec::new();
        if let Some(gj) = generated_json {
            if let Some(steps_plan) = gj.get("steps_plan").and_then(|v| v.as_array()) {
                if !steps_plan.is_empty() {
                    info!("使用预生成的步骤计划: {}, {} 个步骤", skill_name, steps_plan.len());
                    all_steps_plan = steps_plan.clone();
                }
            }
        }
        if all_steps_plan.is_empty() {
            info!("无预生成步骤计划，插入 think_and_generate step: {}", skill_name);
            let mut tg_step = StepState::new(
                new_chat_step_id(),
                StepKind::ThinkAndGenerate,
                format!("根据 skill '{}' 和上下文生成执行步骤", skill_name)
            );
            tg_step.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
            tg_step.runtime.insert(
                "user_content".to_string(),
                st.runtime.get("user_content").cloned().unwrap_or(serde_json::json!(""))
            );
            tg_step.runtime.insert(
                "step_name".to_string(),
                serde_json::json!(format!("生成 {} 执行步骤", skill_name))
            );
            tg_step.runtime.insert("skill_name".to_string(), serde_json::json!(skill_name));
            tg_step.runtime.insert("skill_content".to_string(), serde_json::json!(skill_content));
            // 与 execute_think_and_generate_step 统一从 step_def 读取（agentlite StepDef.params 传递）
            tg_step.step_def.insert("skill_content".to_string(), serde_json::json!(skill_content));
            tg_step.step_def.insert("skill_name".to_string(), serde_json::json!(skill_name));
            rt.push_step(tg_step);
        }
        if !all_steps_plan.is_empty() {
            info!("Total steps plan: {} steps", all_steps_plan.len());
            if let Some(last_step) = all_steps_plan.last() {
                let last_step_type = last_step
                    .get("step_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call_llm");
                if last_step_type == "answer" {
                    if let Some(pos) = rt.steps.iter().position(|s| s.kind == StepKind::Answer) {
                        rt.steps.remove(pos);
                        info!("Removed default answer step (skill guide already includes answer)");
                    }
                }
            }
            for step_def in all_steps_plan.iter().rev() {
                let step_type = step_def
                    .get("step_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("call_llm");
                let sn = step_def
                    .get("step_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("未知步骤");
                let action = step_def
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut s = StepState::new(
                    new_chat_step_id(),
                    Self::parse_step_kind(step_type),
                    action
                );
                s.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
                s.runtime.insert("user_content".to_string(), serde_json::json!(""));
                s.runtime.insert("step_name".to_string(), serde_json::json!(sn));
                s.runtime.insert("step_category".to_string(), serde_json::json!(step_type));
                if let Some(params) = step_def.get("params") {
                    s.runtime.insert("params".to_string(), params.clone());
                }
                if let Some(lp) = step_def.get("llm_prompt").or(step_def.get("system")) {
                    s.runtime.insert("llm_prompt".to_string(), lp.clone());
                }
                if let Some(ot) = step_def.get("output_target") {
                    s.runtime.insert("output_target".to_string(), ot.clone());
                }
                if let Some(v) = step_def.get("validation") {
                    s.runtime.insert("validation".to_string(), v.clone());
                }
                if let Some(vfh) = step_def.get("validation_failure_handling") {
                    s.runtime.insert("validation_failure_handling".to_string(), vfh.clone());
                }
                // agentlite parity: 生成的步骤保留 context_query
                if let Some(cq) = step_def.get("context_query") {
                    s.runtime.insert("context_query".to_string(), cq.clone());
                }
                rt.push_step(s);
            }
        }
        info!("Applied skill guide: {}, inserted {} steps", skill_name, all_steps_plan.len());
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        if let Some(tx) = event_tx {
            emit_step_status(
                &st.step_id,
                "success",
                "技能指导加载完成",
                Some(100),
                &tx
            ).await;
        }
        Ok(String::new())
    }

    async fn execute_think_and_generate_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("思考并生成步骤");
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在生成步骤计划: {}...", step_name),
            Some(0),
            event_tx
        ).await;
        // skill_content / user_content 与 follow_guide 统一从 step_def 读取（兼容 runtime 旧写入）
        let skill_content = st.step_def
            .get("skill_content")
            .and_then(|v| v.as_str())
            .or_else(|| st.runtime.get("skill_content").and_then(|v| v.as_str()))
            .unwrap_or(&st.step_goal)
            .to_string();
        let user_content = st.step_def
            .get("user_content")
            .and_then(|v| v.as_str())
            .or_else(|| st.runtime.get("user_content").and_then(|v| v.as_str()))
            .unwrap_or(&rt.user_content)
            .to_string();
        // 收集上下文 context 作为参考资料
        let mut context_texts: Vec<String> = Vec::new();
        for ctx in &rt.contexts {
            if ctx.tags.iter().any(|t| t == "context" || t.starts_with("block:") ) {
                let text = ctx.content.as_str().unwrap_or("");
                if !text.is_empty() {
                    context_texts.push(
                        format!("- [{}] {}", ctx.kind, &text[..text.len().min(500)])
                    );
                }
            }
        }
        let context_text = if context_texts.is_empty() {
            "无额外上下文".to_string()
        } else {
            context_texts.join("\n")
        };
        let plan_messages = vec![
            ChatMessage::system(
                "你是一个任务规划助手。根据提供的上下文资料和当前任务信息，生成需要执行的步骤计划。\n\n## 步骤类型说明\n- `call_llm`: 调用LLM处理 - 通用步骤类型\n- `bash_command`: 执行命令\n- `answer`: 调用LLM并生成最终回答\n- `block`: 顺序执行块\n- `loop_block`: 循环执行块\n\n## 每个步骤需要包含\n- step_name: 步骤名称\n- step_type: 步骤类型\n- action: 执行动作详细描述\n- llm_prompt: 如需要调用LLM，提供 {system, user} prompt模板\n- output_target: 输出目标配置\n- context_query: 查询已保存的context\n- validation: 如何检查结果有效性\n\n只返回 JSON 数组，不要其他内容"
            ),
            ChatMessage::user(
                &format!(
                    "# 当前任务\n目标: {}\n\n# 参考资料\n{}\n\n# 已有上下文\n{}\n\n# 用户问题\n{}\n\n请根据上述上下文和参考资料，生成执行步骤计划，返回 JSON 数组格式。",
                    st.step_goal,
                    skill_content,
                    context_text,
                    user_content
                )
            )
        ];
        let resp = match self.complete_text(&plan_messages, 0.3, 0.9).await {
            Ok(r) => r,
            Err(e) => {
                warn!("think_and_generate LLM调用失败: {}", e);
                emit_step_status(
                    &st.step_id,
                    "error",
                    &format!("生成步骤失败: {}", e),
                    None,
                    event_tx
                ).await;
                st.mark_completed();
                rt.mark_step_completed(&st.step_id);
                return Ok(String::new());
            }
        };
        // 解析 LLM 返回的步骤计划
        let mut steps_plan: Vec<serde_json::Value> = Vec::new();
        if let Some(extracted) = extract_json_from_text(&resp) {
            if let Some(arr) = extracted.as_array() {
                steps_plan = arr.clone();
            } else if let Some(obj) = extracted.as_object() {
                for key in &["steps", "plan", "steps_plan", "data"] {
                    if let Some(arr) = obj.get(*key).and_then(|v| v.as_array()) {
                        steps_plan = arr.clone();
                        break;
                    }
                }
                if steps_plan.is_empty() {
                    steps_plan.push(extracted);
                }
            }
        }
        if steps_plan.is_empty() {
            warn!("think_and_generate 未生成有效步骤");
            emit_step_status(&st.step_id, "info", "未生成新步骤", None, event_tx).await;
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(String::new());
        }
        info!("think_and_generate 生成了 {} 个步骤", steps_plan.len());
        for step_def in steps_plan.iter().rev() {
            let step_type = step_def
                .get("step_type")
                .and_then(|v| v.as_str())
                .unwrap_or("call_llm");
            let sn = step_def
                .get("step_name")
                .and_then(|v| v.as_str())
                .unwrap_or("未知步骤");
            let action = step_def
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut s = StepState::new(
                new_chat_step_id(),
                Self::parse_step_kind(step_type),
                action
            );
            s.runtime.insert("subject".to_string(), serde_json::json!(rt.subject));
            s.runtime.insert("user_content".to_string(), serde_json::json!(""));
            s.runtime.insert("step_name".to_string(), serde_json::json!(sn));
            s.runtime.insert("step_category".to_string(), serde_json::json!(step_type));
            if let Some(lp) = step_def.get("llm_prompt").or(step_def.get("system")) {
                s.runtime.insert("llm_prompt".to_string(), lp.clone());
            }
            if let Some(ot) = step_def.get("output_target") {
                s.runtime.insert("output_target".to_string(), ot.clone());
            }
            if let Some(v) = step_def.get("validation") {
                s.runtime.insert("validation".to_string(), v.clone());
            }
            // agentlite parity: 生成的步骤保留 validation_failure_handling 与 context_query
            if let Some(vfh) = step_def.get("validation_failure_handling") {
                s.runtime.insert("validation_failure_handling".to_string(), vfh.clone());
            }
            if let Some(cq) = step_def.get("context_query") {
                s.runtime.insert("context_query".to_string(), cq.clone());
            }
            rt.push_step(s);
        }
        emit_step_status(
            &st.step_id,
            "success",
            &format!("生成了 {} 个新步骤", steps_plan.len()),
            Some(100),
            event_tx
        ).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(String::new())
    }

    /// 解析验证结果（agentlite parity: JSON 键为 valid）
    fn parse_validation_result(
        validation_response: &str,
        _validation_rule: &str,
        _step_name: &str
    ) -> (bool, String, String) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(validation_response) {
            if let Some(obj) = json.as_object() {
                let is_valid = obj
                    .get("valid")
                    .and_then(|v| v.as_bool())
                    .or_else(|| obj.get("is_valid").and_then(|v| v.as_bool()))
                    .unwrap_or(false);
                let reason = obj
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or(if is_valid { "验证通过" } else { "验证失败" })
                    .to_string();
                let mut suggested_action = obj
                    .get("suggested_action")
                    .and_then(|v| v.as_str())
                    .unwrap_or(if is_valid { "continue" } else { "abort" })
                    .to_string();
                if suggested_action != "continue" && suggested_action != "abort" {
                    suggested_action = (if is_valid { "continue" } else { "abort" }).to_string();
                }
                return (is_valid, reason, suggested_action);
            }
        }
        warn!("Failed to parse validation result, raw response: {}", validation_response);
        (false, "无法解析验证结果".to_string(), "abort".to_string())
    }

    /// 从 LLM 响应中解析 JSON commands
    fn parse_llm_json_response(&self, response: &str) -> Option<Vec<String>> {
        let json_str = response.trim();
        let json_str = if json_str.starts_with("```") {
            let lines = json_str.lines();
            let mut in_code = false;
            let mut json_lines = Vec::new();
            for line in lines {
                if line.starts_with("```") {
                    in_code = !in_code;
                    continue;
                }
                if in_code {
                    json_lines.push(line);
                }
            }
            json_lines.join("\n")
        } else {
            json_str.to_string()
        };
        match serde_json::from_str::<serde_json::Value>(&json_str) {
            Ok(plan) => {
                if let Some(cmds) = plan.get("commands") {
                    if let Some(arr) = cmds.as_array() {
                        return Some(
                            arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        );
                    }
                }
                None
            }
            Err(e) => {
                warn!(
                    "parse_llm_json_response failed: {}, response: {}",
                    e,
                    &json_str[..json_str.len().min(200)]
                );
                None
            }
        }
    }

    /// 猜测文件 MIME 类型
    fn guess_mime_type(&self, path: &Path) -> String {
        guess_mime_type_from_path(path)
    }

    /// 执行 fast_file_read 步骤
    async fn execute_fast_file_read_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let file_path = st.step_def
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'file_path' parameter"))?;
        let file_name = st.step_def
            .get("file_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let path = Path::new(file_path);
        let file_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        info!("Fast file read: {} (ext: {})", file_path, file_ext);
        emit_step_status(
            &st.step_id,
            "progress",
            &format!("正在读取文件: {}", file_name),
            Some(0),
            event_tx
        ).await;
        // agent-doc 支持的格式
        let agent_doc_exts = [
            "docx",
            "doc",
            "dotx",
            "dot",
            "dotm",
            "docm",
            "xlsx",
            "xls",
            "xlsm",
            "xlsb",
            "xltx",
            "xlt",
            "pptx",
            "ppt",
            "pptm",
            "potx",
            "pot",
            "ppsx",
            "pps",
            "odt",
            "ods",
            "odp",
            "odg",
            "wps",
            "et",
            "dps",
            "pdf",
            "epub",
            "mobi",
            "azw3",
            "fb2",
            "cbz",
            "cbr",
            "chm",
            "html",
            "htm",
            "mhtml",
            "xml",
            "csv",
            "tsv",
            "rtf",
            "png",
            "jpg",
            "jpeg",
            "gif",
            "bmp",
            "tiff",
            "tif",
            "webp",
            "xps",
            "djvu",
            "pages",
            "numbers",
            "key",
        ];
        let content = if agent_doc_exts.contains(&file_ext.as_str()) {
            emit_step_status(
                &st.step_id,
                "info",
                "正在提取文档内容...",
                Some(30),
                event_tx
            ).await;
            let output_path = std::env
                ::temp_dir()
                .join(format!("agent_doc_{}.txt", Uuid::new_v4()));
            let output = std::process::Command
                ::new("agent-doc")
                .arg("extract")
                .arg(file_path)
                .arg("-f")
                .arg("txt")
                .arg("-o")
                .arg(&output_path)
                .output();
            match output {
                Ok(out) => {
                    let c = if out.status.success() && output_path.exists() {
                        std::fs::read_to_string(&output_path).unwrap_or_default()
                    } else {
                        format!("[agent-doc 提取失败: {}]", String::from_utf8_lossy(&out.stderr))
                    };
                    let _ = std::fs::remove_file(&output_path);
                    c
                }
                Err(e) => format!("[agent-doc 执行失败: {}]", e),
            }
        } else {
            std::fs::read_to_string(file_path).unwrap_or_else(|e| format!("[读取失败: {}]", e))
        };
        // 文件过大时截断
        let max_chars = 3_000;
        let content = if content.len() > max_chars {
            format!("{}...", &content[..max_chars])
        } else {
            content
        };
        let file_size = std::fs
            ::metadata(file_path)
            .map(|m| m.len())
            .unwrap_or(0);
        emit_step_status(
            &st.step_id,
            "info",
            &format!("文件已读取（{} 字节）", file_size),
            None,
            event_tx
        ).await;
        // 如果提供了 llm_prompt，调用 LLM 进一步处理
        let result = if let Some(llm_prompt) = st.step_def.get_owned("llm_prompt") {
            let system = llm_prompt
                .get("system")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let user = llm_prompt
                .get("user")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let messages = vec![
                ChatMessage::system(system),
                ChatMessage::user(&format!("## 文件内容：\n{}\n\n## 任务：\n{}", content, user))
            ];
            self.complete_text(&messages, 1.0, 1.0).await?
        } else {
            content
        };
        // 根据 output_target 决定输出
        let step_name_v = st.step_def.get_owned("step_name");
        let step_name = step_name_v
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("快速文件读取");
        if let Some(output_target) = st.step_def.get_owned("output_target").as_ref().and_then(|v| v.as_object()) {
            let output_type = output_target
                .get("output_type")
                .and_then(|v| v.as_str())
                .unwrap_or("context");
            let name = output_target
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(step_name);
            let tags: Vec<String> = output_target
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr|
                    arr
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                )
                .unwrap_or_else(|| vec![step_name.to_string()]);
            if output_type == "file" {
                let conversation_id = if rt.subject.starts_with("conv_") {
                    &rt.subject[5..]
                } else {
                    "default"
                };
                match
                    save_to_file(
                        conversation_id,
                        &st.step_id,
                        &result,
                        name,
                        "txt",
                        &self.chat_files_dir
                    )
                {
                    Ok(file_path) => {
                        let phase_id = rt.phase_id.clone();
                        rt.add_artifact(&phase_id, &st.step_id, name, "file", &file_path, {
                            let mut m = HashMap::new();
                            m.insert("name".to_string(), serde_json::json!(name));
                            m.insert("tags".to_string(), serde_json::json!(&tags));
                            m
                        });
                        rt.add_context(
                            &phase_id,
                            &st.step_id,
                            step_name,
                            serde_json::Value::String(
                                format!("文件已保存至 {file_path}，文件名为 {name}")
                            ),
                            tags.clone()
                        );
                    }
                    Err(e) => {
                        error!("Failed to save fast file read output: {}", e);
                    }
                }
            }
            if output_type == "context" {
                let phase_id = rt.phase_id.clone();
                rt.add_context(
                    &phase_id,
                    &st.step_id,
                    name,
                    serde_json::json!(format_step_result_for_context(&result)),
                    tags
                );
            }
        } else {
            let phase_id = rt.phase_id.clone();
            rt.add_context(
                &phase_id,
                &st.step_id,
                step_name,
                serde_json::json!(format_step_result_for_context(&result)),
                vec![step_name.to_string()]
            );
        }
        emit_step_status(&st.step_id, "success", "文件读取完成", Some(100), event_tx).await;
        st.mark_completed();
        rt.mark_step_completed(&st.step_id);
        Ok(result)
    }

    /// 执行视觉识别
    async fn execute_vision_recognition(
        &self,
        file_path: &str,
        _event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        let path = Path::new(file_path);
        let image_bytes = std::fs::read(path)?;
        let image_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &image_bytes
        );
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();
        let mime_type = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            "webp" => "image/webp",
            _ => "image/png",
        };
        let data_url = format!("data:{};base64,{}", mime_type, image_b64);
        let messages = vec![
            ChatMessage::user_with_image("请仔细识别图片中的所有文字内容，保持原有格式", &data_url)
        ];
        let (token_tx, mut token_rx) = mpsc::channel::<String>(100);
        let self_clone = self.clone();
        let messages_clone = messages.clone();
        tokio::spawn(async move {
            let _ = self_clone.stream_text(&messages_clone, 0.3, 0.9, token_tx).await;
        });
        let mut full_text = String::new();
        while let Some(token) = token_rx.recv().await {
            full_text.push_str(&token);
        }
        if self.llm_config.debug {
            let truncated = if full_text.len() > 2000 {
                format!("{}...", &full_text[..2000])
            } else {
                full_text.clone()
            };
            debug!("视觉识别结果: {}", truncated);
        }
        Ok(full_text)
    }

    /// PDF 转图片
    async fn pdf_to_images(
        &self,
        pdf_path: &str,
        _event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<Vec<String>> {
        let output_dir = std::env::temp_dir().join(format!("pdf_images_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&output_dir)?;
        let executor = crate::executor::Executor::new().with_root_dir(self.chat_files_dir.clone());
        let mut ap = HashMap::new();
        ap.insert(
            "cmd".to_string(),
            serde_json::json!(
                format!(
                    "agent-image pdf2img -i '{}' --output-dir '{}'",
                    pdf_path,
                    output_dir.display()
                )
            )
        );
        let action = crate::types::ActionSpec {
            kind: crate::types::ActionKind::Bash,
            params: ap,

        };
        let result = executor.execute(action).await?;
        if !result.ok {
            return Err(
                anyhow::anyhow!(
                    "Failed to convert PDF to images: {}",
                    result.stderr.unwrap_or_default()
                )
            );
        }
        let image_paths: Vec<String> = serde_json
            ::from_str(&result.stdout.unwrap_or_default())
            .map_err(|e| anyhow::anyhow!("Failed to parse image paths: {}", e))?;
        Ok(image_paths)
    }

    /// 执行文件处理步骤
    async fn execute_file_process_step(
        &self,
        rt: &mut AgentRuntime,
        st: &mut StepState,
        event_tx: &mpsc::Sender<ChatEvent>
    ) -> Result<serde_json::Value> {
        let get_param = |key: &str| -> Option<serde_json::Value> {
            st.step_def
                .get(key)
                .cloned()
                .or_else(|| st.step_def.get_owned(key))
                .or_else(|| st.step_def.get_owned("params").and_then(|p| p.get(key).cloned()))
        };
        let step_name = get_param("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .map(String::from)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "文件处理".to_string());
        let file_paths_val = get_param("file_paths");
        let llm_prompt = get_param("llm_prompt");
        info!("Executing file_process step: {}", step_name);
        let mut resolved_files: Vec<(PathBuf, String)> = Vec::new();
        if let Some(fp_val) = file_paths_val {
            if let Some(fp_str) = fp_val.as_str() {
                if fp_str == "$attachments" {
                    for att in &rt.attachments {
                        if !att.file_path.is_empty() {
                            let mut path = PathBuf::from(&att.file_path);
                            if !path.is_absolute() {
                                path = self.chat_files_dir.join("attachments").join(&att.file_path);
                            }
                            resolved_files.push((path, att.file_name.clone()));
                        }
                    }
                } else {
                    let path = PathBuf::from(fp_str);
                    let abs = if path.is_absolute() {
                        path
                    } else {
                        self.chat_files_dir.join("attachments").join(fp_str)
                    };
                    let fname = abs
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    resolved_files.push((abs, fname));
                }
            } else if let Some(arr) = fp_val.as_array() {
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        let fp_str = obj
                            .get("path")
                            .or_else(|| obj.get("file_path"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let fn_str = obj
                            .get("name")
                            .or_else(|| obj.get("file_name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if fp_str.is_empty() {
                            continue;
                        }
                        let path = PathBuf::from(fp_str);
                        let abs = if path.is_absolute() {
                            path
                        } else {
                            self.chat_files_dir.join("attachments").join(fp_str)
                        };
                        let fname = if fn_str.is_empty() {
                            abs.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default()
                        } else {
                            fn_str.to_string()
                        };
                        resolved_files.push((abs, fname));
                    } else if let Some(s) = item.as_str() {
                        let path = PathBuf::from(s);
                        let abs = if path.is_absolute() {
                            path
                        } else {
                            self.chat_files_dir.join("attachments").join(s)
                        };
                        let fname = abs
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        resolved_files.push((abs, fname));
                    }
                }
            }
        }
        if resolved_files.is_empty() {
            if let Some(fp_str) = get_param("file_path").as_ref().and_then(|v| v.as_str()) {
                let path = PathBuf::from(fp_str);
                let abs = if path.is_absolute() {
                    path
                } else {
                    self.chat_files_dir.join("attachments").join(fp_str)
                };
                let fname = abs
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                resolved_files.push((abs, fname));
            }
        }
        if resolved_files.is_empty() {
            emit_step_status(
                &st.step_id,
                "warning",
                "未指定要处理的文件",
                Some(0),
                event_tx
            ).await;
            st.mark_completed();
            rt.mark_step_completed(&st.step_id);
            return Ok(serde_json::Value::Array(vec![]));
        }
        let total = resolved_files.len();
        if total > 1 {
            emit_step_status(
                &st.step_id,
                "progress",
                &format!("正在处理 {} 个文件...", total),
                Some(0),
                event_tx
            ).await;
        } else {
            emit_step_status(
                &st.step_id,
                "progress",
                &format!("正在处理文件: {}...", resolved_files[0].1),
                Some(0),
                event_tx
            ).await;
        }
        // 并发处理文件
        use futures::stream::{ self, StreamExt };
        let file_tasks = resolved_files
            .into_iter()
            .enumerate()
            .map(|(idx, (full_path, fname))| {
                let step_id = st.step_id.clone();
                let tx = event_tx.clone();
                let llm_prompt = llm_prompt.clone();
                async move {
                    if !full_path.exists() {
                        return Ok::<_, anyhow::Error>((
                            idx,
                            vec![serde_json::Value::String(format!("[文件不存在: {}]", fname))],
                        ));
                    }
                    let file_ext = full_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if total > 1 {
                        let progress = ((idx * 100) / total) as u8;
                        emit_step_status(
                            &step_id,
                            "info",
                            &format!("正在处理 ({}/{total}): {}...", idx + 1, fname),
                            Some(progress),
                            &tx
                        ).await;
                    }
                    let file_results = self.process_single_file(
                        &full_path,
                        &fname,
                        &file_ext,
                        &step_id,
                        &tx,
                        llm_prompt.as_ref()
                    ).await?;
                    Ok::<_, anyhow::Error>((idx, file_results))
                }
            });
        let raw_results: Vec<_> = stream
            ::iter(file_tasks)
            .buffer_unordered(0)
            .collect::<Vec<_>>().await;
        let mut results_with_index: Vec<(usize, Vec<serde_json::Value>)> = raw_results
            .into_iter()
            .collect::<std::result::Result<Vec<_>, _>>()?;
        results_with_index.sort_by_key(|(idx, _)| *idx);
        let mut results: Vec<serde_json::Value> = Vec::new();
        for (_, file_results) in results_with_index {
            for r in file_results {
                results.push(r);
            }
        }
        let done_msg = if total > 1 {
            format!("文件处理完成，共处理 {} 个文件", total)
        } else {
            "文件处理完成".to_string()
        };
        emit_step_status(&st.step_id, "success", &done_msg, Some(100), event_tx).await;
        Ok(serde_json::Value::Array(results))
    }

    /// 处理单个文件
    async fn process_single_file(
        &self,
        full_path: &Path,
        _fname: &str,
        file_ext: &str,
        _step_id: &str,
        event_tx: &mpsc::Sender<ChatEvent>,
        _llm_prompt: Option<&serde_json::Value>
    ) -> Result<Vec<serde_json::Value>> {
        match file_ext {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" => {
                emit_step_status(
                    _step_id,
                    "info",
                    "正在识别图片内容...",
                    Some(30),
                    event_tx
                ).await;
                let content = self.execute_vision_recognition(
                    &full_path.to_string_lossy(),
                    event_tx
                ).await?;
                Ok(vec![ensure_json(&content).unwrap_or_else(|| serde_json::Value::String(content))])
            }
            "pdf" => {
                use futures::stream::{ self, StreamExt };
                emit_step_status(
                    _step_id,
                    "info",
                    "正在将PDF转换为图片...",
                    Some(20),
                    event_tx
                ).await;
                let image_paths = self.pdf_to_images(&full_path.to_string_lossy(), event_tx).await?;
                let total_pages = image_paths.len();
                let raw_results: Vec<_> = stream
                    ::iter(image_paths.into_iter().enumerate())
                    .map(|(idx, img_path)| {
                        let total = total_pages;
                        let status_id = _step_id.to_string();
                        let tx = event_tx.clone();
                        async move {
                            let _ = emit_step_status(
                                &status_id,
                                "progress",
                                &format!("正在识别第{}/{}页...", idx + 1, total),
                                Some(20 + ((((idx + 1) * 80) / total) as u8)),
                                &tx
                            ).await;
                            let page_result = self.execute_vision_recognition(
                                &img_path,
                                &tx
                            ).await?;
                            Ok::<_, anyhow::Error>((idx, ensure_json(&page_result).unwrap_or_else(|| serde_json::Value::String(page_result))))
                        }
                    })
                    .buffer_unordered(0)
                    .collect::<Vec<_>>().await;
                let mut all_results: Vec<(usize, serde_json::Value)> = raw_results
                    .into_iter()
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                all_results.sort_by_key(|(idx, _)| *idx);
                Ok(
                    all_results
                        .into_iter()
                        .map(|(_, v)| v)
                        .collect()
                )
            }
            | "xlsx"
            | "xls"
            | "pptx"
            | "docx"
            | "odt"
            | "ods"
            | "odp"
            | "rtf"
            | "csv"
            | "tsv"
            | "epub"
            | "mobi"
            | "html"
            | "htm"
            | "xml"
            | "chm" => {
                emit_step_status(
                    _step_id,
                    "info",
                    "正在提取文档内容...",
                    Some(30),
                    event_tx
                ).await;
                let output_path = std::env
                    ::temp_dir()
                    .join(format!("agent_doc_{}.txt", Uuid::new_v4()));
                let output = std::process::Command
                    ::new("agent-doc")
                    .arg("extract")
                    .arg(&full_path.to_string_lossy().to_string())
                    .arg("-f")
                    .arg("txt")
                    .arg("-o")
                    .arg(&output_path)
                    .output();
                match output {
                    Ok(out) => {
                        let content = if out.status.success() && output_path.exists() {
                            std::fs::read_to_string(&output_path).unwrap_or_default()
                        } else {
                            format!(
                                "[agent-doc 提取失败: {}]",
                                String::from_utf8_lossy(&out.stderr)
                            )
                        };
                        let _ = std::fs::remove_file(&output_path);
                        Ok(vec![serde_json::Value::String(content)])
                    }
                    Err(e) =>
                        Ok(vec![serde_json::Value::String(format!("[agent-doc 执行失败: {}]", e))]),
                }
            }
            _ => Err(anyhow::anyhow!("Unsupported file type: {}", file_ext)),
        }
    }

    /// 处理步骤输出（支持 output_target 配置）
    async fn handle_step_output_with_target(
        &self,
        rt: &mut AgentRuntime,
        st: &StepState,
        result: serde_json::Value
    ) {
        let step_id = st.step_id.clone();
        let step_name = st.step_def
            .get_owned("step_name")
            .as_ref()
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string();
        let phase_id = st.step_def
            .get("phase_id")
            .and_then(|v| v.as_str())
            .unwrap_or(&rt.phase_id)
            .to_string();
        let output_target = st.step_def.get_owned("output_target");
        let output_type = output_target
            .as_ref()
            .and_then(|v| v.get("output_type"))
            .and_then(|v| v.as_str())
            .unwrap_or("context");
        let name = output_target
            .as_ref()
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&step_name);
        let tags: Vec<String> = output_target
            .as_ref()
            .and_then(|v| v.get("tags"))
            .and_then(|v| v.as_array())
            .map(|arr|
                arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            )
            .unwrap_or_else(|| vec![step_name.to_string()]);
        let identify_artifacts = output_target
            .as_ref()
            .and_then(|v| v.get("identify_artifacts"))
            .cloned();
        info!(
            "Step '{}' output_target: type={}, name={}, tags={:?}",
            step_name,
            output_type,
            name,
            tags
        );
        let mut saved_files: Vec<String> = Vec::new();
        if output_type == "file" {
            let conversation_id = if rt.subject.starts_with("conv_") {
                &rt.subject[5..]
            } else {
                &rt.subject
            };
            let output_dir = self.chat_files_dir.join(conversation_id).join("outputs");
            let _ = std::fs::create_dir_all(&output_dir);
            let safe_name: String = name
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_' )
                .collect::<String>()
                .trim()
                .to_string();
            let safe_name = if safe_name.is_empty() {
                format!("output_{}", step_id)
            } else {
                safe_name
            };
            if let Some(s) = result.as_str() {
                match
                    save_to_file(&rt.subject, &step_id, s, &safe_name, "txt", &self.chat_files_dir)
                {
                    Ok(fp) => {
                        saved_files.push(fp);
                    }
                    Err(e) => error!("Failed to save step '{}' output: {}", step_name, e),
                }
            } else if let Some(arr) = result.as_array() {
                let str_items: Vec<&str> = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect();
                if !str_items.is_empty() {
                    let content = str_items.join("\n");
                    let mut fname = format!("{}.txt", safe_name);
                    let mut fpath = output_dir.join(&fname);
                    let mut counter = 1;
                    while fpath.exists() {
                        fname = format!("{}_{}.txt", safe_name, counter);
                        fpath = output_dir.join(&fname);
                        counter += 1;
                    }
                    let _ = std::fs::write(&fpath, content);
                    if let Ok(p) = fpath.canonicalize() {
                        saved_files.push(p.to_string_lossy().to_string());
                    }
                }
                let mut non_str_items: Vec<serde_json::Value> = Vec::new();
                for item in arr {
                    if item.is_array() {
                        if let Some(inner) = item.as_array() {
                            non_str_items.extend(inner.iter().cloned());
                        }
                    } else if !item.is_string() {
                        non_str_items.push(item.clone());
                    }
                }
                if !non_str_items.is_empty() {
                    let mut lines = Vec::new();
                    for item in &non_str_items {
                        lines.push(serde_json::to_string(item).unwrap_or_default());
                    }
                    let content = lines.join("\n") + "\n";
                    let mut fname = format!("{}.jsonl", safe_name);
                    let mut fpath = output_dir.join(&fname);
                    let mut counter = 1;
                    while fpath.exists() {
                        fname = format!("{}_{}.jsonl", safe_name, counter);
                        fpath = output_dir.join(&fname);
                        counter += 1;
                    }
                    let _ = std::fs::write(&fpath, content);
                    if let Ok(p) = fpath.canonicalize() {
                        saved_files.push(p.to_string_lossy().to_string());
                    }
                }
            } else if result.is_object() || result.is_number() || result.is_boolean() {
                let fname = format!("{}.json", safe_name);
                let mut fpath = output_dir.join(&fname);
                let mut counter = 1;
                while fpath.exists() {
                    fpath = output_dir.join(format!("{}_{}.json", safe_name, counter));
                    counter += 1;
                }
                let content = serde_json::to_string_pretty(&result).unwrap_or_default();
                let _ = std::fs::write(&fpath, content);
                if let Ok(p) = fpath.canonicalize() {
                    saved_files.push(p.to_string_lossy().to_string());
                }
            }
            if !saved_files.is_empty() {
                rt.add_context(
                    &phase_id,
                    &step_id,
                    &step_name,
                    serde_json::Value::String(
                        format!("文件已保存至 {}，文件名为 {}", saved_files.join(", "), name)
                    ),
                    tags.clone()
                );
            }
        }
        if output_type == "context" {
            let formatted_result = if let Some(s) = result.as_str() {
                s.to_string()
            } else if let Some(arr) = result.as_array() {
                let mut parts = Vec::new();
                let mut jsonl_items = Vec::new();
                for item in arr {
                    if let Some(s) = item.as_str() {
                        parts.push(s.to_string());
                    } else {
                        jsonl_items.push(serde_json::to_string(item).unwrap_or_default());
                    }
                }
                let mut r = parts.join("\n");
                if !jsonl_items.is_empty() {
                    if !r.is_empty() {
                        r.push('\n');
                    }
                    r.push_str(&jsonl_items.join("\n"));
                }
                r
            } else {
                serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            };
            rt.add_context(
                &phase_id,
                &step_id,
                &step_name,
                serde_json::Value::String(formatted_result),
                tags
            );
        }
        // 处理 artifact 注册
        if let Some(identify_cfg) = identify_artifacts {
            let identified: Vec<Artifact> = if identify_cfg.is_string() {
                let instruction = identify_cfg.as_str().unwrap_or("");
                self.identify_artifacts(rt, st, &result, instruction, &saved_files).await
            } else {
                saved_files
                    .iter()
                    .enumerate()
                    .map(|(i, fp)| {
                        let _name = Path::new(fp)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let mut meta = HashMap::new();
                        meta.insert("reason".to_string(), serde_json::json!(""));
                        Artifact {
                            artifact_id: format!("art_{}", i + 1),
                            phase_id: phase_id.to_string(),
                            step_id: st.step_id.clone(),
                            step_name: step_name.to_string(),
                            artifact_type: "file".to_string(),
                            path: fp.clone(),
                            metadata: meta,
                            created_at: Utc::now(),
                        }
                    })
                    .collect()
            };
            let artifact_count = identified.len();
            rt.artifacts.extend(identified);
            info!("Step '{}' registered {} artifacts", step_name, artifact_count);
        }
    }

    /// LLM 识别 artifact
    async fn identify_artifacts(
        &self,
        rt: &AgentRuntime,
        st: &StepState,
        _result: &serde_json::Value,
        instruction: &str,
        _saved_files: &[String]
    ) -> Vec<Artifact> {
        let context_text: Vec<String> = rt.contexts
            .iter()
            .map(|ctx| {
                match &ctx.content {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                }
            })
            .collect();
        let context_text = context_text.join("\n---\n");
        let prompt = format!(
            "你是一个文件识别助手。根据以下执行上下文，识别步骤中生成或引用的有效文件（artifact）。\n\n## 执行上下文\n{}\n\n## 识别规则\n{}\n\n## 要求\n1. 从上下文中找出提到的文件路径，只识别对用户有价值的文件，忽略临时文件\n2. 直接使用完整的文件路径\n3. 不需要任何文件时返回空数组\n\n## 输出格式\n返回 JSON：\n{{\"artifacts\": [{{\"path\": \"文件完整路径\", \"name\": \"文件名\", \"reason\": \"为什么这是有效artifact\"}}]}}",
            context_text,
            instruction
        );
        let messages = vec![ChatMessage::user(&prompt)];
        let response = match self.complete_text(&messages, 0.1, 0.9).await {
            Ok(r) => r,
            Err(e) => {
                warn!("identify_artifacts LLM failed: {}", e);
                return Vec::new();
            }
        };
        let mut validated = Vec::new();
        if let Ok(plan) = serde_json::from_str::<serde_json::Value>(&response.trim()) {
            if let Some(arr) = plan.get("artifacts").and_then(|v| v.as_array()) {
                for art in arr {
                    let path = art
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let name = art
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let reason = art
                        .get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if path.is_empty() || name.is_empty() {
                        continue;
                    }
                    let path = if Path::new(path).exists() {
                        path.to_string()
                    } else {
                        let conversation_id = if rt.subject.starts_with("conv_") {
                            &rt.subject[5..]
                        } else {
                            &rt.subject
                        };
                        let output_path = self.chat_files_dir
                            .join(conversation_id)
                            .join("outputs")
                            .join(path);
                        if output_path.exists() {
                            output_path.to_string_lossy().to_string()
                        } else {
                            continue;
                        }
                    };
                    let step_name = st.step_def
                        .get("step_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let mut meta = HashMap::new();
                    meta.insert("reason".to_string(), serde_json::json!(reason));
                    validated.push(Artifact {
                        artifact_id: format!("art_{}", rt.artifacts.len() + validated.len() + 1),
                        phase_id: String::new(),
                        step_id: st.step_id.clone(),
                        step_name: step_name.to_string(),
                        artifact_type: "file".to_string(),
                        path,
                        metadata: meta,
                        created_at: Utc::now(),
                    });
                }
            }
        }
        info!("identify_artifacts found {} valid artifacts", validated.len());
        validated
    }

    /// 从 rt 的所有 artifact 构建前端附件列表
    fn build_attachments_from_artifacts(&self, rt: &AgentRuntime) -> Vec<serde_json::Value> {
        let conversation_id = if rt.subject.starts_with("conv_") {
            &rt.subject[5..]
        } else {
            &rt.subject
        };
        let output_dir = self.chat_files_dir.join(conversation_id).join("outputs");
        let mut attachments = Vec::new();
        let mut seen_paths = std::collections::HashSet::new();
        for art in rt.get_all_artifacts() {
            let path = art.path.clone();
            if path.is_empty() || !seen_paths.insert(path.clone()) {
                continue;
            }
            let p = Path::new(&path);
            let name = art.metadata
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_else(||
                    p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(&path)
                );
            let file_type = self.guess_mime_type(p);
            let file_size = p
                .metadata()
                .map(|m| m.len())
                .unwrap_or(0);
            let (rel_path, url) = if p.is_absolute() {
                match p.strip_prefix(&output_dir) {
                    Ok(rel) => {
                        let rel_str = rel.to_string_lossy().to_string();
                        (
                            rel_str.clone(),
                            format!("/api/chat/files/{}/outputs/{}", conversation_id, rel_str),
                        )
                    }
                    Err(_) => {
                        let fn_str = p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        (
                            fn_str.clone(),
                            format!("/api/chat/files/{}/outputs/{}", conversation_id, fn_str),
                        )
                    }
                }
            } else {
                (path.clone(), format!("/api/chat/files/{}/outputs/{}", conversation_id, path))
            };
            let mut att = serde_json::Map::new();
            att.insert("path".to_string(), serde_json::json!(rel_path));
            att.insert("name".to_string(), serde_json::json!(name));
            att.insert("type".to_string(), serde_json::json!(file_type));
            att.insert("size".to_string(), serde_json::json!(file_size));
            att.insert("url".to_string(), serde_json::json!(url));
            att.insert("source".to_string(), serde_json::json!("artifact"));
            attachments.push(serde_json::Value::Object(att));
        }
        debug!("build_attachments_from_artifacts: {} attachments", attachments.len());
        attachments
    }

    /// 异步版本的 context_query，支持 @llm: 智能查询
    async fn execute_context_query_async(
        &self,
        rt: &AgentRuntime,
        context_query: &[String],
        _step_name: &str
    ) -> Vec<serde_json::Value> {
        if context_query.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::new();
        let sync_queries: Vec<String> = context_query
            .iter()
            .filter(|q| !q.starts_with("@llm:"))
            .cloned()
            .collect();
        if !sync_queries.is_empty() {
            result.extend(rt.execute_context_query(&sync_queries));
        }
        let llm_queries: Vec<String> = context_query
            .iter()
            .filter(|q| q.starts_with("@llm:"))
            .map(|q| q[5..].to_string())
            .collect();
        for instruction in llm_queries {
            if let Ok(llm_result) = self.llm_smart_query(rt, &instruction).await {
                if !llm_result.is_empty() {
                    result.push(serde_json::Value::String(llm_result));
                }
            }
        }
        let mut seen = std::collections::HashSet::new();
        let mut unique_result = Vec::new();
        for item in result {
            let item_str = item.to_string();
            if seen.insert(item_str) {
                unique_result.push(item);
            }
        }
        if !unique_result.is_empty() {
            debug!("context_query (async) matched {} contexts", unique_result.len());
        }
        unique_result
    }

    /// 使用 LLM 智能查询
    async fn llm_smart_query(&self, rt: &AgentRuntime, instruction: &str) -> Result<String> {
        let all_contexts = &rt.contexts;
        if all_contexts.is_empty() {
            return Ok(String::new());
        }
        let summaries: Vec<String> = all_contexts
            .iter()
            .map(|ctx| {
                let preview = ctx.content.to_string();
                let preview = if preview.len() > 300 {
                    format!("{}...", &preview[..300])
                } else {
                    preview
                };
                format!(
                    "[{}] 步骤: {} | 标签: {}\n  内容: {}",
                    ctx.context_id,
                    ctx.step_name,
                    ctx.tags
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    preview
                )
            })
            .collect();
        let prompt = format!(
            "你是一个 shell 命令生成助手。根据用户指令，从以下上下文信息中识别相关文件路径，并生成 bash 命令来读取这些文件的内容。\n\n## 可用的上下文信息\n{}\n\n## 用户指令\n{}\n\n## 要求\n1. 生成读取文件的 bash 命令\n2. 如果文件可能很大，使用 head/tail 等命令\n3. 可以使用 cat, head, tail, grep, jq, wc 等只读命令\n4. 不要使用修改/删除命令\n\n## 输出格式\n返回 JSON：\n{{\"commands\": [\"bash命令1\"], \"reason\": \"说明\"}}",
            summaries.join("\n\n"),
            instruction
        );
        let messages = vec![ChatMessage::user(&prompt)];
        let response = match self.complete_text(&messages, 0.1, 0.9).await {
            Ok(r) => r,
            Err(e) => {
                warn!("@llm query LLM failed: {}", e);
                return Ok(String::new());
            }
        };
        let commands = match self.parse_llm_json_response(&response) {
            Some(cmds) => cmds,
            None => {
                return Ok(String::new());
            }
        };
        let mut results = Vec::new();
        for cmd in commands {
            if cmd.trim().is_empty() {
                continue;
            }
            if !is_safe_command(&cmd) {
                results.push(format!("## 命令: {}\n[命令被拦截：不安全操作]", cmd));
                continue;
            }
            match tokio::process::Command::new("bash").arg("-c").arg(&cmd).output().await {
                Ok(o) => {
                    let mut output_parts = Vec::new();
                    if !o.stdout.is_empty() {
                        let text = String::from_utf8_lossy(&o.stdout);
                        let text = if text.len() > 50000 {
                            format!("{}\n[内容已截断]", &text[..50000])
                        } else {
                            text.to_string()
                        };
                        output_parts.push(text);
                    }
                    if !o.stderr.is_empty() {
                        let err_text = String::from_utf8_lossy(&o.stderr);
                        output_parts.push(
                            format!("(stderr: {})", if err_text.len() > 1000 {
                                err_text[..1000].to_string()
                            } else {
                                err_text.to_string()
                            })
                        );
                    }
                    if output_parts.is_empty() {
                        results.push(format!("## 命令: {}\n[无输出]", cmd));
                    } else {
                        results.push(format!("## 命令: {}\n{}", cmd, output_parts.join("")));
                    }
                }
                Err(e) => {
                    results.push(format!("## 命令: {}\n执行失败: {}", cmd, e));
                }
            }
        }
        if results.is_empty() {
            return Ok(String::new());
        }
        Ok(format!("## 相关上下文（@llm: {}）\n\n{}", instruction, results.join("\n\n")))
    }

    // ========================================================================
    // run_steps / chat / chat_stream
    // ========================================================================

    async fn run_steps(
        &self,
        rt: &mut AgentRuntime,
        temperature: f32,
        top_p: f32,
        event_tx: mpsc::Sender<ChatEvent>
    ) -> Result<String> {
        Self::init_steps(rt);
        // agentlite parity: 构建 LLM 上下文时惰性加载上一轮持久化 contexts 注入 rt.contexts
        if let Some(ctxs) = self.store.load_latest_contexts(&rt.subject).await {
            for c in ctxs {
                let kind = match c.get("kind").and_then(|v| v.as_str()) {
                    Some("artifact") => ContextKind::Artifact,
                    _ => ContextKind::Context,
                };
                rt.contexts.push(StepContext {
                    context_id: c.get("context_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    phase_id: c.get("phase_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    step_id: c.get("step_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    step_name: c.get("step_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    kind,
                    tags: c.get("tags")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                        .unwrap_or_default(),
                    content: c.get("content").cloned().unwrap_or(serde_json::Value::Null),
                    metadata: c.get("metadata")
                        .and_then(|v| v.as_object())
                        .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default(),
                    created_at: Utc::now(),
                });
            }
            if !rt.contexts.is_empty() {
                info!("Loaded {} persisted contexts from previous run", rt.contexts.len());
            }
        }
        loop {
            if rt.cancelled {
                emit_event(
                    ChatEvent::Error { message: "Chat cancelled".to_string() },
                    &event_tx
                ).await;
                return Ok(rt.assistant_text.clone());
            }
            let st = match rt.pop_step() {
                Some(s) => s,
                None => {
                    return Ok(rt.assistant_text.clone());
                }
            };
            let mut st = st;
            // Build LLM messages
            let mut llm_messages: Vec<ChatMessage> = Vec::new();
            if let Some(ref sp) = rt.system_prompt {
                if !sp.trim().is_empty() {
                    llm_messages.push(ChatMessage::system(sp.trim()));
                }
            }
            self.ensure_history_loaded().await?;
            {
                let h = self.history.read().await;
                llm_messages.extend(h.clone());
            }
            // Inject context_query (支持 @llm: 查询) — 统一走 inject_step_context
            self.inject_step_context(rt, &st, &mut llm_messages).await;
            // Inject attachments
            if !rt.attachments.is_empty() {
                let mut attachment_parts: Vec<String> = Vec::new();
                for att in &rt.attachments {
                    let mut att_info = format!("- 文件名: {}", att.file_name);
                    if !att.file_type.is_empty() {
                        att_info.push_str(&format!(", 类型: {}", att.file_type));
                    }
                    if !att.file_path.is_empty() {
                        att_info.push_str(&format!(", 路径: {}", att.file_path));
                    }
                    attachment_parts.push(att_info);
                }
                let att_text = format!(
                    "## 附件信息\n用户上传了以下文件：\n{}",
                    attachment_parts.join("\n")
                );
                if let Some(first) = llm_messages.iter_mut().find(|m| m.role == "system") {
                    if !first.content.contains("## 附件信息") {
                        first.content.push_str("\n\n");
                        first.content.push_str(&att_text);
                    }
                } else {
                    llm_messages.insert(0, ChatMessage::system(&att_text));
                }
            }
            // Execute step
            let result = match st.kind {
                StepKind::Question =>
                    self.execute_question_step(rt, &mut st, &mut llm_messages, &event_tx).await,
                StepKind::Answer => {
                    let r = self.execute_answer_step(
                        rt,
                        &mut st,
                        &llm_messages,
                        temperature,
                        top_p,
                        Some(event_tx.clone())
                    ).await?;
                    return Ok(r);
                }
                StepKind::FastFileRead =>
                    self.execute_fast_file_read_step(rt, &mut st, &event_tx).await,
                StepKind::FileProcess => {
                    match self.execute_file_process_step(rt, &mut st, &event_tx).await {
                        Ok(output) => {
                            self.handle_step_output_with_target(rt, &st, output).await;
                        }
                        Err(e) => {
                            error!("FileProcess step execution failed: {}", e);
                            emit_step_status(
                                &st.step_id,
                                "error",
                                &format!("执行失败: {}", e),
                                Some(100),
                                &event_tx
                            ).await;
                        }
                    }
                    st.mark_completed();
                    rt.mark_step_completed(&st.step_id);
                    continue;
                }
                StepKind::BashCommand =>
                    self.execute_bash_command_step(rt, &mut st, &event_tx).await,
                StepKind::CallLlm =>
                    self.execute_call_llm_step(rt, &mut st, &llm_messages, temperature, top_p, &event_tx).await,
                StepKind::GenerateCommand =>
                    self.execute_generate_command_step(rt, &mut st, &event_tx).await,
                StepKind::CheckValidation =>
                    self.execute_check_validation_step(rt, &mut st, &llm_messages, temperature, top_p, &event_tx).await,
                StepKind::FindMoreContext =>
                    self.execute_find_more_context_step(rt, &mut st, &event_tx).await,
                StepKind::ContentAssemble =>
                    self.execute_content_assemble_step(rt, &mut st, &event_tx).await,
                StepKind::EnterBlock => self.execute_enter_block_step(rt, &mut st, &event_tx).await,
                StepKind::ExitBlock => self.execute_exit_block_step(rt, &mut st, &event_tx).await,
                StepKind::Block => self.execute_block_step(rt, &mut st, &event_tx).await,
                StepKind::LoopBlock => self.execute_loop_block_step(rt, &mut st, &event_tx).await,
                StepKind::RequestUserInput =>
                    self.execute_request_user_input_step(rt, &mut st, &event_tx).await,
                StepKind::Task => self.execute_task_step(rt, &mut st, &event_tx).await,
                StepKind::FollowGuide =>
                    self.execute_follow_guide_step(
                        rt,
                        &mut st,
                        &llm_messages,
                        temperature,
                        top_p,
                        Some(event_tx.clone())
                    ).await,
                StepKind::ThinkAndGenerate =>
                    self.execute_think_and_generate_step(rt, &mut st, &event_tx).await,
            };
            // 处理步骤结果（使用 output_target 处理输出）
            match result {
                Ok(output) => {
                    self.handle_step_output_with_target(
                        rt,
                        &st,
                        serde_json::Value::String(output)
                    ).await;
                }
                Err(e) => {
                    error!("Step execution failed: {}", e);
                    emit_step_status(
                        &st.step_id,
                        "error",
                        &format!("执行失败: {}", e),
                        None,
                        &event_tx
                    ).await;
                }
            }
            if st.status == StepStatus::Running {
                st.mark_completed();
                rt.mark_step_completed(&st.step_id);
            }
        }
    }

    pub async fn chat(
        &self,
        user_content: &str,
        system_prompt: Option<&str>,
        temperature: f32,
        top_p: f32
    ) -> Result<String> {
        self.ensure_history_loaded().await?;
        let sem = crate::core::get_global_session_semaphore().await;
        if let Some(ref s) = sem {
            s.acquire().await;
        }
        let result = (async {
            let mut rt = AgentRuntime::new(
                self.subject.clone(),
                user_content.to_string(),
                system_prompt.map(String::from)
            );
            rt.knowledge_store = self.knowledge_store.clone();
            let mut env_map = HashMap::new();
            env_map.insert(
                "chat_files_root".to_string(),
                serde_json::json!(self.chat_files_dir.to_string_lossy())
            );
            // agentlite parity: 注入 disks_default（由 host 通过 configure_paths 提供）
            if let Some(p) = core_paths() {
                env_map.insert(
                    "disks_default".to_string(),
                    serde_json::json!(p.disks_dir.to_string_lossy())
                );
            }
            rt.env = EnvManager::new(env_map);
            self.append_and_cache("user", user_content, None).await?;
            let (event_tx, _event_rx) = mpsc::channel::<ChatEvent>(100);
            self.run_steps(&mut rt, temperature, top_p, event_tx).await?;
            Ok(rt.assistant_text)
        }).await;
        if let Some(ref s) = sem {
            s.release();
        }
        result
    }

    pub async fn chat_stream(
        &self,
        user_content: &str,
        system_prompt: Option<&str>,
        temperature: f32,
        top_p: f32,
        attachments: Option<Vec<Attachment>>
    ) -> Result<mpsc::Receiver<ChatEvent>> {
        self.ensure_history_loaded().await?;
        let (event_tx, event_rx) = mpsc::channel::<ChatEvent>(100);

        // 标记任务已开始（含排队阶段），清空事件缓冲
        *self.running.write().await = true;
        self.event_buffer.write().await.clear();
        *self.is_queued.write().await = false;
        *self.queue_position.write().await = 0;

        let sem = crate::core::get_global_session_semaphore().await;
        if let Some(ref s) = sem {
            s.acquire_with_events(&event_tx).await;
        }
        *self.is_queued.write().await = false;
        *self.queue_position.write().await = 0;
        *self.started_at.write().await = Some(Utc::now().timestamp());
        let mut rt = AgentRuntime::new(
            self.subject.clone(),
            user_content.to_string(),
            system_prompt.map(String::from)
        );
        rt.knowledge_store = self.knowledge_store.clone();
        let mut env_map = HashMap::new();
        env_map.insert(
            "chat_files_root".to_string(),
            serde_json::json!(self.chat_files_dir.to_string_lossy())
        );
        // agentlite parity: 注入 disks_default（由 host 通过 configure_paths 提供）
        if let Some(p) = core_paths() {
            env_map.insert(
                "disks_default".to_string(),
                serde_json::json!(p.disks_dir.to_string_lossy())
            );
        }
        rt.env = EnvManager::new(env_map);
        // 处理附件：解析相对路径为绝对路径，并构建持久化负载
        let user_attachments_payload: Option<HashMap<String, serde_json::Value>> = if
            let Some(atts) = attachments
        {
            if atts.is_empty() {
                None
            } else {
                let mut resolved_atts = Vec::new();
                let mut items = Vec::new();
                for mut a in atts {
                    let p = std::path::Path::new(&a.file_path);
                    if !p.is_absolute() {
                        let abs = self.chat_files_dir.join(p);
                        a.file_path = abs.to_string_lossy().to_string();
                    }
                    items.push(serde_json::to_value(&a).unwrap_or(serde_json::Value::Null));
                    resolved_atts.push(a);
                }
                rt.attachments = resolved_atts;
                let mut map = HashMap::new();
                map.insert("items".to_string(), serde_json::Value::Array(items));
                Some(map)
            }
        } else {
            None
        };
        self.append_and_cache("user", user_content, user_attachments_payload).await?;
        let self_clone = self.clone();
        tokio::spawn(async move {
            // agentlite parity: 不再在 run_steps 前无条件调用 think()；
            // think 只在 think_and_generate 步骤内部进行。
            // agentlite parity: on failure emit an error event and still send
            // a done marker so the frontend never hangs in "running" state.
            if let Err(e) = self_clone.run_steps(&mut rt, temperature, top_p, event_tx.clone()).await {
                error!("run_steps failed: {}", e);
                emit_event(
                    ChatEvent::Error { message: format!("执行失败: {}", e) },
                    &event_tx
                ).await;
                emit_event(
                    ChatEvent::Done {
                        content: rt.assistant_text.clone(),
                        elapsed_seconds: 0.0,
                        elapsed_display: String::new(),
                        completed_at: chrono::Utc::now().to_rfc3339(),
                    },
                    &event_tx
                ).await;
            }
            *self_clone.running.write().await = false;
            *self_clone.started_at.write().await = None;
            *self_clone.streaming_message_id.write().await = None;
            if let Some(ref s) = sem {
                s.release();
            }
        });
        Ok(event_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_env_manager_basic() {
        let env = EnvManager::new(HashMap::new());
        assert!(env.get("k").is_none());
        env.set("k", serde_json::json!("v"));
        assert_eq!(env.get("k").unwrap().as_str(), Some("v"));
    }
    #[test]
    fn test_env_manager_scope() {
        let env = EnvManager::new(HashMap::new());
        {
            let _s = env.scope([("tmp".to_string(), serde_json::json!(1))].into());
            assert_eq!(env.get("tmp").unwrap().as_i64(), Some(1));
        }
        assert!(env.get("tmp").is_none());
    }
    #[test]
    fn test_runtime_context() {
        let mut rt = AgentRuntime::new("test".into(), "hello".into(), None);
        rt.add_context("p1", "s1", "step1", serde_json::json!("data"), vec!["tag1".to_string()]);
        assert_eq!(rt.contexts.len(), 1);
        let r = rt.execute_context_query(&["$all".to_string()]);
        assert_eq!(r.len(), 1);
    }
    #[tokio::test]
    async fn test_agent_chat_loop() {
        let cl = AgentLoop::new(
            "test_subj".into(),
            Some("anon".to_string()),
            None,
            None,
        );
        assert!(!cl.subject.is_empty());
    }
}
