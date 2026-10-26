use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Inference engine type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum InferenceEngineType {
    #[serde(alias = "llamacpp")]
    LlamaCpp,
    #[serde(alias = "vllm")]
    Vllm,
}

impl std::fmt::Display for InferenceEngineType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceEngineType::LlamaCpp => write!(f, "llamacpp"),
            InferenceEngineType::Vllm => write!(f, "vllm"),
        }
    }
}

impl Default for InferenceEngineType {
    fn default() -> Self {
        InferenceEngineType::LlamaCpp
    }
}

/// Inference process configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProcessConfig {
    pub engine_type: InferenceEngineType,
    pub model_name: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub executable_path: Option<String>,
    /// Multimodal projector file path (e.g., mmproj-F16.gguf for LLaVA)
    #[serde(default)]
    pub mmproj_path: Option<String>,
    /// vLLM virtual environment path (e.g., /app/venv)
    #[serde(default)]
    pub venv_path: Option<String>,
    #[serde(default)]
    pub additional_args: Vec<String>,
    #[serde(default)]
    pub env_vars: HashMap<String, String>,
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    #[serde(default = "default_restart_delay")]
    pub restart_delay: f64,
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval: f64,
    /// 启动阶段轮询间隔（llamacpp 3s / vllm 15s）
    #[serde(default = "default_startup_poll_interval")]
    pub startup_poll_interval: f64,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_max_restarts() -> u32 {
    5
}

fn default_restart_delay() -> f64 {
    3.0
}

fn default_health_check_interval() -> f64 {
    20.0
}

fn default_startup_poll_interval() -> f64 {
    3.0
}

impl Default for InferenceProcessConfig {
    fn default() -> Self {
        Self {
            engine_type: InferenceEngineType::LlamaCpp,
            model_name: String::new(),
            host: default_host(),
            port: default_port(),
            executable_path: None,
            mmproj_path: None,
            venv_path: None,
            additional_args: Vec::new(),
            env_vars: HashMap::new(),
            max_restarts: default_max_restarts(),
            restart_delay: default_restart_delay(),
            health_check_interval: default_health_check_interval(),
            startup_poll_interval: default_startup_poll_interval(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProcessState {
    Stopped,
    Starting,
    WarmingUp,   // 进程已启动，模型加载中（vLLM 等启动慢的引擎）
    Running,     // 进程已就绪，可以接收请求
    Restarting,
    Failed,
}

impl std::fmt::Display for ProcessState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessState::Stopped => write!(f, "stopped"),
            ProcessState::Starting => write!(f, "starting"),
            ProcessState::WarmingUp => write!(f, "warming_up"),
            ProcessState::Running => write!(f, "running"),
            ProcessState::Restarting => write!(f, "restarting"),
            ProcessState::Failed => write!(f, "failed"),
        }
    }
}

/// Inference process status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceProcessStatus {
    pub state: String,
    pub is_ready: bool,
    pub engine_type: Option<String>,
    pub model_name: Option<String>,
    pub mmproj_path: Option<String>,
    pub venv_path: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub restart_count: u32,
    pub start_time: Option<String>,
    pub last_error: Option<String>,
}

/// Start inference process request
#[derive(Debug, Clone, Deserialize)]
pub struct InferenceProcessStartRequest {
    pub config: InferenceProcessConfig,
}

/// Simple message response
#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    pub message: String,
    #[serde(default = "default_success")]
    pub success: bool,
}

fn default_success() -> bool {
    true
}
