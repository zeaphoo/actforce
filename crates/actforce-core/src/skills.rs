use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Sqlite};

use crate::database::{gen_prefix_random_id, DbPool};

use jieba_rs::Jieba;
use once_cell::sync::Lazy;

/// Global Jieba instance for tokenization
static JIEBA: Lazy<Jieba> = Lazy::new(|| Jieba::new());

// ============== DTOs ==============

#[derive(Debug, Clone, Deserialize)]
pub struct SkillCreate {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub extra_json: Option<Value>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub updated_by: Option<String>,
}

fn default_true() -> bool {
    true
}

/// 区分"字段未传"与"显式传 null"：
/// - 字段缺失                -> None
/// - 显式传 null             -> Some(None)
/// - 传了实际值 v            -> Some(Some(v))
///
/// 与 agentlite 的 `payload.model_dump(exclude_unset=True)` 语义一致：
/// 只有请求里显式出现的 key 才会被更新，显式 null 表示把字段清空。
/// 注意：serde 默认的 `Option<Option<T>>` 在此 serde_json 版本下无法区分
/// "缺失" 与 "null"（两者都反序列化为 None），因此需要自定义 deserializer。
fn deserialize_present_field<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    struct PresentField<T>(std::marker::PhantomData<T>);

    impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for PresentField<T> {
        type Value = Option<Option<T>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a value, null, or an absent field")
        }

        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Some(None))
        }

        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Some(None))
        }

        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            T::deserialize(deserializer).map(|v| Some(Some(v)))
        }
    }

    deserializer.deserialize_option(PresentField(std::marker::PhantomData))
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillUpdate {
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub description: Option<Option<String>>,
    #[serde(default)]
    pub is_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub content: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub program: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub extra_json: Option<Option<Value>>,
    #[serde(default, deserialize_with = "deserialize_present_field")]
    pub updated_by: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct SkillRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    // 允许 builtin 列表查询不 SELECT content 列（轻量级响应，与 agentlite 一致）
    #[sqlx(default)]
    pub content: Option<String>,
    pub program: Option<String>,
    pub extra_json: Option<String>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub is_enabled: bool,
    pub content: Option<String>,
    pub program: Option<String>,
    pub extra_json: Option<Value>,
    pub created_by: Option<String>,
    pub updated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<SkillRow> for SkillResponse {
    fn from(row: SkillRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            description: row.description,
            is_enabled: row.is_enabled,
            content: row.content,
            program: row.program,
            extra_json: row.extra_json.and_then(|s| serde_json::from_str(&s).ok()),
            created_by: row.created_by,
            updated_by: row.updated_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillSearchResponse {
    #[serde(flatten)]
    pub skill: SkillResponse,
    pub rank: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_limit() -> i32 {
    100
}

#[derive(Debug, Clone, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    #[serde(default = "default_search_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_search_limit() -> i32 {
    20
}

// ============== State ==============

#[derive(Clone)]
pub struct SkillsState {
    /// Main database pool (for skills table in main db)
    pub db_pool: DbPool,
    /// User skills index database (disk, for FTS on user-edited skills)
    pub skills_db_pool: Pool<Sqlite>,
    /// Builtin skills database (in-memory, recreated on startup)
    pub builtin_skills_db_pool: Pool<Sqlite>,
}

// ============== FTS Tokenization (using jieba) ==============

/// Tokenize text using jieba for Chinese support (matches Python version)
fn tokenize_text(content: &str) -> String {
    if content.is_empty() {
        return String::new();
    }
    
    // Use cut_for_search for better recall (same as Python jieba.cut_for_search)
    let words: Vec<&str> = JIEBA
        .cut_for_search(content, false)
        .into_iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    
    words.join(" ")
}

/// Build FTS match query from tokenized query
fn build_match_query(query: &str) -> String {
    let tokenized = tokenize_text(query);
    let tokens: Vec<String> = tokenized
        .split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| format!("\"{}\"", s.replace('"', "\"\"")))
        .collect();
    
    if tokens.is_empty() {
        return String::new();
    }
    
    // Use OR to match any token (same as Python version)
    tokens.join(" OR ")
}

// ============== Skills Index Database ==============

/// Initialize skills index schema (separate database for FTS)
pub async fn init_skills_index_schema(pool: &Pool<Sqlite>) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS skills (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT,
            is_enabled INTEGER NOT NULL DEFAULT 1,
            content TEXT,
            program TEXT,
            extra_json TEXT,
            created_by TEXT,
            updated_by TEXT,
            created_at TEXT,
            updated_at TEXT
        )
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name)")
        .execute(pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_skills_is_enabled ON skills(is_enabled)")
        .execute(pool)
        .await?;

    // FTS5 virtual table for full-text search
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(id UNINDEXED, name, description, content)"
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Initialize builtin skills into in-memory database
pub async fn init_builtin_skills(
    pool: &Pool<Sqlite>,
    main_pool: &Pool<Sqlite>,
) -> anyhow::Result<()> {
    // Initialize schema first
    init_skills_index_schema(pool).await?;
    
    // Load builtin skills from embedded .skill.json files (compiled into binary)
    let mut builtin_skills = crate::builtin_skills::get_builtin_skills();
    
    // Merge system settings (apply saved enable states)
    builtin_skills = crate::builtin_skills::merge_builtin_skill_settings(main_pool, builtin_skills).await?;
    
    if builtin_skills.is_empty() {
        tracing::info!("No builtin skills to load");
        return Ok(());
    }
    
    tracing::info!("Loading {} builtin skills into memory database", builtin_skills.len());
    
    for skill in builtin_skills {
        let id = skill.id.unwrap_or_else(|| gen_prefix_random_id("sk", 6));
        let extra_json_str = skill.extra_json.map(|v| serde_json::to_string(&v).unwrap());
        let now = chrono::Utc::now().to_rfc3339();
        
        // Insert into skills table
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO skills (id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&skill.name)
        .bind(&skill.description)
        .bind(skill.is_enabled)
        .bind(&skill.content)
        .bind(&skill.program)
        .bind(&extra_json_str)
        .bind(&skill.created_by)
        .bind(&skill.updated_by)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        
        // Insert into FTS index
        let fts_name = tokenize_text(&skill.name);
        let fts_description = skill.description.as_deref().map(tokenize_text).unwrap_or_default();
        let fts_content = skill.content.as_deref().map(tokenize_text).unwrap_or_default();
        
        sqlx::query("INSERT INTO skills_fts (id, name, description, content) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(&fts_name)
            .bind(&fts_description)
            .bind(&fts_content)
            .execute(pool)
            .await?;
    }
    
    tracing::info!("Builtin skills loaded successfully");
    Ok(())
}

/// Upsert skill into index database
async fn upsert_skill_index(pool: &Pool<Sqlite>, skill: &SkillRow) -> anyhow::Result<()> {
    let extra_json_str = skill.extra_json.as_deref();
    let created_at_str = skill.created_at.to_rfc3339();
    let updated_at_str = skill.updated_at.to_rfc3339();
    
    // Upsert into skills table
    sqlx::query(
        r#"
        INSERT INTO skills (id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            is_enabled = excluded.is_enabled,
            content = excluded.content,
            program = excluded.program,
            extra_json = excluded.extra_json,
            created_by = excluded.created_by,
            updated_by = excluded.updated_by,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&skill.id)
    .bind(&skill.name)
    .bind(&skill.description)
    .bind(skill.is_enabled)
    .bind(&skill.content)
    .bind(&skill.program)
    .bind(extra_json_str)
    .bind(&skill.created_by)
    .bind(&skill.updated_by)
    .bind(&created_at_str)
    .bind(&updated_at_str)
    .execute(pool)
    .await?;

    // Update FTS index
    sqlx::query("DELETE FROM skills_fts WHERE id = ?")
        .bind(&skill.id)
        .execute(pool)
        .await?;

    let fts_name = tokenize_text(&skill.name);
    let fts_description = skill.description.as_deref().map(tokenize_text).unwrap_or_default();
    let fts_content = skill.content.as_deref().map(tokenize_text).unwrap_or_default();

    sqlx::query("INSERT INTO skills_fts (id, name, description, content) VALUES (?, ?, ?, ?)")
        .bind(&skill.id)
        .bind(&fts_name)
        .bind(&fts_description)
        .bind(&fts_content)
        .execute(pool)
        .await?;

    Ok(())
}

/// Delete skill from index database
async fn delete_skill_index(pool: &Pool<Sqlite>, skill_id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(skill_id)
        .execute(pool)
        .await?;
    
    sqlx::query("DELETE FROM skills_fts WHERE id = ?")
        .bind(skill_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// Search skills using FTS
async fn search_skills_fts(
    pool: &Pool<Sqlite>,
    query: &str,
    limit: i32,
    offset: i32,
) -> anyhow::Result<Vec<(String, f64)>> {
    let match_query = build_match_query(query);
    if match_query.is_empty() {
        return Ok(vec![]);
    }

    let rows: Vec<(String, f64)> = sqlx::query_as(
        r#"
        SELECT skills_fts.id, bm25(skills_fts, 10.0, 4.0, 1.0) AS rank
        FROM skills_fts
        WHERE skills_fts MATCH ?
        ORDER BY rank
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(&match_query)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows)
}

// ============== Handlers ==============

async fn create_skill(
    State(state): State<SkillsState>,
    Json(payload): Json<SkillCreate>,
) -> Result<Json<SkillResponse>, (StatusCode, String)> {
    let id = gen_prefix_random_id("sk", 6);
    let extra_json_str = payload.extra_json.map(|v| serde_json::to_string(&v).unwrap());

    // Insert into main database
    sqlx::query(
        r#"
        INSERT INTO skills (id, name, description, is_enabled, content, program, extra_json, created_by, updated_by)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&payload.name)
    .bind(&payload.description)
    .bind(payload.is_enabled)
    .bind(&payload.content)
    .bind(&payload.program)
    .bind(&extra_json_str)
    .bind(&payload.created_by)
    .bind(&payload.updated_by)
    .execute(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Fetch the created skill
    let skill: SkillRow = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update index database
    upsert_skill_index(&state.skills_db_pool, &skill)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(skill.into()))
}

async fn list_skills(
    State(state): State<SkillsState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<SkillResponse>>, (StatusCode, String)> {
    // 从磁盘数据库查询自定义技能
    let skills: Vec<SkillRow> = sqlx::query_as(
        r#"
        SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at
        FROM skills
        ORDER BY updated_at DESC, created_at DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(query.limit.min(200))
    .bind(query.offset)
    .fetch_all(&state.skills_db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(skills.into_iter().map(Into::into).collect()))
}

/// List builtin skills without content (lightweight response)
/// 与 agentlite api/skills.py 的 list_builtin_skills 一致：SELECT 不包含 content 列
async fn list_builtin_skills(
    State(state): State<SkillsState>,
) -> Result<Json<Vec<SkillResponse>>, (StatusCode, String)> {
    let skills: Vec<SkillRow> = sqlx::query_as(
        r#"
        SELECT id, name, description, is_enabled, program, extra_json, created_by, updated_by, created_at, updated_at
        FROM skills
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.builtin_skills_db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(skills.into_iter().map(Into::into).collect()))
}

/// Get single builtin skill detail (with content)
async fn get_builtin_skill(
    State(state): State<SkillsState>,
    Path(skill_id): Path<String>,
) -> Result<Json<SkillResponse>, (StatusCode, String)> {
    let skill: Option<SkillRow> = sqlx::query_as(
        r#"
        SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at
        FROM skills
        WHERE id = ?
        "#,
    )
    .bind(&skill_id)
    .fetch_optional(&state.builtin_skills_db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    match skill {
        Some(s) => Ok(Json(s.into())),
        None => Err((StatusCode::NOT_FOUND, "builtin skill not found".to_string())),
    }
}

async fn search_skills(
    State(state): State<SkillsState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SkillSearchResponse>>, (StatusCode, String)> {
    // agentlite parity: limit must be within [1, 100] (422 otherwise)
    let raw_limit = query.limit; // serde default = 20 (agentlite parity)
    if raw_limit < 1 || raw_limit > 100 {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "limit must be between 1 and 100".to_string()));
    }
    let limit = raw_limit.min(100);
    let offset = query.offset;
    
    // Search in both user skills and builtin skills FTS indexes
    let user_fts_results = search_skills_fts(&state.skills_db_pool, &query.query, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    
    let builtin_fts_results = search_skills_fts(&state.builtin_skills_db_pool, &query.query, limit, offset)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Merge results, user skills first, then builtin skills
    let mut all_results: Vec<(String, f64, bool)> = vec![]; // (id, rank, is_builtin)
    
    for (id, rank) in user_fts_results {
        all_results.push((id, rank, false));
    }
    for (id, rank) in builtin_fts_results {
        all_results.push((id, rank, true));
    }
    
    if all_results.is_empty() {
        return Ok(Json(vec![]));
    }
    
    // Sort by rank (lower is better in BM25)
    all_results.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Limit results
    all_results.truncate(limit as usize);

    let mut response: Vec<SkillSearchResponse> = vec![];
    
    // Fetch user skills from main database
    let user_ids: Vec<&String> = all_results.iter()
        .filter(|(_, _, is_builtin)| !is_builtin)
        .map(|(id, _, _)| id)
        .collect();
    
    let user_skills_by_id: std::collections::HashMap<String, SkillRow> = if !user_ids.is_empty() {
        let placeholders = user_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id IN ({})",
            placeholders
        );
        let mut query_builder = sqlx::query_as::<_, SkillRow>(&sql);
        for id in &user_ids {
            query_builder = query_builder.bind(*id);
        }
        query_builder
            .fetch_all(&state.db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    
    // Fetch builtin skills from builtin skills index database
    let builtin_ids: Vec<&String> = all_results.iter()
        .filter(|(_, _, is_builtin)| *is_builtin)
        .map(|(id, _, _)| id)
        .collect();
    
    let builtin_skills_by_id: std::collections::HashMap<String, SkillRow> = if !builtin_ids.is_empty() {
        let placeholders = builtin_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id IN ({})",
            placeholders
        );
        let mut query_builder = sqlx::query_as::<_, SkillRow>(&sql);
        for id in &builtin_ids {
            query_builder = query_builder.bind(*id);
        }
        query_builder
            .fetch_all(&state.builtin_skills_db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect()
    } else {
        std::collections::HashMap::new()
    };

    // Build response maintaining sorted order
    for (id, rank, is_builtin) in &all_results {
        let skill = if *is_builtin {
            builtin_skills_by_id.get(id)
        } else {
            user_skills_by_id.get(id)
        };
        
        if let Some(skill) = skill {
            response.push(SkillSearchResponse {
                skill: skill.clone().into(),
                rank: Some(*rank),
            });
        }
    }

    Ok(Json(response))
}

async fn get_skill(
    State(state): State<SkillsState>,
    Path(skill_id): Path<String>,
) -> Result<Json<SkillResponse>, (StatusCode, String)> {
    let skill: SkillRow = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or_else(|| (StatusCode::NOT_FOUND, "skill not found".to_string()))?;

    Ok(Json(skill.into()))
}

async fn update_skill(
    State(state): State<SkillsState>,
    Path(skill_id): Path<String>,
    Json(payload): Json<SkillUpdate>,
) -> Result<Json<SkillResponse>, (StatusCode, String)> {
    // 更新自定义技能
    let existing: Option<SkillRow> = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_none() {
        return Err((StatusCode::NOT_FOUND, "skill not found".to_string()));
    }

    // Build dynamic update query
    // bind_values 用 Option<String>：字段显式传 null 时绑定 NULL，把字段清空；
    // 字段未出现在请求里时（外层 None）不生成对应的 UPDATE 项，保持原值不动。
    let mut updates = vec!["updated_at = CURRENT_TIMESTAMP"];
    let mut bind_values: Vec<Option<String>> = vec![];

    if let Some(name) = &payload.name {
        updates.push("name = ?");
        bind_values.push(name.clone());
    }
    if let Some(description) = &payload.description {
        updates.push("description = ?");
        bind_values.push(description.clone());
    }
    if let Some(is_enabled) = payload.is_enabled {
        updates.push("is_enabled = ?");
        bind_values.push(Some(if is_enabled { "1" } else { "0" }.to_string()));
    }
    if let Some(content) = &payload.content {
        updates.push("content = ?");
        bind_values.push(content.clone());
    }
    if let Some(program) = &payload.program {
        updates.push("program = ?");
        bind_values.push(program.clone());
    }
    if let Some(extra_json) = &payload.extra_json {
        updates.push("extra_json = ?");
        bind_values.push(extra_json.as_ref().map(|v| serde_json::to_string(v).unwrap()));
    }
    if let Some(updated_by) = &payload.updated_by {
        updates.push("updated_by = ?");
        bind_values.push(updated_by.clone());
    }

    let sql = format!("UPDATE skills SET {} WHERE id = ?", updates.join(", "));
    let mut query = sqlx::query(&sql);
    for value in &bind_values {
        query = query.bind(value);
    }
    query = query.bind(&skill_id);

    query
        .execute(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Fetch updated skill
    let skill: SkillRow = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_one(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Update index database
    upsert_skill_index(&state.skills_db_pool, &skill)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(skill.into()))
}

async fn update_builtin_skill(
    State(state): State<SkillsState>,
    Path(skill_id): Path<String>,
    Json(payload): Json<SkillUpdate>,
) -> Result<Json<SkillResponse>, (StatusCode, String)> {
    // 更新内置技能（同时保存到系统设置，重启后持久化）
    let existing: Option<SkillRow> = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_optional(&state.builtin_skills_db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_none() {
        return Err((StatusCode::NOT_FOUND, "builtin skill not found".to_string()));
    }

    // Build dynamic update query
    // bind_values 用 Option<String>：字段显式传 null 时绑定 NULL，把字段清空；
    // 字段未出现在请求里时（外层 None）不生成对应的 UPDATE 项，保持原值不动。
    let mut updates = vec!["updated_at = CURRENT_TIMESTAMP"];
    let mut bind_values: Vec<Option<String>> = vec![];

    if let Some(name) = &payload.name {
        updates.push("name = ?");
        bind_values.push(name.clone());
    }
    if let Some(description) = &payload.description {
        updates.push("description = ?");
        bind_values.push(description.clone());
    }
    if let Some(is_enabled) = payload.is_enabled {
        updates.push("is_enabled = ?");
        bind_values.push(Some(if is_enabled { "1" } else { "0" }.to_string()));
    }
    if let Some(content) = &payload.content {
        updates.push("content = ?");
        bind_values.push(content.clone());
    }
    if let Some(program) = &payload.program {
        updates.push("program = ?");
        bind_values.push(program.clone());
    }
    if let Some(extra_json) = &payload.extra_json {
        updates.push("extra_json = ?");
        bind_values.push(extra_json.as_ref().map(|v| serde_json::to_string(v).unwrap()));
    }
    if let Some(updated_by) = &payload.updated_by {
        updates.push("updated_by = ?");
        bind_values.push(updated_by.clone());
    }

    if updates.len() > 1 {
        let sql = format!("UPDATE skills SET {} WHERE id = ?", updates.join(", "));
        let mut query = sqlx::query(&sql);
        for value in &bind_values {
            query = query.bind(value);
        }
        query = query.bind(&skill_id);

        query
            .execute(&state.builtin_skills_db_pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    // 如果更新了is_enabled，保存到系统设置
    if payload.is_enabled.is_some() {
        // Fetch all builtin skills
        let all_skills: Vec<SkillRow> = sqlx::query_as(
            "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills"
        )
        .fetch_all(&state.builtin_skills_db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        
        // Convert to BuiltinSkill
        let builtin_skills: Vec<crate::builtin_skills::BuiltinSkill> = all_skills
            .into_iter()
            .map(|row| crate::builtin_skills::BuiltinSkill {
                id: Some(row.id),
                name: row.name,
                description: row.description,
                is_enabled: row.is_enabled,
                content: row.content,
                program: row.program,
                extra_json: row.extra_json.and_then(|s| serde_json::from_str(&s).ok()),
                // generated_json is only present in the compiled-in skill.json files
                // (the DB has no such column, matching agentlite's schema)
                generated_json: None,
                created_by: row.created_by,
                updated_by: row.updated_by,
            })
            .collect();
        
        // Save to system settings
        if let Err(e) = crate::builtin_skills::save_builtin_skills_enabled_state(
            &state.db_pool,
            &builtin_skills,
        ).await {
            tracing::warn!("Failed to save builtin skills enabled state: {}", e);
            // Don't fail the request, just warn
        }
    }

    // Fetch updated skill
    let skill: SkillRow = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_one(&state.builtin_skills_db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(skill.into()))
}

async fn delete_skill_handler(
    State(state): State<SkillsState>,
    Path(skill_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Check if skill exists
    let existing: Option<SkillRow> = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills WHERE id = ?"
    )
    .bind(&skill_id)
    .fetch_optional(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if existing.is_none() {
        return Err((StatusCode::NOT_FOUND, "skill not found".to_string()));
    }

    // Delete from main database
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(&skill_id)
        .execute(&state.db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Delete from index database
    delete_skill_index(&state.skills_db_pool, &skill_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({"id": skill_id, "deleted": true})))
}

async fn rebuild_skills(
    State(state): State<SkillsState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Fetch all skills from main database
    let skills: Vec<SkillRow> = sqlx::query_as(
        "SELECT id, name, description, is_enabled, content, program, extra_json, created_by, updated_by, created_at, updated_at FROM skills ORDER BY updated_at DESC, created_at DESC"
    )
    .fetch_all(&state.db_pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Clear index database
    sqlx::query("DELETE FROM skills_fts")
        .execute(&state.skills_db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query("DELETE FROM skills")
        .execute(&state.skills_db_pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Rebuild index
    for skill in &skills {
        upsert_skill_index(&state.skills_db_pool, skill)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(Json(serde_json::json!({"status": "ok", "count": skills.len()})))
}

// ============== Router ==============

pub fn skills_router() -> Router<SkillsState> {
    Router::new()
        // Custom skills
        .route("/", post(create_skill).get(list_skills))
        .route("/{skill_id}", get(get_skill).put(update_skill).delete(delete_skill_handler))
        // Builtin skills
        .route("/builtin", get(list_builtin_skills))
        .route("/builtin/{skill_id}", get(get_builtin_skill).put(update_builtin_skill))
        // Search & rebuild
        .route("/search", get(search_skills))
        .route("/rebuild", post(rebuild_skills))
}
