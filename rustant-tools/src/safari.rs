//! macOS Safari automation tool — control Safari browser via AppleScript.
//!
//! Provides navigation, tab management, text extraction, and JavaScript
//! execution in Safari for users who prefer it over Chromium-based browsers.
//! macOS only.

use crate::macos::{require_str, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

const TOOL_NAME: &str = "macos_safari";

pub struct MacosSafariTool;

#[async_trait]
impl Tool for MacosSafariTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Control Safari browser on macOS. Actions: navigate (open URL), get_url (current URL), \
         get_text (page text content), run_javascript (execute JS), list_tabs (all tabs), \
         new_tab (open new tab)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["navigate", "get_url", "get_text", "run_javascript", "list_tabs", "new_tab"],
                    "description": "Action to perform"
                },
                "url": {
                    "type": "string",
                    "description": "URL to navigate to (for navigate, new_tab)"
                },
                "script": {
                    "type": "string",
                    "description": "JavaScript code to execute (for run_javascript)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", TOOL_NAME)?;

        match action {
            "navigate" => execute_navigate(&args).await,
            "get_url" => execute_get_url().await,
            "get_text" => execute_get_text().await,
            "run_javascript" => execute_run_javascript(&args).await,
            "list_tabs" => execute_list_tabs().await,
            "new_tab" => execute_new_tab(&args).await,
            other => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid: navigate, get_url, get_text, \
                     run_javascript, list_tabs, new_tab"
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

async fn execute_navigate(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let url = sanitize_applescript_string(require_str(args, "url", TOOL_NAME)?);
    debug!(url = %url, "Navigating Safari");

    let script = format!(
        r#"
tell application "Safari"
    activate
    if (count of windows) is 0 then
        make new document
    end if
    set URL of document 1 to "{url}"
    delay 1
    return "Navigated to " & URL of document 1
end tell
"#
    );

    let result = run_osascript(&script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

async fn execute_get_url() -> Result<ToolOutput, ToolError> {
    debug!("Getting Safari current URL");

    let script = r#"
tell application "Safari"
    if (count of windows) is 0 then
        return "No Safari windows open."
    end if
    return URL of document 1
end tell
"#;

    let result = run_osascript(script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

async fn execute_get_text() -> Result<ToolOutput, ToolError> {
    debug!("Getting Safari page text");

    let script = r#"
tell application "Safari"
    if (count of windows) is 0 then
        return "No Safari windows open."
    end if
    set pageText to do JavaScript "document.body.innerText" in document 1
    return pageText
end tell
"#;

    let result = run_osascript(script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    // Truncate large pages
    let truncated = if result.len() > 5000 {
        format!(
            "{}...\n\n[Truncated — {} chars total]",
            &result[..5000],
            result.len()
        )
    } else {
        result
    };

    Ok(ToolOutput::text(truncated))
}

async fn execute_run_javascript(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let script_code = require_str(args, "script", TOOL_NAME)?;
    // Sanitize JS for AppleScript embedding
    let safe_script = sanitize_applescript_string(script_code);
    debug!(
        script_len = script_code.len(),
        "Running JavaScript in Safari"
    );

    let script = format!(
        r#"
tell application "Safari"
    if (count of windows) is 0 then
        return "No Safari windows open."
    end if
    set jsResult to do JavaScript "{safe_script}" in document 1
    if jsResult is missing value then
        return "undefined"
    end if
    return jsResult as string
end tell
"#
    );

    let result = run_osascript(&script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

async fn execute_list_tabs() -> Result<ToolOutput, ToolError> {
    debug!("Listing Safari tabs");

    let script = r#"
tell application "Safari"
    if (count of windows) is 0 then
        return "No Safari windows open."
    end if
    set output to ""
    set winNum to 1
    repeat with w in windows
        set output to output & "Window " & winNum & ":" & linefeed
        set tabNum to 1
        repeat with t in tabs of w
            set tabName to name of t
            set tabUrl to URL of t
            set activeMarker to ""
            if t is current tab of w then set activeMarker to " *"
            set output to output & "  " & tabNum & ". " & tabName & activeMarker & linefeed
            set output to output & "     " & tabUrl & linefeed
            set tabNum to tabNum + 1
        end repeat
        set winNum to winNum + 1
    end repeat
    return output
end tell
"#;

    let result = run_osascript(script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

async fn execute_new_tab(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let url = args["url"].as_str().unwrap_or("about:blank");
    let safe_url = sanitize_applescript_string(url);
    debug!(url = %safe_url, "Opening new Safari tab");

    let script = format!(
        r#"
tell application "Safari"
    activate
    if (count of windows) is 0 then
        make new document with properties {{URL:"{safe_url}"}}
    else
        tell window 1
            set newTab to make new tab with properties {{URL:"{safe_url}"}}
            set current tab to newTab
        end tell
    end if
    return "Opened new tab: {safe_url}"
end tell
"#
    );

    let result = run_osascript(&script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safari_name() {
        let tool = MacosSafariTool;
        assert_eq!(tool.name(), "macos_safari");
    }

    #[test]
    fn test_safari_risk_level() {
        let tool = MacosSafariTool;
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_safari_timeout() {
        let tool = MacosSafariTool;
        assert_eq!(tool.timeout(), Duration::from_secs(15));
    }

    #[test]
    fn test_safari_schema() {
        let tool = MacosSafariTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("url"));
        assert!(props.contains_key("script"));
    }

    #[tokio::test]
    async fn test_safari_missing_action() {
        let tool = MacosSafariTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_safari");
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_safari_invalid_action() {
        let tool = MacosSafariTool;
        let result = tool.execute(json!({"action": "bad"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_safari");
                assert!(reason.contains("bad"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }
    }
}
