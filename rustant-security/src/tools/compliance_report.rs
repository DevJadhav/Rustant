//! Compliance Report — Generate compliance reports for security frameworks.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::compliance::frameworks::{ControlStatus, FrameworkRegistry};

/// Generate compliance reports for security frameworks such as SOC 2,
/// ISO 27001, NIST CSF, and PCI DSS. Maps findings and controls to
/// framework requirements and produces formatted reports.
pub struct ComplianceReportTool;

#[async_trait]
impl Tool for ComplianceReportTool {
    fn name(&self) -> &str {
        "compliance_report"
    }

    fn description(&self) -> &str {
        "Generate compliance reports for frameworks (SOC 2, ISO 27001, etc.)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "framework": {
                    "type": "string",
                    "description": "Compliance framework (soc2/iso27001/nist-800-53/pci-dss/owasp-asvs/cis-controls)"
                },
                "format": {
                    "type": "string",
                    "description": "Report format (markdown/json)"
                },
                "scanners_run": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of scanners that have been run (sast/sca/secrets/container/iac)"
                },
                "finding_cwes": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "CWE IDs from detected findings (e.g. CWE-89)"
                }
            },
            "required": ["framework"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let framework_id = args
            .get("framework")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "compliance_report".to_string(),
                reason: "'framework' parameter is required".to_string(),
            })?;
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");

        // Parse optional scanner and finding lists
        let scanners_run: Vec<String> = args
            .get("scanners_run")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let finding_cwes: Vec<String> = args
            .get("finding_cwes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let registry = FrameworkRegistry::with_defaults();

        // Verify framework exists
        let framework = registry.get(framework_id).ok_or_else(|| {
            let available: Vec<String> = registry.list().iter().map(|f| f.id.clone()).collect();
            ToolError::InvalidArguments {
                name: "compliance_report".to_string(),
                reason: format!(
                    "Framework '{}' not found. Available: {}",
                    framework_id,
                    available.join(", ")
                ),
            }
        })?;

        // Assess compliance
        let assessment = registry
            .assess(framework_id, &scanners_run, &finding_cwes)
            .ok_or_else(|| ToolError::ExecutionFailed {
                name: "compliance_report".to_string(),
                message: format!("Failed to assess compliance for '{framework_id}'"),
            })?;

        // Format output
        let output = match format {
            "json" => match serde_json::to_string_pretty(&assessment) {
                Ok(json) => json,
                Err(e) => {
                    return Err(ToolError::ExecutionFailed {
                        name: "compliance_report".to_string(),
                        message: format!("JSON serialization failed: {e}"),
                    });
                }
            },
            _ => {
                // Markdown format
                let mut md = String::new();
                md.push_str(&format!(
                    "# Compliance Report: {} ({})\n\n",
                    framework.name, framework.version
                ));
                md.push_str(&format!("{}\n\n", framework.description));

                // Summary
                md.push_str("## Summary\n\n");
                md.push_str(&format!(
                    "- **Compliance rate:** {:.1}%\n",
                    assessment.compliance_rate
                ));
                md.push_str(&format!(
                    "- **Total controls:** {}\n",
                    assessment.summary.total_controls
                ));
                md.push_str(&format!(
                    "- **Compliant:** {}\n",
                    assessment.summary.compliant
                ));
                md.push_str(&format!(
                    "- **Partially compliant:** {}\n",
                    assessment.summary.partially_compliant
                ));
                md.push_str(&format!(
                    "- **Non-compliant:** {}\n",
                    assessment.summary.non_compliant
                ));
                md.push_str(&format!(
                    "- **Not applicable:** {}\n",
                    assessment.summary.not_applicable
                ));
                md.push_str(&format!(
                    "- **Not assessed:** {}\n\n",
                    assessment.summary.not_assessed
                ));

                if !scanners_run.is_empty() {
                    md.push_str(&format!(
                        "**Scanners run:** {}\n\n",
                        scanners_run.join(", ")
                    ));
                } else {
                    md.push_str("**Note:** No scanners specified. Run security scans first for full assessment.\n\n");
                }

                // Control details
                md.push_str("## Control Assessments\n\n");
                md.push_str("| Control | Status | Evidence | Findings |\n");
                md.push_str("|---------|--------|----------|----------|\n");

                for ctrl_assessment in &assessment.assessments {
                    let status_str = match ctrl_assessment.status {
                        ControlStatus::Compliant => "Compliant",
                        ControlStatus::PartiallyCompliant => "Partial",
                        ControlStatus::NonCompliant => "Non-Compliant",
                        ControlStatus::NotApplicable => "N/A",
                        ControlStatus::NotAssessed => "Not Assessed",
                    };
                    let evidence = if ctrl_assessment.evidence.is_empty() {
                        "-".to_string()
                    } else {
                        ctrl_assessment.evidence.join("; ")
                    };
                    let findings = if ctrl_assessment.related_findings.is_empty() {
                        "-".to_string()
                    } else {
                        ctrl_assessment.related_findings.join(", ")
                    };
                    md.push_str(&format!(
                        "| {} | {} | {} | {} |\n",
                        ctrl_assessment.control_id, status_str, evidence, findings
                    ));
                }
                md.push('\n');

                // Non-compliant controls detail
                let non_compliant: Vec<_> = assessment
                    .assessments
                    .iter()
                    .filter(|a| a.status == ControlStatus::NonCompliant)
                    .collect();
                if !non_compliant.is_empty() {
                    md.push_str("## Non-Compliant Controls\n\n");
                    for ctrl in non_compliant {
                        // Look up the control title from the framework
                        let title = framework
                            .controls
                            .iter()
                            .find(|c| c.id == ctrl.control_id)
                            .map(|c| c.title.as_str())
                            .unwrap_or("Unknown");
                        md.push_str(&format!("### {} — {}\n\n", ctrl.control_id, title));
                        md.push_str(&format!(
                            "Related findings: {}\n\n",
                            ctrl.related_findings.join(", ")
                        ));
                    }
                }

                md
            }
        };

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ComplianceReportTool;
        assert_eq!(tool.name(), "compliance_report");
    }

    #[test]
    fn test_schema() {
        let tool = ComplianceReportTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["framework"].is_object());
        assert!(schema["properties"]["format"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("framework")));
    }

    #[test]
    fn test_risk_level() {
        let tool = ComplianceReportTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_with_framework() {
        let tool = ComplianceReportTool;
        let result = tool
            .execute(serde_json::json!({
                "framework": "soc2"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("soc2") || result.content.contains("SOC 2"));
        assert!(result.content.contains("Compliance"));
        assert!(result.content.contains("markdown") || result.content.contains("Summary"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let tool = ComplianceReportTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_format() {
        let tool = ComplianceReportTool;
        let result = tool
            .execute(serde_json::json!({
                "framework": "iso27001",
                "format": "json"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("iso27001"));
        // JSON format should have JSON structure markers
        assert!(result.content.contains('{'));
    }

    #[tokio::test]
    async fn test_execute_unknown_framework() {
        let tool = ComplianceReportTool;
        let result = tool
            .execute(serde_json::json!({
                "framework": "nonexistent"
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_scanners_and_findings() {
        let tool = ComplianceReportTool;
        let result = tool
            .execute(serde_json::json!({
                "framework": "owasp-asvs",
                "scanners_run": ["sast", "secrets", "sca"],
                "finding_cwes": ["CWE-89"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Non-Compliant"));
        assert!(result.content.contains("CWE-89"));
    }
}
