//! License Check — Check license compliance for project dependencies.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::compliance::license::{LicensePolicy, LicenseScanner};
use crate::dep_graph::DependencyGraph;

/// Check license compliance for project dependencies against a configurable
/// policy. Identifies copyleft, restrictive, and unknown licenses across
/// the entire dependency tree.
pub struct LicenseCheckTool;

#[async_trait]
impl Tool for LicenseCheckTool {
    fn name(&self) -> &str {
        "license_check"
    }

    fn description(&self) -> &str {
        "Check license compliance for project dependencies"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to project"
                },
                "policy": {
                    "type": "string",
                    "description": "License policy (strict/permissive)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let policy = args
            .get("policy")
            .and_then(|v| v.as_str())
            .unwrap_or("permissive");

        let workspace = Path::new(path);

        // Build dependency graph from lockfiles
        let dep_graph = match DependencyGraph::build(workspace) {
            Ok(g) => g,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "License compliance check for '{path}' (policy: {policy}):\n\
                     Error building dependency graph: {e}\n\
                     No lockfiles found or failed to parse. \
                     Ensure the project has a Cargo.lock, package-lock.json, or requirements.txt."
                )));
            }
        };

        let all_packages = dep_graph.all_packages();
        if all_packages.is_empty() {
            return Ok(ToolOutput::text(format!(
                "License compliance check for '{path}' (policy: {policy}):\n\
                 No dependencies found in project lockfiles."
            )));
        }

        // Build package tuples: (name, version, spdx_id)
        let packages: Vec<(String, String, String)> = all_packages
            .iter()
            .map(|dep| {
                (
                    dep.name.clone(),
                    dep.version.clone(),
                    dep.license.clone().unwrap_or_else(|| "Unknown".to_string()),
                )
            })
            .collect();

        // Create scanner with appropriate policy
        let scanner = if policy == "strict" {
            LicenseScanner::permissive_only()
        } else {
            // Permissive policy: allow everything except strong copyleft
            LicenseScanner::new(LicensePolicy {
                allowed: Vec::new(), // Empty = no allowlist filtering
                denied: vec!["AGPL-*".into()],
                review_required: vec!["GPL-*".into(), "LGPL-*".into()],
                overrides: std::collections::HashMap::new(),
            })
        };

        let report = scanner.check_packages(&packages);

        // Format output
        let mut output = format!("License compliance check for '{path}' (policy: {policy}):\n\n");

        // Summary
        output.push_str(&format!(
            "Summary: {} packages scanned\n\
             - Permissive: {}\n\
             - Weak copyleft: {}\n\
             - Strong copyleft: {}\n\
             - Proprietary: {}\n\
             - Unknown: {}\n\n",
            report.summary.total_packages,
            report.summary.permissive,
            report.summary.weak_copyleft,
            report.summary.strong_copyleft,
            report.summary.proprietary,
            report.summary.unknown,
        ));

        // Violations
        if report.violations.is_empty() {
            output.push_str("No policy violations found.\n");
        } else {
            output.push_str(&format!("Violations ({}):\n", report.violations.len()));
            for v in &report.violations {
                output.push_str(&format!(
                    "  - {} v{}: license '{}' — {}\n",
                    v.package, v.version, v.license, v.rule
                ));
            }
            output.push('\n');
        }

        // Review required
        if !report.review_required.is_empty() {
            output.push_str(&format!(
                "Review required ({}):\n",
                report.review_required.len()
            ));
            for pkg in &report.review_required {
                output.push_str(&format!(
                    "  - {} v{}: {} ({})\n",
                    pkg.package, pkg.version, pkg.spdx_id, pkg.class
                ));
            }
            output.push('\n');
        }

        // Status
        let status = if report.violations.is_empty() {
            "PASS"
        } else {
            "FAIL"
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
        let tool = LicenseCheckTool;
        assert_eq!(tool.name(), "license_check");
    }

    #[test]
    fn test_schema() {
        let tool = LicenseCheckTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["policy"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = LicenseCheckTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = LicenseCheckTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        // With default path ".", it will either find deps or report no lockfiles
        assert!(
            result.content.contains("permissive")
                || result.content.contains("No lockfiles")
                || result.content.contains("No dependencies")
        );
        assert!(result.content.contains("License compliance check"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = LicenseCheckTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "/my/project",
                "policy": "strict"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("/my/project"));
        assert!(result.content.contains("strict"));
    }
}
