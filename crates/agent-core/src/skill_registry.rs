use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Skill definition matching the .skill.json format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub is_enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_json: Option<Value>,
    /// Whether this skill was loaded from builtins (not user-editable)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(default = "chrono::Utc::now")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "chrono::Utc::now")]
    pub updated_at: DateTime<Utc>,
}

fn default_true() -> bool {
    true
}

/// In-memory skill registry with JSON file persistence for user skills
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDef>,
    builtin_ids: HashSet<String>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    // ── registration ──

    /// Register or update a skill. Returns the previous skill if it existed.
    pub fn register(&mut self, mut skill: SkillDef) -> Option<SkillDef> {
        if skill.is_builtin {
            self.builtin_ids.insert(skill.id.clone());
            skill.created_by = Some("system".into());
            skill.updated_by = Some("system".into());
        }
        skill.updated_at = Utc::now();
        self.skills.insert(skill.id.clone(), skill)
    }

    /// Remove a user skill. Refuses to remove builtins.
    pub fn unregister(&mut self, id: &str) -> Result<(), String> {
        if self.builtin_ids.contains(id) {
            return Err(format!("cannot remove builtin skill: {id}"));
        }
        self.skills.remove(id);
        Ok(())
    }

    /// Toggle enabled state. Returns the new state.
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Option<bool> {
        self.skills.get_mut(id).map(|s| {
            s.is_enabled = enabled;
            s.updated_at = Utc::now();
            enabled
        })
    }

    // ── queries ──

    pub fn get(&self, id: &str) -> Option<&SkillDef> {
        self.skills.get(id)
    }

    pub fn list_all(&self) -> Vec<&SkillDef> {
        let mut v: Vec<_> = self.skills.values().collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        v
    }

    pub fn list_enabled(&self) -> Vec<&SkillDef> {
        self.skills
            .values()
            .filter(|s| s.is_enabled)
            .collect()
    }

    pub fn count(&self) -> usize {
        self.skills.len()
    }

    /// Simple keyword search on name + description
    pub fn search(&self, query: &str) -> Vec<&SkillDef> {
        let q = query.to_lowercase();
        let mut results: Vec<&SkillDef> = self
            .skills
            .values()
            .filter(|s| {
                s.name.to_lowercase().contains(&q)
                    || s.description
                        .as_ref()
                        .map(|d| d.to_lowercase().contains(&q))
                        .unwrap_or(false)
            })
            .collect();
        results.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        results
    }

    // ── persistence (user skills only) ──

    /// Save user skills to a JSON file. Builtin skills are skipped.
    pub fn save_user_skills(&self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let user_skills: Vec<&SkillDef> = self
            .skills
            .values()
            .filter(|s| !s.is_builtin)
            .collect();
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&user_skills)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load user skills from a JSON file. Existing skills with matching IDs are replaced.
    pub fn load_user_skills(&mut self, path: impl AsRef<Path>) -> anyhow::Result<usize> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(0);
        }
        let content = std::fs::read_to_string(path)?;
        let skills: Vec<SkillDef> = serde_json::from_str(&content)?;
        let count = skills.len();
        for skill in skills {
            self.register(skill);
        }
        Ok(count)
    }
}

// ── Conversion from actforce BuiltinSkill ──

/// This impl block provides `From<BuiltinSkill>` conversion.
/// agent-core doesn't import BuiltinSkill directly; the conversion happens in actforce.
impl SkillDef {
    pub fn from_builtin(
        id: Option<String>,
        name: String,
        description: Option<String>,
        is_enabled: bool,
        content: Option<String>,
        program: Option<String>,
        extra_json: Option<Value>,
        generated_json: Option<Value>,
        created_by: Option<String>,
        updated_by: Option<String>,
    ) -> Self {
        Self {
            id: id.unwrap_or_else(|| format!("sk_{}", uuid::Uuid::new_v4().to_string().split('-').next().unwrap_or("unknown"))),
            name,
            description,
            is_enabled,
            content,
            program,
            extra_json,
            generated_json,
            is_builtin: true,
            created_by: created_by.or(Some("system".into())),
            updated_by: updated_by.or(Some("system".into())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_skill(id: &str, name: &str) -> SkillDef {
        SkillDef {
            id: id.into(),
            name: name.into(),
            description: Some("test skill".into()),
            is_enabled: true,
            content: None,
            program: None,
            extra_json: None,
            generated_json: None,
            is_builtin: false,
            created_by: None,
            updated_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn test_register_and_get() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("sk_test", "Test"));
        assert!(reg.get("sk_test").is_some());
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn test_cannot_unregister_builtin() {
        let mut reg = SkillRegistry::new();
        let mut s = make_skill("sk_builtin", "Builtin");
        s.is_builtin = true;
        reg.register(s);
        assert!(reg.unregister("sk_builtin").is_err());
    }

    #[test]
    fn test_search() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("sk_a", "Search and Answer"));
        reg.register(make_skill("sk_b", "Data Query"));
        let results = reg.search("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "sk_a");
    }

    #[test]
    fn test_save_and_load_user_skills() {
        let mut reg = SkillRegistry::new();
        reg.register(make_skill("sk_u1", "User Skill 1"));
        let mut builtin = make_skill("sk_b1", "Builtin");
        builtin.is_builtin = true;
        reg.register(builtin);

        let tmp = tempfile::NamedTempFile::new().unwrap();
        reg.save_user_skills(tmp.path()).unwrap();

        let mut reg2 = SkillRegistry::new();
        let count = reg2.load_user_skills(tmp.path()).unwrap();
        assert_eq!(count, 1);
        assert!(reg2.get("sk_u1").is_some());
        assert!(reg2.get("sk_b1").is_none());
    }
}
