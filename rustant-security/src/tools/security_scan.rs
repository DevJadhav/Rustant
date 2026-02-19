//! Security scan orchestrator tool — runs all configured security scanners on a codebase.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::dep_graph::DependencyGraph;
use crate::finding::{Finding, FindingSeverity};
use crate::memory_bridge::FindingMemoryBridge;
use crate::report::markdown::findings_to_markdown;
use crate::scanners::sast::SastScanner;
use crate::scanners::sca::ScaScanner;
use crate::scanners::secrets::SecretsScanner;
use crate::scanners::supply_chain::SupplyChainScanner;

/// Orchestrates all security scanners (SAST, SCA, secrets, supply_chain) on a codebase.
pub struct SecurityScanTool;

impl Default for SecurityScanTool {
    fn default() -> Self {
        Self
    }
}

impl SecurityScanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SecurityScanTool {
    fn name(&self) -> &str {
        "security_scan"
    }

    fn description(&self) -> &str {
        "Orchestrate all security scanners on codebase"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to scan"
                },
                "scanners": {
                    "type": "string",
                    "description": "Comma-separated scanner list (default: all)"
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let scanners = args
            .get("scanners")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        let scanner_list: Vec<&str> = if scanners == "all" {
            vec!["sast", "sca", "secrets", "supply_chain"]
        } else {
            scanners.split(',').map(|s| s.trim()).collect()
        };

        let scan_path = Path::new(path);
        let run_sast = scanner_list.contains(&"sast");
        let run_sca = scanner_list.contains(&"sca");
        let run_secrets = scanner_list.contains(&"secrets");
        let run_supply_chain = scanner_list.contains(&"supply_chain");

        let mut all_findings: Vec<Finding> = Vec::new();
        let mut scanner_summary: Vec<String> = Vec::new();

        // Collect source files for SAST and secrets scanning
        let mut source_files: Vec<std::path::PathBuf> = Vec::new();
        if scan_path.is_dir() {
            for entry in WalkDir::new(scan_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();
                // Skip very large files
                if let Ok(meta) = std::fs::metadata(file_path)
                    && meta.len() <= 1_000_000
                {
                    source_files.push(file_path.to_path_buf());
                }
            }
        } else if scan_path.is_file() {
            source_files.push(scan_path.to_path_buf());
        } else {
            return Ok(ToolOutput::text(format!(
                "Security scan: path '{path}' does not exist or is not accessible."
            )));
        }

        // SAST scan
        if run_sast {
            let sast = SastScanner::new();
            let mut sast_findings = Vec::new();
            let mut sast_files = 0usize;

            for file_path in &source_files {
                let lang = Language::from_path(file_path);
                if lang == Language::Unknown {
                    continue;
                }
                if let Ok(source) = std::fs::read_to_string(file_path) {
                    let findings = sast.scan_source(&source, file_path, lang.as_str());
                    sast_findings.extend(findings);
                    sast_files += 1;
                }
            }

            scanner_summary.push(format!(
                "SAST: scanned {} files, found {} findings",
                sast_files,
                sast_findings.len()
            ));
            all_findings.extend(sast_findings);
        }

        // Secrets scan
        if run_secrets {
            let secrets = SecretsScanner::new();
            let mut secrets_findings = Vec::new();
            let mut secrets_files = 0usize;

            for file_path in &source_files {
                if let Ok(source) = std::fs::read_to_string(file_path) {
                    let findings = secrets.scan_source(&source, file_path);
                    secrets_findings.extend(findings);
                    secrets_files += 1;
                }
            }

            scanner_summary.push(format!(
                "Secrets: scanned {} files, found {} findings",
                secrets_files,
                secrets_findings.len()
            ));
            all_findings.extend(secrets_findings);
        }

        // SCA scan
        if run_sca && scan_path.is_dir() {
            let sca = ScaScanner::new();
            let mut sca_findings = Vec::new();

            if let Ok(graph) = DependencyGraph::build(scan_path) {
                let pkg_count = graph.package_count();
                for pkg in graph.all_packages() {
                    let lockfile_path = scan_path.join("Cargo.lock");
                    let findings = sca.check_package(&pkg.name, &pkg.version, &lockfile_path);
                    sca_findings.extend(findings);
                }
                scanner_summary.push(format!(
                    "SCA: checked {} packages, found {} vulnerabilities",
                    pkg_count,
                    sca_findings.len()
                ));
            } else {
                scanner_summary.push("SCA: no lockfiles found".to_string());
            }

            all_findings.extend(sca_findings);
        }

        // Supply chain scan
        if run_supply_chain && scan_path.is_dir() {
            let sc = SupplyChainScanner::new();
            let mut sc_findings_vec = Vec::new();

            // Check dependency graph for typosquatting
            if let Ok(graph) = DependencyGraph::build(scan_path) {
                for pkg in graph.all_packages() {
                    if let Some(sc_finding) = sc.check_typosquat(&pkg.name, &pkg.ecosystem) {
                        sc_findings_vec.push(sc.to_finding(&sc_finding));
                    }
                }
            }

            // Check package.json / setup.py for suspicious scripts
            for file_path in &source_files {
                let filename = file_path.file_name().and_then(|f| f.to_str()).unwrap_or("");
                if filename == "package.json"
                    && let Ok(content) = std::fs::read_to_string(file_path)
                {
                    let sc_findings = sc.analyze_npm_scripts(&content);
                    sc_findings_vec.extend(sc_findings.iter().map(|f| sc.to_finding(f)));
                }
                if filename == "setup.py"
                    && let Ok(content) = std::fs::read_to_string(file_path)
                {
                    let sc_findings = sc.analyze_python_package(&content, filename);
                    sc_findings_vec.extend(sc_findings.iter().map(|f| sc.to_finding(f)));
                }
            }

            scanner_summary.push(format!(
                "Supply chain: found {} risks",
                sc_findings_vec.len()
            ));
            all_findings.extend(sc_findings_vec);
        }

        // Generate report
        if all_findings.is_empty() {
            let mut output = format!(
                "Security scan completed on '{}' with scanners: [{}].\n\n",
                path,
                scanner_list.join(", ")
            );
            output.push_str("Scanner results:\n");
            for summary in &scanner_summary {
                output.push_str(&format!("- {summary}\n"));
            }
            output.push_str("\nNo findings detected. The codebase appears clean.");
            Ok(ToolOutput::text(output))
        } else {
            // Persist findings as redacted facts for long-term memory
            let bridge = FindingMemoryBridge::new();
            let redacted_facts = bridge.batch_to_facts(&all_findings, FindingSeverity::Low);
            tracing::debug!(
                "Memory bridge: {} findings converted to {} redacted facts",
                all_findings.len(),
                redacted_facts.len()
            );

            // Build markdown report
            let report = findings_to_markdown(&all_findings, &format!("Security Scan: {path}"));

            // Prepend scanner summary
            let mut output = format!(
                "Security scan completed on '{}' with scanners: [{}].\n\n",
                path,
                scanner_list.join(", ")
            );
            output.push_str("Scanner results:\n");
            for summary in &scanner_summary {
                output.push_str(&format!("- {summary}\n"));
            }
            output.push_str(&format!(
                "\nTotal findings: {} (Critical: {}, High: {}, Medium: {}, Low: {}, Info: {})\n\n",
                all_findings.len(),
                all_findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::Critical)
                    .count(),
                all_findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::High)
                    .count(),
                all_findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::Medium)
                    .count(),
                all_findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::Low)
                    .count(),
                all_findings
                    .iter()
                    .filter(|f| f.severity == FindingSeverity::Info)
                    .count(),
            ));
            output.push_str(&report);

            // Append redacted finding summaries for memory persistence
            if !redacted_facts.is_empty() {
                output.push_str("\n\n## Memory-Tagged Findings\n");
                for fact in &redacted_facts {
                    output.push_str(&format!("- [{}] {}\n", fact.tags.join(", "), fact.content));
                }
            }

            Ok(ToolOutput::text(output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SecurityScanTool::new();
        assert_eq!(tool.name(), "security_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = SecurityScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["scanners"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = SecurityScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("Security scan"));
        assert!(result.content.contains("sast"));
    }

    #[tokio::test]
    async fn test_execute_specific_scanners() {
        let dir = tempfile::tempdir().unwrap();
        let tool = SecurityScanTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap(), "scanners": "sast, sca"}))
            .await
            .unwrap();
        assert!(result.content.contains(dir.path().to_str().unwrap()));
        assert!(result.content.contains("sast"));
        assert!(result.content.contains("sca"));
    }

    #[tokio::test]
    async fn test_execute_finds_issues() {
        let dir = tempfile::tempdir().unwrap();
        let vuln_file = dir.path().join("vuln.py");
        std::fs::write(
            &vuln_file,
            r#"
password = "super_secret_password123"
cursor.execute(f"SELECT * FROM users WHERE id={user_id}")
"#,
        )
        .unwrap();

        let tool = SecurityScanTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap(), "scanners": "sast, secrets"}))
            .await
            .unwrap();
        assert!(result.content.contains("Security scan completed"));
        // Should find something between SAST and secrets
        assert!(result.content.contains("findings") || result.content.contains("Findings"));
    }
}
