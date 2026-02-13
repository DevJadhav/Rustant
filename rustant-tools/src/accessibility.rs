//! macOS Accessibility tree inspector — read-only tool for exploring UI element
//! hierarchies in native macOS applications via System Events.
//!
//! This is a read-only companion to `macos_gui_scripting` for safe inspection
//! of app UI without side effects. macOS only.

use crate::macos::{require_str, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

const TOOL_NAME: &str = "macos_accessibility";
const MAX_OUTPUT_CHARS: usize = 5000;

pub struct MacosAccessibilityTool;

#[async_trait]
impl Tool for MacosAccessibilityTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Read-only inspection of macOS application accessibility trees. \
         Actions: get_tree (element hierarchy with roles/titles/values), \
         find_element (search elements by title or role), \
         get_focused (currently focused element), \
         get_frontmost_app (active app name and window title). \
         Requires Accessibility permissions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get_tree", "find_element", "get_focused", "get_frontmost_app"],
                    "description": "Action to perform"
                },
                "app_name": {
                    "type": "string",
                    "description": "Target application name (required for get_tree, find_element)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query for find_element (matches title, description, or value)"
                },
                "role_filter": {
                    "type": "string",
                    "description": "Filter by role (e.g. 'AXButton', 'AXTextField')"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum depth for tree traversal (default 3, max 6)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", TOOL_NAME)?;

        match action {
            "get_tree" => execute_get_tree(&args).await,
            "find_element" => execute_find_element(&args).await,
            "get_focused" => execute_get_focused().await,
            "get_frontmost_app" => execute_get_frontmost_app().await,
            other => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid: get_tree, find_element, get_focused, get_frontmost_app"
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

/// Truncate output to prevent context window overflow.
fn truncate_output(text: &str) -> String {
    if text.len() > MAX_OUTPUT_CHARS {
        format!(
            "{}...\n\n[Truncated — {} chars total. Use find_element for targeted queries.]",
            &text[..MAX_OUTPUT_CHARS],
            text.len()
        )
    } else {
        text.to_string()
    }
}

fn accessibility_err(e: String) -> ToolError {
    if e.contains("not allowed assistive access")
        || e.contains("access not allowed")
        || e.contains("System Events got an error")
    {
        ToolError::PermissionDenied {
            name: TOOL_NAME.to_string(),
            reason: "Accessibility permission required. Enable it at: \
                     System Settings → Privacy & Security → Accessibility → \
                     add your terminal app."
                .to_string(),
        }
    } else {
        ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        }
    }
}

async fn execute_get_tree(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let app = require_str(args, "app_name", TOOL_NAME)?;
    let safe_app = sanitize_applescript_string(app);
    let max_depth = args["max_depth"].as_u64().unwrap_or(3).min(6);
    debug!(app = %safe_app, max_depth, "Getting accessibility tree");

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        try
            set w to window 1
            set output to my walkTree(w, 0, {max_depth})
            return output
        on error errMsg
            return "Error: " & errMsg
        end try
    end tell
end tell

on walkTree(elem, depth, maxD)
    if depth > maxD then return ""
    set indent to ""
    repeat depth times
        set indent to indent & "  "
    end repeat

    set elemRole to role of elem
    set elemSubrole to ""
    try
        set elemSubrole to subrole of elem as string
    end try
    set elemTitle to ""
    try
        set elemTitle to title of elem as string
    end try
    set elemDesc to ""
    try
        set elemDesc to description of elem as string
    end try
    set elemValue to ""
    try
        set v to value of elem
        if v is not missing value then
            set elemValue to v as string
            if length of elemValue > 60 then
                set elemValue to text 1 thru 60 of elemValue & "..."
            end if
        end if
    end try
    set elemEnabled to true
    try
        set elemEnabled to enabled of elem
    end try

    set line_ to indent & elemRole
    if elemSubrole is not "" then set line_ to line_ & "/" & elemSubrole
    if elemTitle is not "" then set line_ to line_ & " \"" & elemTitle & "\""
    if elemDesc is not "" and elemDesc is not elemTitle then set line_ to line_ & " (" & elemDesc & ")"
    if elemValue is not "" then set line_ to line_ & " [" & elemValue & "]"
    if not elemEnabled then set line_ to line_ & " {{disabled}}"
    set output to line_ & linefeed

    if depth < maxD then
        try
            set kids to UI elements of elem
            repeat with kid in kids
                set output to output & my walkTree(kid, depth + 1, maxD)
            end repeat
        end try
    end if
    return output
end walkTree
"#
    );

    let result = run_osascript(&script).await.map_err(accessibility_err)?;
    let output = truncate_output(&result);
    Ok(ToolOutput::text(format!(
        "Accessibility tree for '{app}' (depth {max_depth}):\n{output}"
    )))
}

