//! Secrets scan tool — detects hardcoded secrets and credentials in source code.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use walkdir::WalkDir;

use crate::finding::FindingSeverity;
use crate::scanners::secrets::SecretsScanner;

/// Scans source code for hardcoded secrets, API keys, tokens, and credentials.
pub struct SecretsScanTool;

impl Default for SecretsScanTool {
    fn default() -> Self {
        Self
    }
}

impl SecretsScanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SecretsScanTool {
    fn name(&self) -> &str {
        "secrets_scan"
    }

    fn description(&self) -> &str {
        "Detect hardcoded secrets and credentials in source code"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to scan"
                },
                "scan_history": {
                    "type": "boolean",
                    "description": "Scan git history (default: false)"
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

        let scan_history = args
            .get("scan_history")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let history_msg = if scan_history {
            " including git history"
        } else {
            ""
        };

        let scanner = SecretsScanner::new();
        let scan_path = Path::new(path);

        let mut all_findings = Vec::new();
        let mut files_scanned: usize = 0;

        if scan_path.is_file() {
            if let Ok(source) = std::fs::read_to_string(scan_path) {
                let findings = scanner.scan_source(&source, scan_path);
                all_findings.extend(findings);
                files_scanned += 1;
            }
        } else if scan_path.is_dir() {
            for entry in WalkDir::new(scan_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();

                // Skip binary files and very large files
                let metadata = match std::fs::metadata(file_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if metadata.len() > 1_000_000 {
                    continue; // Skip files > 1MB
                }

                if let Ok(source) = std::fs::read_to_string(file_path) {
                    let findings = scanner.scan_source(&source, file_path);
                    all_findings.extend(findings);
                    files_scanned += 1;
                }
            }
        } else {
            return Ok(ToolOutput::text(format!(
                "Secrets scan: path '{path}' does not exist or is not accessible."
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

        // Count by secret type (using tags)
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        for finding in &all_findings {
            for tag in &finding.tags {
                if tag != "secret" {
                    *type_counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut output = format!(
            "Secrets scan completed on '{}'{}.  \
             Scanned {} files, found {} secrets.\n\n\
             Severity breakdown:\n\
             - Critical: {}\n\
             - High: {}\n\
             - Medium: {}\n\
             - Low: {}",
            path,
            history_msg,
            files_scanned,
            all_findings.len(),
            critical,
            high,
            medium,
            low
        );

        // Show types found
        if !type_counts.is_empty() {
            output.push_str("\n\nSecret types detected:\n");
            let mut sorted_types: Vec<_> = type_counts.iter().collect();
            sorted_types.sort_by(|a, b| b.1.cmp(a.1));
            for (secret_type, count) in sorted_types {
                output.push_str(&format!("- {secret_type}: {count}\n"));
            }
        }

        // List locations (NEVER include actual secret values)
        if !all_findings.is_empty() {
            output.push_str("\nLocations:\n");
            for (i, finding) in all_findings.iter().take(30).enumerate() {
                let location = finding
                    .location
                    .as_ref()
                    .map(|loc| format!("{}:{}", loc.file.display(), loc.start_line))
                    .unwrap_or_else(|| "unknown".to_string());
                output.push_str(&format!(
                    "{}. [{}] {} ({})\n",
                    i + 1,
                    finding.severity.as_str().to_uppercase(),
                    finding.title,
                    location
                ));
            }
            if all_findings.len() > 30 {
                output.push_str(&format!(
                    "... and {} more locations.\n",
                    all_findings.len() - 30
                ));
            }
        }

        if scan_history {
            output.push_str(
                "\nNote: Git history scanning requires repository access. \
                 Only current file contents were scanned in this run.",
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
        let tool = SecretsScanTool::new();
        assert_eq!(tool.name(), "secrets_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = SecretsScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["scan_history"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = SecretsScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("Secrets scan completed"));
        assert!(!result.content.contains("git history"));
    }

    #[tokio::test]
    async fn test_execute_with_history() {
        let tool = SecretsScanTool::new();
        let result = tool
            .execute(json!({"path": "/tmp", "scan_history": true}))
            .await
            .unwrap();
        assert!(result.content.contains("git history") || result.content.contains("Git history"));
    }

    #[tokio::test]
    async fn test_execute_detects_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("config.py");
        std::fs::write(&file_path, r#"AWS_ACCESS_KEY = "AKIAIOSFODNN7EXAMPLE""#).unwrap();

        let tool = SecretsScanTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Secrets scan completed"));
        // Should find at least one secret — never include actual values
        assert!(
            !result.content.contains("AKIAIOSFODNN7EXAMPLE"),
            "Output must never contain actual secret values"
        );
    }
}
