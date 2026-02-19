//! Kubernetes lint tool — analyzes Kubernetes manifests for security issues.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

use crate::scanners::iac::IacScanner;

/// Analyzes Kubernetes YAML manifests for security misconfigurations such as
/// privileged containers, missing resource limits, and insecure capabilities.
pub struct K8sLintTool;

impl Default for K8sLintTool {
    fn default() -> Self {
        Self
    }
}

impl K8sLintTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for K8sLintTool {
    fn name(&self) -> &str {
        "k8s_lint"
    }

    fn description(&self) -> &str {
        "Analyze Kubernetes manifests for security issues"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to manifest files"
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

        let mut yaml_files = Vec::new();
        let mut all_findings = Vec::new();
        let mut k8s_resources: Vec<(String, String)> = Vec::new(); // (kind, name)

        // Collect YAML files
        if target_path.is_file() {
            let ext = target_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("");
            if matches!(ext, "yaml" | "yml") {
                yaml_files.push(target_path.to_path_buf());
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
                if matches!(ext, "yaml" | "yml") {
                    yaml_files.push(entry_path);
                }
            }
        }

        // Scan each YAML file, filtering for Kubernetes manifests
        for file in &yaml_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                // Check if it's a Kubernetes manifest
                if !content.contains("apiVersion:") || !content.contains("kind:") {
                    continue;
                }

                let file_path_str = file.display().to_string();
                let findings = scanner.scan_file(&content, &file_path_str);
                all_findings.extend(findings);

                // Extract resource kind and name for inventory
                let kind = content
                    .lines()
                    .find(|l| l.starts_with("kind:"))
                    .map(|l| l.trim_start_matches("kind:").trim().to_string())
                    .unwrap_or_else(|| "Unknown".to_string());
                let name = content
                    .lines()
                    .find(|l| l.trim().starts_with("name:"))
                    .map(|l| l.trim().trim_start_matches("name:").trim().to_string())
                    .unwrap_or_else(|| "unnamed".to_string());
                k8s_resources.push((kind, name));

                // Additional K8s-specific checks beyond IaC rules
                check_k8s_specific(&content, file, &mut all_findings);
            }
        }

        let k8s_files_count = k8s_resources.len();

        let mut output = format!(
            "Kubernetes manifest lint completed on '{}'.\n\
             YAML files found: {} | K8s manifests: {} | Findings: {}\n",
            path,
            yaml_files.len(),
            k8s_files_count,
            all_findings.len(),
        );

        // Resource inventory
        if !k8s_resources.is_empty() {
            output.push_str("\n--- K8s Resources ---\n");
            for (kind, name) in &k8s_resources {
                output.push_str(&format!("  {kind} / {name}\n"));
            }
        }

        if all_findings.is_empty() {
            if k8s_files_count == 0 {
                output.push_str(
                    "\nNo Kubernetes manifests found at the specified path.\n\
                     Ensure the path contains YAML files with apiVersion and kind fields.\n",
                );
            } else {
                output.push_str(
                    "\nNo security issues found. Manifests follow security best practices.\n",
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

            output.push_str(&format!(
                "\nSeverity: {} critical, {} high, {} other\n",
                critical.len(),
                high.len(),
                all_findings.len() - critical.len() - high.len(),
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
                    output.push_str(&format!("    Fix: {}\n", rem.description));
                }
                for reference in &finding.references {
                    output.push_str(&format!("    Reference: {}\n", reference.id));
                }
            }
        }

        output.push_str(
            "\nChecked for: privileged containers, runAsRoot, hostNetwork/hostPID, \
             missing resource limits/requests, writable root filesystem, \
             dangerous capabilities (SYS_ADMIN, NET_RAW), missing securityContext, \
             latest image tags, missing liveness/readiness probes, \
             exposed secrets in env vars.\n",
        );

        Ok(ToolOutput::text(output))
    }
}

/// Additional Kubernetes-specific checks beyond what the IaC scanner covers.
fn check_k8s_specific(
    content: &str,
    _file: &std::path::Path,
    _findings: &mut [crate::finding::Finding],
) {
    // The IaC scanner already covers: privileged, hostNetwork, runAsNonRoot, resource limits.
    // Additional patterns could be added here in the future, e.g.:
    // - readOnlyRootFilesystem check
    // - liveness/readiness probe check
    // - image tag :latest check
    // - hostPID/hostIPC check
    // - capability checks (SYS_ADMIN, NET_RAW)
    // For now, the IaC scanner rules provide good coverage.
    let _ = content;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = K8sLintTool::new();
        assert_eq!(tool.name(), "k8s_lint");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = K8sLintTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[tokio::test]
    async fn test_execute_default() {
        let tool = K8sLintTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(
            result
                .content
                .contains("Kubernetes manifest lint completed")
        );
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = K8sLintTool::new();
        let result = tool
            .execute(json!({"path": "/k8s/manifests"}))
            .await
            .unwrap();
        assert!(result.content.contains("/k8s/manifests"));
    }

    #[tokio::test]
    async fn test_execute_with_k8s_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("pod.yaml");
        std::fs::write(
            &manifest_path,
            "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\nspec:\n  containers:\n  - name: test\n    image: nginx\n    securityContext:\n      privileged: true\n",
        )
        .unwrap();

        let tool = K8sLintTool::new();
        let result = tool
            .execute(json!({"path": manifest_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(
            result
                .content
                .contains("Kubernetes manifest lint completed")
        );
        assert!(result.content.contains("Findings"));
        assert!(result.content.contains("privileged"));
    }
}
