//! Voice synthesis tool — text-to-speech via macOS `say` command.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::time::Duration;

use crate::registry::Tool;

pub struct MacosSayTool;

impl Default for MacosSayTool {
    fn default() -> Self {
        Self
    }
}

impl MacosSayTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MacosSayTool {
    fn name(&self) -> &str {
        "macos_say"
    }
    fn description(&self) -> &str {
        "Text-to-speech via macOS say command. Actions: speak, list_voices."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["speak", "list_voices"],
                    "description": "Action to perform"
                },
                "text": { "type": "string", "description": "Text to speak" },
                "voice": { "type": "string", "description": "Voice name (e.g., 'Samantha', 'Alex')" },
                "rate": { "type": "integer", "description": "Speaking rate (words per minute, default: 175)" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "speak" => {
                let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
                if text.is_empty() {
                    return Ok(ToolOutput::text("Please provide text to speak."));
                }
                // Sanitize text for shell safety
                let safe_text = text.replace('\'', "'\\''");

                let mut cmd_args = vec![];
                if let Some(voice) = args.get("voice").and_then(|v| v.as_str()) {
                    let safe_voice = voice.replace('\'', "'\\''");
                    cmd_args.push("-v".to_string());
                    cmd_args.push(safe_voice);
                }
                if let Some(rate) = args.get("rate").and_then(|v| v.as_u64()) {
                    cmd_args.push("-r".to_string());
                    cmd_args.push(rate.to_string());
                }
                cmd_args.push(safe_text);

                let output = std::process::Command::new("say")
                    .args(&cmd_args)
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_say".into(),
                        message: format!("Failed to run say: {}", e),
                    })?;

                if output.status.success() {
                    Ok(ToolOutput::text(format!(
                        "Spoke: \"{}\"",
                        &text[..text.len().min(50)]
                    )))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Ok(ToolOutput::text(format!("Say command failed: {}", stderr)))
                }
            }
            "list_voices" => {
                let output = std::process::Command::new("say")
                    .args(["-v", "?"])
                    .output()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_say".into(),
                        message: format!("Failed to list voices: {}", e),
                    })?;

                let text = String::from_utf8_lossy(&output.stdout);
                let voices: Vec<&str> = text.lines().take(30).collect();
                Ok(ToolOutput::text(format!(
                    "Available voices ({} shown):\n{}",
                    voices.len(),
                    voices.join("\n")
                )))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: speak, list_voices",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_voice_tool_schema() {
        let tool = MacosSayTool::new();
        assert_eq!(tool.name(), "macos_say");
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"]["enum"].is_array());
    }

    #[tokio::test]
    async fn test_voice_tool_empty_text() {
        let tool = MacosSayTool::new();
        let result = tool
            .execute(json!({"action": "speak", "text": ""}))
            .await
            .unwrap();
        assert!(result.content.contains("provide text"));
    }
}
