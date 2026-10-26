#![allow(dead_code)]

//! Skill 转换工具 - 将 Markdown skill 文件转换为包含 JSON 步骤计划的格式
//!
//! 用法:
//!     actforce convert-skills loanrisk.md
//!     actforce convert-skills --all

use clap::Args;
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// LLM 配置
struct LLMConfig {
    base_url: String,
    model: String,
    api_key: String,
    temperature: f64,
    top_p: f64,
}

/// 步骤计划中的单个步骤
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StepPlan {
    step_name: String,
    step_type: String,
    action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    llm_prompt: Option<LlmPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output_target: Option<OutputTarget>,
    validation: String,
    validation_failure_handling: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OutputTarget {
    output_type: String,
    name: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmPrompt {
    system: String,
    user: String,
}

/// 解析 YAML front matter
fn parse_front_matter(content: &str) -> Option<(serde_yaml::Value, String)> {
    let re = Regex::new(r"^---\s*\n(.*?)\n---\s*\n").ok()?;
    let captures = re.captures(content)?;
    let yaml_str = captures.get(1)?.as_str();
    let front_matter: serde_yaml::Value = serde_yaml::from_str(yaml_str).ok()?;
    let rest = content[captures.get(0)?.end()..].to_string();
    Some((front_matter, rest))
}

/// 去除 front matter markers
fn remove_front_matter_markers(content: &str) -> String {
    let re = Regex::new(r"^---\n.*?\n---\n?").unwrap();
    re.replace_all(content, "").trim().to_string()
}

/// 从文本中提取 JSON
fn extract_json_from_text(text: &str) -> Option<Value> {
    let text = text.trim();

    // 尝试直接解析
    if let Ok(value) = serde_json::from_str(text) {
        return Some(value);
    }

    // 尝试从 markdown 代码块中提取
    let patterns = [
        r"```json\s*\n(.*?)\n```",
        r"```\s*\n(.*?)\n```",
        r"```json\s*(.*?)```",
        r"```\s*(.*?)```",
    ];

    for pattern in &patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(captures) = re.captures(text) {
                if let Some(json_str) = captures.get(1) {
                    let json_str = json_str.as_str().trim();
                    if let Ok(value) = serde_json::from_str(json_str) {
                        return Some(value);
                    }
                }
            }
        }
    }

    // 尝试找到最大的有效 JSON 对象
    let mut max_json = None;
    let mut max_len = 0;

    if let Ok(re) = Regex::new(r"\{[^{}]+\}") {
        for captures in re.find_iter(text) {
            let candidate = captures.as_str();
            if let Ok(value) = serde_json::from_str::<Value>(candidate) {
                if candidate.len() > max_len {
                    max_json = Some(value);
                    max_len = candidate.len();
                }
            }
        }
    }

    max_json
}

/// 调用 LLM
async fn call_llm(
    client: &Client,
    config: &LLMConfig,
    messages: Vec<serde_json::Value>,
) -> Result<String> {
    let response = client
        .post(format!("{}/chat/completions", config.base_url))
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(120))
        .json(&json!({
            "model": config.model,
            "messages": messages,
            "temperature": config.temperature,
            "top_p": config.top_p,
            "stream": false,
        }))
        .send()
        .await
        .context("Failed to send request to LLM")?;

    let response = response
        .error_for_status()
        .context("LLM request failed")?;

    let data: Value = response.json().await.context("Failed to parse LLM response")?;

    data["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .context("Failed to extract content from LLM response")
}