async fn execute_find_element(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let app = require_str(args, "app_name", TOOL_NAME)?;
    let safe_app = sanitize_applescript_string(app);
    let query = sanitize_applescript_string(require_str(args, "query", TOOL_NAME)?);
    let role_filter = args["role_filter"]
        .as_str()
        .map(sanitize_applescript_string);
    debug!(app = %safe_app, query = %query, "Finding elements");

    let role_check = if let Some(ref role) = role_filter {
        format!(r#"and elemRole is "{role}""#)
    } else {
        String::new()
    };

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        try
            set w to window 1
            set matches to my findInTree(w, "{query}", 0, 5, "{role_check}")
            if matches is "" then
                return "No elements found matching '{query}'."
            end if
            return matches
        on error errMsg
            return "Error: " & errMsg
        end try
    end tell
end tell

on findInTree(elem, searchQuery, depth, maxD, roleCheck)
    if depth > maxD then return ""
    set matches to ""

    set elemRole to role of elem
    set elemTitle to ""
    try
        set elemTitle to title of elem as string
    end try
    set elemDesc to ""
    try
        set elemDesc to description of elem as string
    end try
    set elemValue to ""
    try
        set v to value of elem
        if v is not missing value then set elemValue to v as string
    end try

    set combinedText to elemTitle & " " & elemDesc & " " & elemValue
    if combinedText contains searchQuery then
        set indent to ""
        repeat depth times
            set indent to indent & "  "
        end repeat
        set line_ to indent & elemRole
        if elemTitle is not "" then set line_ to line_ & " \"" & elemTitle & "\""
        if elemDesc is not "" then set line_ to line_ & " (" & elemDesc & ")"
        if elemValue is not "" then set line_ to line_ & " [" & elemValue & "]"
        set matches to matches & line_ & linefeed
    end if

    try
        set kids to UI elements of elem
        repeat with kid in kids
            set matches to matches & my findInTree(kid, searchQuery, depth + 1, maxD, roleCheck)
        end repeat
    end try

    return matches
end findInTree
"#,
        role_check = role_check
    );

    let result = run_osascript(&script).await.map_err(accessibility_err)?;
    Ok(ToolOutput::text(truncate_output(&result)))
}

async fn execute_get_focused() -> Result<ToolOutput, ToolError> {
    debug!("Getting focused element");

    let script = r#"
tell application "System Events"
    set focusedApp to name of first application process whose frontmost is true
    tell process focusedApp
        try
            set focusElem to focused of window 1
            set elemRole to role of focusElem
            set elemTitle to ""
            try
                set elemTitle to title of focusElem as string
            end try
            set elemValue to ""
            try
                set v to value of focusElem
                if v is not missing value then set elemValue to v as string
            end try
            set output to "App: " & focusedApp & linefeed & "Element: " & elemRole
            if elemTitle is not "" then set output to output & " \"" & elemTitle & "\""
            if elemValue is not "" then set output to output & " [" & elemValue & "]"
            return output
        on error
            return "App: " & focusedApp & linefeed & "No focused element detected."
        end try
    end tell
end tell
"#;

    let result = run_osascript(script).await.map_err(accessibility_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_get_frontmost_app() -> Result<ToolOutput, ToolError> {
    debug!("Getting frontmost application");

    let script = r#"
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set appName to name of frontApp
    set windowTitle to ""
    try
        set windowTitle to title of window 1 of frontApp as string
    end try
    set windowCount to count of windows of frontApp
    set output to "App: " & appName
    if windowTitle is not "" then set output to output & linefeed & "Window: " & windowTitle
    set output to output & linefeed & "Windows: " & (windowCount as string)
    return output
end tell
"#;

    let result = run_osascript(script).await.map_err(accessibility_err)?;
    Ok(ToolOutput::text(result))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_name() {
        let tool = MacosAccessibilityTool;
        assert_eq!(tool.name(), "macos_accessibility");
    }

    #[test]
    fn test_accessibility_risk_level() {
        let tool = MacosAccessibilityTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_accessibility_timeout() {
        let tool = MacosAccessibilityTool;
        assert_eq!(tool.timeout(), Duration::from_secs(15));
    }

    #[test]
    fn test_accessibility_schema() {
        let tool = MacosAccessibilityTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("app_name"));
        assert!(props.contains_key("query"));
        assert!(props.contains_key("role_filter"));
        assert!(props.contains_key("max_depth"));
    }

    #[tokio::test]
    async fn test_accessibility_missing_action() {
        let tool = MacosAccessibilityTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_accessibility");
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_accessibility_invalid_action() {
        let tool = MacosAccessibilityTool;
        let result = tool.execute(json!({"action": "bad"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_accessibility");
                assert!(reason.contains("bad"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }

    #[test]
    fn test_truncate_output_short() {
        let short = "hello world";
        assert_eq!(truncate_output(short), short);
    }

    #[test]
    fn test_truncate_output_long() {
        let long = "a".repeat(6000);
        let result = truncate_output(&long);
        assert!(result.len() < 6000);
        assert!(result.contains("Truncated"));
        assert!(result.contains("6000"));
    }
}
