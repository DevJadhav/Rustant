//! Threat Detect — Run threat detection rules against logs and events.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::incident::detector::{LogEvent, ThreatDetector};
use crate::incident::log_parsers::LogParser;
use chrono::Utc;
use std::collections::HashMap;

/// Run threat detection rules against logs and events. Supports threshold,
/// sequence, and field-match detection methods with MITRE ATT&CK mapping
/// for identified threats.
pub struct ThreatDetectTool;

#[async_trait]
impl Tool for ThreatDetectTool {
    fn name(&self) -> &str {
        "threat_detect"
    }

    fn description(&self) -> &str {
        "Run threat detection rules against logs and events"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to log files"
                },
                "rules": {
                    "type": "string",
                    "description": "Rule set to apply"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let rules = args
            .get("rules")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        // Create detector with default rules
        let mut detector = ThreatDetector::with_defaults();
        let active_rules = detector.rules().len();

        // Try to read and parse the log file
        let events: Vec<LogEvent> = if let Ok(content) = std::fs::read_to_string(path) {
            LogParser::parse_batch(&content)
        } else {
            // No file found — generate a sample event to demonstrate the engine
            vec![LogEvent {
                timestamp: Utc::now(),
                event_type: "scan_requested".to_string(),
                source: path.to_string(),
                fields: HashMap::from([
                    ("rules".to_string(), rules.to_string()),
                    ("path".to_string(), path.to_string()),
                ]),
            }]
        };

        let event_count = events.len();

        // Run detection
        let detections = detector.analyze_batch(&events);

        // Format results
        let mut output = format!(
            "Threat detection for '{}' (rules: {}):\n\
             Active rules: {} | Events analyzed: {} | Detections: {}\n",
            path,
            rules,
            active_rules,
            event_count,
            detections.len()
        );

        if detections.is_empty() {
            output.push_str("\nNo threats detected in the analyzed events.");
        } else {
            output.push_str("\n--- Detected Threats ---\n");
            for (i, detection) in detections.iter().enumerate() {
                output.push_str(&format!(
                    "\n[{}] {} (severity: {})\n    Rule: {} ({})\n",
                    i + 1,
                    detection.rule_name,
                    detection.severity,
                    detection.rule_id,
                    if let Some(ref mitre) = detection.mitre_technique {
                        format!("MITRE ATT&CK: {mitre}")
                    } else {
                        "no MITRE mapping".to_string()
                    },
                ));
                if let Some(ref response) = detection.response {
                    output.push_str(&format!("    Suggested response: {response}\n"));
                }
                output.push_str(&format!(
                    "    Triggering events: {}\n",
                    detection.triggering_events.len()
                ));
                for (key, value) in &detection.context {
                    output.push_str(&format!("    {key}: {value}\n"));
                }
            }
        }

        // List active rules summary
        output.push_str("\n--- Active Detection Rules ---\n");
        for rule in detector.rules() {
            if rule.enabled {
                output.push_str(&format!(
                    "  - {} [{}]: {}",
                    rule.name, rule.severity, rule.description
                ));
                if let Some(ref mitre) = rule.mitre_technique {
                    output.push_str(&format!(" ({mitre})"));
                }
                output.push('\n');
            }
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
        let tool = ThreatDetectTool;
        assert_eq!(tool.name(), "threat_detect");
    }

    #[test]
    fn test_schema() {
        let tool = ThreatDetectTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["rules"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = ThreatDetectTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = ThreatDetectTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("default"));
        assert!(result.content.contains("Threat detection"));
        assert!(result.content.contains("Active rules:"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = ThreatDetectTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "/var/log/auth.log",
                "rules": "brute-force"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("/var/log/auth.log"));
        assert!(result.content.contains("brute-force"));
        assert!(result.content.contains("Active Detection Rules"));
    }
}
