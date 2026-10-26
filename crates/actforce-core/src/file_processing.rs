//! 文件处理模块 - 扫描 raw 目录，提取文本，构建全文索引

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::sync::Mutex;
use once_cell::sync::Lazy;
use tracing::{error, info, warn};

use common::tantivy_cli::{TantivyFieldSpec, Tansearch};

use crate::config::CONFIG;

/// 处理结果
#[derive(Debug, Clone, Serialize)]
pub struct ProcessingResult {
    pub index: String,
    pub status: String,
    pub items: usize,
    pub total_size: i64,
}

/// 文本提取条目（存入 texts.json manifest）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TextEntry {
    path: String,
    text_rel: String,
    mime_type: Option<String>,
    modified_at: String,
    mtime_ns: i64,
    size: i64,
    success: bool,
    method: String,
    char_count: usize,
    updated_at: String,
    error: Option<String>,
}

/// texts.json manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TextsManifest {
    generated_at: String,
    raw_base: String,
    text_base: String,
    items: HashMap<String, TextEntry>,
    stats: Option<serde_json::Value>,
}

/// 全局处理锁
static PROCESSING_LOCK: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

/// 标准化相对路径
fn normalize_rel_path(p: &str) -> String {
    let s = p.replace('\\', "/").trim_start_matches('/').to_string();
    let parts: Vec<&str> = s
        .split('/')
        .filter(|x| !x.is_empty() && *x != "." && *x != "..")
        .collect();
    parts.join("/")
}

/// 决定输出文本文件的扩展名
fn text_out_rel(raw_rel: &str, prefer_markdown: bool) -> String {
    let rel = Path::new(raw_rel);
    if prefer_markdown {
        rel.with_extension("md").to_string_lossy().to_string()
    } else {
        rel.with_extension("txt").to_string_lossy().to_string()
    }
}

/// 安全读取文本（最多 max_bytes 字节）
fn read_text_best_effort(p: &Path, max_bytes: usize) -> String {
    match std::fs::read(p) {
        Ok(data) => {
            let data = if data.len() > max_bytes {
                &data[..max_bytes]
            } else {
                &data[..]
            };
            String::from_utf8_lossy(data).to_string()
        }
        Err(_) => String::new(),
    }
}

/// 创建父目录后写入文件
fn write_text(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

/// 从文本中提取标题（优先 markdown # 标题）
fn default_title_from_text(text: &str, fallback: &str) -> String {
    for line in text.lines() {
        let s = line.trim();
        if s.starts_with('#') {
            let title = s.trim_start_matches('#').trim();
            if !title.is_empty() {
                return title.to_string();
            }
        }
        if !s.is_empty() {
            break;
        }
    }
    fallback.to_string()
}

/// 计算 raw 目录签名 (latest_mtime_ns, file_count, total_size)
fn raw_signature(raw_dir: &Path) -> (i64, usize, i64) {
    let mut latest: i64 = 0;
    let mut count: usize = 0;
    let mut total_size: i64 = 0;

    let entries = walkdir::WalkDir::new(raw_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file());

    for entry in entries {
        if let Ok(meta) = entry.metadata() {
            count += 1;
            total_size += meta.len() as i64;
            if let Ok(modified) = meta.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    let ns = duration.as_nanos() as i64;
                    if ns > latest {
                        latest = ns;
                    }
                }
            }
        }
    }

    (latest, count, total_size)
}

/// 简单的扩展名 -> MIME 映射
fn guess_mime_type(path: &Path) -> Option<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "doc" => "application/msword",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "xls" => "application/vnd.ms-excel",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "ppt" => "application/vnd.ms-powerpoint",
        "txt" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "text/xml",
        _ => return None,
    };
    Some(mime.to_string())
}

// ---------------------------------------------------------------------------
// 构建 Tantivy JSONL
// ---------------------------------------------------------------------------

