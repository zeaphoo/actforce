use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json, Router,
};
use serde::Deserialize;
use tracing::{error, info};

use crate::{
    database::{gen_prefix_random_id, DbPool},
    models::{
        QuickPrompt, QuickPromptCreate, QuickPromptListResponse, QuickPromptResponse, QuickPromptUpdate,
    },
};

/// Application state for quick prompt handlers
#[derive(Clone)]
pub struct QuickPromptsState {
    pub db_pool: DbPool,
}

pub fn router() -> Router<QuickPromptsState> {
    Router::new()
        .route("/public", axum::routing::get(get_public_quick_prompts))
        .route("/", axum::routing::get(get_quick_prompts))
        .route("/", axum::routing::post(create_quick_prompt))
        .route("/{id}", axum::routing::put(update_quick_prompt))
        .route("/{id}", axum::routing::delete(delete_quick_prompt))
}

// ============== Query Parameters ==============

#[derive(Deserialize)]
pub struct GetQuickPromptsQuery {
    #[serde(default)]
    pub include_disabled: bool,
}

// ============== Handlers ==============

/// Get public quick prompts (no authentication required)
/// Only returns enabled prompts, sorted by sort_order
async fn get_public_quick_prompts(
    State(state): State<QuickPromptsState>,
) -> Result<Json<QuickPromptListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let prompts = sqlx::query_as::<_, QuickPrompt>(
        "SELECT * FROM quick_prompts WHERE is_enabled = 1 ORDER BY sort_order, created_at"
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| {
        error!("Failed to fetch public quick prompts: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
        )
    })?;

    let total = prompts.len();
    let prompts: Vec<QuickPromptResponse> = prompts.into_iter().map(|p| p.into()).collect();

    Ok(Json(QuickPromptListResponse { prompts, total }))
}

/// Get all quick prompts (admin only)
async fn get_quick_prompts(
    Query(query): Query<GetQuickPromptsQuery>,
    State(state): State<QuickPromptsState>,
) -> Result<Json<QuickPromptListResponse>, (StatusCode, Json<serde_json::Value>)> {
    let query_str = if query.include_disabled {
        "SELECT * FROM quick_prompts ORDER BY sort_order, created_at"
    } else {
        "SELECT * FROM quick_prompts WHERE is_enabled = 1 ORDER BY sort_order, created_at"
    };

    let prompts = sqlx::query_as::<_, QuickPrompt>(query_str)
        .fetch_all(&state.db_pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch quick prompts: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
            )
        })?;

    let total = prompts.len();
    let prompts: Vec<QuickPromptResponse> = prompts.into_iter().map(|p| p.into()).collect();

    Ok(Json(QuickPromptListResponse { prompts, total }))
}

/// Create a new quick prompt (admin only)
async fn create_quick_prompt(
    State(state): State<QuickPromptsState>,
    Json(req): Json<QuickPromptCreate>,
) -> Result<Json<QuickPromptResponse>, (StatusCode, Json<serde_json::Value>)> {
    // 与 agentlite models.py 一致：id = "qp" + 8 位随机数字（避免同毫秒时间戳撞主键）
    let id = gen_prefix_random_id("qp", 8);
    let now = chrono::Utc::now();

    let prompt = sqlx::query_as::<_, QuickPrompt>(
        "INSERT INTO quick_prompts (id, title, description, prompt, icon, category, sort_order, is_enabled, created_at, updated_at) 
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) 
         RETURNING *"
    )
    .bind(&id)
    .bind(&req.title)
    .bind(&req.description)
    .bind(&req.prompt)
    .bind(&req.icon)
    .bind(&req.category)
    .bind(req.sort_order)
    .bind(req.is_enabled)
    .bind(&now)
    .bind(&now)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        error!("Failed to create quick prompt: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
        )
    })?;

    info!("Quick prompt created: {} ({})", prompt.title, prompt.id);
    Ok(Json(QuickPromptResponse::from(prompt)))
}

/// Update a quick prompt (admin only)
async fn update_quick_prompt(
    Path(id): Path<String>,
    State(state): State<QuickPromptsState>,
    Json(req): Json<QuickPromptUpdate>,
) -> Result<Json<QuickPromptResponse>, (StatusCode, Json<serde_json::Value>)> {
    let _now = chrono::Utc::now();

    // Check if prompt exists
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM quick_prompts WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| {
        error!("Failed to check quick prompt existence: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
        )
    })?;

    if existing == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "detail": "Quick prompt not found" })),
        ));
    }

    // Update fields individually
    let now = chrono::Utc::now();
    let mut updated_any = false;
    
    if let Some(title) = &req.title {
        sqlx::query("UPDATE quick_prompts SET title = ?, updated_at = ? WHERE id = ?")
            .bind(title)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt title: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(description) = &req.description {
        sqlx::query("UPDATE quick_prompts SET description = ?, updated_at = ? WHERE id = ?")
            .bind(description)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await.map_err(|e| {
                error!("Failed to update quick prompt description: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(prompt_text) = &req.prompt {
        sqlx::query("UPDATE quick_prompts SET prompt = ?, updated_at = ? WHERE id = ?")
            .bind(prompt_text)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(icon) = &req.icon {
        sqlx::query("UPDATE quick_prompts SET icon = ?, updated_at = ? WHERE id = ?")
            .bind(icon)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt icon: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(category) = &req.category {
        sqlx::query("UPDATE quick_prompts SET category = ?, updated_at = ? WHERE id = ?")
            .bind(category)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt category: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(sort_order) = req.sort_order {
        sqlx::query("UPDATE quick_prompts SET sort_order = ?, updated_at = ? WHERE id = ?")
            .bind(sort_order)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt sort_order: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    if let Some(is_enabled) = req.is_enabled {
        sqlx::query("UPDATE quick_prompts SET is_enabled = ?, updated_at = ? WHERE id = ?")
            .bind(is_enabled)
            .bind(&now)
            .bind(&id)
            .execute(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to update quick prompt is_enabled: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        updated_any = true;
    }

    // If no fields were updated, just return existing
    if !updated_any {
        let prompt = sqlx::query_as::<_, QuickPrompt>("SELECT * FROM quick_prompts WHERE id = ?")
            .bind(&id)
            .fetch_one(&state.db_pool)
            .await
            .map_err(|e| {
                error!("Failed to fetch quick prompt: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
                )
            })?;
        return Ok(Json(QuickPromptResponse::from(prompt)));
    }

    // Fetch updated prompt
    let prompt = sqlx::query_as::<_, QuickPrompt>("SELECT * FROM quick_prompts WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db_pool)
        .await
        .map_err(|e| {
            error!("Failed to fetch updated quick prompt: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
            )
        })?;

    info!("Quick prompt updated: {} ({})", prompt.title, prompt.id);
    Ok(Json(QuickPromptResponse::from(prompt)))
}

/// Delete a quick prompt (admin only)
async fn delete_quick_prompt(
    Path(id): Path<String>,
    State(state): State<QuickPromptsState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let result = sqlx::query("DELETE FROM quick_prompts WHERE id = ?")
        .bind(&id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| {
            error!("Failed to delete quick prompt: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "detail": format!("Database error: {}", e) })),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "detail": "Quick prompt not found" })),
        ));
    }

    info!("Quick prompt deleted: {}", id);
    Ok(Json(serde_json::json!({ "success": true, "message": "Quick prompt deleted" })))
}
