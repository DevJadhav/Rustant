//! Log Analyze — Parse and analyze security-relevant logs.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::incident::detector::ThreatDetector;
use crate::incident::log_parsers::{LogFormat, LogParser};
use std::collections::HashMap;

/// Parse and analyze security-relevant logs. Supports auto-detection
/// of log formats (syslog, JSON, HTTP access logs) and extracts
/// security-relevant events, anomalies, and indicators of compromise.
pub struct LogAnalyzeTool;

#[async_trait]
impl Tool for LogAnalyzeTool {
    fn name(&self) -> &str {
        "log_analyze"
    }

    fn description(&self) -> &str {
        "Parse and analyze security-relevant logs"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to log file"
                },
                "format": {
                    "type": "string",
                    "description": "Log format (auto/syslog/json/http)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "log_analyze".to_string(),
                reason: "'path' parameter is required".to_string(),
            }
        })?;
        let format_hint = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("auto");

        // Try to read the log file
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "Log analysis for '{path}' (format: {format_hint}):\n\
                     Error: Could not read file: {e}\n\
                     Ensure the path exists and is readable."
                )));
            }
        };

        if content.trim().is_empty() {
            return Ok(ToolOutput::text(format!(
                "Log analysis for '{path}' (format: {format_hint}):\n\
                 File is empty. No events to analyze."
            )));
        }

        // Detect format
        let detected_format = LogParser::detect_format(&content);
        let format_display = match detected_format {
            LogFormat::Syslog => "Syslog",
            LogFormat::JsonStructured => "JSON",
            LogFormat::HttpAccess => "HTTP Access",
            LogFormat::CloudTrail => "CloudTrail",
            LogFormat::KubernetesAudit => "K8s Audit",
            LogFormat::KeyValue => "Key-Value",
        };

        // Parse events
        let events = LogParser::parse_batch(&content);
        let total_lines = content.lines().count();

        // Analyze event types
        let mut event_type_counts: HashMap<String, usize> = HashMap::new();
        let mut source_counts: HashMap<String, usize> = HashMap::new();
        for event in &events {
            *event_type_counts
                .entry(event.event_type.clone())
                .or_insert(0) += 1;
            *source_counts.entry(event.source.clone()).or_insert(0) += 1;
        }

        // Run threat detection on parsed events
        let mut detector = ThreatDetector::with_defaults();
        let detections = detector.analyze_batch(&events);

        // Identify security-relevant events
        let security_event_types = [
            "auth_failure",
            "error",
            "server_error",
            "client_error",
            "privilege_escalation",
            "destructive_action",
            "warning",
        ];
        let security_events: usize = events
            .iter()
            .filter(|e| security_event_types.contains(&e.event_type.as_str()))
            .count();

        // Format output
        let mut output = format!(
            "Log analysis for '{}' (format: {}):\n\
             Detected format: {} | Total lines: {} | Parsed events: {} | Security events: {}\n",
            path,
            format_hint,
            format_display,
            total_lines,
            events.len(),
            security_events,
        );

        // Event type breakdown
        output.push_str("\n--- Event Type Breakdown ---\n");
        let mut sorted_types: Vec<_> = event_type_counts.iter().collect();
        sorted_types.sort_by(|a, b| b.1.cmp(a.1));
        for (event_type, count) in &sorted_types {
            let marker = if security_event_types.contains(&event_type.as_str()) {
                " [!]"
            } else {
                ""
            };
            output.push_str(&format!("  {event_type}: {count}{marker}\n"));
        }

        // Source breakdown
        if source_counts.len() > 1 {
            output.push_str("\n--- Sources ---\n");
            let mut sorted_sources: Vec<_> = source_counts.iter().collect();
            sorted_sources.sort_by(|a, b| b.1.cmp(a.1));
            for (source, count) in sorted_sources.iter().take(10) {
                output.push_str(&format!("  {source}: {count}\n"));
            }
        }

        // Threat detections
        if !detections.is_empty() {
            output.push_str(&format!(
                "\n--- Threat Detections ({}) ---\n",
                detections.len()
            ));
            for detection in &detections {
                output.push_str(&format!(
                    "  [{}] {} - {} events",
                    detection.severity,
                    detection.rule_name,
                    detection.triggering_events.len(),
                ));
                if let Some(ref mitre) = detection.mitre_technique {
                    output.push_str(&format!(" (MITRE: {mitre})"));
                }
                output.push('\n');
            }
        }

        // Anomalies
        if security_events > 0 {
            let security_pct = (security_events as f64 / events.len() as f64) * 100.0;
            output.push_str(&format!(
                "\n--- Summary ---\n\
                 Security event ratio: {:.1}% ({}/{})\n",
                security_pct,
                security_events,
                events.len()
            ));
            if security_pct > 50.0 {
                output.push_str("WARNING: High proportion of security-relevant events detected.\n");
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
        let tool = LogAnalyzeTool;
        assert_eq!(tool.name(), "log_analyze");
    }

    #[test]
    fn test_schema() {
        let tool = LogAnalyzeTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["format"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("path")));
    }

    #[test]
    fn test_risk_level() {
        let tool = LogAnalyzeTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = LogAnalyzeTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "/var/log/syslog"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("/var/log/syslog"));
        assert!(result.content.contains("auto"));
        assert!(result.content.contains("Log analysis"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let tool = LogAnalyzeTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_format() {
        let tool = LogAnalyzeTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "access.log",
                "format": "http"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("access.log"));
        assert!(result.content.contains("http"));
    }

    #[tokio::test]
    async fn test_execute_with_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let log_path = dir.path().join("test.log");
        std::fs::write(
            &log_path,
            r#"{"event_type":"login_failed","source":"auth","user":"admin"}
{"event_type":"login_failed","source":"auth","user":"admin"}
{"event_type":"normal","source":"app","action":"read"}
"#,
        )
        .unwrap();

        let tool = LogAnalyzeTool;
        let result = tool
            .execute(serde_json::json!({
                "path": log_path.to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Parsed events: 3"));
        assert!(result.content.contains("Event Type Breakdown"));
    }
}
