//! Skill security validation.
//!
//! Validates skill definitions for security risks: checks required secrets exist,
//! tool dependencies resolve, and scans for dangerous patterns.

use super::types::{SkillDefinition, SkillRiskLevel};

/// Errors from security validation.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing required secret: {0}")]
    MissingSecret(String),
    #[error("Missing required tool: {0}")]
    MissingTool(String),
    #[error("Dangerous pattern detected: {0}")]
    DangerousPattern(String),
}

/// Result of validating a skill.
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub risk_level: SkillRiskLevel,
    pub warnings: Vec<String>,
    pub errors: Vec<ValidationError>,
}

/// Dangerous patterns that increase risk level.
const DANGEROUS_PATTERNS: &[(&str, &str)] = &[
    ("shell_exec", "Uses shell execution"),
    ("sudo", "Uses privilege escalation"),
    ("rm -rf", "Contains recursive delete"),
    ("chmod", "Modifies file permissions"),
    ("curl", "Makes network requests"),
    ("wget", "Downloads files"),
    ("eval", "Uses eval (code injection risk)"),
    ("exec", "Uses exec"),
    ("/etc/passwd", "Accesses system files"),
    ("DROP TABLE", "Contains SQL destructive command"),
];

/// Validate a skill definition for security.
pub fn validate_skill(
    skill: &SkillDefinition,
    available_tools: &[String],
    available_secrets: &[String],
) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let mut max_risk = SkillRiskLevel::Low;

    // Check required tools
    for req in &skill.requires {
        if req.req_type == "tool" && !available_tools.contains(&req.name) {
            errors.push(ValidationError::MissingTool(req.name.clone()));
        }
    }

    // Check required secrets
    for req in &skill.requires {
        if req.req_type == "secret" && !available_secrets.contains(&req.name) {
            errors.push(ValidationError::MissingSecret(req.name.clone()));
        }
    }

    // Scan tool bodies for dangerous patterns
    for tool in &skill.tools {
        for (pattern, description) in DANGEROUS_PATTERNS {
            if tool.body.contains(pattern) {
                warnings.push(format!(
                    "Tool '{}': {} (pattern: '{}')",
                    tool.name, description, pattern
                ));
                // Escalate risk level based on pattern
                let pattern_risk = pattern_risk_level(pattern);
                if risk_priority(&pattern_risk) > risk_priority(&max_risk) {
                    max_risk = pattern_risk;
                }
            }
        }
    }

    // Check if skill has any secrets (elevates risk to at least Medium)
    let has_secrets = skill.requires.iter().any(|r| r.req_type == "secret");
    if has_secrets && risk_priority(&max_risk) < risk_priority(&SkillRiskLevel::Medium) {
        max_risk = SkillRiskLevel::Medium;
    }

    let is_valid = errors.is_empty();

    ValidationResult {
        is_valid,
        risk_level: max_risk,
        warnings,
        errors,
    }
}

/// Determine risk level for a specific dangerous pattern.
fn pattern_risk_level(pattern: &str) -> SkillRiskLevel {
    match pattern {
        "sudo" | "rm -rf" | "DROP TABLE" | "/etc/passwd" => SkillRiskLevel::Critical,
        "shell_exec" | "exec" | "eval" => SkillRiskLevel::High,
        "curl" | "wget" | "chmod" => SkillRiskLevel::Medium,
        _ => SkillRiskLevel::Low,
    }
}

/// Convert risk level to a numeric priority for comparison.
fn risk_priority(level: &SkillRiskLevel) -> u8 {
    match level {
        SkillRiskLevel::Low => 0,
        SkillRiskLevel::Medium => 1,
        SkillRiskLevel::High => 2,
        SkillRiskLevel::Critical => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{SkillRequirement, SkillToolDef};

    fn make_skill(
        name: &str,
        requires: Vec<SkillRequirement>,
        tools: Vec<SkillToolDef>,
    ) -> SkillDefinition {
        SkillDefinition {
            name: name.into(),
            version: "1.0.0".into(),
            description: "test".into(),
            author: None,
            requires,
            tools,
            config: Default::default(),
            risk_level: SkillRiskLevel::Low,
            source_path: None,
        }
    }

    #[test]
    fn test_validate_all_deps_met() {
        let skill = make_skill(
            "test",
            vec![
                SkillRequirement {
                    req_type: "tool".into(),
                    name: "shell_exec".into(),
                },
                SkillRequirement {
                    req_type: "secret".into(),
                    name: "API_KEY".into(),
                },
            ],
            vec![SkillToolDef {
                name: "safe_tool".into(),
                description: "Safe".into(),
                parameters: serde_json::json!({}),
                body: "echo hello".into(),
            }],
        );

        let result = validate_skill(&skill, &["shell_exec".into()], &["API_KEY".into()]);
        assert!(result.is_valid);
    }

    #[test]
    fn test_validate_missing_secret() {
        let skill = make_skill(
            "test",
            vec![SkillRequirement {
                req_type: "secret".into(),
                name: "MISSING_KEY".into(),
            }],
            vec![],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingSecret(_))));
    }

    #[test]
    fn test_validate_missing_tool() {
        let skill = make_skill(
            "test",
            vec![SkillRequirement {
                req_type: "tool".into(),
                name: "nonexistent_tool".into(),
            }],
            vec![],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert!(!result.is_valid);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingTool(_))));
    }

    #[test]
    fn test_validate_dangerous_shell_exec() {
        let skill = make_skill(
            "test",
            vec![],
            vec![SkillToolDef {
                name: "risky".into(),
                description: "Risky tool".into(),
                parameters: serde_json::json!({}),
                body: "shell_exec: rm -rf /tmp/data".into(),
            }],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert!(result.is_valid); // No missing deps
        assert_eq!(result.risk_level, SkillRiskLevel::Critical); // rm -rf is critical
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_validate_read_only_low_risk() {
        let skill = make_skill(
            "test",
            vec![],
            vec![SkillToolDef {
                name: "safe".into(),
                description: "Safe read-only tool".into(),
                parameters: serde_json::json!({}),
                body: "Read the file contents and summarize".into(),
            }],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert!(result.is_valid);
        assert_eq!(result.risk_level, SkillRiskLevel::Low);
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_secret_elevates_risk() {
        let skill = make_skill(
            "test",
            vec![SkillRequirement {
                req_type: "secret".into(),
                name: "API_KEY".into(),
            }],
            vec![SkillToolDef {
                name: "api_call".into(),
                description: "API caller".into(),
                parameters: serde_json::json!({}),
                body: "Use API key to fetch data".into(),
            }],
        );

        let result = validate_skill(&skill, &[], &["API_KEY".into()]);
        assert!(result.is_valid);
        assert_eq!(result.risk_level, SkillRiskLevel::Medium);
    }

    #[test]
    fn test_validate_sudo_is_critical() {
        let skill = make_skill(
            "test",
            vec![],
            vec![SkillToolDef {
                name: "admin".into(),
                description: "Admin tool".into(),
                parameters: serde_json::json!({}),
                body: "sudo apt-get update".into(),
            }],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert_eq!(result.risk_level, SkillRiskLevel::Critical);
    }

    #[test]
    fn test_validate_network_is_medium() {
        let skill = make_skill(
            "test",
            vec![],
            vec![SkillToolDef {
                name: "fetch".into(),
                description: "Fetcher".into(),
                parameters: serde_json::json!({}),
                body: "curl https://api.example.com/data".into(),
            }],
        );

        let result = validate_skill(&skill, &[], &[]);
        assert_eq!(result.risk_level, SkillRiskLevel::Medium);
    }
}
