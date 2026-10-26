use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use axum::{body::Body, http::{Request, StatusCode, header}, response::{IntoResponse, Response}, routing::get, Router, middleware::Next};
use axum_server::tls_rustls::RustlsConfig;
use serde_json::{json, Map, Value};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::{info, warn};

use crate::builtin_skills::BuiltinSkill;
use crate::process::ProcessManager;
use entitlement::{CommunityProvider, EntitlementProvider, EntitlementService};

use crate::{
    admin_bootstrap, app_config, auto_start, chat, cli, disks, file_processing, system_metrics,
};

use crate::{
    auth::{auth_router, AuthState},
    chat::{chat_router, ChatState},
    config::{CONFIG, VERSION},
    database::{init_db, DbPool},
    file_ops::file_ops_router,
    file_ops::FileOpsState,
    inference::{inference_router, InferenceState},
    quick_prompts::QuickPromptsState,
    skills::{skills_router, SkillsState, init_skills_index_schema, init_builtin_skills},
};
use agent_core::{ChatLoopManager, SkillDef};
use agent_core::builtin_knowledge::create_builtin_knowledge_store;
use crate::builtin_knowledge::register_builtin_knowledge;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub db_pool: DbPool,
    pub entitlements: Arc<EntitlementService>,
}

/// Creates the product-specific entitlement provider and any extra routes
/// before the shared router is finalized.
#[async_trait::async_trait]
pub trait AppFactory: Send + Sync {
    /// Product-specific builtin skills appended to the public set.
    fn builtin_skills(&self) -> Vec<BuiltinSkill> {
        Vec::new()
    }

    async fn create(
        &self,
        db_pool: &DbPool,
    ) -> anyhow::Result<(Arc<dyn EntitlementProvider>, Router)>;
}

/// Default factory for the open-source community build.
pub struct CommunityAppFactory;

#[async_trait::async_trait]
impl AppFactory for CommunityAppFactory {
    async fn create(
        &self,
        _db_pool: &DbPool,
    ) -> anyhow::Result<(Arc<dyn EntitlementProvider>, Router)> {
        Ok((Arc::new(CommunityProvider), Router::new()))
    }
}

/// Entry point for the community build.
pub fn run() -> anyhow::Result<()> {
    run_with_factory(Arc::new(CommunityAppFactory))
}

/// Entry point shared by the ActForce binaries. `factory` injects the
/// product-specific entitlement provider and extra routes.
pub fn run_with_factory(factory: Arc<dyn AppFactory>) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    
    // 解析 CLI 参数
    let cli = cli::Cli::parse();
    
    // Build the runtime explicitly so shutdown is controllable. With
    // #[tokio::main] the runtime is dropped implicitly after main returns,
    // and Runtime::drop WAITS for any in-flight spawn_blocking tasks
    // (file scans, inference-process restart). After Ctrl+C that can hang
    // the process even though graceful shutdown completed. shutdown_timeout
    // gives blocking work a grace period, then terminates the process.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    
    // 如果有子命令，执行后退出
    if let Some(command) = cli.command {
        return rt.block_on(async { command.execute().await });
    }
    
    // 否则启动服务器
    let result = rt.block_on(async { start_server(factory).await });
    
    // Graceful shutdown has completed ("Server shutdown complete"). Dropping
    // the tokio runtime here waits on internal pool shutdown which, depending
    // on the platform/driver state, can hang the process after Ctrl+C (users
    // had to press it repeatedly). Give blocking work a very short grace
    // period, then force-exit instead of waiting indefinitely.
    rt.shutdown_timeout(std::time::Duration::from_secs(1));
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}