fn build_tantivy_jsonl(
    text_dir: &Path,
    manifest: &TextsManifest,
    out_path: &Path,
) -> Result<usize> {
    let items = &manifest.items;
    let mut docs: Vec<String> = Vec::new();

    for (raw_rel, entry) in items.iter() {
        if !entry.success {
            continue;
        }
        let text_rel = &entry.text_rel;
        if text_rel.is_empty() {
            continue;
        }
        let fp = text_dir.join(normalize_rel_path(text_rel));
        if !fp.exists() || !fp.is_file() {
            continue;
        }
        let text = match std::fs::read_to_string(&fp) {
            Ok(t) => t,
            Err(_) => continue,
        };
        if text.trim().is_empty() {
            continue;
        }

        let fallback = Path::new(raw_rel)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(raw_rel);
        let title = default_title_from_text(&text, fallback);

        let doc = serde_json::json!({
            "path": normalize_rel_path(raw_rel),
            "title": title,
            "body": text,
            "mime_type": entry.mime_type,
            "modified_at": entry.modified_at,
            "mtime_ns": entry.mtime_ns as u64,
            "size": entry.size as u64,
        });
        docs.push(doc.to_string());
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = docs.join("\n");
    std::fs::write(out_path, format!("{}\n", content))?;

    Ok(docs.len())
}

// ---------------------------------------------------------------------------
// 核心处理函数
// ---------------------------------------------------------------------------

/// 运行文件处理流程（扫描 raw -> 提取文本 -> 构建索引）
pub async fn run_file_processing() -> Result<ProcessingResult> {
    let raw_dir = CONFIG.disks_raw_dir.clone();
    let processed_dir = CONFIG.disks_processed_dir.clone();
    let text_dir = processed_dir.join("text");
    std::fs::create_dir_all(&text_dir).ok();

    let manifest_path = processed_dir.join("texts.json");
    let tantivy_docs_path = processed_dir.join("tantivy_docs.jsonl");
    let tantivy_index_dir = processed_dir.join("tantivy_index");
    let index_path = processed_dir.join("index.json");
    let status_path = processed_dir.join("status.json");

    // 扫描 raw 目录中所有文件
    let files: Vec<PathBuf> = tokio::task::spawn_blocking({
        let raw_dir = raw_dir.clone();
        move || {
            walkdir::WalkDir::new(&raw_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .map(|e| e.path().to_path_buf())
                .collect()
        }
    })
    .await
    .unwrap_or_default();

    // 当前 raw 路径集合
    let mut current_raw_paths: std::collections::HashSet<String> =
        std::collections::HashSet::new();
    for p in &files {
        if let Ok(rel) = p.strip_prefix(&raw_dir) {
            current_raw_paths.insert(normalize_rel_path(&rel.to_string_lossy()));
        }
    }

    // 加载或初始化 manifest
    let mut manifest: TextsManifest = if manifest_path.exists() {
        match std::fs::read_to_string(&manifest_path) {
            Ok(content) => serde_json::from_str::<TextsManifest>(&content).unwrap_or_else(|_| {
                new_manifest(&raw_dir, &text_dir)
            }),
            Err(_) => new_manifest(&raw_dir, &text_dir),
        }
    } else {
        new_manifest(&raw_dir, &text_dir)
    };

    // 清理已删除文件的旧产物（manifest 中记录的、但 raw 文件已不存在的条目）
    let stale_keys: Vec<String> = manifest
        .items
        .keys()
        .filter(|k| !current_raw_paths.contains(*k))
        .cloned()
        .collect();
    let mut deleted_count = 0usize;
    for raw_rel in &stale_keys {
        if let Some(entry) = manifest.items.get(raw_rel) {
            let out_fp = text_dir.join(normalize_rel_path(&entry.text_rel));
            if out_fp.exists() && out_fp.is_file() {
                match std::fs::remove_file(&out_fp) {
                    Ok(_) => {
                        deleted_count += 1;
                        info!("Deleted processed file for removed raw: {} -> {}", raw_rel, out_fp.display());
                    }
                    Err(e) => warn!("Failed to delete processed file {}: {}", out_fp.display(), e),
                }
            }
        }
        manifest.items.remove(raw_rel);
    }

    // 扫描 text_dir，删除不在 manifest 中的孤立 processed 文件
    let valid_text_paths: std::collections::HashSet<PathBuf> = manifest
        .items
        .values()
        .filter(|e| !e.text_rel.is_empty())
        .map(|e| text_dir.join(normalize_rel_path(&e.text_rel)))
        .collect();

    if text_dir.exists() {
        let orphan_files: Vec<PathBuf> = walkdir::WalkDir::new(&text_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .map(|e| e.path().to_path_buf())
            .filter(|p| !valid_text_paths.contains(p))
            .collect();

        for orphan in orphan_files {
            match std::fs::remove_file(&orphan) {
                Ok(_) => {
                    deleted_count += 1;
                    info!("Deleted orphan processed file: {}", orphan.display());
                }
                Err(e) => warn!("Failed to delete orphan processed file {}: {}", orphan.display(), e),
            }
        }
    }

    if deleted_count > 0 {
        info!("Cleanup complete: {} processed file(s) deleted", deleted_count);
    }

    let total = files.len();
    let mut total_size: i64 = 0;
    let mut extracted_total = 0usize;
    let mut extracted_skipped = 0usize;
    let mut extracted_failed = 0usize;
    let mut overwritten_count = 0usize;

    // 逐文件处理
    for p in &files {
        let rel = match p.strip_prefix(&raw_dir) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        let Ok(meta) = p.metadata() else { continue };
        let size = meta.len() as i64;
        total_size += size;

        let mime_type = guess_mime_type(p);

        let mtime_ns = meta
            .modified()
            .ok()
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos() as i64)
            .unwrap_or(0);

        let modified_at = DateTime::<Utc>::from(
            std::time::UNIX_EPOCH
                + std::time::Duration::from_nanos(mtime_ns.max(0) as u64),
        )
        .to_rfc3339();

        let raw_rel = normalize_rel_path(&rel);
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        // xlsx/xls 输出 csv，其他输出 md（csv 文件本身保持 .csv 扩展名）
        let out_rel = if ext == "xlsx" || ext == "xls" {
            let rel = Path::new(&raw_rel);
            rel.with_extension("csv").to_string_lossy().to_string()
        } else if ext == "csv" {
            let rel = Path::new(&raw_rel);
            rel.with_extension("csv").to_string_lossy().to_string()
        } else {
            // 其他格式统一输出 markdown
            text_out_rel(&raw_rel, true)
        };

        // 检查是否可跳过（mtime 未变且输出文件存在）；若 mtime 变了则视为覆盖更新
        if let Some(prev) = manifest.items.get(&raw_rel) {
            if prev.mtime_ns == mtime_ns {
                let out_fp = text_dir.join(normalize_rel_path(&prev.text_rel));
                if out_fp.exists() && out_fp.is_file() {
                    extracted_skipped += 1;
                    continue;
                }
            } else {
                // raw 文件 mtime 发生变化，说明文件被覆盖/更新，需要重新提取
                overwritten_count += 1;
                info!("Raw file overwritten/updated, re-processing: {}", raw_rel);
            }
        }

        let out_fp = text_dir.join(&out_rel);

        // 提取文本
        let (text_content, method, success, err_msg) = extract_text(&p, &out_fp, &ext).await;

        if success && !text_content.is_empty() {
            // agent-doc 已经直接写入输出文件
            // 但如果文件不存在（某些格式不是通过 agent-doc 处理），则需要写入
            if !out_fp.exists() {
                match write_text(&out_fp, &text_content) {
                    Ok(_) => extracted_total += 1,
                    Err(e) => {
                        warn!("写入提取文本失败 {}: {}", out_fp.display(), e);
                        extracted_failed += 1;
                    }
                }
            } else {
                extracted_total += 1;
            }
        } else {
            extracted_failed += 1;
        }

        let entry = TextEntry {
            path: raw_rel.clone(),
            text_rel: out_rel,
            mime_type,
            modified_at,
            mtime_ns,
            size,
            success,
            method,
            char_count: text_content.len(),
            updated_at: Utc::now().to_rfc3339(),
            error: err_msg,
        };
        manifest.items.insert(raw_rel, entry);
    }

    // 保存 manifest
    manifest.generated_at = Utc::now().to_rfc3339();
    manifest.stats = Some(serde_json::json!({
        "extracted": extracted_total,
        "skipped": extracted_skipped,
        "failed": extracted_failed,
        "overwritten": overwritten_count,
        "deleted": deleted_count,
        "total": total,
    }));
    if let Err(e) = std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)?,
    ) {
        warn!("写入 texts.json 失败: {}", e);
    }

    // 构建 Tantivy 索引
    let mut tantivy_info: serde_json::Value = serde_json::json!({"available": false});

    match Tansearch::discover() {
        Ok(ts) => {
            let indexed_docs =
                build_tantivy_jsonl(&text_dir, &manifest, &tantivy_docs_path).unwrap_or(0);

            if tantivy_index_dir.exists() {
                std::fs::remove_dir_all(&tantivy_index_dir).ok();
            }

            let fields = make_tantivy_fields();
            if let Err(e) = ts.init_index_from_fields(&tantivy_index_dir, fields, 0, true, "jieba")
            {
                tantivy_info =
                    serde_json::json!({"available": false, "error": e.to_string()});
            } else if let Err(e) = ts
                .index_jsonl(&tantivy_index_dir, &tantivy_docs_path)
                .await
            {
                tantivy_info =
                    serde_json::json!({"available": false, "error": e.to_string()});
            } else {
                tantivy_info = serde_json::json!({
                    "available": true,
                    "index_dir": tantivy_index_dir.to_string_lossy().to_string(),
                    "docs_jsonl": tantivy_docs_path.to_string_lossy().to_string(),
                    "indexed_docs": indexed_docs,
                    "tokenizer": "jieba",
                });
            }
        }
        Err(e) => {
            warn!("tansearch not available: {}", e);
            tantivy_info = serde_json::json!({"available": false, "error": e.to_string()});
        }
    }

    // 写入 index.json
    let index_payload = serde_json::json!({
        "generated_at": Utc::now().to_rfc3339(),
        "texts": {
            "manifest": manifest_path.to_string_lossy().to_string(),
            "text_dir": text_dir.to_string_lossy().to_string(),
            "extracted": extracted_total,
            "skipped": extracted_skipped,
            "failed": extracted_failed,
            "overwritten": overwritten_count,
            "deleted": deleted_count,
        },
        "tantivy": tantivy_info,
    });
    if let Err(e) = std::fs::write(&index_path, serde_json::to_string_pretty(&index_payload)?) {
        warn!("写入 index.json 失败: {}", e);
    }

    // 写入 status.json
    let (last_ns, raw_count, raw_size) = raw_signature(&raw_dir);
    let status_payload = serde_json::json!({
        "completed_at": Utc::now().to_rfc3339(),
        "raw_file_count": raw_count,
        "raw_total_size": raw_size,
        "raw_last_modified_ns": last_ns,
        "raw_last_modified_at": if last_ns > 0 {
            Some(DateTime::<Utc>::from(
                std::time::UNIX_EPOCH + std::time::Duration::from_nanos(last_ns as u64)
            ).to_rfc3339())
        } else {
            None
        },
        "index": index_path.to_string_lossy().to_string(),
        "texts": {
            "manifest": manifest_path.to_string_lossy().to_string(),
            "text_dir": text_dir.to_string_lossy().to_string(),
            "extracted": extracted_total,
            "skipped": extracted_skipped,
            "failed": extracted_failed,
            "overwritten": overwritten_count,
            "deleted": deleted_count,
        },
        "tantivy": tantivy_info,
    });
    if let Err(e) = std::fs::write(&status_path, serde_json::to_string_pretty(&status_payload)?) {
        warn!("写入 status.json 失败: {}", e);
    }

    info!(
        "文件处理完成: total={} extracted={} skipped={} failed={} overwritten={} deleted={}",
        total, extracted_total, extracted_skipped, extracted_failed, overwritten_count, deleted_count
    );

    Ok(ProcessingResult {
        index: index_path.to_string_lossy().to_string(),
        status: status_path.to_string_lossy().to_string(),
        items: total,
        total_size,
    })
}

