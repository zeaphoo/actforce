use std::sync::OnceLock;
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite, Row};
use crate::config::CONFIG;

/// Database pool type
pub type DbPool = Pool<Sqlite>;

/// Global pool for use in threads where the main pool isn't accessible
static GLOBAL_POOL: OnceLock<DbPool> = OnceLock::new();

pub fn set_global_pool(pool: DbPool) {
    let _ = GLOBAL_POOL.set(pool);
}

pub fn get_global_pool() -> Option<&'static DbPool> {
    GLOBAL_POOL.get()
}

/// Initialize database connection pool
pub async fn init_db() -> anyhow::Result<DbPool> {
    let database_url = CONFIG.database_url();
    
    tracing::info!("Initializing database: {}", database_url);
    
    // sqlx 0.8 defaults create_if_missing=false; agentlite (SQLAlchemy) creates
    // the DB file automatically, so a fresh deployment must work here too.
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&CONFIG.db_path)
        .create_if_missing(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    
    // Run migrations from SQL files (embedded at compile time)
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await?;
    
    tracing::info!("Database initialized successfully");
    Ok(pool)
}

/// Generate a prefixed random ID (matches Python gen_prefix_random_digits_id)
pub fn gen_prefix_random_id(prefix: &str, digits: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let random_part: String = (0..digits)
        .map(|_| char::from_digit(rng.gen_range(0..10), 10).unwrap())
        .collect();
    format!("{}{}", prefix, random_part)
}

/// Generate a prefixed time-based random ID (matches Python gen_prefix_time_random_digits_id)
pub fn gen_prefix_time_random_id(prefix: &str, digits: usize) -> String {
    use chrono::Local;
    use rand::Rng;
    let time_part = Local::now().format("%Y%m%d%H%M%S").to_string();
    let mut rng = rand::thread_rng();
    let random_part: String = (0..digits)
        .map(|_| char::from_digit(rng.gen_range(0..10), 10).unwrap())
        .collect();
    format!("{}{}{}", prefix, time_part, random_part)
}

/// Create a new conversation
pub async fn create_conversation(
    pool: &DbPool,
    title: Option<String>,
    owner_identity_id: &str,
) -> anyhow::Result<String> {
    let id = gen_prefix_time_random_id("cv", 4);
    sqlx::query(
        r#"
        INSERT INTO conversations (id, title, owner_identity_id, updated_at)
        VALUES (?, ?, ?, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(owner_identity_id)
    .execute(pool)
    .await?;
    
    Ok(id)
}

/// Get conversations by owner_identity_id ordered by updated_at desc
/// Filters by retention days if configured
/// Get all identity IDs for a user (for cross-device conversation access)
pub async fn get_identity_ids_for_user(
    pool: &DbPool,
    user_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query("SELECT id FROM user_identities WHERE user_id = ?")
        .bind(user_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("id")).collect())
}

pub async fn get_conversations_by_identity(
    pool: &DbPool,
    owner_identity_id: &str,
) -> anyhow::Result<Vec<crate::models::Conversation>> {
    // Get retention days from settings (default 30)
    let retention_days = get_setting_i64(pool, "conversation_history_retention_days", 30).await;
    
    let conversations = if retention_days > 0 {
        // Filter by retention period
        let cutoff_timestamp = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::days(retention_days))
            .unwrap()
            .timestamp();
        
        sqlx::query_as::<_, crate::models::Conversation>(
            r#"
            SELECT id, title, owner_identity_id, created_at, updated_at
            FROM conversations
            WHERE owner_identity_id = ? AND updated_at >= datetime(?, 'unixepoch')
            ORDER BY updated_at DESC
            "#,
        )
        .bind(owner_identity_id)
        .bind(cutoff_timestamp)
        .fetch_all(pool)
        .await?
    } else {
        // No retention limit, get all conversations
        sqlx::query_as::<_, crate::models::Conversation>(
            r#"
            SELECT id, title, owner_identity_id, created_at, updated_at
            FROM conversations
            WHERE owner_identity_id = ?
            ORDER BY updated_at DESC
            "#,
        )
        .bind(owner_identity_id)
        .fetch_all(pool)
        .await?
    };
    
    Ok(conversations)
}

