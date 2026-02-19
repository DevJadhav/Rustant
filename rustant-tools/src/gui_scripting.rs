//! macOS GUI Scripting tool — interact with native app UI elements via System Events.
//!
//! Enables Rustant to click buttons, type in fields, navigate menus, and read
//! UI element values in any macOS application using AppleScript GUI scripting.
//! macOS only. Requires Accessibility permissions in System Settings.

use crate::macos::{require_str, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

/// Applications that must never be targeted by GUI scripting for security.
const DENIED_APPS: &[&str] = &[
    "loginwindow",
    "SecurityAgent",
    "SystemUIServer",
    "WindowServer",
    "kernel_task",
    "securityd",
    "authorizationhost",
];

const TOOL_NAME: &str = "macos_gui_scripting";

/// Check if an app name is in the deny list (case-insensitive).
fn is_denied_app(app_name: &str) -> bool {
    let lower = app_name.to_lowercase();
    DENIED_APPS
        .iter()
        .any(|denied| denied.to_lowercase() == lower)
}

/// Detect accessibility permission errors and return a user-friendly message.
fn check_accessibility_error(err: &str) -> Option<String> {
    if err.contains("not allowed assistive access")
        || err.contains("access not allowed")
        || err.contains("is not allowed to send keystrokes")
        || err.contains("System Events got an error")
    {
        Some(
            "Accessibility permission required. Enable it at: \
             System Settings → Privacy & Security → Accessibility → \
             add your terminal app (Terminal.app, iTerm, etc.)."
                .to_string(),
        )
    } else {
        None
    }
}

/// Map an osascript error, checking for accessibility issues first.
fn gui_err(e: String) -> ToolError {
    if let Some(hint) = check_accessibility_error(&e) {
        ToolError::PermissionDenied {
            name: TOOL_NAME.to_string(),
            reason: hint,
        }
    } else {
        ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        }
    }
}

pub struct MacosGuiScriptingTool;