/// 生成步骤计划
async fn generate_steps_plan(
    client: &Client,
    config: &LLMConfig,
    skill_name: &str,
    skill_content: &str,
) -> Result<Vec<Value>> {
    let messages = vec![
        json!({
            "role": "system",
            "content": "你是一个任务规划助手。根据提供的 Skill Guide，生成需要执行的步骤计划。\n\n\
                ## 步骤类型说明（根据任务特点选择合适的类型）\n\
                - `call_llm`: **调用LLM处理** - 通用步骤类型，用于需要调用大模型处理的任务。包括：信息提取、内容生成、数据分析、审查判断等。系统会根据步骤的 llm_prompt 参数自动调用 LLM。\n\
                - `bash_command`: **执行命令** - 当需要运行系统命令时使用（调用外部工具、执行脚本等）。\n\
                ## 每个步骤需要包含\n\
                - step_name: 步骤名称\n\
                - step_type: 步骤类型（见上方说明）\n\
                - action: 执行动作详细描述\n\
                - llm_prompt: 如需要调用LLM，提供 {system, user} prompt模板\n\
                - output_target: 输出目标配置（可选），格式：{\"output_type\": \"context/file\", \"name\": \"输出名称\", \"tags\": [\"tag1\"], \"target_block\": \"current/parent/root\"}\n\
                - validation: 如何检查结果有效性（描述验证规则）\n\
                - validation_failure_handling: 验证失败处理方式"
        }),
        json!({
            "role": "user",
            "content": format!(
                "# Skill: {skill_name}\n\
                {skill_content}\n\n\
                # 任务\n\
                请根据 Skill Guide 生成执行步骤计划，返回 JSON 数组格式。\n\n\
                ## 输出格式\n\
                ```json\n\
                [\n\
                  {{\n\
                    \"step_name\": \"步骤名称\",\n\
                    \"step_type\": \"call_llm/bash_command\",\n\
                    \"action\": \"执行动作详细描述\",\n\
                    \"llm_prompt\": {{\n\
                      \"system\": \"system prompt模板\",\n\
                      \"user\": \"user prompt模板，使用{{input}}占位符\"\n\
                    }},\n\
                    \"output_target\": {{\n\
                      \"output_type\": \"context\",\n\
                      \"name\": \"步骤名称\",\n\
                      \"tags\": [\"$llm\"]\n\
                    }},\n\
                    \"output_target\": {{\n\
                      \"output_type\": \"context\",\n\
                      \"name\": \"步骤名称\",\n\
                      \"tags\": [\"$llm\"]\n\
                    }},\n\
                    \"validation\": \"检查结果有效性的规则描述\",\n\
                    \"validation_failure_handling\": \"验证失败处理方式\"\n\
                  }}\n\
                ]\n\
                ```\n\n\
                ## 注意事项\n\
                1. 注意步骤中的前置条件和情况判断，结合上下文，如果条件不满足，就不要生成步骤\n\
                2. output_target.output_type=\"context\" 表示结果会作为 context 传递给后续步骤\n\
                3. output_target.output_type=\"file\" 表示结果会保存为文件\n\
                4. validation 必须描述具体的验证规则\n\
                5. 只返回 JSON，不要其他内容"
            )
        }),
    ];

    let response = call_llm(client, config, messages).await?;

    // 解析 JSON
    let result = extract_json_from_text(&response);
    match result {
        Some(Value::Array(arr)) => Ok(arr),
        Some(Value::Object(obj)) => {
            // 尝试获取数组字段
            for key in &["steps", "plan", "steps_plan", "data"] {
                if let Some(Value::Array(arr)) = obj.get(*key) {
                    return Ok(arr.clone());
                }
            }
            Ok(vec![Value::Object(obj)])
        }
        _ => {
            eprintln!("  警告: 无法从 LLM 响应中提取 JSON");
            eprintln!("  响应内容: {}...", &response.chars().take(200).collect::<String>());
            Ok(vec![])
        }
    }
}

/// 转换单个 skill 文件
async fn convert_skill_file(
    md_file: &Path,
    config: &LLMConfig,
    output_dir: Option<&Path>,
) -> Result<bool> {
    println!("处理: {}", md_file.file_name().unwrap_or_default().to_string_lossy());

    let content = fs::read_to_string(md_file).context("读取文件失败")?;

    // 解析 front matter
    let (front_matter, rest_content) = parse_front_matter(&content)
        .ok_or_else(|| anyhow::anyhow!("没有 front matter"))?;

    let skill_name = front_matter
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| md_file.file_stem().unwrap_or_default().to_string_lossy().to_string());

    let skill_id = front_matter
        .get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| md_file.file_stem().unwrap_or_default().to_string_lossy().to_string());

    // 去除 front matter markers，获取纯正文
    let content_without_yaml = remove_front_matter_markers(&content);

    // 按 --- 分割内容
    let all_parts: Vec<&str> = content_without_yaml
        .split("---")
        .map(|s: &str| s.trim())
        .filter(|s: &&str| !s.is_empty())
        .collect();

    let content_parts = if all_parts.len() > 1 {
        &all_parts[1..]
    } else {
        &all_parts[..]
    };

    let content_parts = if content_parts.is_empty() {
        vec![content_without_yaml.as_str()]
    } else {
        content_parts.to_vec()
    };

    println!("  Skill: {skill_name} ({skill_id})");
    println!("  内容分为 {} 个部分", content_parts.len());

    // 调用 LLM 生成步骤计划
    let client = Client::new();
    let mut all_steps_plan = Vec::new();

    for (idx, content_part) in content_parts.iter().enumerate() {
        println!("  生成第 {}/{} 部分的步骤计划...", idx + 1, content_parts.len());

        match generate_steps_plan(&client, config, &skill_name, content_part).await {
            Ok(steps_plan) => {
                println!("  生成了 {} 个步骤", steps_plan.len());
                all_steps_plan.extend(steps_plan);
            }
            Err(e) => {
                eprintln!("  警告: 生成步骤计划失败 (部分 {}): {}", idx + 1, e);
            }
        }
    }

    // 将步骤计划添加到 front matter
    let mut front_matter = front_matter;
    if let Some(fm) = front_matter.as_mapping_mut() {
        let steps_plan_yaml = serde_yaml::to_value(&all_steps_plan)
            .unwrap_or(serde_yaml::Value::Null);
        let total_steps_yaml = serde_yaml::Value::Number(all_steps_plan.len().into());
        let generated_at_yaml = serde_yaml::Value::String("auto-generated".to_string());

        let mut generated_mapping = serde_yaml::Mapping::new();
        generated_mapping.insert(
            serde_yaml::Value::String("steps_plan".to_string()),
            steps_plan_yaml,
        );
        generated_mapping.insert(
            serde_yaml::Value::String("total_steps".to_string()),
            total_steps_yaml,
        );
        generated_mapping.insert(
            serde_yaml::Value::String("generated_at".to_string()),
            generated_at_yaml,
        );

        fm.insert(
            serde_yaml::Value::String("generated_json".to_string()),
            serde_yaml::Value::Mapping(generated_mapping),
        );
    }

    // 构建新的文件内容
    let new_front_matter = serde_yaml::to_string(&front_matter)
        .context("序列化 front matter 失败")?;
    let new_content = format!("---\n{new_front_matter}---\n\n{rest_content}");

    // 写入文件
    let output_path = if let Some(dir) = output_dir {
        let path = dir.join(md_file.file_name().unwrap_or_default());
        fs::create_dir_all(path.parent().unwrap())?;
        path
    } else {
        md_file.to_path_buf()
    };

    fs::write(&output_path, new_content).context("写入文件失败")?;
    println!("  已保存: {}", output_path.display());

    Ok(true)
}

