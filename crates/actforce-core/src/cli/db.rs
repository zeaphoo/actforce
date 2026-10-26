/// 数据库管理子命令
/// 
/// 用法:
///     actforce db migrate          # 运行所有 pending 迁移
///     actforce db status           # 查看迁移状态
///     actforce db backup           # 备份数据库

use clap::{Args, Subcommand};
use sqlx::{sqlite::SqlitePool, Pool, Sqlite};
use std::path::PathBuf;
use std::fs;
use chrono::Local;

type DbPool = Pool<Sqlite>;

#[derive(Args)]
pub struct DbArgs {
    #[command(subcommand)]
    pub command: DbCommands,
    
    /// 数据库路径 (默认从环境变量读取)
    #[arg(short, long)]
    pub db_path: Option<String>,
}

#[derive(Subcommand)]
pub enum DbCommands {
    /// 运行所有 pending 的数据库迁移
    Migrate,
    
    /// 查看迁移状态
    Status,
    
    /// 备份数据库
    Backup,
    
    /// 显示数据库表结构
    Schema,
    
    /// 列出所有表
    Tables,
}

/// 获取数据库路径
fn get_db_path(cli_path: Option<String>) -> String {
    cli_path.unwrap_or_else(|| {
        std::env::var("ACTFORCE_DATABASE_URL")
            .ok()
            .and_then(|url| {
                // 解析 sqlite:// 格式的 URL
                url.strip_prefix("sqlite://").map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                // 默认路径
                let data_root = std::env::var("ACTFORCE_DATA_ROOT")
                    .unwrap_or_else(|_| "./data/storage".to_string());
                format!("{}/actforce.db", data_root)
            })
    })
}

/// 运行迁移
pub async fn run_migrations(db_path: &str) -> anyhow::Result<()> {
    println!("🔄 运行数据库迁移...");
    println!("📁 数据库路径: {}", db_path);
    println!();
    
    // 确保数据库目录存在
    if let Some(parent) = PathBuf::from(db_path).parent() {
        fs::create_dir_all(parent)?;
    }
    
    let pool = SqlitePool::connect(db_path).await?;
    
    // 执行迁移
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(_) => {
            println!("✅ 迁移成功完成");
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ 迁移失败: {}", e);
            Err(anyhow::anyhow!("Migration failed: {}", e))
        }
    }
}

/// 查看迁移状态
pub async fn show_status(db_path: &str) -> anyhow::Result<()> {
    println!("📊 数据库迁移状态");
    println!("📁 数据库路径: {}", db_path);
    println!();
    
    // 检查数据库文件是否存在
    if !PathBuf::from(db_path).exists() {
        println!("⚠️  数据库文件不存在");
        println!("💡 运行 'actforce db migrate' 创建数据库");
        return Ok(());
    }
    
    let pool = SqlitePool::connect(db_path).await?;
    
    // 查询已应用的迁移
    println!("✅ 已应用的迁移:");
    let migrations: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT version, description, installed_on FROM _sqlx_migrations ORDER BY version"
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    
    if migrations.is_empty() {
        println!("   (无)");
    } else {
        for (version, desc, installed_on) in migrations {
            println!("   [{:04}] {} (应用时间: {})", version, desc, installed_on);
        }
    }
    
    println!();
    
    // 列出迁移文件
    println!("📄 可用的迁移文件:");
    let migrations_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
    
    if !migrations_dir.exists() {
        println!("   ⚠️  migrations 目录不存在");
        return Ok(());
    }
    
    let mut found = false;
    for entry in fs::read_dir(&migrations_dir)? {
        let entry = entry?;
        let filename = entry.file_name();
        let filename_str = filename.to_string_lossy();
        
        if filename_str.ends_with(".up.sql") {
            found = true;
            let version = filename_str.strip_suffix(".up.sql").unwrap_or(&filename_str);
            let down_file = migrations_dir.join(format!("{}.down.sql", version));
            
            println!("   ✅ {}.up.sql", version);
            if down_file.exists() {
                println!("      ✅ {}.down.sql", version);
            } else {
                println!("      ⚠️  {}.down.sql (缺失)", version);
            }
        }
    }
    
    if !found {
        println!("   (无)");
    }
    
    Ok(())
}

