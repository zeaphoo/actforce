use std::env;
use std::collections::HashMap;
use tracing::{info, warn, error};
use crate::process::{ProcessManager, InferenceProcessConfig, InferenceEngineType};
use std::sync::Arc;
use crate::config::CONFIG;

/// Auto-start inference process based on environment configuration
pub async fn maybe_start_inference_process(pm: Arc<ProcessManager>) {
    let auto_start = env::var("INFERENCE_AUTO_START")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);
    
    if !auto_start {
        info!("Inference auto-start is disabled");
        return;
    }
    
    let model_name = match env::var("INFERENCE_MODEL_NAME") {
        Ok(name) if !name.is_empty() => name,
        _ => {
            warn!("INFERENCE_MODEL_NAME not set, cannot auto-start inference process");
            return;
        }
    };
    
    // 读取引擎类型
    let engine_type_str = env::var("INFERENCE_ENGINE_TYPE")
        .unwrap_or_else(|_| "llamacpp".to_string())
        .to_lowercase();
    
    let engine_type = match engine_type_str.as_str() {
        "llamacpp" => InferenceEngineType::LlamaCpp,
        "vllm" => InferenceEngineType::Vllm,
        _ => {
            warn!("Unsupported engine type: {}, using llamacpp", engine_type_str);
            InferenceEngineType::LlamaCpp
        }
    };
    
    let config = InferenceProcessConfig {
        engine_type,
        model_name,
        executable_path: if engine_type == InferenceEngineType::LlamaCpp {
            env::var("INFERENCE_EXECUTABLE_PATH").ok()
        } else {
            None
        },
        mmproj_path: env::var("INFERENCE_MMPROJ_PATH").ok(),
        venv_path: env::var("VLLM_VENV_PATH").ok(),
        host: CONFIG.inference_host.clone(),
        port: CONFIG.inference_port,
        additional_args: if engine_type == InferenceEngineType::LlamaCpp {
            env::var("INFERENCE_ADDITIONAL_ARGS")
                .ok()
                .map(|args| args.split_whitespace().map(String::from).collect())
                .unwrap_or_default()
        } else {
            env::var("INFERENCE_ADDITIONAL_ARGS_VLLM")
                .ok()
                .map(|args| args.split_whitespace().map(String::from).collect())
                .unwrap_or_default()
        },
        env_vars: HashMap::new(),
        max_restarts: env::var("INFERENCE_MAX_RESTARTS")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5),
        restart_delay: env::var("INFERENCE_RESTART_DELAY")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3.0),
        health_check_interval: env::var("INFERENCE_HEALTH_CHECK_INTERVAL")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(20.0),
        // 启动轮询间隔：llamacpp 启动快用 3s，vLLM 启动慢用 15s
        startup_poll_interval: if engine_type == InferenceEngineType::LlamaCpp { 3.0 } else { 15.0 },
    };
    
    let venv_info = if engine_type == InferenceEngineType::Vllm {
        format!(" (venv: {})", config.venv_path.as_deref().unwrap_or("system"))
    } else {
        String::new()
    };
    
    info!("Auto-starting inference process: {} ({}){}", engine_type, config.model_name, venv_info);
    
    match pm.start(config).await {
        Ok(_) => {
            let state = pm.state().await;
            info!("Inference process started (state: {}), waiting for readiness in background...", state);
        }
        Err(e) => {
            error!("Failed to auto-start inference process: {}", e);
        }
    }
}