#[derive(Args)]
pub struct ConvertSkillsArgs {
    /// Skill 文件路径
    #[arg(value_name = "FILE")]
    pub file: Option<PathBuf>,
    
    /// 转换所有 skill 文件
    #[arg(short, long)]
    pub all: bool,
    
    /// 输出目录
    #[arg(long)]
    pub output_dir: Option<PathBuf>,
    
    /// Skills 目录 (默认: src/actforce/skills)
    #[arg(long, default_value = "src/actforce/skills")]
    pub skills_dir: PathBuf,

    /// LLM 服务地址 (默认: 环境变量 SKILL_LLM_BASE_URL 或 http://127.0.0.1:8080/v1)
    #[arg(long)]
    pub llm_base_url: Option<String>,

    /// 模型名称 (默认: 环境变量 SKILL_LLM_MODEL 或 default)
    #[arg(long)]
    pub llm_model: Option<String>,

    /// API Key (默认: 环境变量 SKILL_LLM_API_KEY 或 no-key)
    #[arg(long)]
    pub llm_api_key: Option<String>,
}

/// 执行技能转换命令
pub async fn execute(args: ConvertSkillsArgs) -> Result<()> {
    // 命令行参数优先于 SKILL_LLM_* 环境变量，环境变量未设置时使用与
    // agentlite convert_skill.py 一致的默认值（与 Python argparse 默认值
    // DEFAULT_LLM_* = os.getenv("SKILL_LLM_*", <默认>) 的优先级完全相同）。
    let config = LLMConfig {
        base_url: args
            .llm_base_url
            .unwrap_or_else(|| env::var("SKILL_LLM_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080/v1".to_string())),
        model: args
            .llm_model
            .unwrap_or_else(|| env::var("SKILL_LLM_MODEL").unwrap_or_else(|_| "default".to_string())),
        api_key: args
            .llm_api_key
            .unwrap_or_else(|| env::var("SKILL_LLM_API_KEY").unwrap_or_else(|_| "no-key".to_string())),
        temperature: env::var("SKILL_TEMPERATURE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0),
        top_p: env::var("SKILL_TOP_P")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.9),
    };

    // 确定要处理的文件
    let files_to_process: Vec<PathBuf> = if let Some(file) = args.file {
        if !file.exists() {
            anyhow::bail!("文件不存在: {}", file.display());
        }
        vec![file]
    } else if args.all {
        if !args.skills_dir.exists() {
            anyhow::bail!("skills 目录不存在: {}", args.skills_dir.display());
        }

        let mut files: Vec<PathBuf> = fs::read_dir(&args.skills_dir)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map(|ext| ext == "md")
                    .unwrap_or(false)
            })
            .map(|entry| entry.path())
            .collect();

        files.sort();

        if files.is_empty() {
            println!("在 {} 中没有找到 .md 文件", args.skills_dir.display());
            return Ok(());
        }

        files
    } else {
        println!("请指定文件路径或使用 --all 参数");
        std::process::exit(1);
    };

    println!("LLM 配置:");
    println!("  Base URL: {}", config.base_url);
    println!("  Model: {}", config.model);
    println!("  Temperature: {}", config.temperature);
    println!("  Top P: {}", config.top_p);
    println!();

    let mut success_count = 0;

    for md_file in &files_to_process {
        match convert_skill_file(md_file, &config, args.output_dir.as_deref()).await {
            Ok(success) => {
                if success {
                    success_count += 1;
                }
            }
            Err(e) => {
                eprintln!("  错误: {e}");
            }
        }
        println!();
    }

    println!("完成: {success_count}/{} 个文件转换成功", files_to_process.len());

    Ok(())
}
