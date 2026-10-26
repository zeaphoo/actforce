use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Sqlite, Row};
use tracing::{info, warn, debug};

/// Builtin skill definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinSkill {
    pub id: Option<String>,
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
    pub generated_json: Option<Value>,
    #[serde(default)]
    pub created_by: Option<String>,
    #[serde(default)]
    pub updated_by: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Join a Value that is either a string or an array of strings into a String.
/// Arrays are joined with '\n'. Non-string values return None.
fn join_string_field(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Array(arr) => {
            if arr.is_empty() {
                return Some(String::new());
            }
            if arr.iter().all(|v| v.is_string()) {
                let joined: String = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                Some(joined)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Long-text fields within a step that support array-form input.
const STEP_LONG_FIELDS: &[&str] = &["action", "validation", "step_name"];
const LLM_PROMPT_LONG_FIELDS: &[&str] = &["system", "user"];

/// Normalize long-text fields in steps_plan: join string arrays only on known fields,
/// leaving lists like context_query and tags untouched.
fn normalize_step_long_fields(steps: &mut Value) {
    let Value::Array(steps_arr) = steps else { return };
    for step in steps_arr.iter_mut() {
        let Value::Object(step_obj) = step else { continue };

        // Top-level step fields
        for field in STEP_LONG_FIELDS {
            if let Some(v) = step_obj.get_mut(*field) {
                if let Some(joined) = join_string_field(v) {
                    *v = Value::String(joined);
                }
            }
        }

        // validation_failure_handling: only join if it's a string or string array
        if let Some(v) = step_obj.get_mut("validation_failure_handling") {
            if let Some(joined) = join_string_field(v) {
                *v = Value::String(joined);
            }
        }

        // llm_prompt.system / llm_prompt.user
        if let Some(Value::Object(llm_prompt)) = step_obj.get_mut("llm_prompt") {
            for field in LLM_PROMPT_LONG_FIELDS {
                if let Some(v) = llm_prompt.get_mut(*field) {
                    if let Some(joined) = join_string_field(v) {
                        *v = Value::String(joined);
                    }
                }
            }
        }

        // Recurse into children (nested blocks)
        if let Some(children) = step_obj.get_mut("children") {
            normalize_step_long_fields(children);
        }
    }
}

/// 从 .skill.json 内容加载单个技能。私有产品可复用该 loader 加载自己的技能。
pub fn load_skill_from_json_content(
    filename: &str,
    json_content: &str,
    md_content: Option<&str>,
) -> Option<BuiltinSkill> {
    let mut skill_json: Value = match serde_json::from_str(json_content) {
        Ok(v) => v,
        Err(e) => {
            warn!("Failed to parse skill JSON {}: {}", filename, e);
            return None;
        }
    };

    let obj = skill_json.as_object_mut()?;

    // Normalize description (top-level long field)
    if let Some(v) = obj.get_mut("description") {
        if let Some(joined) = join_string_field(v) {
            *v = Value::String(joined);
        }
    }

    // Normalize long-text fields inside generated_json.steps_plan
    if let Some(Value::Object(gen)) = obj.get_mut("generated_json") {
        if let Some(steps) = gen.get_mut("steps_plan") {
            normalize_step_long_fields(steps);
        }
    }

    let obj = skill_json.as_object()?;

    // 提取 skill_id
    let skill_id = obj.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            // 从文件名提取（去掉 .skill.json 后缀）
            filename.strip_suffix(".skill.json")
                .map(|s| s.to_string())
        });

    // 提取 name
    let name = obj.get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| skill_id.clone().unwrap_or_default());

    // 提取 description
    let description = obj.get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 提取 is_enabled
    let is_enabled = obj.get("is_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // 提取 program
    let program = obj.get("program")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // 提取 extra_json
    let extra_json = obj.get("extra_json").cloned();

    // 提取 generated_json
    let generated_json = obj.get("generated_json").cloned();

    // 使用传入的 md_content 作为 content（skill guide 文本）
    let content = md_content.map(|s| s.to_string());

    Some(BuiltinSkill {
        id: skill_id,
        name,
        description,
        is_enabled,
        content,
        program,
        extra_json,
        generated_json,
        created_by: Some("system".to_string()),
        updated_by: Some("system".to_string()),
    })
}

/// 从嵌入的文件加载所有内置技能
pub fn get_builtin_skills() -> Vec<BuiltinSkill> {
    // 使用 include_str! 在编译时嵌入 .skill.json 和 .md 文件
    const SKILL_FILES: &[(&str, &str, &str)] = &[
        (
            "default.skill.json",
            include_str!("skills/default.skill.json"),
            include_str!("skills/default.md"),
        ),
        (
            "document_organize.skill.json",
            include_str!("skills/document_organize.skill.json"),
            include_str!("skills/document_organize.md"),
        ),
        (
            "excel_query.skill.json",
            include_str!("skills/excel_query.skill.json"),
            include_str!("skills/excel_query.md"),
        ),
        (
            "search_and_answer.skill.json",
            include_str!("skills/search_and_answer.skill.json"),
            include_str!("skills/search_and_answer.md"),
        ),
    ];

    let mut skills = Vec::new();

    for (filename, json_content, md_content) in SKILL_FILES {
        if let Some(skill) = load_skill_from_json_content(filename, json_content, Some(md_content)) {
            debug!("Loaded builtin skill: {} ({:?})", skill.name, skill.id);
            skills.push(skill);
        }
    }

    // 按名称排序以保持一致性
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    info!("Loaded {} builtin skills (compiled into binary)", skills.len());
    skills
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_string_field() {
        // String → returns as-is
        assert_eq!(join_string_field(&Value::String("hello".into())), Some("hello".into()));

        // String array → joined
        assert_eq!(
            join_string_field(&serde_json::json!(["a", "b", "c"])),
            Some("a\nb\nc".into())
        );

        // Empty array → empty string
        assert_eq!(join_string_field(&serde_json::json!([])), Some("".into()));

        // Non-string array → None
        assert_eq!(join_string_field(&serde_json::json!([1, 2, 3])), None);

        // Number → None
        assert_eq!(join_string_field(&Value::Number(42.into())), None);
    }

    #[test]
    fn test_normalize_step_long_fields_leaves_context_query_untouched() {
        let mut steps = serde_json::json!([
            {
                "action": ["do", "something"],
                "context_query": ["$builtin_skills", "$search"],
                "tags": ["tag1", "tag2"],
                "llm_prompt": {
                    "system": ["sys1", "sys2"],
                    "user": "plain"
                }
            }
        ]);

        normalize_step_long_fields(&mut steps);

        let s = &steps[0];
        // action joined
        assert_eq!(s["action"], Value::String("do\nsomething".into()));
        // llm_prompt.system joined
        assert_eq!(s["llm_prompt"]["system"], Value::String("sys1\nsys2".into()));
        // llm_prompt.user unchanged (was plain string)
        assert_eq!(s["llm_prompt"]["user"], Value::String("plain".into()));
        // context_query NOT joined — stays as array
        assert_eq!(s["context_query"], serde_json::json!(["$builtin_skills", "$search"]));
        // tags NOT joined — stays as array
        assert_eq!(s["tags"], serde_json::json!(["tag1", "tag2"]));
    }

    #[test]
    fn test_normalize_step_long_fields_nested_children() {
        let mut steps = serde_json::json!([
            {
                "step_name": "parent",
                "step_type": "block",
                "children": [
                    {
                        "action": ["child", "action"],
                        "context_query": ["tag"]
                    }
                ]
            }
        ]);

        normalize_step_long_fields(&mut steps);

        assert_eq!(steps[0]["children"][0]["action"], Value::String("child\naction".into()));
        assert_eq!(steps[0]["children"][0]["context_query"], serde_json::json!(["tag"]));
    }

    #[test]
    fn test_load_builtin_skills() {
        let skills = get_builtin_skills();

        println!("Loaded {} skills", skills.len());
        for skill in &skills {
            println!("  - {} ({:?})", skill.name, skill.id);
        }

        // 应该加载至少一些技能
        assert!(!skills.is_empty(), "Should load at least some skills");

        // 验证关键技能是否加载
        let skill_ids: Vec<_> = skills.iter()
            .filter_map(|s| s.id.as_ref())
            .collect();

        assert!(skill_ids.iter().any(|id| id.contains("search_and_answer")), "Should load search_and_answer skill");
    }
}

/// Merge system settings into builtin skills (apply saved enable states)
pub async fn merge_builtin_skill_settings(
    pool: &Pool<Sqlite>,
    mut skills: Vec<BuiltinSkill>,
) -> anyhow::Result<Vec<BuiltinSkill>> {
    // Read system settings
    let result = sqlx::query("SELECT value_json FROM system_settings WHERE key = 'builtin_skills_enabled'")
        .fetch_optional(pool)
        .await?;

    let Some(row) = result else {
        return Ok(skills);
    };

    let value_json: String = row.get("value_json");
    let enabled_settings: serde_json::Map<String, Value> = match serde_json::from_str(&value_json) {
        Ok(obj) => obj,
        Err(_) => return Ok(skills),
    };

    // Merge enable states
    for skill in &mut skills {
        if let Some(id) = &skill.id {
            if let Some(enabled_value) = enabled_settings.get(id) {
                if let Some(enabled) = enabled_value.as_bool() {
                    skill.is_enabled = enabled;
                }
            }
        }
    }

    Ok(skills)
}

/// Save builtin skills enable states to system settings
pub async fn save_builtin_skills_enabled_state(
    pool: &Pool<Sqlite>,
    skills: &[BuiltinSkill],
) -> anyhow::Result<()> {
    // Build enabled state map
    let mut enabled_state = serde_json::Map::new();
    for skill in skills {
        if let Some(id) = &skill.id {
            if id.starts_with("sk_") {
                enabled_state.insert(id.clone(), Value::Bool(skill.is_enabled));
            }
        }
    }

    if enabled_state.is_empty() {
        return Ok(());
    }

    let value_json = serde_json::to_string(&enabled_state)?;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if exists
    let exists = sqlx::query("SELECT 1 FROM system_settings WHERE key = 'builtin_skills_enabled'")
        .fetch_optional(pool)
        .await?;

    if exists.is_some() {
        sqlx::query("UPDATE system_settings SET value_json = ?, updated_at = ? WHERE key = 'builtin_skills_enabled'")
            .bind(&value_json)
            .bind(&now)
            .execute(pool)
            .await?;
    } else {
        sqlx::query("INSERT INTO system_settings (key, value_json, created_at, updated_at) VALUES ('builtin_skills_enabled', ?, ?, ?)")
            .bind(&value_json)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
    }

    Ok(())
}
