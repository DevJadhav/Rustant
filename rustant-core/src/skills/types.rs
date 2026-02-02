//! Skill type definitions.

use serde::{Deserialize, Serialize};

/// Risk level for a skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

/// A requirement for a skill (tool or secret).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirement {
    /// Type of requirement: "tool" or "secret".
    pub req_type: String,
    /// Name of the required tool or secret.
    pub name: String,
}

/// A tool definition within a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillToolDef {
    /// Tool name.
    pub name: String,
    /// Tool description.
    pub description: String,
    /// JSON Schema for parameters.
    pub parameters: serde_json::Value,
    /// Tool body (template or instruction for the agent).
    pub body: String,
}

/// Configuration section for a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// Whether the skill is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Custom configuration values.
    #[serde(default)]
    pub values: std::collections::HashMap<String, String>,
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            values: std::collections::HashMap::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

/// Complete definition of a skill parsed from a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    /// Skill name.
    pub name: String,
    /// Skill version.
    pub version: String,
    /// Human-readable description.
    pub description: String,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
    /// Required tools and secrets.
    #[serde(default)]
    pub requires: Vec<SkillRequirement>,
    /// Tool definitions provided by this skill.
    #[serde(default)]
    pub tools: Vec<SkillToolDef>,
    /// Skill configuration.
    #[serde(default)]
    pub config: SkillConfig,
    /// Assessed risk level.
    #[serde(default = "default_risk")]
    pub risk_level: SkillRiskLevel,
    /// Source file path.
    #[serde(default)]
    pub source_path: Option<String>,
}

fn default_risk() -> SkillRiskLevel {
    SkillRiskLevel::Low
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_risk_level_serialization() {
        let json = serde_json::to_string(&SkillRiskLevel::High).unwrap();
        let restored: SkillRiskLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, SkillRiskLevel::High);
    }

    #[test]
    fn test_skill_definition_serialization() {
        let skill = SkillDefinition {
            name: "test-skill".into(),
            version: "1.0.0".into(),
            description: "A test skill".into(),
            author: Some("Test Author".into()),
            requires: vec![SkillRequirement {
                req_type: "tool".into(),
                name: "shell_exec".into(),
            }],
            tools: vec![SkillToolDef {
                name: "test_tool".into(),
                description: "Test tool".into(),
                parameters: serde_json::json!({"type": "object"}),
                body: "echo hello".into(),
            }],
            config: SkillConfig::default(),
            risk_level: SkillRiskLevel::Medium,
            source_path: None,
        };

        let json = serde_json::to_string(&skill).unwrap();
        let restored: SkillDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test-skill");
        assert_eq!(restored.risk_level, SkillRiskLevel::Medium);
        assert_eq!(restored.tools.len(), 1);
        assert_eq!(restored.requires.len(), 1);
    }

    #[test]
    fn test_skill_config_defaults() {
        let config = SkillConfig::default();
        assert!(config.enabled);
        assert!(config.values.is_empty());
    }
}
