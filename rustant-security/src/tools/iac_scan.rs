//! IaC scan tool — scans infrastructure-as-code files for security misconfigurations.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

use crate::scanners::iac::IacScanner;

/// Scans infrastructure-as-code (Terraform, Kubernetes, CloudFormation) for
/// security misconfigurations, overly permissive IAM policies, and compliance violations.
pub struct IacScanTool;

impl Default for IacScanTool {
    fn default() -> Self {
        Self
    }
}

impl IacScanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for IacScanTool {
    fn name(&self) -> &str {
        "iac_scan"
    }

    fn description(&self) -> &str {
        "Scan infrastructure-as-code for security misconfigurations"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to IaC files"
                },
                "framework": {
                    "type": "string",
                    "description": "Framework (terraform/kubernetes/cloudformation)"
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

        let framework = args
            .get("framework")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        let valid_frameworks = ["terraform", "kubernetes", "cloudformation", "auto"];
        if !valid_frameworks.contains(&framework) {
            return Ok(ToolOutput::text(format!(
                "Unknown IaC framework '{framework}'. Supported: terraform, kubernetes, cloudformation"
            )));
        }

        let scanner = IacScanner::new();

        // Collect IaC files from the path
        let target_path = std::path::Path::new(path);
        let mut files_scanned = 0;
        let mut all_findings = Vec::new();

        if target_path.is_file() {
            // Single file scan
            if let Ok(content) = std::fs::read_to_string(target_path) {
                let file_path_str = target_path.display().to_string();
                let findings = scanner.scan_file(&content, &file_path_str);
                all_findings.extend(findings);
                files_scanned = 1;
            }
        } else if target_path.is_dir() {
            // Directory scan — walk and find IaC files
            if let Ok(entries) = std::fs::read_dir(target_path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    let ext = entry_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("");
                    if matches!(ext, "tf" | "tfvars" | "yaml" | "yml" | "json")
                        && let Ok(content) = std::fs::read_to_string(&entry_path)
                    {
                        let file_path_str = entry_path.display().to_string();
                        let findings = scanner.scan_file(&content, &file_path_str);
                        all_findings.extend(findings);
                        files_scanned += 1;
                    }
                }
            }
        }

        let mut output = format!(
            "IaC scan completed on '{}' (framework: {}).\n\
             Files scanned: {} | Findings: {}\n",
            path,
            framework,
            files_scanned,
            all_findings.len(),
        );

        if all_findings.is_empty() {
            if files_scanned == 0 {
                output.push_str(
                    "\nNo IaC files found at the specified path. \
                     Ensure the path contains .tf, .yaml, .yml, or .json files.\n",
                );
            } else {
                output.push_str(
                    "\nNo misconfigurations found. IaC follows security best practices.\n",
                );
            }
        } else {
            // Group by severity
            let critical: Vec<_> = all_findings
                .iter()
                .filter(|f| matches!(f.severity, crate::finding::FindingSeverity::Critical))
                .collect();
            let high: Vec<_> = all_findings
                .iter()
                .filter(|f| matches!(f.severity, crate::finding::FindingSeverity::High))
                .collect();
            let medium: Vec<_> = all_findings
                .iter()
                .filter(|f| matches!(f.severity, crate::finding::FindingSeverity::Medium))
                .collect();
            let low_info: Vec<_> = all_findings
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
                "Severity: {} critical, {} high, {} medium, {} low/info\n",
                critical.len(),
                high.len(),
                medium.len(),
                low_info.len(),
            ));

            output.push_str("\n--- Findings ---\n");
            for (i, finding) in all_findings.iter().enumerate() {
                output.push_str(&format!(
                    "\n[{}] {} ({})\n    {}\n",
                    i + 1,
                    finding.title,
                    finding.severity,
                    finding.description,
                ));
                if let Some(ref loc) = finding.location {
                    output.push_str(&format!(
                        "    File: {}:{}\n",
                        loc.file.display(),
                        loc.start_line,
                    ));
                }
                if let Some(ref rem) = finding.remediation {
                    output.push_str(&format!("    Remediation: {}\n", rem.description));
                }
                // Show CIS references
                for reference in &finding.references {
                    output.push_str(&format!("    Reference: {}\n", reference.id));
                }
            }
        }

        output.push_str(
            "\nChecked for: overly permissive IAM policies, unencrypted storage, \
             public network exposure, missing logging/monitoring, \
             insecure defaults, resource limits.\n",
        );

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = IacScanTool::new();
        assert_eq!(tool.name(), "iac_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = IacScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["framework"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = IacScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("IaC scan completed"));
        assert!(result.content.contains("framework: auto"));
    }

    #[tokio::test]
    async fn test_execute_invalid_framework() {
        let tool = IacScanTool::new();
        let result = tool.execute(json!({"framework": "ansible"})).await.unwrap();
        assert!(result.content.contains("Unknown IaC framework"));
    }

    #[tokio::test]
    async fn test_execute_terraform_file() {
        let dir = tempfile::tempdir().unwrap();
        let tf_path = dir.path().join("main.tf");
        std::fs::write(
            &tf_path,
            r#"resource "aws_security_group" "open" {
  ingress {
    from_port   = 0
    to_port     = 65535
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#,
        )
        .unwrap();

        let tool = IacScanTool::new();
        let result = tool
            .execute(json!({"path": tf_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("IaC scan completed"));
        assert!(result.content.contains("Findings"));
    }
}