/// 启动服务器
async fn start_server(factory: Arc<dyn AppFactory>) -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    
    // 根据 ACTFORCE_SQL_ECHO 配置设置 SQL 日志级别
    let sql_echo = std::env::var("ACTFORCE_SQL_ECHO")
        .map(|v| v.to_lowercase() == "true")
        .unwrap_or(false);
    
    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(tracing::Level::INFO.into());
    
    let env_filter = if sql_echo {
        // 开启 SQL 语句输出
        env_filter
            .add_directive("sqlx::query=debug".parse().unwrap())
            .add_directive("sqlx=info".parse().unwrap())
    } else {
        // 关闭 SQL 语句输出，只显示警告和错误
        env_filter
            .add_directive("sqlx::query=warn".parse().unwrap())
            .add_directive("sqlx=warn".parse().unwrap())
    };
    
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();
    
    info!("Starting ActForce v{}", VERSION);
    info!("Data root: {}", CONFIG.data_root.display());
    // Inject storage paths into agent-core (chat loops, attachments, outputs)
    agent_core::configure_paths(
        CONFIG.data_root.clone(),
        CONFIG.chat_files_dir.clone(),
        CONFIG.disks_dir.clone(),
    );
    
    let db_pool = init_db().await?;
    crate::database::set_global_pool(db_pool.clone());
    info!("Database initialized");

    // Ensure an initial admin user exists (writes password to data_root/admin_password.txt)
    // agentlite parity: a failing admin bootstrap aborts startup
    match admin_bootstrap::ensure_admin_user(&db_pool).await {
        Ok(true) => info!("Admin user bootstrap: created initial admin"),
        Ok(false) => info!("Admin user bootstrap: admin already exists"),
        Err(e) => {
            tracing::error!("Admin user bootstrap failed: {}", e);
            return Err(e);
        }
    }
    
    // Initialize user skills index database (disk, for FTS on user-edited skills)
    let skills_db_url = CONFIG.skills_database_url();
    info!("Initializing user skills index database: {}", skills_db_url);
    let skills_db_options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&CONFIG.skills_db_path)
        .create_if_missing(true)
        .busy_timeout(std::time::Duration::from_secs(5));
    let skills_db_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(skills_db_options)
        .await?;
    init_skills_index_schema(&skills_db_pool).await?;
    info!("User skills index database initialized");
    
    // Initialize builtin skills database (in-memory, recreated on each startup)
    let builtin_skills_db_url = CONFIG.builtin_skills_database_url();
    info!("Initializing builtin skills database (in-memory): {}", builtin_skills_db_url);
    let builtin_skills_db_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&builtin_skills_db_url)
        .await?;
    init_builtin_skills(&builtin_skills_db_pool, &db_pool).await?;
    info!("Builtin skills database initialized");
    
    let process_manager = Arc::new(ProcessManager::new());
    auto_start::maybe_start_inference_process(process_manager.clone()).await;

    // Start file monitoring + file watcher
    tokio::spawn(async {
        file_processing::start_file_monitoring().await;
    });
    info!("File monitoring started");
    
    let auth_state = AuthState { db_pool: db_pool.clone() };

    // Read concurrency limits from system_settings (agentlite defaults: 5 / 20)
    let max_concurrent = read_setting_i64(&db_pool, "max_concurrent_sessions")
        .await
        .filter(|v| *v > 0)
        .map(|v| v as usize)
        .or(Some(5));
    let global_llm = read_setting_i64(&db_pool, "global_llm_requests")
        .await
        .filter(|v| *v > 0)
        .map(|v| v as usize)
        .unwrap_or(20);
    info!("Concurrency limits: max_concurrent_sessions={:?}, global_llm_requests={}", max_concurrent, global_llm);

    let loop_manager = Arc::new(ChatLoopManager::new(
        30 * 60,  // 30 minute idle TTL
        60,       // 60 second sweep interval
        max_concurrent, // max concurrent sessions (from settings)
    ));
    agent_core::core::set_global_llm_limit(global_llm).await;
    let chat_state = ChatState { db_pool: db_pool.clone(), loop_manager: loop_manager.clone() };

    // Register builtin skills from compiled .skill.json files
    {
        let mut builtins = crate::builtin_skills::get_builtin_skills();
        builtins.extend(factory.builtin_skills());
        info!("Registering {} builtin skills into agent-core", builtins.len());
        for bs in builtins {
            let skill = SkillDef::from_builtin(
                bs.id,
                bs.name,
                bs.description,
                bs.is_enabled,
                bs.content,
                bs.program,
                bs.extra_json,
                bs.generated_json,
                bs.created_by,
                bs.updated_by,
            );
            loop_manager.register_builtin_skill(skill).await;
        }
    }

    // Register builtin knowledge into agent-core
    {
        let knowledge_store = create_builtin_knowledge_store().await;
        register_builtin_knowledge(&knowledge_store).await;
        info!("Builtin knowledge store created ({})", knowledge_store.count());
        loop_manager.set_builtin_knowledge_store(knowledge_store).await;
    }

    let inference_state = InferenceState { db_pool: db_pool.clone(), process_manager: process_manager.clone() };
    let file_ops_state = FileOpsState { db_pool: db_pool.clone() };
    let skills_state = SkillsState { 
        db_pool: db_pool.clone(), 
        skills_db_pool: skills_db_pool.clone(),
        builtin_skills_db_pool: builtin_skills_db_pool.clone(),
    };
    
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);
    
    // The product factory decides which provider (community/enterprise) and
    // which extra routes (e.g. the private license API) to install.
    let (entitlement_provider, extra_routes) = factory.create(&db_pool).await?;
    let entitlement_service = Arc::new(
        EntitlementService::new(entitlement_provider)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize entitlements: {e}"))?,
    );
    let app_state = AppState {
        db_pool: db_pool.clone(),
        entitlements: entitlement_service,
    };
    
    // 确定静态目录：优先 public/，否则回退到 web/dist
    let public_dir = determine_public_dir();
    let app = if public_dir.join("index.html").exists() {
        info!("静态文件服务已启动，目录: {:?}", public_dir);
        // SPA fallback: 所有非文件请求返回 index.html
        let serve_dir = ServeDir::new(&public_dir)
            .fallback(SpaFallbackService::new(&public_dir, db_pool.clone()));
        
        // File service routes must be before SPA fallback
        Router::new()
            .route("/health", get(health_handler))
            .route("/api/entitlements", get(entitlements_handler).with_state(app_state.clone()))
            .route("/api/system/metrics", get(system_metrics::get_system_metrics).with_state(app_state.clone()))
            .nest("/api/disks", disks::disks_router())
            .nest("/api/config", app_config::app_config_router().with_state(app_config::AppConfigState { db_pool: db_pool.clone(), loop_manager: loop_manager.clone() }))
            .nest("/api/quick-prompts", crate::quick_prompts::router().with_state(QuickPromptsState { db_pool: db_pool.clone() }))
            .nest("/api/auth", auth_router().with_state(auth_state))
            .nest("/api/chat", chat_router().with_state(chat_state.clone()))
            // The frontend posts to /api/chat/ (trailing slash); register a
            // root-only router at /api/chat/ too (agentlite 307-redirects it).
            .nest("/api/chat/", chat::chat_root_trailing_router().with_state(chat_state))
            .nest("/api/inference", inference_router().with_state(inference_state))
            .nest("/api", file_ops_router().with_state(file_ops_state))
            .nest("/api/skills", skills_router().with_state(skills_state))
            .nest("/api/aupage", crate::aupage::aupage_router())
            .nest("/api/markdown", crate::markdown_export::markdown_router())
            // axum 0.8: root nesting is unsupported; use fallback_service for SPA static files
            .fallback_service(serve_dir)
    } else {
        info!("未找到前端静态文件，跳过静态文件服务挂载: {:?}", public_dir);
        Router::new()
            .route("/", get(root_handler))
            .route("/health", get(health_handler))
            .route("/api/entitlements", get(entitlements_handler).with_state(app_state.clone()))
            .route("/api/system/metrics", get(system_metrics::get_system_metrics).with_state(app_state.clone()))
            .nest("/api/disks", disks::disks_router())
            .nest("/api/config", app_config::app_config_router().with_state(app_config::AppConfigState { db_pool: db_pool.clone(), loop_manager: loop_manager.clone() }))
            .nest("/api/quick-prompts", crate::quick_prompts::router().with_state(QuickPromptsState { db_pool: db_pool.clone() }))
            .nest("/api/auth", auth_router().with_state(auth_state))
            .nest("/api/chat", chat_router().with_state(chat_state.clone()))
            // The frontend posts to /api/chat/ (trailing slash); register a
            // root-only router at /api/chat/ too (agentlite 307-redirects it).
            .nest("/api/chat/", chat::chat_root_trailing_router().with_state(chat_state))
            .nest("/api/inference", inference_router().with_state(inference_state))
            .nest("/api", file_ops_router().with_state(file_ops_state))
            .nest("/api/skills", skills_router().with_state(skills_state))
            .nest("/api/aupage", crate::aupage::aupage_router())
            .nest("/api/markdown", crate::markdown_export::markdown_router())
    };

    // Product-specific routes (e.g. the private license API) and shared
    // middleware are applied last so they cover every route.
    let app = app
        .merge(extra_routes)
        .layer(axum::middleware::from_fn_with_state(
            app_state.entitlements.clone(),
            entitlement_guard,
        ))
        .layer(cors)
        .layer(axum::middleware::from_fn(log_requests_middleware));
    
    let host = std::env::var("ACTFORCE_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("ACTFORCE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(5020);
    
    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    
    // Setup graceful shutdown
    let pm_shutdown = process_manager.clone();
    
    // TLS / HTTP/2 配置
    // 浏览器只支持 TLS 上的 HTTP/2 (h2)，axum-server 通过 ALPN 自动协商
    let ssl_certfile = std::env::var("ACTFORCE_SSL_CERTFILE").ok();
    let ssl_keyfile = std::env::var("ACTFORCE_SSL_KEYFILE").ok();
    let has_tls = ssl_certfile.is_some() && ssl_keyfile.is_some();
    
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    
    // Spawn shutdown handler
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        info!("Received shutdown signal, cleaning up...");
        shutdown_handle.shutdown();
    });
    
    if has_tls {
        let (cert_path, key_path) = (ssl_certfile.unwrap(), ssl_keyfile.unwrap());
        info!("Server with HTTP/2 (TLS) listening on https://{}", addr);
        let tls_config = RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to load TLS config: {}", e))?;
        
        axum_server::bind_rustls(addr, tls_config)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await?;
    } else {
        info!("Server listening on http://{} (HTTP/1.1, set ACTFORCE_SSL_CERTFILE/ACTFORCE_SSL_KEYFILE for HTTP/2)", addr);
        axum_server::bind(addr)
            .handle(handle)
            // ConnectInfo is required by the request logger to print the real
            // client IP; plain into_make_service() never injects it -> "unknown".
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await?;
    }
    
    // Stop inference process on shutdown
    info!("Stopping inference process...");
    if let Err(e) = pm_shutdown.stop().await {
        tracing::error!("Failed to stop inference process: {}", e);
    } else {
        info!("Inference process stopped successfully");
    }
    
    
    info!("Server shutdown complete");
    
    Ok(())
}

