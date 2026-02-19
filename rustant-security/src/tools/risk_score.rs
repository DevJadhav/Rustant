//! Risk Score — Calculate multi-dimensional security risk score.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::compliance::risk::{
    BusinessContext, DeploymentFrequency, RiskCalculator, RiskInput, UserBaseSize,
};
use crate::dep_graph::DependencyGraph;

/// Calculate a multi-dimensional security risk score for a project.
/// Evaluates vulnerability density, dependency risk, code complexity,
/// and business context to produce an overall risk assessment.
pub struct RiskScoreTool;

#[async_trait]
impl Tool for RiskScoreTool {
    fn name(&self) -> &str {
        "risk_score"
    }

    fn description(&self) -> &str {
        "Calculate multi-dimensional security risk score"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to project"
                },
                "context": {
                    "type": "string",
                    "description": "Business context (internal/public/critical)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let context_str = args
            .get("context")
            .and_then(|v| v.as_str())
            .unwrap_or("internal");

        let workspace = Path::new(path);

        // Build risk input from workspace analysis
        let mut risk_input = RiskInput::default();

        // Load findings if available
        let findings_path = workspace
            .join(".rustant")
            .join("security")
            .join("findings.json");
        if let Ok(content) = std::fs::read_to_string(&findings_path)
            && let Ok(findings) = serde_json::from_str::<Vec<crate::finding::Finding>>(&content)
        {
            for f in &findings {
                match f.severity {
                    crate::finding::FindingSeverity::Critical => risk_input.critical_findings += 1,
                    crate::finding::FindingSeverity::High => risk_input.high_findings += 1,
                    crate::finding::FindingSeverity::Medium => risk_input.medium_findings += 1,
                    crate::finding::FindingSeverity::Low => risk_input.low_findings += 1,
                    crate::finding::FindingSeverity::Info => {}
                }
            }
        }

        // Analyze dependency health
        if let Ok(dep_graph) = DependencyGraph::build(workspace) {
            risk_input.total_deps = dep_graph.package_count();
            // Count deps without licenses as potential compliance issues
            for dep in dep_graph.all_packages() {
                if dep.license.is_none() {
                    risk_input.license_violations += 1;
                }
            }
        }

        // Build business context from parameter
        let business_context = match context_str {
            "critical" => BusinessContext {
                internet_exposed: true,
                data_sensitivity: 5,
                deployment_frequency: DeploymentFrequency::Daily,
                user_base: UserBaseSize::Enterprise,
            },
            "public" => BusinessContext {
                internet_exposed: true,
                data_sensitivity: 3,
                deployment_frequency: DeploymentFrequency::Weekly,
                user_base: UserBaseSize::Large,
            },
            _ => BusinessContext::default(), // internal
        };

        // Calculate risk
        let calculator = RiskCalculator::default();
        let assessment = calculator.calculate(&risk_input, &business_context);

        // Format output
        let mut output = format!("Risk score for '{path}' (context: {context_str}):\n\n");

        output.push_str(&format!(
            "Overall risk score: {:.1}/100 ({})\n",
            assessment.score, assessment.level
        ));
        output.push_str(&format!("Trend: {}\n", assessment.trend));
        output.push_str(&format!(
            "Context multiplier: {:.2}x\n\n",
            assessment.context_multiplier
        ));

        // Dimension breakdown
        output.push_str("Dimension breakdown:\n");
        output.push_str(&format!(
            "  Security:   {:.1}/100 (weight: {:.0}%)\n",
            assessment.dimensions.security,
            calculator.security_weight * 100.0,
        ));
        output.push_str(&format!(
            "  Quality:    {:.1}/100 (weight: {:.0}%)\n",
            assessment.dimensions.quality,
            calculator.quality_weight * 100.0,
        ));
        output.push_str(&format!(
            "  Dependency: {:.1}/100 (weight: {:.0}%)\n",
            assessment.dimensions.dependency,
            calculator.dependency_weight * 100.0,
        ));
        output.push_str(&format!(
            "  Compliance: {:.1}/100 (weight: {:.0}%)\n\n",
            assessment.dimensions.compliance,
            calculator.compliance_weight * 100.0,
        ));

        // Input data summary
        output.push_str("Input data:\n");
        output.push_str(&format!(
            "  Findings: {} critical, {} high, {} medium, {} low\n",
            risk_input.critical_findings,
            risk_input.high_findings,
            risk_input.medium_findings,
            risk_input.low_findings,
        ));
        output.push_str(&format!(
            "  Dependencies: {} total, {} license issues\n",
            risk_input.total_deps, risk_input.license_violations,
        ));
        output.push('\n');

        // Top risk factors
        if !assessment.top_factors.is_empty() {
            output.push_str("Top risk factors:\n");
            for factor in &assessment.top_factors {
                output.push_str(&format!(
                    "  - {} (impact: {:.1}, dimension: {})\n",
                    factor.description, factor.impact, factor.dimension,
                ));
                if let Some(ref mitigation) = factor.mitigation {
                    output.push_str(&format!("    Mitigation: {mitigation}\n"));
                }
            }
        } else {
            output.push_str("No significant risk factors identified.\n");
        }

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
        let tool = RiskScoreTool;
        assert_eq!(tool.name(), "risk_score");
    }

    #[test]
    fn test_schema() {
        let tool = RiskScoreTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["context"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = RiskScoreTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = RiskScoreTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("internal"));
        assert!(result.content.contains("Risk score"));
        assert!(result.content.contains("Dimension breakdown"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = RiskScoreTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "/critical/service",
                "context": "critical"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("/critical/service"));
        assert!(result.content.contains("critical"));
    }
}
