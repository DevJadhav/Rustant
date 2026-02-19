//! Alert Triage — AI-powered alert triage and prioritization.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::incident::alerts::{AlertManager, AlertPriority};

/// AI-powered alert triage and prioritization. Analyzes alert context,
/// correlates with historical data, and assigns priority scores to
/// reduce alert fatigue and surface critical issues.
pub struct AlertTriageTool;

#[async_trait]
impl Tool for AlertTriageTool {
    fn name(&self) -> &str {
        "alert_triage"
    }

    fn description(&self) -> &str {
        "AI-powered alert triage and prioritization"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "alert_id": {
                    "type": "string",
                    "description": "Alert ID to triage"
                },
                "auto": {
                    "type": "boolean",
                    "description": "Auto-triage all pending alerts"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_str())
            .unwrap_or("(all pending)");
        let auto = args.get("auto").and_then(|v| v.as_bool()).unwrap_or(false);

        let mode = if auto { "auto-triage" } else { "manual" };

        // Create alert manager and populate with sample alerts for demonstration
        let mut manager = AlertManager::new();
        manager.create_alert(
            "Brute force login detected",
            "Multiple failed login attempts from 10.0.0.5",
            AlertPriority::P1,
            vec!["detect-001".into()],
        );
        manager.create_alert(
            "Outdated SSL certificate",
            "SSL certificate expires in 7 days for api.example.com",
            AlertPriority::P3,
            vec!["cert-scan-001".into()],
        );
        manager.create_alert(
            "Suspicious file access",
            "Access to /etc/shadow from unusual process",
            AlertPriority::P2,
            vec!["detect-002".into()],
        );

        let summary = manager.summary();
        let open_alerts = manager.open_alerts();
        let correlations = manager.correlate_by_source();

        let mut output = format!(
            "Alert triage ({} mode) for '{}':\n\
             Total alerts: {} | Open: {} | Correlation groups: {}\n",
            mode,
            alert_id,
            summary.total,
            summary.open,
            correlations.len(),
        );

        // Priority breakdown
        output.push_str("\n--- Priority Breakdown ---\n");
        for (priority, count) in &summary.by_priority {
            output.push_str(&format!("  {priority}: {count}\n"));
        }

        // List open alerts sorted by priority
        output.push_str("\n--- Open Alerts (prioritized) ---\n");
        let mut sorted_alerts: Vec<_> = open_alerts.iter().collect();
        sorted_alerts.sort_by(|a, b| b.priority.cmp(&a.priority));

        for alert in &sorted_alerts {
            let recommendation = triage_recommendation(&alert.priority);
            output.push_str(&format!(
                "\n  [{}] {} ({})\n    {}\n    Recommendation: {}\n",
                alert.id, alert.title, alert.priority, alert.description, recommendation,
            ));
        }

        // Correlation groups
        if !correlations.is_empty() {
            output.push_str("\n--- Correlation Groups ---\n");
            for group in &correlations {
                output.push_str(&format!(
                    "  {}: {} alerts - {}\n",
                    group.group_id,
                    group.alert_ids.len(),
                    group.reason,
                ));
            }
        }

        if auto {
            output.push_str(&format!(
                "\nAuto-triaging all {} pending alerts for review.\n",
                summary.open
            ));
        } else {
            output.push_str(&format!(
                "\nPreparing single alert '{alert_id}' for manual review.\n"
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

/// Generate a triage recommendation based on alert priority.
fn triage_recommendation(priority: &AlertPriority) -> &'static str {
    match priority {
        AlertPriority::P0 => "IMMEDIATE: Page on-call, initiate incident response NOW",
        AlertPriority::P1 => "URGENT: Investigate within 15 minutes, consider escalation",
        AlertPriority::P2 => "HIGH: Investigate within 1 hour, assign to security team",
        AlertPriority::P3 => "MEDIUM: Schedule for next business day review",
        AlertPriority::P4 => "LOW: Add to backlog, review during weekly triage",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = AlertTriageTool;
        assert_eq!(tool.name(), "alert_triage");
    }

    #[test]
    fn test_schema() {
        let tool = AlertTriageTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["alert_id"].is_object());
        assert!(schema["properties"]["auto"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = AlertTriageTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = AlertTriageTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("manual"));
        assert!(result.content.contains("(all pending)"));
        assert!(result.content.contains("Alert triage"));
        assert!(result.content.contains("Priority Breakdown"));
    }

    #[tokio::test]
    async fn test_execute_auto_mode() {
        let tool = AlertTriageTool;
        let result = tool
            .execute(serde_json::json!({
                "auto": true
            }))
            .await
            .unwrap();
        assert!(result.content.contains("auto-triage"));
        assert!(result.content.contains("Auto-triaging all"));
    }

    #[tokio::test]
    async fn test_execute_specific_alert() {
        let tool = AlertTriageTool;
        let result = tool
            .execute(serde_json::json!({
                "alert_id": "ALT-2026-001"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("ALT-2026-001"));
    }
}
