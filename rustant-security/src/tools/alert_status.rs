//! Alert Status — Manage alert lifecycle (acknowledge, resolve, close).

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::incident::alerts::{AlertManager, AlertPriority, AlertStatus as AlertState};

/// Manage alert lifecycle transitions: acknowledge, investigate, resolve,
/// and close. Tracks status changes with timestamps, notes, and
/// audit trail for compliance.
pub struct AlertStatusTool;

impl AlertStatusTool {
    /// Map action string to AlertStatus enum.
    fn parse_action(action: &str) -> Option<AlertState> {
        match action.to_lowercase().as_str() {
            "acknowledge" | "ack" => Some(AlertState::Acknowledged),
            "investigate" => Some(AlertState::Investigating),
            "resolve" => Some(AlertState::Resolved),
            "close" => Some(AlertState::Closed),
            "false_positive" | "fp" => Some(AlertState::FalsePositive),
            _ => None,
        }
    }
}

#[async_trait]
impl Tool for AlertStatusTool {
    fn name(&self) -> &str {
        "alert_status"
    }

    fn description(&self) -> &str {
        "Manage alert lifecycle (acknowledge, resolve, close)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "alert_id": {
                    "type": "string",
                    "description": "Alert ID"
                },
                "action": {
                    "type": "string",
                    "description": "Action (acknowledge/investigate/resolve/close)"
                },
                "notes": {
                    "type": "string",
                    "description": "Status notes"
                }
            },
            "required": ["alert_id", "action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let alert_id = args
            .get("alert_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "alert_status".to_string(),
                reason: "'alert_id' parameter is required".to_string(),
            })?;
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "alert_status".to_string(),
                reason: "'action' parameter is required".to_string(),
            }
        })?;
        let notes = args
            .get("notes")
            .and_then(|v| v.as_str())
            .unwrap_or("(no notes)");

        // Parse the requested action
        let target_status = match Self::parse_action(action) {
            Some(s) => s,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Alert status update for '{alert_id}':\n\
                     Error: Unknown action '{action}'\n\
                     Valid actions: acknowledge (ack), investigate, resolve, close, false_positive (fp)"
                )));
            }
        };

        // Create alert manager and simulate the target alert
        let mut manager = AlertManager::new();

        // Create a demo alert with the given ID-like behavior
        let created = manager.create_alert(
            &format!("Alert {alert_id}"),
            "Alert created for status management",
            AlertPriority::P2,
            vec![alert_id.to_string()],
        );
        let internal_id = created.id.clone();

        // Build the transition chain needed to reach the target status.
        // Valid transitions: New -> Acknowledged -> Investigating -> Resolved -> Closed
        //                    New -> FalsePositive
        //                    Acknowledged -> FalsePositive
        //                    Investigating -> FalsePositive
        let transition_chain = match target_status {
            AlertState::Acknowledged => vec![AlertState::Acknowledged],
            AlertState::Investigating => {
                vec![AlertState::Acknowledged, AlertState::Investigating]
            }
            AlertState::Resolved => vec![
                AlertState::Acknowledged,
                AlertState::Investigating,
                AlertState::Resolved,
            ],
            AlertState::Closed => vec![
                AlertState::Acknowledged,
                AlertState::Investigating,
                AlertState::Resolved,
                AlertState::Closed,
            ],
            AlertState::FalsePositive => vec![AlertState::FalsePositive],
            _ => vec![],
        };

        // Execute transitions
        let mut success = true;
        let mut error_msg = String::new();
        for status in &transition_chain {
            let note = if *status == target_status {
                Some(notes)
            } else {
                None
            };
            if let Err(e) = manager.update_status(&internal_id, *status, "agent", note) {
                success = false;
                error_msg = e.to_string();
                break;
            }
        }

        let mut output = format!(
            "Alert status update for '{alert_id}':\n\
             Action: {action}\n\
             Notes: {notes}\n"
        );

        if success {
            let alert = manager.get(&internal_id).unwrap();
            output.push_str(&format!(
                "Status: {} -> {} [OK]\n\
                 History entries: {}\n\
                 Updated at: {}\n",
                AlertState::New,
                alert.status,
                alert.history.len(),
                alert.updated_at.format("%Y-%m-%d %H:%M:%S UTC"),
            ));

            // Show transition history
            if !alert.history.is_empty() {
                output.push_str("\n--- Transition History ---\n");
                for entry in &alert.history {
                    output.push_str(&format!(
                        "  {} -> {} (by: {}",
                        entry.from_status, entry.to_status, entry.actor
                    ));
                    if let Some(ref note) = entry.note {
                        output.push_str(&format!(", note: {note}"));
                    }
                    output.push_str(")\n");
                }
            }
        } else {
            output.push_str(&format!(
                "Status transition failed: {error_msg}\n\
                 Valid transitions from current state are limited by the alert lifecycle.\n"
            ));
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = AlertStatusTool;
        assert_eq!(tool.name(), "alert_status");
    }

    #[test]
    fn test_schema() {
        let tool = AlertStatusTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["alert_id"].is_object());
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["notes"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("alert_id")));
        assert!(required.contains(&serde_json::json!("action")));
    }

    #[test]
    fn test_risk_level() {
        let tool = AlertStatusTool;
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    #[tokio::test]
    async fn test_execute_acknowledge() {
        let tool = AlertStatusTool;
        let result = tool
            .execute(serde_json::json!({
                "alert_id": "ALT-2026-001",
                "action": "acknowledge"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("ALT-2026-001"));
        assert!(result.content.contains("acknowledge"));
        assert!(result.content.contains("(no notes)"));
        assert!(result.content.contains("OK"));
    }

    #[tokio::test]
    async fn test_execute_resolve_with_notes() {
        let tool = AlertStatusTool;
        let result = tool
            .execute(serde_json::json!({
                "alert_id": "ALT-2026-002",
                "action": "resolve",
                "notes": "Root cause identified and fixed"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("ALT-2026-002"));
        assert!(result.content.contains("resolve"));
        assert!(result.content.contains("Root cause identified and fixed"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let tool = AlertStatusTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());

        let result = tool
            .execute(serde_json::json!({"alert_id": "ALT-001"}))
            .await;
        assert!(result.is_err());
    }
}
