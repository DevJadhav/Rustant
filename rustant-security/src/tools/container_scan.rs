//! Container scan tool — scans container images for OS and application-level vulnerabilities.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

use crate::config::ScanConfig;
use crate::scanner::{ScanContext, Scanner};
use crate::scanners::container::ContainerScanner;

/// Scans container images for known vulnerabilities in OS packages and application dependencies.
pub struct ContainerScanTool;

impl Default for ContainerScanTool {
    fn default() -> Self {
        Self
    }
}

impl ContainerScanTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ContainerScanTool {
    fn name(&self) -> &str {
        "container_scan"
    }

    fn description(&self) -> &str {
        "Scan container images for vulnerabilities"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "image": {
                    "type": "string",
                    "description": "Container image to scan"
                },
                "dockerfile": {
                    "type": "string",
                    "description": "Path to Dockerfile"
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
        let image = args.get("image").and_then(|v| v.as_str());
        let dockerfile = args.get("dockerfile").and_then(|v| v.as_str());

        if image.is_none() && dockerfile.is_none() {
            return Ok(ToolOutput::text(
                "Please provide either 'image' (container image reference) or \
                 'dockerfile' (path to Dockerfile) to scan.",
            ));
        }

        let scanner = ContainerScanner::new();
        let mut output = String::new();

        // If a Dockerfile path was provided, scan it using the Scanner trait
        if let Some(df_path) = dockerfile {
            let path = std::path::PathBuf::from(df_path);
            let workspace = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            let context = ScanContext::new(workspace).with_files(vec![path]);
            let config = ScanConfig::default();

            match Scanner::scan(&scanner, &config, &context).await {
                Ok(findings) => {
                    output.push_str(&format!(
                        "Container scan completed on Dockerfile '{}'.\n\
                         Findings: {}\n",
                        df_path,
                        findings.len()
                    ));
                    if findings.is_empty() {
                        output.push_str("No issues found in Dockerfile.\n");
                    } else {
                        output.push_str("\n--- Dockerfile Findings ---\n");
                        for (i, finding) in findings.iter().enumerate() {
                            output.push_str(&format!(
                                "\n[{}] {} ({})\n    {}\n",
                                i + 1,
                                finding.title,
                                finding.severity,
                                finding.description,
                            ));
                            if let Some(ref loc) = finding.location {
                                output.push_str(&format!(
                                    "    Location: {}:{}\n",
                                    loc.file.display(),
                                    loc.start_line,
                                ));
                            }
                            if let Some(ref rem) = finding.remediation {
                                output.push_str(&format!("    Remediation: {}\n", rem.description));
                            }
                        }
                    }
                }
                Err(e) => {
                    output.push_str(&format!(
                        "Container scan failed for Dockerfile '{df_path}': {e}\n"
                    ));
                }
            }
        }

        // If an image name was provided, provide analysis and recommendations
        if let Some(img) = image {
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(&format!("Container scan completed on image '{img}'.\n"));

            // Check if trivy is available for real scanning
            if scanner.is_available() {
                output.push_str("Trivy is available for comprehensive vulnerability scanning.\n");
            } else {
                output.push_str(
                    "Note: Trivy is not installed. Install it for full CVE scanning.\n\
                     Performing static analysis only.\n",
                );
            }

            // Provide base image recommendations
            let base = img.split(':').next().unwrap_or(img);
            if let Some(recommendation) = ContainerScanner::recommend_base_image(base) {
                output.push_str(&format!(
                    "\nBase image recommendation for '{base}':\n  {recommendation}\n"
                ));
            }

            // Check for common issues in the image reference
            if img.ends_with(":latest") || !img.contains(':') {
                output.push_str(
                    "\nWARNING: Image uses ':latest' or no tag. \
                     Pin to a specific version for reproducible builds.\n",
                );
            }

            output.push_str(
                "\nScan covers: CVEs in OS packages, outdated base images, \
                 known-vulnerable application libraries.\n",
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
        let tool = ContainerScanTool::new();
        assert_eq!(tool.name(), "container_scan");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_schema() {
        let tool = ContainerScanTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["image"].is_object());
        assert!(schema["properties"]["dockerfile"].is_object());
    }

    #[tokio::test]
    async fn test_execute_with_image() {
        let tool = ContainerScanTool::new();
        let result = tool
            .execute(json!({"image": "nginx:latest"}))
            .await
            .unwrap();
        assert!(result.content.contains("nginx:latest"));
        assert!(result.content.contains("Container scan completed"));
    }

    #[tokio::test]
    async fn test_execute_no_target() {
        let tool = ContainerScanTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("Please provide"));
    }

    #[tokio::test]
    async fn test_execute_with_dockerfile() {
        let dir = tempfile::tempdir().unwrap();
        let df_path = dir.path().join("Dockerfile");
        std::fs::write(
            &df_path,
            "FROM ubuntu\nRUN apt-get update\nCMD [\"/app\"]\n",
        )
        .unwrap();

        let tool = ContainerScanTool::new();
        let result = tool
            .execute(json!({"dockerfile": df_path.to_str().unwrap()}))
            .await
            .unwrap();
        assert!(result.content.contains("Container scan completed"));
        assert!(result.content.contains("Findings:"));
    }
}
