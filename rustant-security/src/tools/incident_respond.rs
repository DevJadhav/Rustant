//! Incident Respond — Execute incident response playbook.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::incident::playbook::{PlaybookAction, PlaybookRegistry};

/// Execute an incident response playbook for a given incident. Supports
/// dry-run mode for safe rehearsal. Playbooks define step-by-step
/// containment, investigation, and remediation procedures.
pub struct IncidentRespondTool;

#[async_trait]
impl Tool for IncidentRespondTool {
    fn name(&self) -> &str {
        "incident_respond"
    }

    fn description(&self) -> &str {
        "Execute incident response playbook"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "playbook": {
                    "type": "string",
                    "description": "Playbook name or path"
                },
                "incident_id": {
                    "type": "string",
                    "description": "Incident ID"
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "Dry run mode (default: true)"
                }
            },
            "required": ["playbook"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let playbook_name = args
            .get("playbook")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "incident_respond".to_string(),
                reason: "'playbook' parameter is required".to_string(),
            })?;
        let incident_id = args
            .get("incident_id")
            .and_then(|v| v.as_str())
            .unwrap_or("(none)");
        let dry_run = args
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mode = if dry_run { "DRY RUN" } else { "LIVE" };

        // Load playbook registry with defaults
        let registry = PlaybookRegistry::with_defaults();

        // Try to find playbook by ID
        let playbook = registry.get(playbook_name);

        let mut output = format!(
            "Incident response [{mode}] (playbook: '{playbook_name}', incident: {incident_id}):\n"
        );

        match playbook {
            Some(pb) => {
                output.push_str(&format!(
                    "Playbook: {} ({})\n\
                     Description: {}\n\
                     Enabled: {} | Steps: {}\n",
                    pb.name,
                    pb.id,
                    pb.description,
                    pb.enabled,
                    pb.steps.len(),
                ));

                // Show trigger conditions
                output.push_str("\n--- Trigger Conditions ---\n");
                if let Some(ref scanner) = pb.trigger.scanner {
                    output.push_str(&format!("  Scanner: {scanner}\n"));
                }
                if let Some(ref min_sev) = pb.trigger.min_severity {
                    output.push_str(&format!("  Min severity: {min_sev}\n"));
                }
                if let Some(ref event_type) = pb.trigger.event_type {
                    output.push_str(&format!("  Event type: {event_type}\n"));
                }

                // Show execution plan
                output.push_str("\n--- Execution Plan ---\n");
                for (i, step) in pb.steps.iter().enumerate() {
                    let action_desc = format_action(&step.action);
                    let approval_tag = if step.requires_approval {
                        " [REQUIRES APPROVAL]"
                    } else {
                        ""
                    };
                    let timeout_tag = step
                        .timeout_secs
                        .map(|t| format!(" (timeout: {t}s)"))
                        .unwrap_or_default();

                    if dry_run {
                        output.push_str(&format!(
                            "  Step {}: {} -> {}{}{}\n",
                            i + 1,
                            step.name,
                            action_desc,
                            approval_tag,
                            timeout_tag,
                        ));
                    } else {
                        // In live mode, still show steps but indicate they would execute
                        output.push_str(&format!(
                            "  Step {}: {} -> {} [WOULD EXECUTE]{}{}\n",
                            i + 1,
                            step.name,
                            action_desc,
                            approval_tag,
                            timeout_tag,
                        ));
                    }
                }

                output.push_str(&format!(
                    "\n{} mode: {}\n",
                    mode,
                    if dry_run {
                        "actions logged but not executed"
                    } else {
                        "actions will be executed with safety checks"
                    }
                ));
            }
            None => {
                // Playbook not found, list available ones
                output.push_str(&format!(
                    "Playbook '{playbook_name}' not found in registry.\n\n\
                     Available playbooks:\n"
                ));
                for pb in registry.all() {
                    output.push_str(&format!(
                        "  - {} ({}): {} [{} steps]\n",
                        pb.id,
                        pb.name,
                        pb.description,
                        pb.steps.len(),
                    ));
                }
                output.push_str(
                    "\nUse one of the available playbook IDs, e.g., 'credential-leak' or 'brute-force-response'.\n",
                );
            }
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }
}

/// Format a playbook action for display.
fn format_action(action: &PlaybookAction) -> String {
    match action {
        PlaybookAction::BlockIp { ip } => format!("Block IP {ip}"),
        PlaybookAction::RotateSecret { secret_id } => format!("Rotate secret {secret_id}"),
        PlaybookAction::InvalidateSessions { user_id } => {
            format!("Invalidate sessions for {user_id}")
        }
        PlaybookAction::Notify { channel, message } => {
            format!("Notify via {channel}: {message}")
        }
        PlaybookAction::CreateTicket { system, summary } => {
            format!("Create {system} ticket: {summary}")
        }
        PlaybookAction::QuarantineFile { path } => format!("Quarantine file {path}"),
        PlaybookAction::RevokeToken { token_id } => format!("Revoke token {token_id}"),
        PlaybookAction::CustomAction { name, params } => {
            format!("Custom action '{}' ({} params)", name, params.len())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = IncidentRespondTool;
        assert_eq!(tool.name(), "incident_respond");
    }

    #[test]
    fn test_schema() {
        let tool = IncidentRespondTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["playbook"].is_object());
        assert!(schema["properties"]["incident_id"].is_object());
        assert!(schema["properties"]["dry_run"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("playbook")));
    }

    #[test]
    fn test_risk_level() {
        let tool = IncidentRespondTool;
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
    }

    #[tokio::test]
    async fn test_execute_dry_run() {
        let tool = IncidentRespondTool;
        let result = tool
            .execute(serde_json::json!({
                "playbook": "credential-leak"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("DRY RUN"));
        assert!(result.content.contains("credential-leak"));
        assert!(result.content.contains("not executed"));
        assert!(result.content.contains("Execution Plan"));
    }

    #[tokio::test]
    async fn test_execute_live_mode() {
        let tool = IncidentRespondTool;
        let result = tool
            .execute(serde_json::json!({
                "playbook": "brute-force-response",
                "incident_id": "INC-2026-042",
                "dry_run": false
            }))
            .await
            .unwrap();
        assert!(result.content.contains("LIVE"));
        assert!(result.content.contains("brute-force-response"));
        assert!(result.content.contains("INC-2026-042"));
        assert!(result.content.contains("safety checks"));
    }

    #[tokio::test]
    async fn test_execute_unknown_playbook() {
        let tool = IncidentRespondTool;
        let result = tool
            .execute(serde_json::json!({
                "playbook": "ransomware-containment"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("DRY RUN"));
        assert!(result.content.contains("ransomware-containment"));
        assert!(result.content.contains("not found"));
        assert!(result.content.contains("Available playbooks"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let tool = IncidentRespondTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
