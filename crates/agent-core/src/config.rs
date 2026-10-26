use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// 模型类型分类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ModelCategory {
    /// 大语言模型 (GPT, Claude, Llama 等)
    Llm,
    /// 视觉语言模型 (GPT-4V, Qwen-VL 等)
    Vlm,
    /// 多模态模型 (支持文本+图像+音频)
    Lmm,
    /// 图像生成模型
    Image,
    /// 视频生成模型
    Video,
    /// 音频生成模型
    Audio,
    /// 语音识别模型
    Asr,
}

impl Default for ModelCategory {
    fn default() -> Self {
        ModelCategory::Llm
    }
}

/// 单个模型配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// 配置名称（唯一标识）
    pub name: String,
    /// 模型分类
    #[serde(default)]
    pub category: ModelCategory,
    /// API 提供商 
    pub provider: String,
    /// API 端点
    pub api_url: String,
    /// API 密钥
    pub api_key: Option<String>,
    /// 模型名称
    pub model: String,
    /// 最大 tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 温度参数
    #[serde(default = "default_temperature")]
    pub temperature: f32,
}

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// 多模型配置表 (key: 配置名称)
    pub models: HashMap<String, ModelConfig>,
    /// 默认模型配置名称
    #[serde(default = "default_model_name")]
    pub default_model: String,
    /// 工作目录
    pub workspace_dir: PathBuf,
    /// 日志级别
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_max_tokens() -> u32 {
    4096
}

fn default_temperature() -> f32 {
    0.7
}

fn default_model_name() -> String {
    "default".to_string()
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if !config_path.exists() {
            let default_config = Self::default();
            default_config.save()?;
            return Ok(default_config);
        }

        let content = std::fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;

        Ok(())
    }

    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to get config directory")?;
        Ok(config_dir.join("actforce").join("agent-core.toml"))
    }

    /// 获取指定名称的模型配置
    pub fn get_model(&self, name: &str) -> Option<&ModelConfig> {
        self.models.get(name)
    }

    /// 获取默认模型配置
    pub fn get_default_model(&self) -> Option<&ModelConfig> {
        self.models.get(&self.default_model)
    }

    /// 按分类获取模型配置列表
    pub fn get_models_by_category(&self, category: ModelCategory) -> Vec<&ModelConfig> {
        self.models
            .values()
            .filter(|m| m.category == category)
            .collect()
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut models = HashMap::new();
        
        // 默认 LLM 配置
        models.insert(
            "default".to_string(),
            ModelConfig {
                name: "default".to_string(),
                category: ModelCategory::Llm,
                provider: "openai-compatible".to_string(),
                api_url: "http://localhost:11434/v1".to_string(),
                api_key: None,
                model: "gpt-4o-mini".to_string(),
                max_tokens: 4096,
                temperature: 0.7,
            },
        );

        Self {
            models,
            default_model: "default".to_string(),
            workspace_dir: std::env::current_dir().unwrap_or_else(|_| ".".into()),
            log_level: "info".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.models.contains_key("default"));
        assert_eq!(config.default_model, "default");
        
        let default_model = config.get_default_model().unwrap();
        assert_eq!(default_model.category, ModelCategory::Llm);
        assert_eq!(default_model.name, "default");
    }

    #[test]
    fn test_model_categories() {
        let mut config = Config::default();
        
        // 添加 VLM 配置
        config.models.insert(
            "vlm".to_string(),
            ModelConfig {
                name: "vlm".to_string(),
                category: ModelCategory::Vlm,
                provider: "openai".to_string(),
                api_url: "https://api.openai.com/v1".to_string(),
                api_key: Some("sk-xxx".to_string()),
                model: "gpt-4o".to_string(),
                max_tokens: 4096,
                temperature: 0.7,
            },
        );

        let vlms = config.get_models_by_category(ModelCategory::Vlm);
        assert_eq!(vlms.len(), 1);
        assert_eq!(vlms[0].name, "vlm");
    }
}
