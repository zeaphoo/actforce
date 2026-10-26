use axum::{extract::State, http::StatusCode, response::Json as AxumJson, routing::{get, post, put}, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::{SqlitePool, Row};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use agent_core::ChatLoopManager;

use crate::config::VERSION;

/// Router state: DB pool + runtime knobs that settings updates must reach.
#[derive(Clone)]
pub struct AppConfigState {
    pub db_pool: SqlitePool,
    pub loop_manager: Arc<ChatLoopManager>,
}

/// Apply a runtime-affecting setting immediately (mirrors agentlite api/config.py).
async fn apply_runtime_setting(
    loop_manager: &Arc<ChatLoopManager>,
    key: &str,
    value: &serde_json::Value,
) {
    let as_i64 = || value.as_i64().or_else(|| value.as_u64().map(|v| v as i64));
    match key {
        "max_concurrent_sessions" => {
            loop_manager.update_max_concurrent_sessions(as_i64().filter(|v| *v > 0).map(|v| v as usize));
        }
        "global_llm_requests" => {
            match as_i64().filter(|v| *v > 0) {
                Some(n) => agent_core::core::set_global_llm_limit(n as usize).await,
                // agentlite parity: invalid/<=0 resets to the default (20)
                None => agent_core::core::set_global_llm_limit(20).await,
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 产品名称
    pub product_name: String,
    /// 版本号
    pub version: String,
    /// 发布日期
    pub release_date: String,
    
    /// 是否允许匿名使用
    pub allow_anonymous: bool,
    
    /// UI 配置
    pub ui: HashMap<String, serde_json::Value>,
}

fn get_bool_env(key: &str, default: bool) -> bool {
    env::var(key)
        .map(|v| v.to_lowercase() == "true" || v == "1" || v.to_lowercase() == "yes")
        .unwrap_or(default)
}

fn get_string_env(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

/// 从数据库获取系统设置值（用于 app config）
async fn get_setting_value(pool: &SqlitePool, key: &str, default: bool) -> bool {
    let result = sqlx::query("SELECT value_json FROM system_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await;

    match result {
        Ok(Some(row)) => {
            let value_json: String = row.get("value_json");
            serde_json::from_str(&value_json).unwrap_or(default)
        }
        _ => default,
    }
}

// --- System Settings API ---

#[derive(Debug, Serialize)]
struct SystemSettingResponse {
    key: String,
    value: serde_json::Value,
    updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct SystemSettingsListResponse {
    settings: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct UpdateSystemSettingRequest {
    key: String,
    value: serde_json::Value,
}

/// Get setting value as JSON Value from database
async fn get_setting_value_json(pool: &SqlitePool, key: &str) -> Option<(serde_json::Value, Option<String>)> {
    let result = sqlx::query("SELECT value_json, updated_at FROM system_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await;

    match result {
        Ok(Some(row)) => {
            let value_json: String = row.get("value_json");
            let updated_at: Option<String> = row.get("updated_at");
            let value: serde_json::Value = serde_json::from_str(&value_json).ok()?;
            Some((value, updated_at))
        }
        _ => None,
    }
}

/// Set setting value in database
async fn set_setting_value_json(pool: &SqlitePool, key: &str, value: &serde_json::Value) -> anyhow::Result<Option<String>> {
    let now = chrono::Utc::now().to_rfc3339();
    let value_str = serde_json::to_string(value)?;

    let exists = sqlx::query("SELECT 1 FROM system_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;

    if exists.is_some() {
        sqlx::query("UPDATE system_settings SET value_json = ?, updated_at = ? WHERE key = ?")
            .bind(&value_str)
            .bind(&now)
            .bind(key)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO system_settings (key, value_json, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(key)
            .bind(&value_str)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
    }

    Ok(Some(now))
}

/// Settings keys exposed to the frontend admin UI
const EXPOSED_SETTINGS: &[&str] = &[
    "allow_anonymous",
    "conversation_history_retention_days",
    "max_concurrent_sessions",
    "global_llm_requests",
];

/// GET /api/config/settings - Get system settings exposed to admin UI
async fn get_system_settings(
    State(state): State<AppConfigState>,
) -> AxumJson<SystemSettingsListResponse> {
    let result = sqlx::query("SELECT key, value_json, updated_at, updated_by FROM system_settings")
        .fetch_all(&state.db_pool)
        .await;

    let mut settings = HashMap::new();
    if let Ok(rows) = result {
        for row in rows {
            let key: String = row.get("key");
            if !EXPOSED_SETTINGS.contains(&key.as_str()) {
                continue;
            }
            let value_json: String = row.get("value_json");
            let updated_at: Option<String> = row.get("updated_at");
            let updated_by: Option<String> = row.get("updated_by");
            let value: serde_json::Value = serde_json::from_str(&value_json).unwrap_or(serde_json::Value::Null);
            // agentlite parity: include updated_by (null, updates never set it)
            settings.insert(key, serde_json::json!({
                "value": value,
                "updated_at": updated_at,
                "updated_by": updated_by,
            }));
        }
    }

    AxumJson(SystemSettingsListResponse { settings })
}

/// GET /api/config/settings/{key} - Get a single system setting
async fn get_system_setting(
    State(state): State<AppConfigState>,
    axum::extract::Path(key): axum::extract::Path<String>,
) -> Result<AxumJson<SystemSettingResponse>, (StatusCode, AxumJson<serde_json::Value>)> {
    match get_setting_value_json(&state.db_pool, &key).await {
        Some((value, updated_at)) => Ok(AxumJson(SystemSettingResponse {
            key,
            value,
            updated_at,
        })),
        None => Err((
            StatusCode::NOT_FOUND,
            AxumJson(serde_json::json!({"detail": format!("Setting '{}' not found", key)})),
        )),
    }
}

/// PUT /api/config/settings/{key} - Update a system setting
async fn update_system_setting(
    State(state): State<AppConfigState>,
    axum::extract::Path(key): axum::extract::Path<String>,
    AxumJson(request): AxumJson<UpdateSystemSettingRequest>,
) -> Result<AxumJson<SystemSettingResponse>, (StatusCode, AxumJson<serde_json::Value>)> {
    let updated_at = set_setting_value_json(&state.db_pool, &key, &request.value)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            AxumJson(serde_json::json!({"detail": format!("Failed to update setting: {}", e)})),
        ))?;

    // agentlite parity: single-key PUT only writes the DB; runtime updates
    // happen via the batch endpoint.
    Ok(AxumJson(SystemSettingResponse {
        key,
        value: request.value,
        updated_at,
    }))
}

/// POST /api/config/settings/batch - Batch update system settings
async fn batch_update_system_settings(
    State(state): State<AppConfigState>,
    AxumJson(settings): AxumJson<HashMap<String, serde_json::Value>>,
) -> AxumJson<serde_json::Value> {
    let mut updated = Vec::new();
    for (key, value) in &settings {
        if let Ok(updated_at) = set_setting_value_json(&state.db_pool, key, value).await {
            updated.push(serde_json::json!({
                "key": key,
                "value": value,
                "updated_at": updated_at,
            }));
        }
    }

    for (key, value) in &settings {
        apply_runtime_setting(&state.loop_manager, key, value).await;
    }

    AxumJson(serde_json::json!({ "success": true, "updated": updated }))
}

/// GET /api/config/app - 获取应用公开配置（无需认证）
async fn get_app_config(
    State(state): State<AppConfigState>,
) -> Json<AppConfig> {
    let mut ui = HashMap::new();
    ui.insert("theme".to_string(), serde_json::json!(get_string_env("UI_THEME", "light")));
    ui.insert("logo_url".to_string(), serde_json::json!(get_string_env("UI_LOGO_URL", "")));
    ui.insert("favicon_url".to_string(), serde_json::json!(get_string_env("UI_FAVICON_URL", "")));
    ui.insert("title".to_string(), serde_json::json!(get_string_env("UI_TITLE", "轻元智能体")));
    ui.insert("footer_text".to_string(), serde_json::json!(get_string_env("UI_FOOTER_TEXT", "")));
    ui.insert("show_version".to_string(), serde_json::json!(get_bool_env("UI_SHOW_VERSION", true)));

    // 从数据库读取 allow_anonymous 配置
    let allow_anonymous = get_setting_value(&state.db_pool, "allow_anonymous", true).await;

    Json(AppConfig {
        product_name: get_string_env("APP_PRODUCT_NAME", "轻元智能体"),
        version: VERSION.to_string(),
        release_date: "2025-04-03".to_string(),
        
        allow_anonymous,
        
        ui,
    })
}

pub fn app_config_router() -> Router<AppConfigState> {
    Router::new()
        .route("/app", get(get_app_config))
        .route("/settings", get(get_system_settings))
        .route("/settings/{key}", get(get_system_setting))
        .route("/settings/{key}", put(update_system_setting))
        .route("/settings/batch", post(batch_update_system_settings))
}