/// 扫描 raw 目录，检测变化后触发处理
pub async fn scan_and_trigger_file_processing() {
    let raw_dir = &CONFIG.disks_raw_dir;
    let processed_dir = &CONFIG.disks_processed_dir;

    if !raw_dir.exists() {
        return;
    }

    let (last_ns, raw_count, raw_size) = raw_signature(raw_dir);
    if raw_count == 0 {
        return;
    }

    // 与上次 status 对比
    let status_path = processed_dir.join("status.json");
    let mut need_process = true;
    if status_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&status_path) {
            if let Ok(status) = serde_json::from_str::<serde_json::Value>(&data) {
                let prev_ns = status
                    .get("raw_last_modified_ns")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let prev_count = status
                    .get("raw_file_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) as usize;
                let prev_size = status
                    .get("raw_total_size")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);

                if last_ns == prev_ns && raw_count == prev_count && raw_size == prev_size {
                    need_process = false;
                }
            }
        }
    }

    if !need_process {
        info!("No changes detected in raw files, skipping processing");
        return;
    }

    let mut lock = PROCESSING_LOCK.lock().await;
    if *lock {
        info!("File processing already in progress, skipping");
        return;
    }
    *lock = true;
    drop(lock);

    match run_file_processing().await {
        Ok(result) => info!("File processing completed: {} items", result.items),
        Err(e) => error!("File processing failed: {}", e),
    }

    let mut lock = PROCESSING_LOCK.lock().await;
    *lock = false;
}

