use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tokio::time::sleep;
use tracing::{info, warn, error};

use super::health::LLMHealthChecker;
use super::types::{InferenceProcessConfig, InferenceEngineType, InferenceProcessStatus, ProcessState};

/// Shared process manager
pub type SharedProcessManager = Arc<ProcessManager>;

/// Inference process manager
#[derive(Debug)]
pub struct ProcessManager {
    inner: Arc<ProcessManagerInner>,
}

#[derive(Debug)]
struct ProcessManagerInner {
    process: RwLock<Option<Child>>,
    config: RwLock<Option<InferenceProcessConfig>>,
    state: RwLock<ProcessState>,
    restart_count: RwLock<u32>,
    last_error: RwLock<Option<String>>,
    start_time: RwLock<Option<chrono::DateTime<Utc>>>,
}

impl ProcessManager {
    /// Create a new process manager
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ProcessManagerInner {
                process: RwLock::new(None),
                config: RwLock::new(None),
                state: RwLock::new(ProcessState::Stopped),
                restart_count: RwLock::new(0),
                last_error: RwLock::new(None),
                start_time: RwLock::new(None),
            }),
        }
    }
    
    /// Get current state
    pub async fn state(&self) -> ProcessState {
        *self.inner.state.read().await
    }
    
    /// Check if process is alive (Starting, WarmingUp, or Running)
    pub async fn is_running(&self) -> bool {
        matches!(self.state().await, ProcessState::Starting | ProcessState::WarmingUp | ProcessState::Running)
    }

    /// Check if inference service is ready to accept requests
    pub async fn is_ready(&self) -> bool {
        matches!(self.state().await, ProcessState::Running)
    }
    
    /// Get restart count
    pub async fn restart_count(&self) -> u32 {
        *self.inner.restart_count.read().await
    }
    
    /// Get last error
    pub async fn last_error(&self) -> Option<String> {
        self.inner.last_error.read().await.clone()
    }
    
    /// Get status
    pub async fn get_status(&self) -> InferenceProcessStatus {
        let state = self.state().await;
        let config = self.inner.config.read().await.clone();
        let process = self.inner.process.read().await;
        let restart_count = self.restart_count().await;
        let last_error = self.last_error().await;
        let start_time = self.inner.start_time.read().await.as_ref().map(|t| t.to_rfc3339());
        
        let is_ready = matches!(state, ProcessState::Running);

        InferenceProcessStatus {
            state: state.to_string(),
            is_ready,
            engine_type: config.as_ref().map(|c| c.engine_type.to_string()),
            model_name: config.as_ref().map(|c| c.model_name.clone()),
            mmproj_path: config.as_ref().and_then(|c| c.mmproj_path.clone()),
            venv_path: config.as_ref().and_then(|c| c.venv_path.clone()),
            host: config.as_ref().map(|c| c.host.clone()),
            port: config.as_ref().map(|c| c.port),
            pid: process.as_ref().and_then(|p| p.id()),
            restart_count,
            start_time,
            last_error,
        }
    }
    
    /// Start the inference process
    pub async fn start(&self, config: InferenceProcessConfig) -> anyhow::Result<()> {
        if self.is_running().await {
            info!("Inference process is already running");
            return Ok(());
        }
        
        // Update state
        *self.inner.state.write().await = ProcessState::Starting;
        *self.inner.last_error.write().await = None;
        
        info!("Starting {} inference process", config.engine_type);
        
        // Build command
        let cmd = self.build_command(&config)?;
        info!("Executing command: {}", cmd.join(" "));
        
        // Prepare environment
        let mut envs: HashMap<String, String> = std::env::vars().collect();
        envs.extend(config.env_vars.clone());
        
        // Start process
        let mut child = Command::new(&cmd[0])
            .args(&cmd[1..])
            .envs(&envs)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        
        info!("Inference process started, PID: {:?}", child.id());
        
        // Start output streaming if debug mode is enabled
        let debug_mode = std::env::var("ACTFORCE_DEBUG")
            .map(|v| v == "true" || v == "1")
            .unwrap_or(false);
        
        if debug_mode {
            // Capture stdout
            if let Some(stdout) = child.stdout.take() {
                tokio::spawn(async move {
                    use tokio::io::AsyncBufReadExt;
                    let reader = tokio::io::BufReader::new(stdout);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::info!(target: "llama_server::stdout", "{}", line);
                    }
                });
            }
            
            // Capture stderr
            if let Some(stderr) = child.stderr.take() {
                tokio::spawn(async move {
                    use tokio::io::AsyncBufReadExt;
                    let reader = tokio::io::BufReader::new(stderr);
                    let mut lines = reader.lines();
                    while let Ok(Some(line)) = lines.next_line().await {
                        tracing::warn!(target: "llama_server::stderr", "{}", line);
                    }
                });
            }
        }
        
        // Store process and config
        *self.inner.process.write().await = Some(child);
        *self.inner.config.write().await = Some(config.clone());
        *self.inner.state.write().await = ProcessState::WarmingUp;
        *self.inner.restart_count.write().await = 0;
        *self.inner.start_time.write().await = Some(Utc::now());
        
        // Start monitor tasks (传入 manager 以便崩溃后真正调用 restart 自动重启)
        let inner = self.inner.clone();
        let manager = Arc::new(ProcessManager { inner: inner.clone() });
        tokio::spawn(monitor_process(manager));
        
        // Start health check loop (periodic, uses config interval)
        let hc_checker = LLMHealthChecker::new_with_timeout(
            &config.host, config.port, config.health_check_interval, 10.0,
        );
        tokio::spawn(health_check_loop(inner.clone(), hc_checker, config.health_check_interval));

        // Background: wait for readiness with engine-appropriate polling interval
        let ready_checker = LLMHealthChecker::new_with_timeout(
            &config.host, config.port, config.startup_poll_interval, 10.0,
        );
        let ready_timeout = std::env::var("INFERENCE_READY_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(600.0);
        tokio::spawn(wait_until_ready(inner.clone(), ready_checker, ready_timeout));
        
        Ok(())
    }
    
    /// Stop the inference process
    pub async fn stop(&self) -> anyhow::Result<()> {
        let mut process = self.inner.process.write().await;
        
        if let Some(mut child) = process.take() {
            info!("Stopping inference process (PID: {:?})", child.id());
            
            // Try graceful shutdown
            let _ = child.start_kill();
            
            // Wait for process to exit
            match tokio::time::timeout(Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => {
                    info!("Process exited with status: {}", status);
                }
                _ => {
                    warn!("Process did not exit gracefully, killing...");
                    let _ = child.kill().await;
                }
            }
        }
        
        *self.inner.state.write().await = ProcessState::Stopped;
        info!("Inference process stopped");
        
        Ok(())
    }
    
    /// Restart the inference process
    pub async fn restart(&self) -> anyhow::Result<()> {
        let config = self.inner.config.read().await.clone();
        
        if let Some(config) = config {
            *self.inner.state.write().await = ProcessState::Restarting;
            
            self.stop().await?;
            sleep(Duration::from_secs_f64(config.restart_delay)).await;
            
            self.start(config).await?;
            info!("Inference process restarted successfully");
        } else {
            error!("No configuration available, cannot restart");
            return Err(anyhow::anyhow!("No configuration available"));
        }
        
        Ok(())
    }
    
    /// Build command for the process
    fn build_command(&self, config: &InferenceProcessConfig) -> anyhow::Result<Vec<String>> {
        // vLLM engine automatically uses Docker mode
        if config.engine_type == InferenceEngineType::Vllm {
            return self.build_docker_command(config);
        }
        
        // llamaCpp uses regular executable
        let executable = config.executable_path.clone()
            .or_else(|| get_default_executable(config.engine_type))
            .ok_or_else(|| anyhow::anyhow!("No executable path provided"))?;
        
        let mut cmd = vec![executable];
        
        match config.engine_type {
            InferenceEngineType::LlamaCpp => {
                self.build_llamacpp_command(config, &mut cmd)?;
            }
            InferenceEngineType::Vllm => {
                // Already handled above
            }
        }
        
        Ok(cmd)
    }
    
    /// Build LlamaCpp command
    fn build_llamacpp_command(&self, config: &InferenceProcessConfig, cmd: &mut Vec<String>) -> anyhow::Result<()> {
        // Add model path
        let model_path = if std::path::Path::new(&config.model_name).is_absolute() {
            config.model_name.clone()
        } else {
            // Get models dir from env or use default
            let data_root = std::env::var("ACTFORCE_DATA_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("./data/storage/"));
            let models_dir = data_root.join("models");
            models_dir.join(&config.model_name).to_string_lossy().to_string()
        };
        
        // Check for Qwen model before moving model_path
        let model_name_lower = model_path.to_lowercase();
        let has_chat_template_kwargs = config.additional_args.iter().any(|arg| arg.contains("--chat-template-kwargs"));
        
        cmd.push("-m".to_string());
        cmd.push(model_path);
        
        // Add host and port
        cmd.push("--host".to_string());
        cmd.push(config.host.clone());
        cmd.push("--port".to_string());
        cmd.push(config.port.to_string());
        
        // Add multimodal projector if provided (for LLaVA, etc.)
        if let Some(mmproj_path) = &config.mmproj_path {
            let resolved_path = if std::path::Path::new(mmproj_path).is_absolute() {
                mmproj_path.clone()
            } else {
                let data_root = std::env::var("ACTFORCE_DATA_ROOT")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::path::PathBuf::from("./data/storage/"));
                let models_dir = data_root.join("models");
                models_dir.join(mmproj_path).to_string_lossy().to_string()
            };
            cmd.push("--mmproj".to_string());
            cmd.push(resolved_path);
        }
        
        // Auto-disable thinking mode for Qwen models
        if model_name_lower.contains("qwen") && !has_chat_template_kwargs {
            info!("Detected Qwen model, auto-disabling thinking mode");
            cmd.push("--chat-template-kwargs".to_string());
            cmd.push(r#"{"enable_thinking": false}"#.to_string());
        }
        
        // Add additional arguments
        cmd.extend(config.additional_args.clone());
        
        Ok(())
    }
    
    /// Build Docker command (for vLLM)
    fn build_docker_command(&self, config: &InferenceProcessConfig) -> anyhow::Result<Vec<String>> {
        // 本地运行 vLLM
        let vllm_bin = if let Some(venv) = &config.venv_path {
            format!("{}/bin/vllm", venv)
        } else {
            "vllm".to_string()
        };
        
        let mut model_name = config.model_name.clone();
        if !model_name.starts_with("/") {
            let data_root = std::env::var("ACTFORCE_DATA_ROOT")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("./data/storage/"));
            let models_dir = data_root.join("models");
            model_name = models_dir.join(&model_name).to_string_lossy().to_string();
        }
        
        let mut cmd = vec![
            vllm_bin, "serve".to_string(),
            "--model".to_string(), model_name.clone(),
            "--served-model-name".to_string(), config.model_name.clone(),  // 保持 API 兼容性
            "--host".to_string(), config.host.clone(),
            "--port".to_string(), config.port.to_string(),
        ];
        cmd.extend(config.additional_args.clone());
        Ok(cmd)
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

fn get_default_executable(engine_type: InferenceEngineType) -> Option<String> {
    match engine_type {
        InferenceEngineType::LlamaCpp => Some("llama-server".to_string()),
        InferenceEngineType::Vllm => Some("vllm".to_string()),
    }
}

/// Monitor the process
async fn monitor_process(manager: Arc<ProcessManager>) {
    let inner = manager.inner.clone();
    loop {
        sleep(Duration::from_secs(1)).await;
        
        let mut process_guard = inner.process.write().await;
        
        if let Some(ref mut process) = *process_guard {
            match process.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code().unwrap_or(-1);
                    warn!("Inference process exited with code: {}", code);
                    
                    let new_state = if code != 0 {
                        ProcessState::Failed
                    } else {
                        ProcessState::Stopped
                    };
                    *inner.state.write().await = new_state;
                    
                    // Handle auto-restart
                    if code != 0 {
                        let restart_count = *inner.restart_count.read().await;
                        let config = inner.config.read().await.clone();
                        
                        if let Some(config) = config {
                            if restart_count < config.max_restarts {
                                drop(process_guard);
                                
                                info!("Preparing to restart process ({}/{})", restart_count + 1, config.max_restarts);
                                *inner.restart_count.write().await = restart_count + 1;
                                *inner.last_error.write().await = Some(format!("Process exited with code: {}", code));
                                
                                sleep(Duration::from_secs_f64(config.restart_delay)).await;
                                
                                // 对齐 agentlite process_manager.py：进程崩溃后真实调用 restart() 自动重启
                                // （agentlite 中 _restart_count 从不递增，实际上无限重启；这里保留计数上限，
                                //   但在未达上限时照 agentlite 逻辑真正重启，而不是只清空进程）。
                                //
                                // 注意：restart() -> start() 内部会 tokio::spawn(monitor_process(...))，而
                                // monitor_process 又 await restart()，形成类型级循环依赖导致 future 非 Send。
                                // 因此这里通过 spawn_blocking + Handle::block_on 在 blocking 线程中执行
                                // restart（block_on 不要求 future Send），打破类型环。
                                let handle = tokio::runtime::Handle::current();
                                let manager2 = manager.clone();
                                let restart_result = tokio::task::spawn_blocking(move || {
                                    handle.block_on(manager2.restart())
                                })
                                .await;
                                match restart_result {
                                    Ok(Ok(())) => {
                                        info!("Inference process restarted successfully");
                                    }
                                    Ok(Err(e)) => {
                                        error!("Failed to restart inference process: {}", e);
                                        *inner.state.write().await = ProcessState::Failed;
                                        *inner.last_error.write().await = Some(format!(
                                            "Process exited (code: {}), restart failed: {}",
                                            code, e
                                        ));
                                    }
                                    Err(e) => {
                                        error!("Restart task failed to run: {}", e);
                                        *inner.state.write().await = ProcessState::Failed;
                                        *inner.last_error.write().await = Some(format!(
                                            "Process exited (code: {}), restart task failed: {}",
                                            code, e
                                        ));
                                    }
                                }
                            } else {
                                error!("Max restarts reached ({}), giving up", config.max_restarts);
                                *inner.last_error.write().await = Some(format!(
                                    "Process exited (code: {}), max restarts reached",
                                    code
                                ));
                            }
                        }
                    }
                    
                    break;
                }
                Ok(None) => {
                    // Process still running
                }
                Err(e) => {
                    error!("Error checking process status: {}", e);
                    break;
                }
            }
        } else {
            break;
        }
    }
}

