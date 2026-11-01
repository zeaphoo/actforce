use crate::shell_parser::analyze_bash;
use crate::python_parser::{analyze_python, check_dangerous_imports};
use crate::sql_parser::{validate_sql, extract_tables};
use crate::types::{ActionKind, ActionSpec, ExecuteResult};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, info, warn};
use uuid::Uuid;

/// 文件系统快照：relative_path -> (mtime_secs, size)
type FsSnapshot = HashMap<String, (f64, u64)>;

/// shell 参数引用：等价 Python shlex.quote（executor.py 使用 shlex.quote 引用每个参数）。
///
/// 仅含安全字符（ASCII 字母数字 + `_@%+=:,./-`）时原样返回；否则用单引号包裹，
/// 并把内部的单引号转义为 `'\"'\"'`（shell 拼接序列）。
fn quote_shell(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let safe = s.bytes().all(|b| {
        b.is_ascii_alphanumeric() || b == b'_' || b"@%+=:,./-".contains(&b)
    });
    if safe {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// 解析 timeout 参数：对齐 agentlite 的 `float(params.get("timeout"))` 强转，
/// 同时支持数字与字符串（如 "30"）。解析失败或非正数返回 None。
fn parse_timeout_secs(value: Option<&serde_json::Value>) -> Option<f64> {
    let v = value?;
    if let Some(f) = v.as_f64() {
        return Some(f);
    }
    if let Some(s) = v.as_str() {
        return s.trim().parse::<f64>().ok();
    }
    None
}

/// 对目录进行文件系统快照
fn take_fs_snapshot(dir: &Path) -> FsSnapshot {
    let mut snapshot = HashMap::new();
    if !dir.is_dir() {
        return snapshot;
    }
    walk_dir(dir, dir, &mut snapshot);
    snapshot
}

fn walk_dir(base: &Path, current: &Path, snapshot: &mut FsSnapshot) {
    let entries = match std::fs::read_dir(current) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.is_dir() {
            walk_dir(base, &path, snapshot);
        } else if path.is_file() {
            if let Ok(meta) = std::fs::metadata(&path) {
                let rel = path
                    .strip_prefix(base)
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| path.to_string_lossy().to_string());
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                snapshot.insert(rel, (mtime, meta.len()));
            }
        }
    }
}

/// 比较两个快照，返回文件变更
fn diff_snapshots(before: &FsSnapshot, after: &FsSnapshot) -> serde_json::Value {
    let mut created = Vec::new();
    let mut modified = Vec::new();
    let mut deleted = Vec::new();

    for (path, (mtime, size)) in after {
        if let Some((before_mtime, before_size)) = before.get(path) {
            if *mtime != *before_mtime || *size != *before_size {
                modified.push(path.clone());
            }
        } else {
            created.push(path.clone());
        }
    }

    for path in before.keys() {
        if !after.contains_key(path) {
            deleted.push(path.clone());
        }
    }

    serde_json::json!({
        "created": created,
        "modified": modified,
        "deleted": deleted,
        "total_new": created.len(),
        "total_modified": modified.len(),
        "total_deleted": deleted.len(),
    })
}

/// Action 执行器（与 actforce Executor 对齐）
pub struct Executor {
    root_dir: Option<PathBuf>,
    output_dir: Option<PathBuf>,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

impl Executor {
    // 与 actforce 对齐的允许命令列表
    const ALLOWED_BASH_PROGRAMS: &'static [&'static str] = &[
        "bash", "sh", "env", "printf", "echo", "cat", "head", "tail", "nl", "yes",
        "ls", "pwd", "whoami", "id", "groups", "date", "time", "timeout", "sleep",
        "true", "false", "test", "[", "mkdir", "rm", "cp", "mv", "chmod", "chown",
        "touch", "find", "xargs", "grep", "sed", "awk", "paste", "join", "column",
        "sort", "uniq", "cut", "tr", "wc", "tee", "split", "which", "uname",
        "hostname", "uptime", "who", "w", "ps", "pgrep", "pkill", "kill", "killall",
        "nice", "renice", "nohup", "nproc", "lscpu", "lsmem", "free", "df", "du",
        "tar", "gzip", "gunzip", "zip", "unzip", "bzcat", "xzcat", "zcat", "lz4",
        "lz4cat", "zstd", "zstdcat", "base64", "sha256sum", "sha1sum", "md5sum",
        "cksum", "file", "stat", "readlink", "dirname", "basename", "realpath",
        "curl", "wget", "ping", "traceroute", "tracepath", "dig", "nslookup", "host",
        "ip", "ss", "netstat", "python", "python3", "pip", "pip3", "node", "npm",
        "git", "make", "ripgrep", "rg", "jq", "qsv",
        // actforce 特有命令
        "agent-image", "agent-doc", "agent-crawl", "tansearch",
        // 对齐 agentlite ALLOWED_BASH_PROGRAMS：补齐 aupage / viacli
        "aupage", "viacli",
    ];

