//! Policy Check — Evaluate code against security and compliance policies.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::compliance::policy::{PolicyEngine, PolicyRule, PolicyType};
use crate::finding::{Finding, FindingCategory, FindingProvenance, FindingSeverity};

/// Evaluate code and configuration against security and compliance policies
/// defined in a policy file. Checks for policy violations including
/// gate (block), warn, and inform severity levels.
pub struct PolicyCheckTool;

/// Try to load policy rules from a TOML file.
/// The expected TOML format is:
/// ```toml
/// [[rules]]
/// id = "no-critical-vulns"
/// policy_type = "gate"
/// scanner = "all"
/// max_count = 0
/// message = "No critical vulnerabilities allowed"
/// ```
fn load_policies_from_file(policy_path: &Path) -> Option<Vec<PolicyRule>> {
    let content = std::fs::read_to_string(policy_path).ok()?;

    #[derive(serde::Deserialize)]
    struct PolicyFile {
        #[serde(default)]
        rules: Vec<PolicyRule>,
    }

    let parsed: PolicyFile = toml::from_str(&content).ok()?;
    if parsed.rules.is_empty() {
        None
    } else {
        Some(parsed.rules)
    }
}

/// Scan the workspace for any findings to evaluate against policies.
/// This does a lightweight scan: checks for lockfile presence and
/// basic file patterns that indicate potential issues.
fn collect_workspace_findings(workspace: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    // Check for common security indicator files
    let cargo_lock = workspace.join("Cargo.lock");
    let pkg_lock = workspace.join("package-lock.json");

    // If lockfiles exist, we can do basic dependency checks
    if cargo_lock.exists() || pkg_lock.exists() {
        // Use dep_graph to find dependencies, check for missing licenses
        if let Ok(graph) = crate::dep_graph::DependencyGraph::build(workspace) {
            for dep in graph.all_packages() {
                if dep.license.is_none() {
                    findings.push(Finding::new(
                        format!("Unknown license for {}", dep.name),
                        format!(
                            "Dependency {} v{} has no declared license",
                            dep.name, dep.version
                        ),
                        FindingSeverity::Medium,
                        FindingCategory::Compliance,
                        FindingProvenance {
                            scanner: "license".into(),
                            rule_id: Some("unknown-license".into()),
                            confidence: 0.8,
                            consensus: None,
                        },
                    ));
                }
            }
        }
    }

    findings
}

#[async_trait]
impl Tool for PolicyCheckTool {
    fn name(&self) -> &str {
        "policy_check"
    }

    fn description(&self) -> &str {
        "Evaluate code against security and compliance policies"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to evaluate"
                },
                "policy_file": {
                    "type": "string",
                    "description": "Path to policy file"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let policy_file = args
            .get("policy_file")
            .and_then(|v| v.as_str())
            .unwrap_or(".rustant/policies.toml");

        let workspace = Path::new(path);
        let policy_path = if Path::new(policy_file).is_absolute() {
            std::path::PathBuf::from(policy_file)
        } else {
            workspace.join(policy_file)
        };

        // Load policies: try file first, fall back to defaults
        let engine = if let Some(rules) = load_policies_from_file(&policy_path) {
            PolicyEngine::new(rules)
        } else {
            PolicyEngine::with_defaults()
        };

        let policy_source = if policy_path.exists() {
            format!("{}", policy_path.display())
        } else {
            "built-in defaults".to_string()
        };

        // Collect findings from workspace
        let findings = collect_workspace_findings(workspace);

        // Evaluate policies
        let report = engine.evaluate(&findings);

        // Format output
        let mut output = format!("Policy evaluation for '{path}' (policy: {policy_source}):\n\n");

        output.push_str(&format!(
            "Rules evaluated: {} ({} passed, {} failed)\n",
            report.passed + report.failed,
            report.passed,
            report.failed
        ));

        if !findings.is_empty() {
            output.push_str(&format!("Findings scanned: {}\n", findings.len()));
        }
        output.push('\n');

        // Detailed results per rule
        for eval in &report.evaluations {
            let status_icon = if eval.passed { "PASS" } else { "FAIL" };
            let policy_label = match eval.policy_type {
                PolicyType::Gate => "[GATE]",
                PolicyType::Warn => "[WARN]",
                PolicyType::Inform => "[INFO]",
            };
            output.push_str(&format!(
                "  {} {} {}: {}\n",
                status_icon, policy_label, eval.rule_id, eval.message
            ));
        }
        output.push('\n');

        // Gate failures
        let gate_failures = report.gate_failures();
        if !gate_failures.is_empty() {
            output.push_str(&format!(
                "Gate violations ({} blocking):\n",
                gate_failures.len()
            ));
            for fail in gate_failures {
                output.push_str(&format!("  - {}: {}\n", fail.rule_id, fail.message));
            }
            output.push('\n');
        }

        // Warnings
        let warnings = report.warnings();
        if !warnings.is_empty() {
            output.push_str(&format!("Warnings ({}):\n", warnings.len()));
            for warn in warnings {
                output.push_str(&format!("  - {}: {}\n", warn.rule_id, warn.message));
            }
            output.push('\n');
        }

        // Overall status
        let status = if report.has_gate_failures {
            "FAIL (gate violations found)"
        } else if report.has_warnings {
            "PASS with warnings"
        } else {
            "PASS"
        };
        output.push_str(&format!("Policy status: {status}"));

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = PolicyCheckTool;
        assert_eq!(tool.name(), "policy_check");
    }

    #[test]
    fn test_schema() {
        let tool = PolicyCheckTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["policy_file"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = PolicyCheckTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = PolicyCheckTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(
            result.content.contains("policies.toml")
                || result.content.contains("built-in defaults")
        );
        assert!(result.content.contains("Policy evaluation"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = PolicyCheckTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "src/",
                "policy_file": "custom-policy.toml"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("src/"));
        assert!(
            result.content.contains("custom-policy.toml")
                || result.content.contains("built-in defaults")
        );
    }
}
