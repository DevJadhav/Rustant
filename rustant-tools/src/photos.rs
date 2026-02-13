//! macOS Photos tool — search and list photos via AppleScript.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{json, Value};
use std::time::Duration;

use crate::registry::Tool;

pub struct MacosPhotosTool;

impl Default for MacosPhotosTool {
    fn default() -> Self {
        Self
    }
}

impl MacosPhotosTool {
    pub fn new() -> Self {
        Self
    }

    fn run_osascript(script: &str) -> Result<String, ToolError> {
        let output = std::process::Command::new("osascript")
            .args(["-e", script])
            .output()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "macos_photos".into(),
                message: format!("Failed to run osascript: {}", e),
            })?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(format!("Error: {}", stderr.trim()))
        }
    }
}

#[async_trait]
impl Tool for MacosPhotosTool {
    fn name(&self) -> &str {
        "macos_photos"
    }
    fn description(&self) -> &str {
        "Search and browse macOS Photos.app. Actions: search, list_albums, recent."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "list_albums", "recent"],
                    "description": "Action to perform"
                },
                "query": { "type": "string", "description": "Search query for photos" },
                "limit": { "type": "integer", "description": "Max results (default: 20)", "default": 20 }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20);

        match action {
            "list_albums" => {
                let script = format!(
                    r#"tell application "Photos"
    set albumNames to {{}}
    set albumList to albums
    repeat with a in albumList
        if (count of albumNames) >= {} then exit repeat
        set end of albumNames to (name of a) & " (" & (count of media items of a) & " items)"
    end repeat
    return albumNames as text
end tell"#,
                    limit
                );
                let result = Self::run_osascript(&script)?;
                Ok(ToolOutput::text(format!("Photo albums:\n{}", result)))
            }
            "recent" => {
                let script = format!(
                    r#"tell application "Photos"
    set recentItems to {{}}
    set allItems to media items
    set itemCount to count of allItems
    set startIdx to (itemCount - {} + 1)
    if startIdx < 1 then set startIdx to 1
    repeat with i from startIdx to itemCount
        set item_ to item i of allItems
        set end of recentItems to (filename of item_) & " — " & (date of item_ as text)
    end repeat
    return recentItems as text
end tell"#,
                    limit
                );
                let result = Self::run_osascript(&script)?;
                Ok(ToolOutput::text(format!("Recent photos:\n{}", result)))
            }
            "search" => {
                let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
                if query.is_empty() {
                    return Ok(ToolOutput::text("Please provide a search query."));
                }
                let safe_query = crate::macos::sanitize_applescript_string(query);
                let script = format!(
                    r#"tell application "Photos"
    set results to {{}}
    set searchResults to (search for "{}")
    repeat with item_ in searchResults
        if (count of results) >= {} then exit repeat
        set end of results to (filename of item_) & " — " & (date of item_ as text)
    end repeat
    if (count of results) = 0 then return "No photos found."
    return results as text
end tell"#,
                    safe_query, limit
                );
                let result = Self::run_osascript(&script)?;
                Ok(ToolOutput::text(format!(
                    "Search results for '{}':\n{}",
                    query, result
                )))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: search, list_albums, recent",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_photos_schema() {
        let tool = MacosPhotosTool::new();
        assert_eq!(tool.name(), "macos_photos");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["action"]["enum"].is_array());
    }

    #[tokio::test]
    async fn test_photos_search_empty() {
        let tool = MacosPhotosTool::new();
        let result = tool
            .execute(json!({"action": "search", "query": ""}))
            .await
            .unwrap();
        assert!(result.content.contains("provide a search"));
    }
}