    // 与 actforce 对齐的危险程序列表（包含 bash/sh/zsh）
    const DANGEROUS_BASH_PROGRAMS: &'static [&'static str] = &[
        "sudo", "su", "rm", "mv", "chmod", "chown", "tee", "dd", "mkfs",
        "mount", "umount", "kill", "killall", "npm", "pip", "pkill",
        "bash", "sh", "zsh",
    ];

    pub fn new() -> Self {
        Self {
            root_dir: None,
            output_dir: None,
        }
    }

    pub fn with_root_dir(mut self, root_dir: impl Into<PathBuf>) -> Self {
        self.root_dir = Some(root_dir.into());
        self
    }

    pub fn with_output_dir(mut self, output_dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(output_dir.into());
        self
    }

    /// 兼容旧 API
    pub fn from_dirs(root_dir: Option<String>, output_dir: Option<String>) -> Self {
        Self {
            root_dir: root_dir.filter(|s| !s.is_empty()).map(PathBuf::from),
            output_dir: output_dir.filter(|s| !s.is_empty()).map(PathBuf::from),
        }
    }

    /// 执行 action
    pub async fn execute(&self, action: ActionSpec) -> anyhow::Result<ExecuteResult> {
        match action.kind {
            ActionKind::Http => self._execute_http(&action.params).await,
            ActionKind::Python => self._execute_python(&action.params).await,
            ActionKind::Bash => self._execute_bash(&action.params).await,
            ActionKind::Sql => self._execute_sql(&action.params).await,
        }
    }

    // ============================================================
    // HTTP 执行
    // ============================================================

    async fn _execute_http(&self, params: &HashMap<String, serde_json::Value>) -> anyhow::Result<ExecuteResult> {
        let url = match params.get("url").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(u) => u,
            _ => return Ok(ExecuteResult::error("http action requires string params.url")),
        };

        let use_urllib = params
            .get("use_urllib")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
            || params
                .get("impl")
                .and_then(|v| v.as_str())
                .map(|s| s.to_lowercase() == "urllib")
                .unwrap_or(false);

        if use_urllib {
            self._execute_http_with_reqwest(url, params).await
        } else {
            self._execute_http_with_curl(url, params).await
        }
    }

    /// 使用 reqwest 执行 HTTP 请求
    async fn _execute_http_with_reqwest(
        &self,
        url: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<ExecuteResult> {
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let timeout_secs = parse_timeout_secs(params.get("timeout")).filter(|t| *t > 0.0);

        let client = match reqwest::Client::builder()
            .timeout(timeout_secs.map(|t| Duration::from_secs_f64(t)).unwrap_or(Duration::from_secs(30)))
            .build()
        {
            Ok(c) => c,
            Err(e) => return Ok(ExecuteResult::error(format!("Failed to build HTTP client: {}", e))),
        };

        let mut req_builder = match method.as_str() {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "PUT" => client.put(url),
            "DELETE" => client.delete(url),
            "PATCH" => client.patch(url),
            _ => client.request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url),
        };

        // 添加 headers
        if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    req_builder = req_builder.header(k, val);
                }
            }
        }

        // 添加 body
        if let Some(json_data) = params.get("json") {
            req_builder = req_builder.json(json_data);
        } else if let Some(data) = params.get("data") {
            if let Some(data_str) = data.as_str() {
                req_builder = req_builder.body(data_str.to_string());
            } else {
                req_builder = req_builder.json(data);
            }
        }

        debug!("HTTP request: {} {}", method, url);

        match req_builder.send().await {
            Ok(resp) => {
                let status = resp.status();
                let status_code = status.as_u16() as i32;
                match resp.text().await {
                    Ok(body) => {
                        let ok = (200..300).contains(&status_code);
                        let mut result = ExecuteResult {
                            ok,
                            stdout: Some(body),
                            status_code: Some(status_code),
                            ..Default::default()
                        };
                        result.data = Some(serde_json::json!({"impl": "reqwest"}));
                        Ok(result)
                    }
                    Err(e) => Ok(ExecuteResult::error(format!("Failed to read response body: {}", e))),
                }
            }
            Err(e) => Ok(ExecuteResult::error(format!("HTTP request failed: {}", e))),
        }
    }

    /// 使用 curl 执行 HTTP 请求
    async fn _execute_http_with_curl(
        &self,
        url: &str,
        params: &HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<ExecuteResult> {
        let method = params
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("GET")
            .to_uppercase();

        let timeout_secs = parse_timeout_secs(params.get("timeout")).filter(|t| *t > 0.0);

        let follow_redirects = params
            .get("follow_redirects")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut cmd_parts = vec!["curl".to_string(), "-sS".to_string()];
        if follow_redirects {
            cmd_parts.push("-L".to_string());
        }
        cmd_parts.extend(["-X".to_string(), method]);

        // headers
        if let Some(headers) = params.get("headers").and_then(|v| v.as_object()) {
            for (k, v) in headers {
                if let Some(val) = v.as_str() {
                    cmd_parts.extend(["-H".to_string(), format!("{}: {}", k, val)]);
                }
            }
        }

        // data
        if let Some(json_data) = params.get("json") {
            let data = serde_json::to_string(json_data).unwrap_or_default();
            cmd_parts.extend(["--data".to_string(), data]);
        } else if let Some(data) = params.get("data") {
            let data_str = if let Some(s) = data.as_str() {
                s.to_string()
            } else {
                serde_json::to_string(data).unwrap_or_default()
            };
            cmd_parts.extend(["--data".to_string(), data_str]);
        }

        if let Some(timeout) = timeout_secs {
            cmd_parts.extend(["--max-time".to_string(), timeout.to_string()]);
        }

        // 捕获状态码
        let trailer = "\n__ACTFORCE_HTTP_STATUS__:%{http_code}\n";
        cmd_parts.extend(["-w".to_string(), trailer.to_string(), url.to_string()]);

        // 对齐 agentlite executor.py:413：对每个参数做 shell 引用，防止 URL/header/data
        // 中的 shell 元字符导致命令截断或注入。
        let cmd = cmd_parts
            .iter()
            .map(|p| quote_shell(p))
            .collect::<Vec<_>>()
            .join(" ");
        let mut bash_params = HashMap::new();
        bash_params.insert("cmd".to_string(), serde_json::Value::String(cmd));

        if let Some(cwd) = params.get("__cwd").and_then(|v| v.as_str()) {
            bash_params.insert("__cwd".to_string(), serde_json::Value::String(cwd.to_string()));
        }

        let res = self._execute_bash(&bash_params).await?;

        let Some(stdout) = &res.stdout else {
            return Ok(res);
        };

        let marker = "__ACTFORCE_HTTP_STATUS__:";
        if !stdout.contains(marker) {
            return Ok(res);
        }

        let mut status_code: Option<i32> = None;
        let mut body = stdout.clone();

        if let Some(pos) = stdout.rfind(marker) {
            body = stdout[..pos].to_string();
            let tail = &stdout[pos + marker.len()..];
            if let Some(line) = tail.lines().next() {
                let code_str = line.trim();
                if let Ok(code) = code_str.parse::<i32>() {
                    status_code = Some(code);
                }
            }
        }

        let ok = status_code.map(|c| (200..300).contains(&c)).unwrap_or(res.ok);
        let mut result = ExecuteResult {
            ok,
            stdout: Some(body.trim_end_matches('\n').to_string()),
            stderr: res.stderr,
            exit_code: res.exit_code,
            status_code,
            ..Default::default()
        };
        result.data = Some(serde_json::json!({"impl": "curl"}));
        Ok(result)
    }

    // ============================================================
    // Python 执行
    // ============================================================

    async fn _execute_python(&self, params: &HashMap<String, serde_json::Value>) -> anyhow::Result<ExecuteResult> {
        // 支持 code/source 参数执行代码
        let code = params
            .get("code")
            .or_else(|| params.get("source"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        if let Some(code_str) = code {
            // 验证 Python 语法
            let validation = analyze_python(code_str);
            if !validation.ok {
                let error_msgs: Vec<String> = validation.errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                let mut result = ExecuteResult::error(format!("Python syntax error: {}", error_msgs.join("; ")));
                result.data = Some(serde_json::json!({"validation": "failed"}));
                return Ok(result);
            }

            // 检查危险导入
            let dangerous = check_dangerous_imports(code_str);
            if !dangerous.is_empty() {
                warn!("execute_python: dangerous imports detected: {:?}", dangerous);
            }

            let py_bin = params
                .get("python")
                .and_then(|v| v.as_str())
                .unwrap_or("python3");
            let delim = format!("__ACTFORCE_PY_{}", &Uuid::new_v4().simple().to_string()[..10]);
            let body = code_str.trim_end();
            // 对齐 agentlite executor.py:475：对 py_bin 做 shell 引用
            let cmd = format!("{} - <<'{}'\n{}\n{}\n", quote_shell(py_bin), delim, body, delim);

            let mut bash_params = HashMap::new();
            bash_params.insert("cmd".to_string(), serde_json::Value::String(cmd));
            if let Some(t) = params.get("timeout") {
                bash_params.insert("timeout".to_string(), t.clone());
            }
            if let Some(cwd) = params.get("__cwd").and_then(|v| v.as_str()) {
                bash_params.insert("__cwd".to_string(), serde_json::Value::String(cwd.to_string()));
            }
            return self._execute_bash(&bash_params).await;
        }

        // 支持 file 参数执行文件
        if let Some(file_path) = params.get("file").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            let py_bin = params
                .get("python")
                .and_then(|v| v.as_str())
                .unwrap_or("python3");
            // 对齐 agentlite executor.py:485-492：优先 argv，其次 args；非字符串值强转为字符串
            let extra_args: Vec<String> = params
                .get("argv")
                .or_else(|| params.get("args"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))
                        .collect()
                })
                .unwrap_or_default();

            // 对齐 agentlite executor.py:492：py_bin / file / argv 每一项都做 shell 引用
            let mut cmd_parts = vec![py_bin.to_string(), file_path.to_string()];
            cmd_parts.extend(extra_args);
            let cmd = cmd_parts
                .iter()
                .map(|p| quote_shell(p))
                .collect::<Vec<_>>()
                .join(" ");

            let mut bash_params = HashMap::new();
            bash_params.insert("cmd".to_string(), serde_json::Value::String(cmd));
            if let Some(t) = params.get("timeout") {
                bash_params.insert("timeout".to_string(), t.clone());
            }
            if let Some(cwd) = params.get("__cwd").and_then(|v| v.as_str()) {
                bash_params.insert("__cwd".to_string(), serde_json::Value::String(cwd.to_string()));
            }
            return self._execute_bash(&bash_params).await;
        }

        Ok(ExecuteResult::error("python action requires params.code, params.source, or params.file"))
    }

    // ============================================================
    // Bash 执行
    // ============================================================

    /// 基于 AST 的命令白名单检查（与 actforce 对齐）
    ///
    /// 对齐 agentlite executor.py：`_bash_disallowed_programs` 定义了但从未被调用，
    /// bash 执行只拦 dangerous 列表。此处保留函数仅作参考，不参与执行路径。
    #[allow(dead_code)]
    fn _bash_disallowed_programs(&self, script: &str) -> HashSet<String> {
        let mut disallowed = HashSet::new();
        let allowed_set: HashSet<&str> = Self::ALLOWED_BASH_PROGRAMS.iter().copied().collect();

        let analysis = analyze_bash(script);

        // 如果有解析错误，使用 fallback 解析
        if !analysis.errors.is_empty() {
            warn!("shell_parser had errors, using fallback: {:?}", analysis.errors);
            return self._bash_disallowed_fallback(script, &allowed_set);
        }

        for cmd in &analysis.main_commands {
            if cmd == "." || cmd == "source" || cmd == "sudo" {
                disallowed.insert(cmd.clone());
                continue;
            }

            if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("../") {
                disallowed.insert(cmd.clone());
                continue;
            }

            let prog = cmd.split('/').last().unwrap_or(cmd);
            if !prog.is_empty() && !allowed_set.contains(prog) {
                disallowed.insert(prog.to_string());
            }
        }

        disallowed
    }

    #[allow(dead_code)]
    fn _bash_disallowed_fallback(&self, script: &str, allowed_set: &HashSet<&str>) -> HashSet<String> {
        let mut disallowed = HashSet::new();
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let cmd = parts[0].trim_end_matches(|c: char| c == ';' || c == '|' || c == '&' || c == '(' || c == ')');

            if cmd.is_empty() {
                continue;
            }

            if cmd == "." || cmd == "source" || cmd == "sudo" {
                disallowed.insert(cmd.to_string());
                continue;
            }

            if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("../") {
                disallowed.insert(cmd.to_string());
                continue;
            }

            let prog = cmd.split('/').last().unwrap_or(cmd);
            if !prog.is_empty() && !allowed_set.contains(prog) {
                disallowed.insert(prog.to_string());
            }
        }
        disallowed
    }

    /// 基于 AST 的危险程序检测（与 actforce 对齐）
    fn _bash_dangerous_programs(&self, script: &str) -> HashSet<String> {
        let mut found = HashSet::new();
        let dangerous_set: HashSet<&str> = Self::DANGEROUS_BASH_PROGRAMS.iter().copied().collect();

        let analysis = analyze_bash(script);

        if !analysis.errors.is_empty() {
            warn!("shell_parser had errors, using fallback: {:?}", analysis.errors);
            return self._bash_dangerous_fallback(script, &dangerous_set);
        }

        for cmd in &analysis.main_commands {
            if cmd == "." || cmd == "source" {
                continue;
            }
            if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("../") {
                continue;
            }

            let prog = cmd.split('/').last().unwrap_or(cmd);
            if !prog.is_empty() && dangerous_set.contains(prog) {
                found.insert(prog.to_string());
            }
        }

        found
    }

    fn _bash_dangerous_fallback(&self, script: &str, dangerous_set: &HashSet<&str>) -> HashSet<String> {
        let mut found = HashSet::new();
        for line in script.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }

            let cmd = parts[0].trim_end_matches(|c: char| c == ';' || c == '|' || c == '&' || c == '(' || c == ')');

            if cmd.is_empty() {
                continue;
            }

            if cmd == "." || cmd == "source" {
                continue;
            }
            if cmd.starts_with('/') || cmd.starts_with("./") || cmd.starts_with("../") {
                continue;
            }

            let prog = cmd.split('/').last().unwrap_or(cmd);
            if !prog.is_empty() && dangerous_set.contains(prog) {
                found.insert(prog.to_string());
            }
        }
        found
    }

    // ============================================================
    // 路径安全（与 actforce 对齐）
    // ============================================================

    fn _resolve_cwd(&self, params: &HashMap<String, serde_json::Value>) -> Option<PathBuf> {
        let cwd = params
            .get("__cwd")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
            .or_else(|| self.root_dir.as_ref().map(|p| p.to_string_lossy().to_string()));

        let Some(cwd_str) = cwd else {
            return None;
        };

        let cwd_path = self._resolve_under_root(&cwd_str).ok()?;

        // 尝试创建目录
        let _ = std::fs::create_dir_all(&cwd_path);

        Some(cwd_path)
    }

    fn _root_path(&self) -> Option<PathBuf> {
        self.root_dir.as_ref().and_then(|p| {
            p.to_str()
                .filter(|s| !s.trim().is_empty())
                .map(|_| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        })
    }

    #[allow(dead_code)]
    fn _path_within_root(&self, p: &Path) -> bool {
        let Some(rp) = self._root_path() else {
            return true;
        };
        match std::fs::canonicalize(p) {
            Ok(resolved) => resolved.starts_with(&rp),
            Err(_) => true,
        }
    }

    fn _resolve_under_root(&self, p: &str) -> Result<PathBuf> {
        let Some(rp) = self._root_path() else {
            return Ok(PathBuf::from(p));
        };

        let pp = PathBuf::from(p);
        let resolved = if pp.is_absolute() {
            pp
        } else {
            rp.join(&pp)
        };

        let resolved = std::fs::canonicalize(&resolved).unwrap_or_else(|_| resolved);

        if !resolved.starts_with(&rp) {
            return Err(anyhow::anyhow!("path escapes root_dir"));
        }

        Ok(resolved)
    }

    /// 执行 Bash action
    async fn _execute_bash(&self, params: &HashMap<String, serde_json::Value>) -> anyhow::Result<ExecuteResult> {
        let cmd_str = match params
            .get("cmd")
            .or_else(|| params.get("command"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            Some(c) => c,
            _ => return Ok(ExecuteResult::error("bash action requires string params.cmd")),
        };

        // 检查危险程序（基于 AST）
        let dangerous = self._bash_dangerous_programs(cmd_str);
        if !dangerous.is_empty() {
            let mut dangerous_sorted: Vec<String> = dangerous.into_iter().collect();
            dangerous_sorted.sort();

            let allowed: Vec<String> = Self::ALLOWED_BASH_PROGRAMS.iter().map(|s| s.to_string()).collect();
            warn!(
                "execute_bash: dangerous bash programs detected: {:?}, cmd={:?}",
                dangerous_sorted, cmd_str
            );

            let mut result = ExecuteResult::error(format!(
                "bash action contains dangerous programs: {}",
                dangerous_sorted.join(", ")
            ));
            result.data = Some(serde_json::json!({
                "dangerous_programs": dangerous_sorted,
                "allowed_programs": allowed,
            }));
            return Ok(result);
        }

        // 对齐 agentlite executor.py：白名单检查（_bash_disallowed_programs）定义了但从未被调用，
        // bash 执行只拦 dangerous 列表，非白名单命令（docker/apt-get 等）照常执行。
        // 因此这里不再做白名单强拦。

        // 解析 timeout（对齐 agentlite：float() 强转，字符串 "30" 也可解析）
        let timeout_secs = parse_timeout_secs(params.get("timeout")).filter(|t| *t > 0.0);

        // 解析 cwd
        let cwd = self._resolve_cwd(params);

        // 捕获执行前的文件系统快照
        let work_dir = cwd.clone().or_else(|| self.root_dir.clone());
        let snapshot_before = work_dir.as_ref().map(|d| take_fs_snapshot(d));

        // 执行命令
        let mut command = Command::new("/bin/bash");
        command.arg("-c").arg(cmd_str);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        if let Some(ref cwd_path) = cwd {
            command.current_dir(cwd_path);
        }

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => return Ok(ExecuteResult::error(format!("Failed to spawn bash process: {}", e))),
        };

        // 预先接管 stdout/stderr 管道：超时 kill 进程后仍能收集部分输出（对齐 agentlite executor.py:639-643）。
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let stdout_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut pipe) = stdout_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut pipe) = stderr_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });

        let mut result = if let Some(dur) = timeout_secs {
            match timeout(Duration::from_secs_f64(dur), child.wait()).await {
                Ok(Ok(status)) => {
                    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
                    let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();
                    let exit_code = status.code().unwrap_or(-1);
                    ExecuteResult {
                        ok: exit_code == 0,
                        stdout: Some(stdout),
                        stderr: Some(stderr),
                        exit_code: Some(exit_code),
                        ..Default::default()
                    }
                }
                Ok(Err(e)) => ExecuteResult::error(format!("Failed to execute command: {}", e)),
                Err(_) => {
                    // 超时：kill 子进程并回收，避免孤儿进程泄漏；返回已收集的部分 stdout，stderr="timeout"
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
                    let _partial_stderr =
                        String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();
                    let mut result = ExecuteResult::error("timeout");
                    result.stdout = Some(stdout);
                    result
                }
            }
        } else {
            match child.wait().await {
                Ok(status) => {
                    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
                    let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();
                    let exit_code = status.code().unwrap_or(-1);
                    ExecuteResult {
                        ok: exit_code == 0,
                        stdout: Some(stdout),
                        stderr: Some(stderr),
                        exit_code: Some(exit_code),
                        ..Default::default()
                    }
                }
                Err(e) => ExecuteResult::error(format!("Failed to execute command: {}", e)),
            }
        };

        // 捕获执行后的文件系统快照并计算差异
        if let Some(ref dir) = work_dir {
            let snapshot_after = take_fs_snapshot(dir);
            if let Some(before) = snapshot_before {
                let file_changes = diff_snapshots(&before, &snapshot_after);
                let total: usize = file_changes["total_new"].as_u64().unwrap_or(0) as usize
                    + file_changes["total_modified"].as_u64().unwrap_or(0) as usize
                    + file_changes["total_deleted"].as_u64().unwrap_or(0) as usize;
                if total > 0 {
                    info!(
                        "execute_bash: file changes detected: new={}, modified={}, deleted={}",
                        file_changes["total_new"], file_changes["total_modified"], file_changes["total_deleted"]
                    );
                    result.data = Some(match result.data.take() {
                        Some(existing) => {
                            let mut merged = existing;
                            if let Some(obj) = merged.as_object_mut() {
                                obj.insert("file_changes".to_string(), file_changes);
                            }
                            merged
                        }
                        None => file_changes,
                    });
                }
            }
        }

        // 如果是 tansearch 命令，格式化输出
        if cmd_str.to_lowercase().contains("tansearch") {
            if let Some(ref stdout) = result.stdout {
                let formatted = Self::_format_tansearch_output(stdout);
                result.stdout = Some(formatted);
            }
        }

        Ok(result)
    }
    // ============================================================

    async fn _execute_sql(&self, params: &HashMap<String, serde_json::Value>) -> anyhow::Result<ExecuteResult> {
        let query = params
            .get("query")
            .or_else(|| params.get("sql"))
            .or_else(|| params.get("qsv_sql"))
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty());

        let Some(query_str) = query else {
            return Ok(ExecuteResult::error("sql action requires string params.query or params.sql"));
        };

        // 验证 SQL 语法
        let validation = validate_sql(query_str, None);
        if !validation.ok {
            let error_msgs: Vec<String> = validation.errors.iter()
                .map(|e| e.message.clone())
                .collect();
            let mut result = ExecuteResult::error(format!("SQL syntax error: {}", error_msgs.join("; ")));
            result.data = Some(serde_json::json!({"validation": "failed"}));
            return Ok(result);
        }

        // 提取表名用于审计
        if let Ok(tables) = extract_tables(query_str, None) {
            if !tables.is_empty() {
                debug!("SQL query references tables: {:?}", tables);
            }
        }

        let qsv_path = params
            .get("qsv_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("qsv");

        let output_format = params
            .get("output_format")
            .or_else(|| params.get("format"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("csv");

        let output_file = params
            .get("output_file")
            .or_else(|| params.get("output"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());

        let timeout_secs = parse_timeout_secs(params.get("timeout")).filter(|t| *t > 0.0);

        // 处理表映射 (table_name:file_path)
        let table_files: HashMap<String, String> = params
            .get("tables")
            .or_else(|| params.get("files"))
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        // 构建 qsv sqlp 命令
        let mut cmd_parts = vec![qsv_path.to_string(), "sqlp".to_string()];

        if output_format.to_lowercase() != "csv" {
            cmd_parts.extend(["--format".to_string(), output_format.to_string()]);
        }

        if let Some(of) = output_file {
            cmd_parts.extend(["--output".to_string(), of.to_string()]);
        }

        // 对齐 agentlite executor.py:909-911, 922-923, 937-938：delimiter/delim 透传 --delimiter
        let delimiter = params
            .get("delimiter")
            .or_else(|| params.get("delim"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        if let Some(delim) = delimiter {
            cmd_parts.extend(["--delimiter".to_string(), delim.to_string()]);
        }

        // qsv 选项
        let qsv_options = [
            ("streaming", "streaming"),
            ("try_parsedates", "try-parsedates"),
            ("cache_schema", "cache-schema"),
            ("low_memory", "low-memory"),
            ("ignore_errors", "ignore-errors"),
            ("no_optimizations", "no-optimizations"),
            ("truncate_ragged_lines", "truncate-ragged-lines"),
        ];

        for (param_key, qsv_flag) in &qsv_options {
            if params.get(*param_key).and_then(|v| v.as_bool()).unwrap_or(false) {
                cmd_parts.push(format!("--{}", qsv_flag));
            }
        }

        // 添加表映射
        for (table, file_path) in &table_files {
            cmd_parts.push(format!("{}:{}", table, file_path));
        }

        // 添加 SQL 查询
        cmd_parts.push(query_str.to_string());

        info!("execute_sql, argv: {:?}", cmd_parts);

        // 执行 qsv 命令
        let cwd = self._resolve_cwd(params);
        let mut command = Command::new(&cmd_parts[0]);
        command.args(&cmd_parts[1..]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        if let Some(cwd_path) = &cwd {
            command.current_dir(cwd_path);
        }

        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => return Ok(ExecuteResult::error(format!("qsv not found: {}", e))),
        };

        // 超时 kill 进程后仍能收集部分输出（对齐 agentlite executor.py:840-851）
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let stdout_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut pipe) = stdout_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });
        let stderr_task = tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = Vec::new();
            if let Some(mut pipe) = stderr_pipe {
                let _ = pipe.read_to_end(&mut buf).await;
            }
            buf
        });

        let wait_result = match timeout_secs {
            Some(t) => match timeout(Duration::from_secs_f64(t), child.wait()).await {
                Ok(Ok(status)) => Ok(status),
                Ok(Err(e)) => Err(anyhow::anyhow!("qsv execution failed: {}", e)),
                Err(_) => {
                    // 超时：kill 进程并回收，返回已收集的部分 stdout，stderr="timeout"
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
                    let _partial_stderr =
                        String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();
                    let mut result = ExecuteResult::error("timeout");
                    result.stdout = Some(stdout);
                    result.data = Some(serde_json::json!({"impl": "qsv"}));
                    return Ok(result);
                }
            },
            None => child
                .wait()
                .await
                .map_err(|e| anyhow::anyhow!("qsv execution failed: {}", e)),
        };

        let output = match wait_result {
            Ok(status) => {
                let stdout = String::from_utf8_lossy(&stdout_task.await.unwrap_or_default()).to_string();
                let stderr = String::from_utf8_lossy(&stderr_task.await.unwrap_or_default()).to_string();
                let exit_code = status.code().unwrap_or(-1);
                ExecuteResult {
                    ok: exit_code == 0,
                    stdout: Some(stdout),
                    stderr: Some(stderr),
                    exit_code: Some(exit_code),
                    ..Default::default()
                }
            }
            Err(e) => {
                let mut result = ExecuteResult::error(e.to_string());
                result.data = Some(serde_json::json!({"impl": "qsv"}));
                return Ok(result);
            }
        };

        let mut result = output;
        let mut data = serde_json::json!({"impl": "qsv"});
        if let Some(of) = output_file {
            data["output_file"] = serde_json::json!(of);
        }
        data["output_format"] = serde_json::json!(output_format);
        result.data = Some(data);
        Ok(result)
    }

    // ============================================================
    // 辅助方法
    // ============================================================

    /// 格式化 tansearch 输出为中文 key 的结构化格式
    fn _format_tansearch_output(stdout: &str) -> String {
        let lines: Vec<&str> = stdout.trim().lines().collect();
        if lines.is_empty() || lines[0].trim().is_empty() {
            return stdout.to_string();
        }

        // 尝试解析 JSONL
        let results: Vec<serde_json::Value> = lines
            .iter()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();

        if results.is_empty() {
            return stdout.to_string();
        }

        let mut parts = Vec::new();
        parts.push(format!("## 检索到 {} 个相关文档\n", results.len()));

        for (idx, item) in results.iter().enumerate() {
            let doc = item.get("doc").and_then(|d| d.as_object());
            let score = item.get("score").and_then(|s| s.as_f64()).unwrap_or(0.0);

            let Some(doc) = doc else { continue };

            let extract_field = |field_name: &str, default: &str| -> String {
                doc.get(field_name)
                    .map(|v| {
                        if let Some(arr) = v.as_array() {
                            arr.first().and_then(|x| x.as_str()).unwrap_or(default).to_string()
                        } else {
                            v.as_str().unwrap_or(default).to_string()
                        }
                    })
                    .unwrap_or_else(|| default.to_string())
            };

            let path = extract_field("path", "");
            let title = extract_field("title", "");
            let mime_type = extract_field("mime_type", "");
            let size = extract_field("size", "0").parse::<f64>().unwrap_or(0.0);
            let body = extract_field("body", "");

            parts.push(format!("### 文档 {}", idx + 1));
            parts.push(format!("- **文件路径**: {}", path));
            if !title.is_empty() {
                parts.push(format!("- **文件名**: {}", title));
            }
            if !mime_type.is_empty() {
                parts.push(format!("- **文件类型**: {}", mime_type));
            }
            if size > 0.0 {
                parts.push(format!("- **文件大小**: {:.1} KB", size / 1024.0));
            }
            parts.push(format!("- **相关度评分**: {:.2}", score));

            if !body.is_empty() {
                let body_preview = if body.len() > 500 {
                    format!("{}...", &body[..500])
                } else {
                    body.clone()
                };
                parts.push("- **内容摘要**:".to_string());
                parts.push("```".to_string());
                parts.push(body_preview);
                parts.push("```".to_string());
            }

            parts.push(String::new());
        }

        parts.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_executor_bash_echo() {
        let executor = Executor::new();
        let mut params = HashMap::new();
        params.insert("cmd".to_string(), serde_json::Value::String("echo hello".to_string()));
        let action = ActionSpec {
            kind: ActionKind::Bash,
            params,
        };

        let result = executor.execute(action).await.unwrap();
        assert!(result.ok);
        assert!(result.stdout.as_ref().unwrap().contains("hello"));
    }

    #[tokio::test]
    async fn test_executor_bash_timeout() {
        let executor = Executor::new();
        let mut params = HashMap::new();
        params.insert("cmd".to_string(), serde_json::Value::String("sleep 10".to_string()));
        params.insert("timeout".to_string(), serde_json::json!(0.1));
        let action = ActionSpec {
            kind: ActionKind::Bash,
            params,
        };

        let result = executor.execute(action).await.unwrap();
        assert!(!result.ok);
        assert!(result.stderr.as_ref().unwrap().contains("timeout"));
    }

    #[tokio::test]
    async fn test_executor_python_syntax_validation() {
        let executor = Executor::new();
        let mut params = HashMap::new();
        params.insert("code".to_string(), serde_json::Value::String("def foo(:\n    pass".to_string()));
        let action = ActionSpec {
            kind: ActionKind::Python,
            params,
        };

        let result = executor.execute(action).await.unwrap();
        assert!(!result.ok);
        assert!(result.stderr.as_ref().unwrap().contains("syntax error"));
    }

    #[tokio::test]
    async fn test_executor_sql_validation() {
        let executor = Executor::new();
        let mut params = HashMap::new();
        params.insert("query".to_string(), serde_json::Value::String("SELECTT * FORM".to_string()));
        let action = ActionSpec {
            kind: ActionKind::Sql,
            params,
        };

        let result = executor.execute(action).await.unwrap();
        // qsv may not be installed, but SQL validation should catch syntax first
        // or qsv not found error
        assert!(!result.ok);
    }

    #[test]
    fn test_fs_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let snap1 = take_fs_snapshot(dir.path());

        // Create a file
        std::fs::write(dir.path().join("test.txt"), "hello").unwrap();
        let snap2 = take_fs_snapshot(dir.path());

        let diff = diff_snapshots(&snap1, &snap2);
        assert_eq!(diff["total_new"].as_u64().unwrap(), 1);
    }
}