/// 查询文件处理状态
pub fn get_file_processing_status() -> Option<serde_json::Value> {
    let status_path = CONFIG.disks_processed_dir.join("status.json");
    if !status_path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(&status_path).ok()?;
    serde_json::from_str(&data).ok()
}

/// 全文搜索文件
pub async fn search_files(
    query: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>> {
    let index_dir = CONFIG.disks_processed_dir.join("tantivy_index");
    if !index_dir.exists() {
        return Ok(vec![]);
    }

    // agentlite parity: if the tansearch binary is missing, return an empty
    // result list instead of failing the search request.
    let ts = match Tansearch::discover() {
        Ok(ts) => ts,
        Err(_) => return Ok(vec![]),
    };
    let lines = ts.search(&index_dir, query, limit, 0, None, true).await?;

    let mut results = Vec::new();
    for line in lines {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
            results.push(val);
        }
    }
    Ok(results)
}

/// 供 API 调用的安全触发入口
pub async fn trigger_file_processing_safe() {
    let mut lock = PROCESSING_LOCK.lock().await;
    if *lock {
        info!("File processing already in progress");
        return;
    }
    *lock = true;
    drop(lock);

    match run_file_processing().await {
        Ok(result) => info!("File processing completed: {} items", result.items),
        Err(e) => error!("File processing failed: {}", e),
    }

    let mut lock = PROCESSING_LOCK.lock().await;
    *lock = false;
}

