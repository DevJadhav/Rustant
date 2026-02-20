//! Siri integration tool for managing Siri shortcuts and voice control.
//!
//! Provides actions for installing shortcuts, listing available shortcuts,
//! checking Siri status, and running workflows via Siri.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;

use crate::registry::Tool;

/// Siri integration tool.
pub struct SiriIntegrationTool;

/// Get the Rustant state directory (~/.rustant).
fn get_rustant_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".rustant")
}

#[async_trait]
impl Tool for SiriIntegrationTool {
    fn name(&self) -> &str {
        "siri_integration"
    }

    fn description(&self) -> &str {
        "Manage Siri integration: install shortcuts, check status, control activation. Actions: install_shortcuts, list_shortcuts, status, activate, deactivate."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The Siri action to perform",
                    "enum": ["install_shortcuts", "list_shortcuts", "status", "activate", "deactivate"]
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: self.name().to_string(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        match action {
            "status" => {
                let base_dir = get_rustant_dir();
                let siri_active = base_dir.join("siri_active").exists();
                let daemon_pid = base_dir.join("daemon.pid");
                let daemon_running = daemon_pid.exists();

                let status = format!(
                    "Siri Integration Status:\n  Siri active: {}\n  Daemon running: {}\n  Base dir: {}",
                    siri_active,
                    daemon_running,
                    base_dir.display()
                );
                Ok(ToolOutput::text(status))
            }
            "install_shortcuts" => Ok(ToolOutput::text(
                "To install Siri shortcuts:\n\n\
                     1. Open the Shortcuts app on macOS\n\
                     2. Create the following shortcuts:\n\n\
                     - 'Activate Rustant': Run Shell Script -> `rustant daemon start --siri-mode`\n\
                     - 'Deactivate Rustant': Run Shell Script -> `rustant daemon stop --siri-mode`\n\
                     - 'Ask Rustant': Ask for Input -> Run Shell Script -> `rustant siri send \"$input\"`\n\n\
                     Or run `rustant siri setup` from the command line for guided installation.",
            )),
            "list_shortcuts" => {
                let shortcuts = vec![
                    ("Activate Rustant", "Hey Siri, activate Rustant"),
                    ("Deactivate Rustant", "Hey Siri, deactivate Rustant"),
                    ("Ask Rustant", "Hey Siri, ask Rustant [question]"),
                    (
                        "Rustant Calendar",
                        "Hey Siri, check my calendar with Rustant",
                    ),
                    ("Rustant Briefing", "Hey Siri, Rustant briefing"),
                    ("Rustant Security", "Hey Siri, Rustant security scan"),
                    ("Rustant Research", "Hey Siri, Rustant research [topic]"),
                    ("Rustant Status", "Hey Siri, Rustant status"),
                ];

                let mut output = "Available Siri Shortcuts:\n\n".to_string();
                for (name, trigger) in &shortcuts {
                    output.push_str(&format!("  {name}: \"{trigger}\"\n"));
                }
                Ok(ToolOutput::text(output))
            }
            "activate" => {
                let base_dir = get_rustant_dir();
                let flag_path = base_dir.join("siri_active");
                std::fs::create_dir_all(&base_dir).map_err(|e| ToolError::ExecutionFailed {
                    name: self.name().to_string(),
                    message: format!("Failed to create directory: {e}"),
                })?;
                std::fs::write(&flag_path, "1").map_err(|e| ToolError::ExecutionFailed {
                    name: self.name().to_string(),
                    message: format!("Failed to write flag: {e}"),
                })?;
                Ok(ToolOutput::text(
                    "Siri mode activated. Rustant will now respond to Siri shortcuts.",
                ))
            }
            "deactivate" => {
                let base_dir = get_rustant_dir();
                let flag_path = base_dir.join("siri_active");
                if flag_path.exists() {
                    std::fs::remove_file(&flag_path).map_err(|e| ToolError::ExecutionFailed {
                        name: self.name().to_string(),
                        message: format!("Failed to remove flag: {e}"),
                    })?;
                }
                Ok(ToolOutput::text("Siri mode deactivated."))
            }
            _ => Err(ToolError::InvalidArguments {
                name: self.name().to_string(),
                reason: format!(
                    "Unknown action: {action}. Use: install_shortcuts, list_shortcuts, status, activate, deactivate"
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;

    #[test]
    fn test_tool_metadata() {
        let tool = SiriIntegrationTool;
        assert_eq!(tool.name(), "siri_integration");
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
    }

    #[tokio::test]
    async fn test_list_shortcuts() {
        let tool = SiriIntegrationTool;
        let result = tool
            .execute(serde_json::json!({"action": "list_shortcuts"}))
            .await
            .unwrap();
        assert!(result.content.contains("Activate Rustant"));
    }

    #[tokio::test]
    async fn test_status() {
        let tool = SiriIntegrationTool;
        let result = tool
            .execute(serde_json::json!({"action": "status"}))
            .await
            .unwrap();
        assert!(result.content.contains("Siri Integration Status"));
    }
}