async fn root_handler() -> axum::Json<Map<String, Value>> {
    let mut response = CONFIG.version_info();
    response.insert("message".to_string(), Value::String("Welcome to ActForce".to_string()));
    axum::Json(response)
}

async fn health_handler() -> axum::Json<Value> {
    axum::Json(json!({"status": "ok"}))
}

/// Generic authorization status consumed by the frontend. The community
/// build returns an unrestricted community entitlement set.
async fn entitlements_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> axum::Json<entitlement::Entitlements> {
    axum::Json(state.entitlements.current().await)
}

/// Global authorization gate.
///
/// Community builds are never blocked. Private builds block business APIs
/// until their entitlement provider reports `active`/`grace`. Login, the
/// generic entitlements endpoint and the private license API stay reachable so
/// an administrator can activate a license while the product is locked.
async fn entitlement_guard(
    axum::extract::State(entitlements): axum::extract::State<Arc<EntitlementService>>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let is_public = request.method() == axum::http::Method::OPTIONS
        || path == "/health"
        || path == "/api/entitlements"
        || path == "/api/config/app"
        || matches!(
            path.as_str(),
            "/api/auth/login"
                | "/api/auth/refresh"
                | "/api/auth/identity/init"
                | "/api/auth/logout"
                | "/api/auth/me"
        )
        || path.starts_with("/api/super/license")
        || !path.starts_with("/api/");

    if is_public {
        return next.run(request).await;
    }

    if entitlements.is_blocked().await {
        return (
            StatusCode::FORBIDDEN,
            axum::Json(json!({
                "code": "license_required",
                "message": "A valid license is required for this operation."
            })),
        )
            .into_response();
    }

    next.run(request).await
}

