//! HomeKit tool — control smart home accessories via macOS Shortcuts.
//!
//! Uses the `shortcuts` CLI to list and run HomeKit-related shortcuts.
//! Requires macOS 12+ with Shortcuts app configured.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::process::Command;

use crate::registry::Tool;

/// Tool for HomeKit smart home control via macOS Shortcuts.
pub struct HomeKitTool;

impl Default for HomeKitTool {
    fn default() -> Self {
        Self
    }
}

impl HomeKitTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for HomeKitTool {
    fn name(&self) -> &str {
        "homekit"
    }

    fn description(&self) -> &str {
        "Control HomeKit smart home accessories via macOS Shortcuts. Actions: list_shortcuts, run_shortcut, run_with_input"
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list_shortcuts", "run_shortcut", "run_with_input"],
                    "description": "The action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Name of the shortcut to run"
                },
                "input": {
                    "type": "string",
                    "description": "Input to pass to the shortcut (for run_with_input)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "homekit".to_string(),
                reason: "Missing 'action' parameter".to_string(),
            })?;

        match action {
            "list_shortcuts" => {
                let output = Command::new("shortcuts")
                    .arg("list")
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "homekit".to_string(),
                        message: format!("Failed to list shortcuts: {}", e),
                    })?;

                let stdout = String::from_utf8_lossy(&output.stdout);
                // Filter to HomeKit-related shortcuts (heuristic: name contains "home", "light", "scene", etc.)
                let homekit_keywords = [
                    "home",
                    "light",
                    "scene",
                    "lock",
                    "thermostat",
                    "fan",
                    "blind",
                    "curtain",
                    "door",
                    "garage",
                    "climate",
                    "switch",
                    "plug",
                ];
                let relevant: Vec<&str> = stdout
                    .lines()
                    .filter(|line| {
                        let lower = line.to_lowercase();
                        homekit_keywords.iter().any(|kw| lower.contains(kw))
                    })
                    .collect();

                if relevant.is_empty() {
                    Ok(ToolOutput::text(format!(
                        "No HomeKit-related shortcuts found. All available shortcuts:\n{}",
                        stdout.lines().take(20).collect::<Vec<_>>().join("\n")
                    )))
                } else {
                    Ok(ToolOutput::text(format!(
                        "HomeKit shortcuts ({}):\n{}",
                        relevant.len(),
                        relevant.join("\n")
                    )))
                }
            }
            "run_shortcut" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "homekit".to_string(),
                        reason: "Missing 'name' parameter for run_shortcut".to_string(),
                    })?;

                let output = Command::new("shortcuts")
                    .args(["run", name])
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "homekit".to_string(),
                        message: format!("Failed to run shortcut '{}': {}", name, e),
                    })?;

                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.is_empty() {
                        Ok(ToolOutput::text(format!(
                            "Shortcut '{}' executed successfully.",
                            name
                        )))
                    } else {
                        Ok(ToolOutput::text(format!(
                            "Shortcut '{}' output:\n{}",
                            name, stdout
                        )))
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(ToolError::ExecutionFailed {
                        name: "homekit".to_string(),
                        message: format!("Shortcut '{}' failed: {}", name, stderr),
                    })
                }
            }
            "run_with_input" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "homekit".to_string(),
                        reason: "Missing 'name' parameter for run_with_input".to_string(),
                    })?;
                let input = args["input"]
                    .as_str()
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "homekit".to_string(),
                        reason: "Missing 'input' parameter for run_with_input".to_string(),
                    })?;

                let output = Command::new("shortcuts")
                    .args(["run", name, "--input-type", "text", "--input", input])
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "homekit".to_string(),
                        message: format!("Failed to run shortcut '{}' with input: {}", name, e),
                    })?;

                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if stdout.is_empty() {
                        Ok(ToolOutput::text(format!(
                            "Shortcut '{}' executed with input '{}'.",
                            name, input
                        )))
                    } else {
                        Ok(ToolOutput::text(format!(
                            "Shortcut '{}' output:\n{}",
                            name, stdout
                        )))
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(ToolError::ExecutionFailed {
                        name: "homekit".to_string(),
                        message: format!("Shortcut '{}' failed: {}", name, stderr),
                    })
                }
            }
            _ => Err(ToolError::InvalidArguments {
                name: "homekit".to_string(),
                reason: format!("Unknown action: {}", action),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_homekit_schema() {
        let tool = HomeKitTool::new();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"].is_object());
        assert!(schema["properties"]["name"].is_object());
        assert!(schema["properties"]["input"].is_object());
    }

    #[test]
    fn test_homekit_name() {
        let tool = HomeKitTool::new();
        assert_eq!(tool.name(), "homekit");
    }

    #[tokio::test]
    async fn test_homekit_invalid_action() {
        let tool = HomeKitTool::new();
        let result = tool.execute(json!({"action": "invalid"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_homekit_missing_name() {
        let tool = HomeKitTool::new();
        let result = tool.execute(json!({"action": "run_shortcut"})).await;
        assert!(result.is_err());
    }
}