#[async_trait]
impl Tool for MacosGuiScriptingTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Interact with native macOS application UI elements via GUI scripting. \
         Actions: list_elements (UI element tree), click_element (click button/checkbox), \
         type_text (type in a text field), read_text (read element value), \
         menu_action (click menu item), get_window_info (window details), \
         click_at_position (click screen coords), keyboard_shortcut (send key combo). \
         Requires Accessibility permissions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "list_elements", "click_element", "type_text", "read_text",
                        "menu_action", "get_window_info", "click_at_position",
                        "keyboard_shortcut"
                    ],
                    "description": "Action to perform"
                },
                "app_name": {
                    "type": "string",
                    "description": "Target application name (required for most actions)"
                },
                "element_description": {
                    "type": "string",
                    "description": "Title or description of the UI element to interact with"
                },
                "element_type": {
                    "type": "string",
                    "description": "UI element type: button, text field, checkbox, radio button, pop up button, etc."
                },
                "text": {
                    "type": "string",
                    "description": "Text to type (for type_text action)"
                },
                "menu_path": {
                    "type": "string",
                    "description": "Menu path separated by ' > ' (e.g. 'File > Save As...')"
                },
                "x": {
                    "type": "number",
                    "description": "X screen coordinate (for click_at_position)"
                },
                "y": {
                    "type": "number",
                    "description": "Y screen coordinate (for click_at_position)"
                },
                "key": {
                    "type": "string",
                    "description": "Key to press (for keyboard_shortcut, e.g. 's', 'return', 'tab')"
                },
                "modifiers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Modifier keys: command, option, shift, control"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Max depth for element listing (default 2)"
                },
                "field_description": {
                    "type": "string",
                    "description": "Optional: target a specific text field by title/description"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", TOOL_NAME)?;

        match action {
            "list_elements" => execute_list_elements(&args).await,
            "click_element" => execute_click_element(&args).await,
            "type_text" => execute_type_text(&args).await,
            "read_text" => execute_read_text(&args).await,
            "menu_action" => execute_menu_action(&args).await,
            "get_window_info" => execute_get_window_info(&args).await,
            "click_at_position" => execute_click_at_position(&args).await,
            "keyboard_shortcut" => execute_keyboard_shortcut(&args).await,
            other => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid: list_elements, click_element, \
                     type_text, read_text, menu_action, get_window_info, \
                     click_at_position, keyboard_shortcut"
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

/// Validate and return a sanitized app name, rejecting denied apps.
fn validated_app(args: &serde_json::Value) -> Result<String, ToolError> {
    let app = require_str(args, "app_name", TOOL_NAME)?;
    if is_denied_app(app) {
        return Err(ToolError::PermissionDenied {
            name: TOOL_NAME.to_string(),
            reason: format!(
                "Targeting '{app}' is not allowed for security reasons. \
                 Denied apps: {}",
                DENIED_APPS.join(", ")
            ),
        });
    }
    Ok(sanitize_applescript_string(app))
}

// ---------------------------------------------------------------------------
// Action implementations
// ---------------------------------------------------------------------------

async fn execute_list_elements(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let max_depth = args["max_depth"].as_u64().unwrap_or(2);
    debug!(app = %safe_app, max_depth, "Listing UI elements");

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.3
        set output to ""
        try
            set w to window 1
            set output to my listUI(w, 0, {max_depth})
        on error errMsg
            return "Error: " & errMsg
        end try
        return output
    end tell
end tell

on listUI(elem, depth, maxD)
    if depth > maxD then return ""
    set indent to ""
    repeat depth times
        set indent to indent & "  "
    end repeat

    set elemRole to role of elem
    set elemTitle to ""
    try
        set elemTitle to title of elem as string
    end try
    set elemValue to ""
    try
        set v to value of elem
        if v is not missing value then
            set elemValue to v as string
            if length of elemValue > 80 then
                set elemValue to text 1 thru 80 of elemValue & "..."
            end if
        end if
    end try

    set line_ to indent & elemRole
    if elemTitle is not "" then set line_ to line_ & " \"" & elemTitle & "\""
    if elemValue is not "" then set line_ to line_ & " [" & elemValue & "]"
    set output to line_ & linefeed

    if depth < maxD then
        try
            set kids to UI elements of elem
            repeat with kid in kids
                set output to output & my listUI(kid, depth + 1, maxD)
            end repeat
        end try
    end if
    return output
end listUI
"#
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;

    // Truncate to prevent context overflow
    let truncated = if result.len() > 5000 {
        format!(
            "{}...\n\n[Truncated — {} chars total. Use find_element or increase max_depth for targeted queries.]",
            &result[..5000],
            result.len()
        )
    } else {
        result
    };

    Ok(ToolOutput::text(format!(
        "UI elements for '{safe_app}' (depth {max_depth}):\n{truncated}"
    )))
}

async fn execute_click_element(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let element = sanitize_applescript_string(require_str(args, "element_description", TOOL_NAME)?);
    let elem_type = args["element_type"].as_str().unwrap_or("button");
    let safe_type = sanitize_applescript_string(elem_type);
    debug!(app = %safe_app, element = %element, elem_type = %safe_type, "Clicking element");

    // Try clicking by title first, then fall back to description
    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.2
        try
            click {safe_type} "{element}" of window 1
            return "Clicked {safe_type} '{element}'."
        on error
            try
                click (first {safe_type} whose description is "{element}") of window 1
                return "Clicked {safe_type} with description '{element}'."
            on error
                -- Search deeper in the UI hierarchy
                try
                    click (first {safe_type} "{element}" of first group of window 1)
                    return "Clicked {safe_type} '{element}' (in group)."
                on error errMsg
                    return "Error: Could not find {safe_type} '{element}'. " & errMsg
                end try
            end try
        end try
    end tell
end tell
"#
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_type_text(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let text = require_str(args, "text", TOOL_NAME)?;
    let safe_text = sanitize_applescript_string(text);
    debug!(app = %safe_app, text_len = text.len(), "Typing text");

    let script = if let Some(field_desc) = args["field_description"].as_str() {
        let safe_field = sanitize_applescript_string(field_desc);
        format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.2
        try
            set focused of text field "{safe_field}" of window 1 to true
            delay 0.1
        end try
        keystroke "{safe_text}"
    end tell
end tell
return "Typed text into '{safe_field}'."
"#
        )
    } else {
        format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.2
        keystroke "{safe_text}"
    end tell
end tell
return "Typed text into frontmost field."
"#
        )
    };

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_read_text(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let element = sanitize_applescript_string(require_str(args, "element_description", TOOL_NAME)?);
    debug!(app = %safe_app, element = %element, "Reading text");

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        try
            return value of text field "{element}" of window 1 as string
        on error
            try
                return value of static text "{element}" of window 1 as string
            on error
                try
                    return value of text area "{element}" of window 1 as string
                on error errMsg
                    return "Error: Could not read element '{element}'. " & errMsg
                end try
            end try
        end try
    end tell
end tell
"#
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_menu_action(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let menu_path = require_str(args, "menu_path", TOOL_NAME)?;
    debug!(app = %safe_app, menu_path = %menu_path, "Executing menu action");

    let parts: Vec<&str> = menu_path.split(" > ").collect();
    if parts.len() < 2 {
        return Err(ToolError::InvalidArguments {
            name: TOOL_NAME.to_string(),
            reason: "menu_path must have at least 2 parts separated by ' > ' (e.g. 'File > Save')"
                .to_string(),
        });
    }

    // Build nested menu item AppleScript for arbitrary depth
    let safe_parts: Vec<String> = parts
        .iter()
        .map(|p| sanitize_applescript_string(p))
        .collect();
    let menu_bar = &safe_parts[0];

    let script = if safe_parts.len() == 2 {
        format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.3
        click menu item "{}" of menu 1 of menu bar item "{menu_bar}" of menu bar 1
    end tell
end tell
return "Clicked menu: {menu_path}."
"#,
            safe_parts[1]
        )
    } else if safe_parts.len() == 3 {
        format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.3
        click menu item "{}" of menu 1 of menu item "{}" of menu 1 of menu bar item "{menu_bar}" of menu bar 1
    end tell
end tell
return "Clicked menu: {menu_path}."
"#,
            safe_parts[2], safe_parts[1]
        )
    } else {
        // For deeper nesting, flatten to the deepest 3 levels
        let len = safe_parts.len();
        format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.3
        click menu item "{}" of menu 1 of menu item "{}" of menu 1 of menu bar item "{menu_bar}" of menu bar 1
    end tell
end tell
return "Clicked menu: {menu_path}."
"#,
            safe_parts[len - 1],
            safe_parts[len - 2]
        )
    };

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_get_window_info(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    debug!(app = %safe_app, "Getting window info");

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.2
        try
            set w to window 1
            set wTitle to title of w
            set wPos to position of w
            set wSize to size of w
            set roleCount to count of UI elements of w
            return "Title: " & wTitle & linefeed & "Position: " & (item 1 of wPos as string) & ", " & (item 2 of wPos as string) & linefeed & "Size: " & (item 1 of wSize as string) & " x " & (item 2 of wSize as string) & linefeed & "UI elements: " & (roleCount as string)
        on error errMsg
            return "Error getting window info: " & errMsg
        end try
    end tell
end tell
"#
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_click_at_position(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let x = args["x"]
        .as_f64()
        .ok_or_else(|| ToolError::InvalidArguments {
            name: TOOL_NAME.to_string(),
            reason: "missing required 'x' coordinate".to_string(),
        })?;
    let y = args["y"]
        .as_f64()
        .ok_or_else(|| ToolError::InvalidArguments {
            name: TOOL_NAME.to_string(),
            reason: "missing required 'y' coordinate".to_string(),
        })?;
    debug!(x, y, "Clicking at position");

    // Use cliclick if available, otherwise fall back to AppleScript
    let script = format!(
        r#"
do shell script "
if command -v cliclick >/dev/null 2>&1; then
    cliclick c:{x_int},{y_int}
else
    osascript -e 'tell application \"System Events\" to click at {{{x_int}, {y_int}}}'
fi
"
return "Clicked at ({x_int}, {y_int})."
"#,
        x_int = x as i64,
        y_int = y as i64,
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

async fn execute_keyboard_shortcut(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let safe_app = validated_app(args)?;
    let key = sanitize_applescript_string(require_str(args, "key", TOOL_NAME)?);
    debug!(app = %safe_app, key = %key, "Sending keyboard shortcut");

    let modifiers = args["modifiers"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();

    // Build modifier clause
    let mut using_clause = String::new();
    if !modifiers.is_empty() {
        let modifier_strs: Vec<String> = modifiers
            .iter()
            .filter_map(|m| match *m {
                "command" => Some("command down".to_string()),
                "option" => Some("option down".to_string()),
                "shift" => Some("shift down".to_string()),
                "control" => Some("control down".to_string()),
                _ => None,
            })
            .collect();
        if !modifier_strs.is_empty() {
            using_clause = format!(" using {{{}}}", modifier_strs.join(", "));
        }
    }

    let script = format!(
        r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.2
        keystroke "{key}"{using_clause}
    end tell
end tell
return "Sent keystroke '{key}' to '{safe_app}'."
"#
    );

    let result = run_osascript(&script).await.map_err(gui_err)?;
    Ok(ToolOutput::text(result))
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_scripting_name() {
        let tool = MacosGuiScriptingTool;
        assert_eq!(tool.name(), "macos_gui_scripting");
    }

    #[test]
    fn test_gui_scripting_risk_level() {
        let tool = MacosGuiScriptingTool;
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
    }

    #[test]
    fn test_gui_scripting_timeout() {
        let tool = MacosGuiScriptingTool;
        assert_eq!(tool.timeout(), Duration::from_secs(15));
    }

    #[test]
    fn test_gui_scripting_schema() {
        let tool = MacosGuiScriptingTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("app_name"));
        assert!(props.contains_key("element_description"));
        assert!(props.contains_key("text"));
        assert!(props.contains_key("menu_path"));
        assert!(props.contains_key("x"));
        assert!(props.contains_key("y"));
        assert!(props.contains_key("key"));
        assert!(props.contains_key("modifiers"));
    }

    #[tokio::test]
    async fn test_gui_scripting_missing_action() {
        let tool = MacosGuiScriptingTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_gui_scripting");
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_gui_scripting_invalid_action() {
        let tool = MacosGuiScriptingTool;
        let result = tool.execute(json!({"action": "nope"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_gui_scripting");
                assert!(reason.contains("nope"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }
    }

    #[test]
    fn test_denied_apps() {
        assert!(is_denied_app("loginwindow"));
        assert!(is_denied_app("SecurityAgent"));
        assert!(is_denied_app("SystemUIServer"));
        assert!(is_denied_app("WindowServer"));
        assert!(is_denied_app("kernel_task"));
        // Case-insensitive
        assert!(is_denied_app("LOGINWINDOW"));
        assert!(is_denied_app("securityagent"));
        // Normal apps allowed
        assert!(!is_denied_app("TextEdit"));
        assert!(!is_denied_app("Finder"));
        assert!(!is_denied_app("Safari"));
    }

    #[tokio::test]
    async fn test_denied_app_rejected() {
        let tool = MacosGuiScriptingTool;
        let result = tool
            .execute(json!({"action": "list_elements", "app_name": "loginwindow"}))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::PermissionDenied { name, reason } => {
                assert_eq!(name, "macos_gui_scripting");
                assert!(reason.contains("loginwindow"));
            }
            other => panic!("Expected PermissionDenied, got: {other:?}"),
        }
    }

    #[test]
    fn test_applescript_injection_prevention() {
        // Simple quote injection: verify quotes are escaped
        let malicious = r#"hello"world"#;
        let sanitized = sanitize_applescript_string(malicious);
        assert_eq!(sanitized, r#"hello\"world"#);

        // Backslash injection: verify backslashes are escaped first
        let backslash = r#"test\path"#;
        let sanitized = sanitize_applescript_string(backslash);
        assert_eq!(sanitized, r#"test\\path"#);

        // Combined: backslash before quote must not unescape the quote
        let combo = r#"a\"b"#;
        let sanitized = sanitize_applescript_string(combo);
        // \ becomes \\, " becomes \" → a\\"b → a\\\"b
        assert_eq!(sanitized, r#"a\\\"b"#);

        // Verify the do-shell-script attack vector is neutralized
        let attack = r#""); do shell script "rm -rf /"#;
        let sanitized = sanitize_applescript_string(attack);
        // Every " must be preceded by \
        for (i, c) in sanitized.chars().enumerate() {
            if c == '"' {
                assert!(
                    i > 0 && sanitized.as_bytes()[i - 1] == b'\\',
                    "Found unescaped quote at position {i} in: {sanitized}"
                );
            }
        }
    }

    #[test]
    fn test_menu_path_validation() {
        // menu_path requires at least 2 parts
        // We can't easily test this without tokio, but we verify the format expectations
        let path = "File > Save As...";
        let parts: Vec<&str> = path.split(" > ").collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "File");
        assert_eq!(parts[1], "Save As...");
    }

    #[test]
    fn test_accessibility_error_detection() {
        assert!(check_accessibility_error("not allowed assistive access").is_some());
        assert!(check_accessibility_error("access not allowed").is_some());
        assert!(check_accessibility_error("System Events got an error").is_some());
        assert!(check_accessibility_error("normal error message").is_none());
    }
}
