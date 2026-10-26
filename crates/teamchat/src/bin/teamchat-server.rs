use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use clap::Parser;
use teamchat::config::Config;
use teamchat::db::{create_sqlite_pool, run_migrations_sqlite};
use teamchat::server;
use teamchat::store::{MemoryStore, SqliteStore};

#[derive(Parser)]
#[command(name = "teamchat-server")]
struct Cli {
    #[arg(long, default_value = "0.0.0.0")]
    bind: String,

    #[arg(short, long, default_value = "3001")]
    port: u16,

    #[arg(long)]
    sqlite: Option<String>,

    #[arg(long, default_value = "false")]
    memory: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    let config = Arc::new(Config {
        bind: cli.bind.clone(),
        port: cli.port,
        ..Default::default()
    });

    if cli.memory {
        let store = Arc::new(MemoryStore::new());
        let app = server::build_router(store, config.clone());
        let addr: SocketAddr = format!("{}:{}", config.bind, config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("TeamChat server (memory) listening on {}", addr);
        axum::serve(listener, app).await?;
    } else {
        let db_path = cli.sqlite.as_deref().unwrap_or("teamchat.db");
        let url = format!("sqlite://{}?mode=rwc", db_path);
        let pool = create_sqlite_pool(&url).await?;
        run_migrations_sqlite(&pool).await?;
        let store = Arc::new(SqliteStore::new(pool));
        let app = server::build_router(store, config.clone());
        let addr: SocketAddr = format!("{}:{}", config.bind, config.port).parse()?;
        let listener = TcpListener::bind(addr).await?;
        tracing::info!("TeamChat server (sqlite: {}) listening on {}", db_path, addr);
        axum::serve(listener, app).await?;
    }

    Ok(())
}