/// 请求日志中间件 - 记录所有 HTTP 请求信息（与 Python 版本的 log_requests 对齐）
async fn log_requests_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let start_time = Instant::now();
    
    // 记录请求信息
    let method = request.method().clone();
    let uri = request.uri().clone();
    let path = uri.path().to_string();
    let query = uri.query().unwrap_or("").to_string();
    
    // 获取客户端 IP
    let client_ip = request
        .extensions()
        .get::<axum::extract::connect_info::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    
    // 记录请求开始
    if query.is_empty() {
        info!("→ {} {} | Client: {}", method, path, client_ip);
    } else {
        info!("→ {}?{} | Client: {}", method, format!("{}?{}", path, query), client_ip);
    }
    
    // 处理请求
    let response = next.run(request).await;
    
    // 计算处理时间
    let elapsed = start_time.elapsed();
    let process_time_ms = elapsed.as_secs_f64() * 1000.0;
    
    // 记录响应信息
    let status_code = response.status();
    if status_code.as_u16() >= 400 {
        warn!("← {} {} | Status: {} | Time: {:.2}ms", method, path, status_code, process_time_ms);
    } else {
        info!("← {} {} | Status: {} | Time: {:.2}ms", method, path, status_code, process_time_ms);
    }
    
    // 添加响应头显示处理时间
    let (parts, body) = response.into_parts();
    let mut response = Response::from_parts(parts, body);
    if let Ok(header_value) = header::HeaderValue::from_str(&format!("{:.2} ms", process_time_ms)) {
        response.headers_mut().insert("X-Process-Time", header_value);
    }
    
    response
}

