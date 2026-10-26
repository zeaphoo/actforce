use std::path::PathBuf;
use once_cell::sync::Lazy;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const RELEASE_DATE: &str = "2025-04-03";

/// Global configuration instance
pub static CONFIG: Lazy<Config> = Lazy::new(|| Config::new());

/// Application configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Data root directory
    pub data_root: PathBuf,
    /// Builtin data directory (for built-in resources)
    pub builtin_dir: PathBuf,
    /// Database path
    pub db_path: PathBuf,
    /// Skills database path (for user-edited skills with FTS index)
    pub skills_db_path: PathBuf,
    /// Disks directory (data_root/disks/default)
    pub disks_dir: PathBuf,
    /// Raw disks directory for dufs (data_root/disks/default/raw)
    pub disks_raw_dir: PathBuf,
    /// Processed disks directory (data_root/disks/default/processed)
    pub disks_processed_dir: PathBuf,
    /// Chat attachments directory (data_root/files/chat)
    pub chat_attachments_dir: PathBuf,
    /// Chat files directory (data_root/files/chat) - same as chat_attachments_dir
    pub chat_files_dir: PathBuf,
    /// Models directory
    pub models_dir: PathBuf,
    /// Logs directory
    pub logs_dir: PathBuf,
    /// Debug mode
    pub debug: bool,
    /// SQL echo mode (output all SQL statements)
    pub sql_echo: bool,
    /// Inference service port
    pub inference_port: u16,
    /// Inference service host
    pub inference_host: String,
    /// LLM model name (llama-server ignores this parameter)
    pub llm_model: String,
    /// LLM debug mode (log LLM requests and responses)
    pub llm_debug: bool,
    /// LLM retry parameters (agentlite parity)
    pub llm_max_retries: usize,
    pub llm_retry_min_wait: f64,
    pub llm_retry_max_wait: f64,
    /// LLM max sequence length (context window size in tokens)
    pub llm_max_seq_len: usize,
    /// Upload max size in bytes
    pub upload_max_size: usize,
    /// Show sensitive info like model name in frontend
    pub show_sensitive_info: bool,
}

impl Config {
    /// Create a new configuration instance
    pub fn new() -> Self {
        dotenvy::dotenv().ok();
        
        let data_root = std::env::var("ACTFORCE_DATA_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./data/storage/"));
        
        // Ensure directories exist
        std::fs::create_dir_all(&data_root).ok();
        
        let builtin_dir = std::env::var("ACTFORCE_BUILTIN_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("./builtin/"));
        
        
        let db_path = data_root.join("actforce.db");
        let skills_db_path = data_root.join("actforce_skills.db");
        let disks_dir = data_root.join("disks").join("default");
        let disks_raw_dir = disks_dir.join("raw");
        let disks_processed_dir = disks_dir.join("processed");
        let chat_attachments_dir = data_root.join("files").join("chat");
        let chat_files_dir = chat_attachments_dir.clone(); // Same directory
        
        std::fs::create_dir_all(&disks_dir).ok();
        std::fs::create_dir_all(&disks_raw_dir).ok();
        std::fs::create_dir_all(&disks_processed_dir).ok();
        std::fs::create_dir_all(&chat_attachments_dir).ok();
        
        let models_dir = data_root.join("models");
        let logs_dir = data_root.join("logs");
        
        std::fs::create_dir_all(&models_dir).ok();
        std::fs::create_dir_all(&logs_dir).ok();
        
        let debug = std::env::var("ACTFORCE_DEBUG")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
        
        let sql_echo = std::env::var("ACTFORCE_SQL_ECHO")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
        
        let inference_port = std::env::var("INFERENCE_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        
        let inference_host = std::env::var("INFERENCE_HOST")
            .unwrap_or_else(|_| "127.0.0.1".to_string());
        
        // agentlite parity: LLM_MODEL when inference is not auto-started,
        // INFERENCE_MODEL_NAME otherwise; default "default"
        let auto_start = std::env::var("INFERENCE_AUTO_START")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);
        let llm_model = std::env::var(if auto_start { "INFERENCE_MODEL_NAME" } else { "LLM_MODEL" })
            .unwrap_or_else(|_| "default".to_string());
        
        let llm_debug = std::env::var("LLM_DEBUG")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        // agentlite parity: retry parameters
        let llm_max_retries = std::env::var("LLM_MAX_RETRIES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let llm_retry_min_wait = std::env::var("LLM_RETRY_MIN_WAIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1.0);
        let llm_retry_max_wait = std::env::var("LLM_RETRY_MAX_WAIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10.0);

        let llm_max_seq_len = std::env::var("LLM_MAX_SEQ_LEN")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(32768);

        let upload_max_size_mb: usize = std::env::var("UPLOAD_MAX_SIZE_MB")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(35);
        let upload_max_size = upload_max_size_mb * 1024 * 1024;

        let show_sensitive_info = std::env::var("ACTFORCE_SHOW_SENSITIVE_INFO")
            .map(|v| v.to_lowercase() == "true")
            .unwrap_or(false);

        Self {
            data_root,
            builtin_dir,
            db_path,
            skills_db_path,
            disks_dir,
            disks_raw_dir,
            disks_processed_dir,
            chat_attachments_dir,
            chat_files_dir,
            models_dir,
            logs_dir,
            debug,
            sql_echo,
            inference_port,
            inference_host,
            llm_model,
            llm_debug,
            llm_max_retries,
            llm_retry_min_wait,
            llm_retry_max_wait,
            llm_max_seq_len,
            upload_max_size,
            show_sensitive_info,
        }
    }

    
    /// Get database URL
    pub fn database_url(&self) -> String {
        format!("sqlite:{}", self.db_path.display())
    }
    
    /// Get skills database URL (for user-edited skills with FTS index)
    pub fn skills_database_url(&self) -> String {
        format!("sqlite:{}", self.skills_db_path.display())
    }
    
    /// Get builtin skills database URL (in-memory SQLite)
    /// Uses shared cache mode so multiple connections can access the same in-memory db
    pub fn builtin_skills_database_url(&self) -> String {
        "sqlite:file:builtin_skills?mode=memory&cache=shared".to_string()
    }
    
    /// Get version info as a map
    pub fn version_info(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut map = serde_json::Map::new();
        map.insert("version".to_string(), serde_json::Value::String(VERSION.to_string()));
        map.insert("release_date".to_string(), serde_json::Value::String(RELEASE_DATE.to_string()));
        map.insert("data_root".to_string(), serde_json::Value::String(self.data_root.to_string_lossy().to_string()));
        map
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}
