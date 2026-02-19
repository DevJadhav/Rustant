//! Dockerfile lint tool — analyzes Dockerfiles for security best practices and misconfigurations.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

use crate::scanners::dockerfile::DockerfileLinter;

/// Analyzes a Dockerfile for security issues such as running as root,
/// using latest tags, exposing unnecessary ports, and missing health checks.
pub struct DockerfileLintTool;

impl Default for DockerfileLintTool {
    fn default() -> Self {
        Self
    }
}

impl DockerfileLintTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for DockerfileLintTool {
    fn name(&self) -> &str {
        "dockerfile_lint"
    }

    fn description(&self) -> &str {
        "Analyze Dockerfile for security best practices"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to Dockerfile"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = match args.get("path").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => {
                return Ok(ToolOutput::text(
                    "Required parameter 'path' is missing or empty. \
                     Please provide the path to a Dockerfile.",
                ));
            }
        };

        // Read the Dockerfile
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "Dockerfile lint completed for '{path}'. \
                     Error: Could not read file: {e}"
                )));
            }
        };

        // Parse and lint
        let linter = DockerfileLinter::new();
        let findings = linter.lint(&content, path);
        let parsed = DockerfileLinter::parse(&content);

        let mut output = format!(
            "Dockerfile lint completed for '{}'.\n\
             Instructions: {} | Stages: {} | Multi-stage: {} | Findings: {}\n",
            path,
            parsed.instructions.len(),
            parsed.stages.len(),
            if parsed.is_multi_stage { "yes" } else { "no" },
            findings.len(),
        );

        if findings.is_empty() {
            output.push_str("\nNo issues found. Dockerfile follows security best practices.\n");
        } else {
            // Group by severity
            let critical_high: Vec<_> = findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.severity,
                        crate::finding::FindingSeverity::Critical
                            | crate::finding::FindingSeverity::High
                    )
                })
                .collect();
            let medium: Vec<_> = findings
                .iter()
                .filter(|f| matches!(f.severity, crate::finding::FindingSeverity::Medium))
                .collect();
            let low_info: Vec<_> = findings
                .iter()
                .filter(|f| {
                    matches!(
                        f.severity,
                        crate::finding::FindingSeverity::Low
                            | crate::finding::FindingSeverity::Info
                    )
                })
                .collect();

            output.push_str(&format!(
                "Severity breakdown: {} critical/high, {} medium, {} low/info\n",
                critical_high.len(),
                medium.len(),
                low_info.len(),
            ));

            output.push_str("\n--- Findings ---\n");
            for (i, finding) in findings.iter().enumerate() {
                output.push_str(&format!(
                    "\n[{}] {} ({})\n    {}\n",
                    i + 1,
                    finding.title,
                    finding.severity,
                    finding.description,
                ));
                if let Some(ref loc) = finding.location {
                    output.push_str(&format!("    Line: {}\n", loc.start_line));
                }
                if let Some(ref rem) = finding.remediation {
                    output.push_str(&format!("    Fix: {}\n", rem.description));
                }
            }
        }

        output.push_str(
            "\nChecks performed: running as root, use of :latest tag, \
             unnecessary ADD (vs COPY), missing HEALTHCHECK, \
             consecutive RUN consolidation, apt cache cleanup, \
             absolute WORKDIR paths, pinned package versions.",
        );

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = DockerfileLintTool::new();
        assert_eq!(tool.name(), "dockerfile_lint");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = DockerfileLintTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("path"))
        );
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let dir = tempfile::tempdir().unwrap();
        let df_path = dir.path().join("Dockerfile");
        std::fs::write(
            &df_path,
            "FROM ubuntu:22.04\nUSER 1001\nCMD [\"/bin/bash\"]\n",
        )
        .unwrap();

        let tool = DockerfileLintTool::new();
        let result = tool
            .execute(json!({"path": df_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Dockerfile lint completed"));
        assert!(result.content.contains("Findings:"));
    }

    #[tokio::test]
    async fn test_execute_missing_path() {
        let tool = DockerfileLintTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("missing"));
    }

    #[tokio::test]
    async fn test_execute_with_issues() {
        let dir = tempfile::tempdir().unwrap();
        let df_path = dir.path().join("Dockerfile");
        std::fs::write(&df_path, "FROM ubuntu\nADD app.py /app/\nCMD [\"/app\"]\n").unwrap();

        let tool = DockerfileLintTool::new();
        let result = tool
            .execute(json!({"path": df_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Dockerfile lint completed"));
        assert!(result.content.contains("Findings ---"));
    }
}