// ---------------------------------------------------------------------------
// 内部辅助
// ---------------------------------------------------------------------------

fn new_manifest(raw_dir: &Path, text_dir: &Path) -> TextsManifest {
    TextsManifest {
        generated_at: Utc::now().to_rfc3339(),
        raw_base: raw_dir.to_string_lossy().to_string(),
        text_base: text_dir.to_string_lossy().to_string(),
        items: HashMap::new(),
        stats: None,
    }
}

/// 根据扩展名提取文本内容
/// 返回 (text, method, success, error_msg)
async fn extract_text(p: &Path, output_path: &Path, ext: &str) -> (String, String, bool, Option<String>) {
    match ext {
        "md" | "markdown" | "txt" | "json" => {
            let text = read_text_best_effort(p, 2_000_000);
            (text, "plain".to_string(), true, None)
        }
        "xlsx" | "xls" => {
            // Excel 文件转为 CSV 格式
            extract_with_agent_doc(p, output_path, "csv").await
        }
        "csv" => {
            // CSV 文件直接读取
            let text = read_text_best_effort(p, 2_000_000);
            (text, "plain".to_string(), true, None)
        }
        "doc" | "docx" | "odt" | "uot" | "uof" | "ofd" | "pdf" | "pptx" | "ppt" | "ods" | "odp" | "uos" | "uop" => {
            // 文档文件转为 Markdown 格式（使用 agent-doc 统一处理）
            extract_with_agent_doc(p, output_path, "md").await
        }
        _ => {
            let text = read_text_best_effort(p, 2_000_000);
            let success = !text.trim().is_empty();
            (text, "best_effort".to_string(), success, None)
        }
    }
}

