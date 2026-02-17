//! HTTP API tool — make HTTP requests (GET, POST, PUT, DELETE).

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::time::Duration;

use crate::registry::Tool;

pub struct HttpApiTool;

impl Default for HttpApiTool {
    fn default() -> Self {
        Self
    }
}

impl HttpApiTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for HttpApiTool {
    fn name(&self) -> &str {
        "http_api"
    }
    fn description(&self) -> &str {
        "Make HTTP API requests. Actions: get, post, put, delete. Returns status code and response body."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "post", "put", "delete"],
                    "description": "HTTP method"
                },
                "url": { "type": "string", "description": "Request URL" },
                "body": { "type": "string", "description": "Request body (JSON string for post/put)" },
                "headers": {
                    "type": "object",
                    "description": "Custom headers as key-value pairs",
                    "additionalProperties": { "type": "string" }
                }
            },
            "required": ["action", "url"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("get");
        let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        if url.is_empty() {
            return Ok(ToolOutput::text("Please provide a URL."));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .build()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "http_api".into(),
                message: format!("Failed to create HTTP client: {}", e),
            })?;

        let mut builder = match action {
            "get" => client.get(url),
            "post" => client.post(url),
            "put" => client.put(url),
            "delete" => client.delete(url),
            _ => return Ok(ToolOutput::text(format!("Unknown method: {}", action))),
        };

        // Add custom headers
        if let Some(headers) = args.get("headers").and_then(|v| v.as_object()) {
            for (key, value) in headers {
                if let Some(val_str) = value.as_str() {
                    builder = builder.header(key.as_str(), val_str);
                }
            }
        }

        // Add body for post/put
        if let Some(body) = args.get("body").and_then(|v| v.as_str()) {
            builder = builder
                .header("Content-Type", "application/json")
                .body(body.to_string());
        }

        let response = builder
            .send()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "http_api".into(),
                message: format!("HTTP request failed: {}", e),
            })?;

        let status = response.status();
        let headers_str: Vec<String> = response
            .headers()
            .iter()
            .take(10)
            .map(|(k, v)| format!("  {}: {}", k, v.to_str().unwrap_or("?")))
            .collect();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "<binary>".to_string());

        // Truncate large responses
        let body_display = if body.len() > 5000 {
            format!(
                "{}...\n(truncated, {} bytes total)",
                &body[..5000],
                body.len()
            )
        } else {
            body
        };

        Ok(ToolOutput::text(format!(
            "HTTP {} {} → {}\nHeaders:\n{}\nBody:\n{}",
            action.to_uppercase(),
            url,
            status,
            headers_str.join("\n"),
            body_display
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_api_missing_url() {
        let tool = HttpApiTool::new();
        let result = tool
            .execute(json!({"action": "get", "url": ""}))
            .await
            .unwrap();
        assert!(result.content.contains("provide a URL"));
    }

    #[tokio::test]
    async fn test_http_api_schema() {
        let tool = HttpApiTool::new();
        assert_eq!(tool.name(), "http_api");
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"]["enum"].is_array());
    }

    #[tokio::test]
    async fn test_http_api_invalid_url() {
        let tool = HttpApiTool::new();
        let result = tool
            .execute(json!({"action": "get", "url": "not-a-url"}))
            .await;
        // Should return error or error message
        assert!(result.is_err() || result.unwrap().content.contains("failed"));
    }
}
