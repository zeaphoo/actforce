use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Pool, Postgres, Sqlite};
use std::time::Duration;

pub type PgPool = Pool<Postgres>;
pub type SqlitePool = Pool<Sqlite>;

#[derive(Debug, Clone)]
pub enum DbPool {
    Postgres(PgPool),
    Sqlite(SqlitePool),
}

pub async fn create_postgres_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(30))
        .connect(database_url)
        .await
}

pub async fn create_sqlite_pool(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn run_migrations_sqlite(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            channel_type INTEGER NOT NULL DEFAULT 0,
            name TEXT,
            topic TEXT,
            position INTEGER NOT NULL DEFAULT 0,
            last_message_id TEXT,
            recipients TEXT NOT NULL DEFAULT '[]',
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            channel_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_name TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            embeds TEXT NOT NULL DEFAULT '[]',
            attachments TEXT NOT NULL DEFAULT '[]',
            mentions TEXT NOT NULL DEFAULT '[]',
            pinned INTEGER NOT NULL DEFAULT 0,
            message_type INTEGER NOT NULL DEFAULT 0,
            message_reference TEXT,
            timestamp TEXT NOT NULL,
            edited_timestamp TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id, timestamp);",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn run_migrations_postgres(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            channel_type INTEGER NOT NULL DEFAULT 0,
            name TEXT,
            topic TEXT,
            position INTEGER NOT NULL DEFAULT 0,
            last_message_id TEXT,
            recipients JSONB NOT NULL DEFAULT '[]',
            created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            channel_id TEXT NOT NULL,
            sender_id TEXT NOT NULL,
            sender_name TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            embeds JSONB NOT NULL DEFAULT '[]',
            attachments JSONB NOT NULL DEFAULT '[]',
            mentions JSONB NOT NULL DEFAULT '[]',
            pinned BOOLEAN NOT NULL DEFAULT FALSE,
            message_type INTEGER NOT NULL DEFAULT 0,
            message_reference JSONB,
            timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            edited_timestamp TIMESTAMPTZ
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_messages_channel ON messages(channel_id, timestamp);",
    )
    .execute(pool)
    .await?;

    Ok(())
}