/// 使用 agent-doc CLI 提取文档
/// 返回 (text, method, success, error_msg)
///
/// 对齐 agentlite file_processing.py `_extract_with_agent_doc`：
/// - 120 秒超时（agentlite 用 subprocess.run(timeout=120)），超时记 method="agent-doc_timeout"
/// - 命令未找到记 method="agent-doc_not_found"，其他启动失败记 method="agent-doc_error"
/// - 使用 tokio::process::Command 异步执行，避免同步阻塞 tokio worker（否则 agent-doc 卡死会
///   永久锁死持有 PROCESSING_LOCK 的文件处理任务）
async fn extract_with_agent_doc(p: &Path, output_path: &Path, format: &str) -> (String, String, bool, Option<String>) {
    // 确保输出目录存在
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut command = tokio::process::Command::new("agent-doc");
    command
        .arg("extract")
        .arg(p)
        .arg("-f")
        .arg(format)
        .arg("-o")
        .arg(output_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            let method = if e.kind() == std::io::ErrorKind::NotFound {
                "agent-doc_not_found"
            } else {
                "agent-doc_error"
            };
            return (
                String::new(),
                method.to_string(),
                false,
                Some(format!("Failed to run agent-doc: {}", e)),
            );
        }
    };

    // 120 秒超时，防止 agent-doc 卡死导致文件处理任务被永久锁住
    let status = match tokio::time::timeout(Duration::from_secs(120), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => {
            return (
                String::new(),
                "agent-doc_error".to_string(),
                false,
                Some(format!("Failed to wait for agent-doc: {}", e)),
            );
        }
        Err(_) => {
            warn!("agent-doc 提取超时 (120s): {}", p.display());
            let _ = child.kill().await;
            let _ = child.wait().await;
            return (
                String::new(),
                "agent-doc_timeout".to_string(),
                false,
                Some("agent-doc timed out after 120s".to_string()),
            );
        }
    };

    // 进程已退出，读取 stderr 用于错误信息
    let mut stderr_bytes = Vec::new();
    if let Some(mut se) = child.stderr.take() {
        let _ = se.read_to_end(&mut stderr_bytes).await;
    }

    if status.success() {
        // 读取输出文件内容
        if output_path.exists() {
            let text = match std::fs::read_to_string(output_path) {
                Ok(s) => s,
                Err(_) => String::new(),
            };
            let success = !text.trim().is_empty();
            (text, format!("agent-doc_{}", format), success, None)
        } else {
            (
                String::new(),
                format!("agent-doc_{}", format),
                false,
                Some("Output file not generated by agent-doc".to_string()),
            )
        }
    } else {
        let stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
        (
            String::new(),
            format!("agent-doc_{}", format),
            false,
            Some(stderr),
        )
    }
}

/// 构造 Tantivy 字段定义列表
fn make_tantivy_fields() -> Vec<TantivyFieldSpec> {
    vec![
        TantivyFieldSpec {
            name: "path".to_string(),
            field_type: "Text".to_string(),
            stored: true,
            indexed: false,
            fast: false,
            tokenized: false,
            term_frequencies: false,
            term_positions: false,
            tokenizer: None,
        },
        TantivyFieldSpec {
            name: "title".to_string(),
            field_type: "Text".to_string(),
            stored: true,
            indexed: true,
            fast: false,
            tokenized: true,
            term_frequencies: true,
            term_positions: true,
            tokenizer: Some("jieba".to_string()),
        },
        TantivyFieldSpec {
            name: "body".to_string(),
            field_type: "Text".to_string(),
            stored: true,
            indexed: true,
            fast: false,
            tokenized: true,
            term_frequencies: true,
            term_positions: true,
            tokenizer: Some("jieba".to_string()),
        },
        TantivyFieldSpec {
            name: "mime_type".to_string(),
            field_type: "Text".to_string(),
            stored: true,
            indexed: false,
            fast: false,
            tokenized: false,
            term_frequencies: false,
            term_positions: false,
            tokenizer: None,
        },
        TantivyFieldSpec {
            name: "modified_at".to_string(),
            field_type: "Text".to_string(),
            stored: true,
            indexed: false,
            fast: false,
            tokenized: false,
            term_frequencies: false,
            term_positions: false,
            tokenizer: None,
        },
        TantivyFieldSpec {
            name: "mtime_ns".to_string(),
            field_type: "u64".to_string(),
            stored: true,
            indexed: false,
            fast: false,
            tokenized: false,
            term_frequencies: false,
            term_positions: false,
            tokenizer: None,
        },
        TantivyFieldSpec {
            name: "size".to_string(),
            field_type: "u64".to_string(),
            stored: true,
            indexed: false,
            fast: false,
            tokenized: false,
            term_frequencies: false,
            term_positions: false,
            tokenizer: None,
        },
    ]
}