/// Get a setting value as i64 with default
async fn get_setting_i64(pool: &DbPool, key: &str, default: i64) -> i64 {
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

/// Create a new message
pub async fn create_message(
    pool: &DbPool,
    conversation_id: &str,
    role: &str,
    content: &str,
    content_json: Option<&str>,
) -> anyhow::Result<String> {
    let id = gen_prefix_time_random_id("ms", 8);
    create_message_with_id(pool, conversation_id, role, content, content_json, Some(&id)).await?;
    Ok(id)
}

/// Create message with a pre-generated ID (or auto-generate if None)
pub async fn create_message_with_id(
    pool: &DbPool,
    conversation_id: &str,
    role: &str,
    content: &str,
    content_json: Option<&str>,
    message_id: Option<&str>,
) -> anyhow::Result<String> {
    let id = message_id.map(|s| s.to_string()).unwrap_or_else(|| gen_prefix_time_random_id("ms", 8));
    sqlx::query(
        r#"
        INSERT INTO messages (id, conversation_id, role, content, content_json)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(content_json)
    .execute(pool)
    .await?;
    
    // Update conversation updated_at
    sqlx::query(
        r#"
        UPDATE conversations SET updated_at = CURRENT_TIMESTAMP WHERE id = ?
        "#,
    )
    .bind(conversation_id)
    .execute(pool)
    .await?;
    
    Ok(id)
}

/// Get messages by conversation id
pub async fn get_messages_by_conversation(
    pool: &DbPool,
    conversation_id: &str,
) -> anyhow::Result<Vec<crate::models::Message>> {
    let messages = sqlx::query_as::<_, crate::models::Message>(
        r#"
        SELECT id, conversation_id, role, content, content_json, created_at, updated_at
        FROM messages
        WHERE conversation_id = ?
        ORDER BY created_at ASC
        "#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;
    
    Ok(messages)
}

/// Update conversation title
pub async fn update_conversation_title(
    pool: &DbPool,
    conversation_id: &str,
    title: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE conversations SET title = ? WHERE id = ?
        "#,
    )
    .bind(title)
    .bind(conversation_id)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Get conversation by id, checking against multiple identity IDs
pub async fn get_conversation_by_id_any(
    pool: &DbPool,
    conversation_id: &str,
    owner_identity_ids: &[String],
) -> anyhow::Result<Option<crate::models::Conversation>> {
    if owner_identity_ids.is_empty() {
        return Ok(None);
    }
    // Build IN clause with placeholders
    let placeholders: Vec<String> = owner_identity_ids.iter().map(|_| "?".to_string()).collect();
    let query = format!(
        "SELECT id, title, owner_identity_id, created_at, updated_at FROM conversations WHERE id = ? AND owner_identity_id IN ({})",
        placeholders.join(",")
    );
    let mut q = sqlx::query_as::<_, crate::models::Conversation>(&query)
        .bind(conversation_id);
    for id in owner_identity_ids {
        q = q.bind(id);
    }
    let conversation = q.fetch_optional(pool).await?;
    Ok(conversation)
}

/// Get conversation by id and verify ownership (single identity, for backward compat)
pub async fn get_conversation_by_id(
    pool: &DbPool,
    conversation_id: &str,
    owner_identity_id: &str,
) -> anyhow::Result<Option<crate::models::Conversation>> {
    get_conversation_by_id_any(pool, conversation_id, &[owner_identity_id.to_string()]).await
}

/// Auto-name conversation if title is empty
pub async fn auto_name_conversation_if_empty(
    pool: &DbPool,
    conversation_id: &str,
    first_message: &str,
) -> anyhow::Result<()> {
    // Check if title is NULL or empty
    let conv = sqlx::query_as::<_, crate::models::Conversation>(
        r#"
        SELECT id, title, owner_identity_id, created_at, updated_at
        FROM conversations
        WHERE id = ?
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;
    
    if let Some(c) = conv {
        if c.title.is_none() || c.title.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
            // Use first message as title (max 30 chars)
            let title = if first_message.len() > 30 {
                first_message.chars().take(30).collect::<String>()
            } else {
                first_message.to_string()
            };
            
            update_conversation_title(pool, conversation_id, &title).await?;
        }
    }
    
    Ok(())
}