// ============================================================
// SPA 静态文件服务
// ============================================================

/// Read an integer value from system_settings (value_json), if present.
async fn read_setting_i64(pool: &sqlx::SqlitePool, key: &str) -> Option<i64> {
    use sqlx::Row;
    let row = sqlx::query("SELECT value_json FROM system_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()?;
    let value: serde_json::Value = row.try_get("value_json").ok()?;
    value.as_i64().or_else(|| value.as_u64().map(|v| v as i64))
}

fn determine_public_dir() -> PathBuf {
    // Products may point at their own frontend build directory.
    if let Ok(dir) = std::env::var("ACTFORCE_PUBLIC_DIR") {
        let dir = PathBuf::from(dir);
        if dir.join("index.html").exists() {
            return dir;
        }
    }
    let public = PathBuf::from("public");
    if public.join("index.html").exists() {
        return public;
    }
    let dist = PathBuf::from("web").join("dist");
    if dist.join("index.html").exists() {
        return dist;
    }
    public
}

/// SPA fallback service: 当静态文件不存在时返回 index.html
#[derive(Clone)]
struct SpaFallbackService {
    public_dir: PathBuf,
    db_pool: sqlx::SqlitePool,
}

impl SpaFallbackService {
    fn new(public_dir: &PathBuf, db_pool: sqlx::SqlitePool) -> Self {
        Self {
            public_dir: public_dir.clone(),
            db_pool,
        }
    }

    async fn render_index_html(&self) -> anyhow::Result<String> {
        let index_path = self.public_dir.join("index.html");
        let mut html_text = tokio::fs::read_to_string(&index_path).await?;

        // 尝试从数据库读取系统名称并替换 <title>
        if let Ok(system_name) = self.get_system_name().await {
            if !system_name.is_empty() {
                if let Ok(re) = regex::Regex::new(r"(?is)<title>.*?</title>") {
                    let safe_title = html_escape::encode_text(&system_name);
                    html_text = re
                        .replace(&html_text, format!("<title>{}</title>", safe_title))
                        .to_string();
                }
            }
        }

        Ok(html_text)
    }

    async fn get_system_name(&self) -> anyhow::Result<String> {
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT value_json FROM system_settings WHERE key = 'system_name' LIMIT 1",
        )
        .fetch_optional(&self.db_pool)
        .await?;
        Ok(row
            .map(|r| r.try_get::<String, _>(0))
            .transpose()
            .unwrap_or_default()
            .unwrap_or_default())
    }
}

impl tower::Service<Request<Body>> for SpaFallbackService {
    type Response = Response<Body>;
    type Error = std::convert::Infallible;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Body>) -> Self::Future {
        let this = self.clone();
        Box::pin(async move {
            match this.render_index_html().await {
                Ok(html) => {
                    let response = Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                        .header("Cache-Control", "no-cache")
                        .body(Body::from(html))
                        .unwrap();
                    Ok(response)
                }
                Err(e) => {
                    tracing::error!("Failed to render index.html: {}", e);
                    let response = Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(Body::from("Failed to load page"))
                        .unwrap();
                    Ok(response)
                }
            }
        })
    }
}