// ---------------------------------------------------------------------------
// 文件监控（watcher + 定时轮询）
// ---------------------------------------------------------------------------

use notify::{RecommendedWatcher, Watcher, RecursiveMode, Event, EventKind};
use tokio::sync::mpsc as tokio_mpsc;
use std::time::Duration;

/// 启动文件监控（watcher + 定时轮询）
pub async fn start_file_monitoring() {
    info!("Starting file monitoring (watcher + periodic scan)");
    
    // 先执行一次初始扫描
    scan_and_trigger_file_processing().await;
    
    let raw_dir = CONFIG.disks_raw_dir.clone();
    
    // 创建通道用于 watcher 触发处理
    let (watcher_tx, mut watcher_rx) = tokio_mpsc::channel::<()>(10);
    
    // 启动文件 watcher
    let watcher_handle = tokio::spawn({
        let raw_dir = raw_dir.clone();
        let tx = watcher_tx.clone();
        async move {
            if let Err(e) = start_file_watcher(&raw_dir, tx).await {
                error!("File watcher error: {}", e);
            }
        }
    });
    
    // 启动定时轮询（60秒）
    let poll_handle = tokio::spawn({
        let raw_dir = raw_dir.clone();
        async move {
            start_periodic_scan(&raw_dir, 60).await;
        }
    });
    
    // 等待 watcher 触发的事件
    tokio::spawn(async move {
        while let Some(()) = watcher_rx.recv().await {
            info!("File watcher detected changes, triggering processing...");
            trigger_file_processing_safe().await;
        }
    });
    
    info!("File monitoring started");
    
    // 保持任务运行
    let _ = tokio::join!(watcher_handle, poll_handle);
}

/// 启动文件 watcher（实时监听）
async fn start_file_watcher(raw_dir: &Path, tx: tokio_mpsc::Sender<()>) -> Result<()> {
    use tokio::time::sleep;
    
    let (sync_tx, sync_rx) = std::sync::mpsc::channel();
    
    // 创建 watcher
    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        match res {
            Ok(event) => {
                // 只关心创建和修改事件
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    let _ = sync_tx.send(());
                }
            }
            Err(e) => error!("Watch error: {:?}", e),
        }
    })?;
    
    // 监听目录
    watcher.watch(raw_dir, RecursiveMode::Recursive)?;
    info!("File watcher started on: {:?}", raw_dir);
    
    // 防抖：等待一段时间后再触发
    let debounce_duration = Duration::from_secs(5);
    
    // 转发事件到 async channel
    loop {
        match sync_rx.recv_timeout(Duration::from_secs(1)) {
            Ok(()) => {
                // 等待防抖时间
                sleep(debounce_duration).await;
                
                // 清空队列中的所有事件
                while sync_rx.try_recv().is_ok() {}
                
                // 触发处理
                if tx.send(()).await.is_err() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // 超时，继续等待
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
    
    Ok(())
}

/// 定时轮询扫描文件变更
async fn start_periodic_scan(_raw_dir: &Path, interval_seconds: u64) {
    info!("Starting periodic file scan every {}s", interval_seconds);
    
    let mut interval = tokio::time::interval(Duration::from_secs(interval_seconds));
    
    loop {
        interval.tick().await;
        scan_and_trigger_file_processing().await;
    }
}
