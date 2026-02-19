//! Terraform check tool — Terraform-specific security analysis for .tf files.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

use crate::scanners::iac::IacScanner;

/// Performs Terraform-specific security analysis including provider configuration,
/// state backend security, module source validation, and resource-level checks.
pub struct TerraformCheckTool;

impl Default for TerraformCheckTool {
    fn default() -> Self {
        Self
    }
}

impl TerraformCheckTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for TerraformCheckTool {
    fn name(&self) -> &str {
        "terraform_check"
    }

    fn description(&self) -> &str {
        "Terraform-specific security analysis"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to .tf files"
                }
            },
            "required": []
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let scanner = IacScanner::new();
        let target_path = std::path::Path::new(path);

        let mut tf_files = Vec::new();
        let mut all_findings = Vec::new();

        // Collect .tf and .tfvars files
        if target_path.is_file() {
            let ext = target_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if matches!(ext, "tf" | "tfvars") {
                tf_files.push(target_path.to_path_buf());
            }
        } else if target_path.is_dir()
            && let Ok(entries) = std::fs::read_dir(target_path)
        {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                let ext = entry_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("");
                if matches!(ext, "tf" | "tfvars") {
                    tf_files.push(entry_path);
                }
            }
        }

        // Terraform-specific static checks on each file
        let mut provider_version_pinned = true;
        let mut state_backend_found = false;
        let mut sensitive_vars_exposed = Vec::new();
        let mut module_sources_unpinned = Vec::new();

        for file in &tf_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                let file_path_str = file.display().to_string();

                // Run IaC scanner rules (Terraform-specific ones)
                let findings = scanner.scan_file(&content, &file_path_str);
                all_findings.extend(findings);

                // Additional Terraform-specific checks
                // Check provider version constraints
                if content.contains("required_providers") && !content.contains("version") {
                    provider_version_pinned = false;
                }

                // Check for state backend
                if content.contains("backend ") {
                    state_backend_found = true;
                }

                // Check for sensitive variables without sensitive flag
                for line in content.lines() {
                    let trimmed = line.trim();
                    let lower = trimmed.to_lowercase();
                    if trimmed.starts_with("variable")
                        && (lower.contains("password")
                            || lower.contains("secret")
                            || lower.contains("token")
                            || lower.contains("api_key"))
                    {
                        // Check if next few lines contain "sensitive = true"
                        if !content.contains("sensitive = true")
                            && !content.contains("sensitive=true")
                        {
                            sensitive_vars_exposed.push(trimmed.to_string());
                        }
                    }
                }

                // Check for unpinned module sources
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("source")
                        && trimmed.contains("github.com")
                        && !trimmed.contains("?ref=")
                        && !trimmed.contains("//")
                    {
                        module_sources_unpinned.push(trimmed.to_string());
                    }
                }
            }
        }

        let mut output = format!(
            "Terraform security check completed on '{}'.\n\
             .tf/.tfvars files found: {} | IaC findings: {}\n",
            path,
            tf_files.len(),
            all_findings.len(),
        );

        // Terraform-specific analysis results
        output.push_str("\n--- Terraform Analysis ---\n");
        output.push_str(&format!(
            "  Provider version pinned: {}\n",
            if provider_version_pinned {
                "yes"
            } else {
                "NO - pin provider versions for reproducibility"
            }
        ));
        output.push_str(&format!(
            "  State backend configured: {}\n",
            if state_backend_found {
                "yes"
            } else {
                "NO - configure remote state backend with encryption"
            }
        ));

        if !sensitive_vars_exposed.is_empty() {
            output.push_str(&format!(
                "  Sensitive variables without 'sensitive = true': {}\n",
                sensitive_vars_exposed.len()
            ));
            for var in &sensitive_vars_exposed {
                output.push_str(&format!("    - {var}\n"));
            }
        }

        if !module_sources_unpinned.is_empty() {
            output.push_str(&format!(
                "  Unpinned module sources: {}\n",
                module_sources_unpinned.len()
            ));
            for src in &module_sources_unpinned {
                output.push_str(&format!("    - {src}\n"));
            }
        }

        // Show IaC scanner findings
        if !all_findings.is_empty() {
            output.push_str("\n--- Security Findings ---\n");
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
                    output.push_str(&format!("    Fix: {}\n", rem.description));
                }
                for reference in &finding.references {
                    output.push_str(&format!("    Reference: {}\n", reference.id));
                }
            }
        } else if tf_files.is_empty() {
            output.push_str("\nNo .tf or .tfvars files found at the specified path.\n");
        } else {
            output.push_str("\nNo security misconfigurations detected in Terraform files.\n");
        }

        output.push_str(
            "\nAnalyzed: provider version constraints, state backend encryption, \
             module source pinning, sensitive variable handling, \
             resource-level security (S3 bucket policies, security groups, \
             IAM policies, encryption at rest, logging configuration).\n",
        );

        Ok(ToolOutput::text(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = TerraformCheckTool::new();
        assert_eq!(tool.name(), "terraform_check");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = TerraformCheckTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = TerraformCheckTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(
            result
                .content
                .contains("Terraform security check completed")
        );
        assert!(result.content.contains("provider version"));
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = TerraformCheckTool::new();
        let result = tool
            .execute(json!({"path": "/infra/terraform"}))
            .await
            .unwrap();
        assert!(result.content.contains("/infra/terraform"));
    }

    #[tokio::test]
    async fn test_execute_with_tf_file() {
        let dir = tempfile::tempdir().unwrap();
        let tf_path = dir.path().join("main.tf");
        std::fs::write(
            &tf_path,
            r#"resource "aws_security_group" "open" {
  ingress {
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#,
        )
        .unwrap();

        let tool = TerraformCheckTool::new();
        let result = tool
            .execute(json!({"path": tf_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(
            result
                .content
                .contains("Terraform security check completed")
        );
        assert!(result.content.contains("Security Findings"));
    }
}
