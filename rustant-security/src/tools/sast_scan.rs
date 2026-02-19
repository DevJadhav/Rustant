//! SAST scan tool — static application security testing for source code.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::finding::{Finding, FindingSeverity};
use crate::scanners::sast::SastScanner;

/// Runs static application security testing (SAST) to detect vulnerabilities in source code.
pub struct SastScanTool;

impl Default for SastScanTool {
    fn default() -> Self {
        Self
    }
}

impl SastScanTool {
    pub fn new() -> Self {
        Self
    }
}

/// Parse a severity threshold string into a `FindingSeverity`.
fn parse_severity(s: &str) -> Option<FindingSeverity> {
    match s {
        "info" => Some(FindingSeverity::Info),
        "low" => Some(FindingSeverity::Low),
        "medium" => Some(FindingSeverity::Medium),
        "high" => Some(FindingSeverity::High),
        "critical" => Some(FindingSeverity::Critical),
        _ => None,
    }
}

#[async_trait]
impl Tool for SastScanTool {
    fn name(&self) -> &str {
        "sast_scan"
    }

    fn description(&self) -> &str {
        "Run static application security testing (SAST)"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to scan"
                },
                "language": {
                    "type": "string",
                    "description": "Target language (auto-detected if omitted)"
                },
                "severity_threshold": {
                    "type": "string",
                    "description": "Minimum severity (info/low/medium/high/critical)"
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

        let language_filter = args
            .get("language")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        let severity_threshold = args
            .get("severity_threshold")
            .and_then(|v| v.as_str())
            .unwrap_or("info");

        // Validate severity threshold
        let valid_severities = ["info", "low", "medium", "high", "critical"];
        if !valid_severities.contains(&severity_threshold) {
            return Ok(ToolOutput::text(format!(
                "Invalid severity threshold '{}'. Valid values: {}",
                severity_threshold,
                valid_severities.join(", ")
            )));
        }

        let threshold = parse_severity(severity_threshold).unwrap_or(FindingSeverity::Info);
        let scanner = SastScanner::new();
        let scan_path = Path::new(path);

        let mut all_findings: Vec<Finding> = Vec::new();
        let mut files_scanned: usize = 0;

        if scan_path.is_file() {
            // Scan a single file
            if let Ok(source) = std::fs::read_to_string(scan_path) {
                let lang = Language::from_path(scan_path);
                let lang_str = if language_filter != "auto" {
                    language_filter.to_string()
                } else {
                    lang.as_str().to_string()
                };
                if lang != Language::Unknown || language_filter != "auto" {
                    let findings = scanner.scan_source(&source, scan_path, &lang_str);
                    all_findings.extend(findings);
                    files_scanned += 1;
                }
            }
        } else if scan_path.is_dir() {
            // Walk directory and scan files
            for entry in WalkDir::new(scan_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let file_path = entry.path();
                let lang = Language::from_path(file_path);

                // Skip unknown languages unless a filter is specified
                if lang == Language::Unknown && language_filter == "auto" {
                    continue;
                }

                let lang_str = if language_filter != "auto" {
                    language_filter.to_string()
                } else {
                    lang.as_str().to_string()
                };

                if let Ok(source) = std::fs::read_to_string(file_path) {
                    let findings = scanner.scan_source(&source, file_path, &lang_str);
                    all_findings.extend(findings);
                    files_scanned += 1;
                }
            }
        } else {
            return Ok(ToolOutput::text(format!(
                "SAST scan: path '{path}' does not exist or is not accessible."
            )));
        }

        // Filter by severity threshold
        all_findings.retain(|f| f.severity >= threshold);

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
        let info = all_findings
            .iter()
            .filter(|f| f.severity == FindingSeverity::Info)
            .count();

        let mut output = format!(
            "SAST scan completed on '{}' (language: {}, threshold: {}).\n\
             Analyzed {} files, found {} findings.\n\n\
             Severity breakdown:\n\
             - Critical: {}\n\
             - High: {}\n\
             - Medium: {}\n\
             - Low: {}\n\
             - Info: {}",
            path,
            language_filter,
            severity_threshold,
            files_scanned,
            all_findings.len(),
            critical,
            high,
            medium,
            low,
            info
        );

        // List top findings (up to 20)
        if !all_findings.is_empty() {
            output.push_str("\n\nTop findings:\n");
            for (i, finding) in all_findings.iter().take(20).enumerate() {
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
            if all_findings.len() > 20 {
                output.push_str(&format!(
                    "... and {} more findings.\n",
                    all_findings.len() - 20
                ));
            }
        }

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SastScanTool::new();
        assert_eq!(tool.name(), "sast_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = SastScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["language"].is_object());
        assert!(schema["properties"]["severity_threshold"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = SastScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("SAST scan completed"));
        assert!(result.content.contains("language: auto"));
    }

    #[tokio::test]
    async fn test_execute_invalid_severity() {
        let tool = SastScanTool::new();
        let result = tool
            .execute(json!({"severity_threshold": "banana"}))
            .await
            .unwrap();
        assert!(result.content.contains("Invalid severity threshold"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_path() {
        let tool = SastScanTool::new();
        let result = tool
            .execute(json!({"path": "/nonexistent/path/that/does/not/exist"}))
            .await
            .unwrap();
        assert!(result.content.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_execute_finds_vulnerabilities() {
        // Create a temp file with a known vulnerability pattern
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("vuln.py");
        std::fs::write(
            &file_path,
            r#"cursor.execute(f"SELECT * FROM users WHERE id={user_id}")"#,
        )
        .unwrap();

        let tool = SastScanTool::new();
        let result = tool
            .execute(json!({"path": dir.path().to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("SAST scan completed"));
        // Should find at least one finding for SQL injection
        assert!(
            result.content.contains("found") && !result.content.contains("found 0 findings"),
            "Should detect SQL injection in test file"
        );
    }
}
