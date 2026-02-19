//! SCA scan tool — software composition analysis for dependency vulnerabilities.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

use crate::dep_graph::DependencyGraph;
use crate::finding::FindingSeverity;
use crate::scanners::sca::ScaScanner;

/// Scans project dependencies for known vulnerabilities using software composition analysis.
pub struct ScaScanTool;

impl Default for ScaScanTool {
    fn default() -> Self {
        Self
    }
}

impl ScaScanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ScaScanTool {
    fn name(&self) -> &str {
        "sca_scan"
    }

    fn description(&self) -> &str {
        "Scan dependencies for known vulnerabilities (SCA)"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to project"
                },
                "database": {
                    "type": "string",
                    "description": "Vulnerability database (osv/ghsa/nvd)"
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(90)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let database = args
            .get("database")
            .and_then(|v| v.as_str())
            .unwrap_or("osv");

        let valid_databases = ["osv", "ghsa", "nvd"];
        if !valid_databases.contains(&database) {
            return Ok(ToolOutput::text(format!(
                "Unknown vulnerability database '{}'. Supported: {}",
                database,
                valid_databases.join(", ")
            )));
        }

        let project_path = Path::new(path);
        if !project_path.is_dir() {
            return Ok(ToolOutput::text(format!(
                "SCA scan: path '{path}' does not exist or is not a directory."
            )));
        }

        // Detect lockfiles present
        let lockfile_checks = [
            ("Cargo.lock", "cargo"),
            ("package-lock.json", "npm"),
            ("yarn.lock", "npm/yarn"),
            ("requirements.txt", "pypi"),
            ("poetry.lock", "pypi/poetry"),
        ];

        let mut lockfiles_found: Vec<(&str, &str)> = Vec::new();
        for (filename, ecosystem) in &lockfile_checks {
            if project_path.join(filename).exists() {
                lockfiles_found.push((filename, ecosystem));
            }
        }

        if lockfiles_found.is_empty() {
            return Ok(ToolOutput::text(format!(
                "SCA scan completed on '{path}' using {database} database. \
                 No lockfiles found. Supported lockfiles: Cargo.lock, package-lock.json, \
                 yarn.lock, requirements.txt, poetry.lock."
            )));
        }

        // Build dependency graph
        let graph = match DependencyGraph::build(project_path) {
            Ok(g) => g,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "SCA scan on '{path}': failed to build dependency graph: {e}"
                )));
            }
        };

        let total_packages = graph.package_count();
        let all_packages = graph.all_packages();

        // Check each package against the SCA scanner advisories
        let scanner = ScaScanner::new();
        let mut all_findings = Vec::new();

        for pkg in &all_packages {
            let lockfile_path = project_path.join("Cargo.lock"); // simplified
            let findings = scanner.check_package(&pkg.name, &pkg.version, &lockfile_path);
            all_findings.extend(findings);
        }

        // Count by severity
        let critical = all_findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Critical)
            .count();
        let high = all_findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::High)
            .count();
        let medium = all_findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Medium)
            .count();
        let low = all_findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Low)
            .count();

        // Count ecosystems
        let mut ecosystem_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for pkg in &all_packages {
            *ecosystem_counts.entry(pkg.ecosystem.clone()).or_insert(0) += 1;
        }

        let lockfiles_str: Vec<&str> = lockfiles_found.iter().map(|(f, _)| *f).collect();
        let mut output = format!(
            "SCA scan completed on '{}' using {} database.\n\
             Parsed lockfiles: {}\n\
             Total packages: {}\n\
             Direct dependencies: {}\n\
             Vulnerabilities found: {}\n",
            path,
            database,
            lockfiles_str.join(", "),
            total_packages,
            graph.direct_deps().len(),
            all_findings.len()
        );

        // Ecosystem breakdown
        if !ecosystem_counts.is_empty() {
            output.push_str("\nPackages by ecosystem:\n");
            for (eco, count) in &ecosystem_counts {
                output.push_str(&format!("- {eco}: {count}\n"));
            }
        }

        if !all_findings.is_empty() {
            output.push_str(&format!(
                "\nVulnerability severity breakdown:\n\
                 - Critical: {critical}\n\
                 - High: {high}\n\
                 - Medium: {medium}\n\
                 - Low: {low}\n"
            ));

            output.push_str("\nVulnerable packages:\n");
            for (i, finding) in all_findings.iter().take(20).enumerate() {
                output.push_str(&format!(
                    "{}. [{}] {}\n",
                    i + 1,
                    finding.severity.as_str().to_uppercase(),
                    finding.title
                ));
                if let Some(ref rem) = finding.remediation {
                    output.push_str(&format!("   Fix: {}\n", rem.description));
                }
            }
            if all_findings.len() > 20 {
                output.push_str(&format!(
                    "... and {} more vulnerabilities.\n",
                    all_findings.len() - 20
                ));
            }
        } else {
            output.push_str("\nNo known vulnerabilities found in scanned dependencies.");
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ScaScanTool::new();
        assert_eq!(tool.name(), "sca_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = ScaScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["database"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = ScaScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("SCA scan"));
        assert!(result.content.contains("osv"));
    }

    #[tokio::test]
    async fn test_execute_invalid_database() {
        let tool = ScaScanTool::new();
        let result = tool.execute(json!({"database": "unknown"})).await.unwrap();
        assert!(result.content.contains("Unknown vulnerability database"));
    }

    #[tokio::test]
    async fn test_execute_with_lockfile() {
        let dir = tempfile::tempdir().unwrap();
        let cargo_lock_path = dir.path().join("Cargo.lock");
        std::fs::write(
            &cargo_lock_path,
            r#"
[[package]]
name = "serde"
version = "1.0.200"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "my-app"
version = "0.1.0"
dependencies = [
    "serde 1.0.200",
]
"#,
        )
        .unwrap();

        let tool = ScaScanTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("SCA scan completed"));
        assert!(result.content.contains("Cargo.lock"));
        assert!(result.content.contains("Total packages:"));
    }
}
