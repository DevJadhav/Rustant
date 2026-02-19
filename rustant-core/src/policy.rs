//! Policy Engine — loads SRE operational policies from TOML configuration.
//!
//! Policies define constraints on tool execution (time windows, blast radius limits,
//! trust level requirements) and are evaluated by the existing `ContractEnforcer` in safety.rs.

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::warn;

/// A policy definition loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyDefinition {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub enabled: bool,
    pub predicate: PolicyPredicate,
    #[serde(default)]
    pub scope: PolicyScope,
}

/// The predicate type for a policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyPredicate {
    TimeWindow {
        #[serde(default)]
        allowed_days: Vec<u8>,
        #[serde(default)]
        allowed_hours: (u8, u8),
    },
    MaxBlastRadius {
        threshold: f64,
    },
    MinTrustLevel {
        level: u8,
    },
    RequiresConsensus {
        threshold: f64,
    },
    MaxConcurrentDeployments {
        limit: usize,
    },
    /// Maximum model size in GB.
    MaxModelSize {
        max_gb: f64,
    },
    /// Require PII scanning on specified datasets.
    RequirePiiScan {
        #[serde(default)]
        datasets: Vec<String>,
    },
    /// Maximum GPU memory usage in GB.
    MaxGpuMemory {
        max_gb: f64,
    },
    /// Require alignment testing with specified methods.
    RequireAlignmentTest {
        #[serde(default)]
        methods: Vec<String>,
    },
}

/// Scope defines which tools/actions a policy applies to.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PolicyScope {
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub actions: Vec<String>,
}

/// Root structure for the policies TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PoliciesFile {
    #[serde(default)]
    policies: Vec<PolicyDefinition>,
}

/// Errors from policy loading.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("Failed to read policy file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse policy TOML: {0}")]
    ParseError(#[from] toml::de::Error),
}

/// Load policies from a TOML file.
///
/// The file format is:
/// ```toml
/// [[policies]]
/// name = "no-friday-deploys"
/// description = "Block deployments on Fridays"
/// enabled = true
/// [policies.predicate]
/// type = "time_window"
/// allowed_days = [1, 2, 3, 4]
/// allowed_hours = [8, 17]
/// [policies.scope]
/// tools = ["kubernetes", "deployment_intel"]
/// actions = ["rollout_restart", "scale"]
/// ```
pub fn load_policies(path: &Path) -> Result<Vec<PolicyDefinition>, PolicyError> {
    let content = std::fs::read_to_string(path)?;
    let file: PoliciesFile = toml::from_str(&content)?;
    let active: Vec<PolicyDefinition> = file.policies.into_iter().filter(|p| p.enabled).collect();
    Ok(active)
}

/// Load policies from the default location in the workspace.
/// Returns an empty vec if the file doesn't exist.
pub fn load_workspace_policies(workspace: &Path) -> Vec<PolicyDefinition> {
    let policy_path = workspace.join(".rustant").join("policies.toml");
    if !policy_path.exists() {
        return Vec::new();
    }
    match load_policies(&policy_path) {
        Ok(policies) => {
            if !policies.is_empty() {
                tracing::info!(
                    "Loaded {} SRE policies from {}",
                    policies.len(),
                    policy_path.display()
                );
            }
            policies
        }
        Err(e) => {
            warn!(
                "Failed to load policies from {}: {}",
                policy_path.display(),
                e
            );
            Vec::new()
        }
    }
}

/// Check if a tool/action is within the scope of a policy.
pub fn policy_applies(policy: &PolicyDefinition, tool_name: &str, action: Option<&str>) -> bool {
    // If no scope defined, policy applies to everything
    if policy.scope.tools.is_empty() && policy.scope.actions.is_empty() {
        return true;
    }

    let tool_match =
        policy.scope.tools.is_empty() || policy.scope.tools.iter().any(|t| t == tool_name);

    let action_match = policy.scope.actions.is_empty()
        || action.is_none_or(|a| policy.scope.actions.iter().any(|pa| pa == a));

    tool_match && action_match
}

/// Evaluate a time window policy against the current time.
pub fn check_time_window(allowed_days: &[u8], allowed_hours: (u8, u8)) -> bool {
    let now = chrono::Local::now();
    let weekday = now.format("%u").to_string().parse::<u8>().unwrap_or(1); // 1=Monday
    let hour = now.format("%H").to_string().parse::<u8>().unwrap_or(12);

    let day_ok = allowed_days.is_empty() || allowed_days.contains(&weekday);
    let hour_ok = hour >= allowed_hours.0 && hour < allowed_hours.1;

    day_ok && hour_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_policies_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("policies.toml");
        std::fs::write(&path, "").unwrap();
        let policies = load_policies(&path).unwrap();
        assert!(policies.is_empty());
    }

    #[test]
    fn test_load_policies_valid() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("policies.toml");
        std::fs::write(
            &path,
            r#"
[[policies]]
name = "test-policy"
description = "A test policy"
enabled = true
[policies.predicate]
type = "max_blast_radius"
threshold = 0.5
[policies.scope]
tools = ["kubernetes"]
actions = ["scale"]
"#,
        )
        .unwrap();
        let policies = load_policies(&path).unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "test-policy");
    }

    #[test]
    fn test_load_policies_disabled_filtered() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("policies.toml");
        std::fs::write(
            &path,
            r#"
[[policies]]
name = "active"
description = "Active policy"
enabled = true
[policies.predicate]
type = "min_trust_level"
level = 3

[[policies]]
name = "disabled"
description = "Disabled policy"
enabled = false
[policies.predicate]
type = "min_trust_level"
level = 4
"#,
        )
        .unwrap();
        let policies = load_policies(&path).unwrap();
        assert_eq!(policies.len(), 1);
        assert_eq!(policies[0].name, "active");
    }

    #[test]
    fn test_policy_applies() {
        let policy = PolicyDefinition {
            name: "test".into(),
            description: "test".into(),
            enabled: true,
            predicate: PolicyPredicate::MaxBlastRadius { threshold: 0.5 },
            scope: PolicyScope {
                tools: vec!["kubernetes".into()],
                actions: vec!["scale".into()],
            },
        };
        assert!(policy_applies(&policy, "kubernetes", Some("scale")));
        assert!(!policy_applies(&policy, "prometheus", Some("query")));
        assert!(!policy_applies(&policy, "kubernetes", Some("pods")));
    }

    #[test]
    fn test_policy_applies_empty_scope() {
        let policy = PolicyDefinition {
            name: "global".into(),
            description: "applies everywhere".into(),
            enabled: true,
            predicate: PolicyPredicate::MinTrustLevel { level: 2 },
            scope: PolicyScope::default(),
        };
        assert!(policy_applies(&policy, "any_tool", Some("any_action")));
    }

    #[test]
    fn test_load_workspace_policies_missing_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let policies = load_workspace_policies(dir.path());
        assert!(policies.is_empty());
    }

    #[test]
    fn test_check_time_window_always_open() {
        // Empty days list = always allowed
        assert!(check_time_window(&[], (0, 24)));
    }
}
