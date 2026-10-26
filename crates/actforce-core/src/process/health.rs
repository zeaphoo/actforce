use std::time::{Duration, Instant};
use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;

/// Health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_ready: bool,
    pub is_running: bool,
    pub message: String,
    pub last_check: chrono::DateTime<Utc>,
    pub response_time_ms: Option<f64>,
    pub error: Option<String>,
}

/// LLM Health checker
#[derive(Debug, Clone)]
pub struct LLMHealthChecker {
    base_url: String,
    client: Client,
    check_interval: Duration,
    http_timeout: Duration,
}

/// OpenAI compatible models response
#[derive(Debug, Clone, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelData>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModelData {
    id: String,
}

impl LLMHealthChecker {
    /// Create a new health checker
    pub fn new(host: &str, port: u16, check_interval: f64) -> Self {
        let base_url = format!("http://{}:{}", host, port);
        let timeout = Duration::from_secs(10);
        Self {
            base_url,
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_default(),
            check_interval: Duration::from_secs_f64(check_interval),
            http_timeout: timeout,
        }
    }

    /// Create a new health checker with custom HTTP timeout
    pub fn new_with_timeout(host: &str, port: u16, check_interval: f64, http_timeout_secs: f64) -> Self {
        let base_url = format!("http://{}:{}", host, port);
        let timeout = Duration::from_secs_f64(http_timeout_secs);
        Self {
            base_url,
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_default(),
            check_interval: Duration::from_secs_f64(check_interval),
            http_timeout: timeout,
        }
    }
    
    /// Create with base URL
    pub fn with_base_url(base_url: &str, check_interval: f64) -> Self {
        let timeout = Duration::from_secs(10);
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_default(),
            check_interval: Duration::from_secs_f64(check_interval),
            http_timeout: timeout,
        }
    }
    
    /// Check health status
    pub async fn check_health(&self) -> HealthStatus {
        let start = Instant::now();
        let now = Utc::now();
        
        let endpoints = [
            format!("{}/v1/models", self.base_url),
            format!("{}/health", self.base_url),
        ];
        
        let mut last_error = None;
        
        for endpoint in &endpoints {
            match self.client.get(endpoint).send().await {
                Ok(response) => {
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    
                    if response.status().is_success() {
                        return HealthStatus {
                            is_ready: true,
                            is_running: true,
                            message: "LLM server is running normally".to_string(),
                            last_check: now,
                            response_time_ms: Some(elapsed),
                            error: None,
                        };
                    }
                }
                Err(e) => {
                    last_error = Some(e.to_string());
                }
            }
        }
        
        // All endpoints failed
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        // Check if it's a connection error (server not running)
        let is_connect_error = last_error.as_ref()
            .map(|e| e.contains("connection") || e.contains("Connect"))
            .unwrap_or(false);
        
        HealthStatus {
            is_ready: false,
            is_running: !is_connect_error,
            message: "LLM server health check failed".to_string(),
            last_check: now,
            response_time_ms: Some(elapsed),
            error: last_error,
        }
    }
    
    /// Wait for server to be ready
    pub async fn wait_for_ready(&self, timeout_secs: f64) -> bool {
        self.wait_for_ready_with_interval(timeout_secs, None).await
    }

    /// Wait for server to be ready with custom poll interval
    pub async fn wait_for_ready_with_interval(&self, timeout_secs: f64, poll_interval: Option<f64>) -> bool {
        let start = Instant::now();
        let timeout = Duration::from_secs_f64(timeout_secs);
        let interval = poll_interval
            .map(Duration::from_secs_f64)
            .unwrap_or(self.check_interval);
        
        loop {
            // 先等待间隔，给进程留出启动时间
            tokio::time::sleep(interval).await;
            
            let status = self.check_health().await;
            
            if status.is_ready {
                tracing::info!(
                    "LLM server is ready, response time: {:.2}ms",
                    status.response_time_ms.unwrap_or(0.0)
                );
                return true;
            }
            
            if start.elapsed() >= timeout {
                tracing::warn!("Waiting for LLM server ready timed out ({:.0}s)", timeout_secs);
                return false;
            }
            
            tracing::debug!("LLM server not ready, retrying in {:?}...", interval);
        }
    }
    
    /// Get check interval
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }
}
