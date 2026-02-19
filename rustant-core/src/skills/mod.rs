//! # Skills System
//!
//! Declarative skill definitions parsed from SKILL.md files.
//! Skills define tool registrations via YAML frontmatter and markdown-based
//! tool definitions with parameter schemas and body templates.

pub mod parser;
pub mod types;
pub mod validator;

pub use parser::{ParseError, parse_skill_md};
pub use types::{SkillConfig, SkillDefinition, SkillRequirement, SkillRiskLevel, SkillToolDef};
pub use validator::{ValidationError, ValidationResult, validate_skill};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Registry of loaded skills.
#[derive(Debug, Default)]
pub struct SkillRegistry {
    skills: HashMap<String, SkillDefinition>,
    active_skills: Vec<String>,
}

impl SkillRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a skill definition.
    pub fn register(&mut self, skill: SkillDefinition) {
        self.skills.insert(skill.name.clone(), skill);
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&SkillDefinition> {
        self.skills.get(name)
    }

    /// List all loaded skill names.
    pub fn list_names(&self) -> Vec<&str> {
        self.skills.keys().map(|k| k.as_str()).collect()
    }

    /// Number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// Activate a skill by name. Returns an error if the skill does not exist.
    pub fn activate(&mut self, name: &str) -> Result<(), String> {
        if !self.skills.contains_key(name) {
            return Err(format!("Skill '{name}' not found"));
        }
        if !self.active_skills.iter().any(|s| s == name) {
            self.active_skills.push(name.to_string());
        }
        Ok(())
    }

    /// Deactivate a skill by name.
    pub fn deactivate(&mut self, name: &str) {
        self.active_skills.retain(|s| s != name);
    }

    /// Get the list of currently active skill names.
    pub fn active_skills(&self) -> &[String] {
        &self.active_skills
    }

    /// Auto-detect skills whose trigger_patterns match the given task description.
    /// Uses simple `contains()` matching (case-sensitive).
    pub fn auto_detect(&self, task: &str) -> Vec<String> {
        let task_lower = task.to_lowercase();
        self.skills
            .values()
            .filter(|skill| {
                skill
                    .trigger_patterns
                    .iter()
                    .any(|pattern| task_lower.contains(&pattern.to_lowercase()))
            })
            .map(|skill| skill.name.clone())
            .collect()
    }

    /// Collect system_prompts from all active skills that have one.
    pub fn get_active_system_prompts(&self) -> Vec<&str> {
        self.active_skills
            .iter()
            .filter_map(|name| self.skills.get(name))
            .filter_map(|skill| skill.system_prompt.as_deref())
            .collect()
    }
}

/// Loads skills from a directory of SKILL.md files.
pub struct SkillLoader {
    skills_dir: PathBuf,
}

impl SkillLoader {
    pub fn new(skills_dir: impl Into<PathBuf>) -> Self {
        Self {
            skills_dir: skills_dir.into(),
        }
    }

    /// Scan the skills directory and load all .md files.
    pub fn scan(&self) -> Vec<Result<SkillDefinition, (PathBuf, ParseError)>> {
        let mut results = Vec::new();

        let entries = match std::fs::read_dir(&self.skills_dir) {
            Ok(entries) => entries,
            Err(_) => return results,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                match self.load_file(&path) {
                    Ok(mut skill) => {
                        skill.source_path = Some(path.to_string_lossy().into_owned());
                        results.push(Ok(skill));
                    }
                    Err(e) => {
                        results.push(Err((path, e)));
                    }
                }
            }
        }

