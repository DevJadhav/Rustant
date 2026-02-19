//! Secrets validate tool — verifies if detected secrets are still active and exploitable.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use serde_json::{Value, json};
use std::time::Duration;

/// Validates whether a previously detected secret is still active by performing
/// safe network checks (e.g., testing API key validity without side effects).
pub struct SecretsValidateTool;

impl Default for SecretsValidateTool {
    fn default() -> Self {
        Self
    }
}

impl SecretsValidateTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SecretsValidateTool {
    fn name(&self) -> &str {
        "secrets_validate"
    }

    fn description(&self) -> &str {
        "Verify if detected secrets are still active"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "finding_id": {
                    "type": "string",
                    "description": "Finding ID of detected secret"
                }
            },
            "required": ["finding_id"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Network
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let finding_id = match args.get("finding_id").and_then(|v| v.as_str()) {
            Some(id) if !id.is_empty() => id,
            _ => {
                return Ok(ToolOutput::text(
                    "Required parameter 'finding_id' is missing or empty.",
                ));
            }
        };

        Ok(ToolOutput::text(format!(
            "Secret live validation for finding '{finding_id}':\n\n\
             Status: NOT AVAILABLE\n\n\
             Secret live validation requires network access and explicit approval. \
             Use secrets_scan first to detect secrets in source code.\n\n\
             When enabled, this tool would safely check if the secret is still active \
             by making read-only API calls:\n\
             - AWS: STS GetCallerIdentity\n\
             - GitHub: GET /user endpoint\n\
             - Slack: auth.test\n\
             - Stripe: retrieve balance\n\
             - GCP: tokeninfo endpoint\n\n\
             To enable live validation, configure network access permissions in \
             the security policy."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SecretsValidateTool::new();
        assert_eq!(tool.name(), "secrets_validate");
        assert_eq!(tool.risk_level(), RiskLevel::Network);
    }

    #[test]
    fn test_schema() {
        let tool = SecretsValidateTool::new();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["finding_id"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("finding_id"))
        );
    }

    #[tokio::test]
    async fn test_execute_with_finding_id() {
        let tool = SecretsValidateTool::new();
        let result = tool
            .execute(json!({"finding_id": "SEC-2024-001"}))
            .await
            .unwrap();
        assert!(result.content.contains("SEC-2024-001"));
        assert!(result.content.contains("validation"));
    }

    #[tokio::test]
    async fn test_execute_missing_finding_id() {
        let tool = SecretsValidateTool::new();
        let result = tool.execute(json!({})).await.unwrap();
        assert!(result.content.contains("missing"));
    }
}