/// Health check loop
async fn health_check_loop(
    inner: Arc<ProcessManagerInner>,
    checker: LLMHealthChecker,
    interval_secs: f64,
) {
    let interval = Duration::from_secs_f64(interval_secs);
    
    loop {
        sleep(interval).await;
        
        let state = *inner.state.read().await;
        if !matches!(state, ProcessState::Starting | ProcessState::WarmingUp | ProcessState::Running) {
            break;
        }
        
        let status = checker.check_health().await;
        
        if status.is_ready {
            // 健康检查通过，如果还在 WARMING_UP 则切换为 RUNNING
            let mut state_guard = inner.state.write().await;
            if *state_guard == ProcessState::WarmingUp {
                *state_guard = ProcessState::Running;
                info!("Inference service is ready (warmup complete), response time: {:.2}ms",
                    status.response_time_ms.unwrap_or(0.0));
            } else {
                tracing::debug!(
                    "LLM health check passed, response time: {:.2}ms",
                    status.response_time_ms.unwrap_or(0.0)
                );
            }
        } else {
            warn!("LLM health check failed: {}", status.message);
        }
    }
}

/// Background task: wait for inference service to be ready, then transition state
async fn wait_until_ready(
    inner: Arc<ProcessManagerInner>,
    checker: LLMHealthChecker,
    timeout_secs: f64,
) {
    info!("Waiting for inference service to be ready (timeout: {}s)...", timeout_secs);
    let ready = checker.wait_for_ready_with_interval(timeout_secs, Some(3.0)).await;
    if ready {
        let mut state_guard = inner.state.write().await;
        if *state_guard == ProcessState::WarmingUp {
            *state_guard = ProcessState::Running;
            info!("Inference service is ready");
        }
    } else {
        let state = *inner.state.read().await;
        if state == ProcessState::WarmingUp {
            warn!("Inference service not ready within {}s, keeping warming_up state", timeout_secs);
            // 不切到 FAILED，让健康检查循环继续尝试
        }
    }
}