        results
    }

    /// Load a single skill file.
    pub fn load_file(&self, path: &Path) -> Result<SkillDefinition, ParseError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ParseError::InvalidYaml(format!("Failed to read file: {e}")))?;
        parse_skill_md(&content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_registry_register_and_get() {
        let mut registry = SkillRegistry::new();
        let skill = SkillDefinition {
            name: "test".into(),
            version: "1.0.0".into(),
            description: "Test skill".into(),
            author: None,
            requires: vec![],
            tools: vec![],
            config: SkillConfig::default(),
            risk_level: SkillRiskLevel::Low,
            source_path: None,
            trigger_patterns: vec![],
            system_prompt: None,
            required_tools: vec![],
        };

        registry.register(skill);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        let found = registry.get("test").unwrap();
        assert_eq!(found.version, "1.0.0");
    }

    #[test]
    fn test_skill_registry_list_names() {
        let mut registry = SkillRegistry::new();
        for name in &["alpha", "beta", "gamma"] {
            registry.register(SkillDefinition {
                name: name.to_string(),
                version: "1.0.0".into(),
                description: "".into(),
                author: None,
                requires: vec![],
                tools: vec![],
                config: Default::default(),
                risk_level: SkillRiskLevel::Low,
                source_path: None,
                trigger_patterns: vec![],
                system_prompt: None,
                required_tools: vec![],
            });
        }
        let names = registry.list_names();
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn test_skill_activate_deactivate() {
        let mut registry = SkillRegistry::new();
        registry.register(SkillDefinition {
            name: "code_review".into(),
            version: "1.0.0".into(),
            description: "Code review skill".into(),
            author: None,
            requires: vec![],
            tools: vec![],
            config: Default::default(),
            risk_level: SkillRiskLevel::Low,
            source_path: None,
            trigger_patterns: vec!["review".into(), "PR".into()],
            system_prompt: Some("Focus on code quality and security.".into()),
            required_tools: vec!["file_read".into()],
        });

        assert!(registry.activate("code_review").is_ok());
        assert_eq!(registry.active_skills().len(), 1);
        assert!(registry.activate("nonexistent").is_err());

        registry.deactivate("code_review");
        assert!(registry.active_skills().is_empty());
    }

    #[test]
    fn test_skill_auto_detect() {
        let mut registry = SkillRegistry::new();
        registry.register(SkillDefinition {
            name: "security_audit".into(),
            version: "1.0.0".into(),
            description: "Security skill".into(),
            author: None,
            requires: vec![],
            tools: vec![],
            config: Default::default(),
            risk_level: SkillRiskLevel::High,
            source_path: None,
            trigger_patterns: vec!["security".into(), "vulnerability".into(), "CVE".into()],
            system_prompt: Some("Enable paranoid safety mode.".into()),
            required_tools: vec![],
        });

        let matches = registry.auto_detect("Check for security vulnerabilities");
        assert!(matches.contains(&"security_audit".to_string()));

        let matches = registry.auto_detect("Refactor the auth module");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_skill_get_active_system_prompts() {
        let mut registry = SkillRegistry::new();
        registry.register(SkillDefinition {
            name: "debugging".into(),
            version: "1.0.0".into(),
            description: "Debugging skill".into(),
            author: None,
            requires: vec![],
            tools: vec![],
            config: Default::default(),
            risk_level: SkillRiskLevel::Low,
            source_path: None,
            trigger_patterns: vec![],
            system_prompt: Some("Enable verbose tool output.".into()),
            required_tools: vec![],
        });
        registry.activate("debugging").unwrap();

        let prompts = registry.get_active_system_prompts();
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0], "Enable verbose tool output.");
    }

    #[test]
    fn test_skill_loader_scan_empty_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let loader = SkillLoader::new(dir.path());
        let results = loader.scan();
        assert!(results.is_empty());
    }

    #[test]
    fn test_skill_loader_scan_with_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let skill_path = dir.path().join("test.skill.md");
        std::fs::write(
            &skill_path,
            "---\nname: test-skill\nversion: 1.0.0\ndescription: A test\n---\n",
        )
        .unwrap();

        let loader = SkillLoader::new(dir.path());
        let results = loader.scan();
        assert_eq!(results.len(), 1);
        let skill = results[0].as_ref().unwrap();
        assert_eq!(skill.name, "test-skill");
    }

    #[test]
    fn test_skill_loader_scan_nonexistent_dir() {
        let loader = SkillLoader::new("/nonexistent/path");
        let results = loader.scan();
        assert!(results.is_empty());
    }
}