/// 备份数据库
pub async fn backup_database(db_path: &str) -> anyhow::Result<()> {
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let backup_path = format!("{}.backup.{}", db_path, timestamp);
    
    println!("💾 备份数据库...");
    println!("📁 源文件: {}", db_path);
    println!("📁 目标文件: {}", backup_path);
    println!();
    
    if !PathBuf::from(db_path).exists() {
        eprintln!("❌ 数据库文件不存在: {}", db_path);
        return Err(anyhow::anyhow!("Database file not found: {}", db_path));
    }
    
    fs::copy(db_path, &backup_path)?;
    
    let backup_size = fs::metadata(&backup_path)?.len();
    let size_mb = backup_size as f64 / 1024.0 / 1024.0;
    
    println!("✅ 备份成功");
    println!("📊 备份大小: {:.2} MB", size_mb);
    println!("📁 备份路径: {}", backup_path);
    
    Ok(())
}

/// 显示数据库结构
pub async fn show_schema(db_path: &str) -> anyhow::Result<()> {
    println!("🏗️  数据库结构");
    println!("📁 数据库路径: {}", db_path);
    println!();
    
    if !PathBuf::from(db_path).exists() {
        eprintln!("❌ 数据库文件不存在");
        return Err(anyhow::anyhow!("Database file not found"));
    }
    
    let pool = SqlitePool::connect(db_path).await?;
    
    // 查询所有表的结构
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )
    .fetch_all(&pool)
    .await?;
    
    for (table_name,) in tables {
        println!("📋 表: {}", table_name);
        println!();
        
        // 获取表的 DDL
        let ddl: Vec<(String,)> = sqlx::query_as(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name = ?"
        )
        .bind(&table_name)
        .fetch_all(&pool)
        .await?;
        
        for (sql,) in ddl {
            println!("{}", sql);
        }
        println!();
    }
    
    Ok(())
}

/// 列出所有表
pub async fn show_tables(db_path: &str) -> anyhow::Result<()> {
    println!("📋 数据库表列表");
    println!("📁 数据库路径: {}", db_path);
    println!();
    
    if !PathBuf::from(db_path).exists() {
        eprintln!("❌ 数据库文件不存在");
        return Err(anyhow::anyhow!("Database file not found"));
    }
    
    let pool = SqlitePool::connect(db_path).await?;
    
    let tables: Vec<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )
    .fetch_all(&pool)
    .await?;
    
    if tables.is_empty() {
        println!("(无表)");
    } else {
        println!("共 {} 个表:", tables.len());
        println!();
        for (table_name,) in tables {
            println!("  - {}", table_name);
            
            // 显示行数
            let count: Result<(i64,), _> = sqlx::query_as(
                &format!("SELECT COUNT(*) FROM \"{}\"", table_name)
            )
            .fetch_one(&pool)
            .await;
            
            if let Ok((count,)) = count {
                println!("    ({}) 行", count);
            }
        }
    }
    
    Ok(())
}

/// 执行数据库命令
pub async fn execute(args: DbArgs) -> anyhow::Result<()> {
    let db_path = get_db_path(args.db_path);
    
    match args.command {
        DbCommands::Migrate => {
            run_migrations(&db_path).await?;
        }
        DbCommands::Status => {
            show_status(&db_path).await?;
        }
        DbCommands::Backup => {
            backup_database(&db_path).await?;
        }
        DbCommands::Schema => {
            show_schema(&db_path).await?;
        }
        DbCommands::Tables => {
            show_tables(&db_path).await?;
        }
    }
    
    Ok(())
}
