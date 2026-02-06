//! macOS native tools — Calendar, Reminders, Notes, App Control, Notifications,
//! Clipboard, Screenshot, System Info, Spotlight, and Finder integration.
//!
//! These tools enable Rustant to function as a complete daily macOS assistant
//! by bridging to native macOS apps via AppleScript and CLI commands.
//! macOS only.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

// ── Shared Helpers ──────────────────────────────────────────────────────────

/// Run an AppleScript via osascript and return stdout.
async fn run_osascript(script: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .await
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("AppleScript error: {stderr}"))
    }
}

/// Run a CLI command and return stdout.
async fn run_command(cmd: &str, args: &[&str]) -> Result<String, String> {
    let output = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .map_err(|e| format!("Failed to run {cmd}: {e}"))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("{cmd} error: {stderr}"))
    }
}

/// Sanitize a string for safe use inside AppleScript quoted strings.
/// Prevents AppleScript injection by escaping backslashes and double quotes.
fn sanitize_applescript_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Helper to extract a required string argument.
fn require_str<'a>(
    args: &'a serde_json::Value,
    field: &str,
    tool_name: &str,
) -> Result<&'a str, ToolError> {
    args[field]
        .as_str()
        .ok_or_else(|| ToolError::InvalidArguments {
            name: tool_name.to_string(),
            reason: format!("missing required '{}' parameter", field),
        })
}

// ── 1. Calendar Tool ────────────────────────────────────────────────────────

pub struct MacosCalendarTool;

#[async_trait]
impl Tool for MacosCalendarTool {
    fn name(&self) -> &str {
        "macos_calendar"
    }

    fn description(&self) -> &str {
        "Manage macOS Calendar events. Actions: list (upcoming events), create (new event), \
         delete (remove event), search (find events by title). Uses the native Calendar.app."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "delete", "search"],
                    "description": "Action to perform"
                },
                "calendar": {
                    "type": "string",
                    "description": "Calendar name (default: all calendars for list/search)"
                },
                "title": {
                    "type": "string",
                    "description": "Event title (required for create/search)"
                },
                "start": {
                    "type": "string",
                    "description": "Start date/time in ISO 8601 format (for create)"
                },
                "end": {
                    "type": "string",
                    "description": "End date/time in ISO 8601 format (for create)"
                },
                "days_ahead": {
                    "type": "integer",
                    "description": "Number of days ahead to list events (default: 7)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_calendar")?;

        match action {
            "list" => {
                let days = args["days_ahead"].as_u64().unwrap_or(7);
                debug!(days = days, "Listing upcoming calendar events");
                let script = format!(
                    r#"tell application "Calendar"
    set output to ""
    set today to current date
    set endDate to today + ({days} * days)
    repeat with cal in calendars
        set calEvents to (every event of cal whose start date >= today and start date <= endDate)
        repeat with evt in calEvents
            set output to output & (summary of evt) & " | " & (start date of evt as string) & " | " & (name of cal) & linefeed
        end repeat
    end repeat
    if output is "" then
        return "No upcoming events in the next {days} days."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_calendar".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "create" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_calendar")?);
                let start = require_str(&args, "start", "macos_calendar")?;
                let end = args["end"].as_str().unwrap_or(start);
                let calendar = args["calendar"].as_str().unwrap_or("Calendar");
                let cal = sanitize_applescript_string(calendar);

                debug!(title = %title, start = %start, "Creating calendar event");
                let script = format!(
                    r#"tell application "Calendar"
    set targetCal to first calendar whose name is "{cal}"
    set startDate to date "{start}"
    set endDate to date "{end}"
    make new event at end of events of targetCal with properties {{summary:"{title}", start date:startDate, end date:endDate}}
    return "Event '{title}' created successfully."
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_calendar".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "search" => {
                let query =
                    sanitize_applescript_string(require_str(&args, "title", "macos_calendar")?);
                debug!(query = %query, "Searching calendar events");
                let script = format!(
                    r#"tell application "Calendar"
    set output to ""
    repeat with cal in calendars
        set matchingEvents to (every event of cal whose summary contains "{query}")
        repeat with evt in matchingEvents
            set output to output & (summary of evt) & " | " & (start date of evt as string) & " | " & (name of cal) & linefeed
        end repeat
    end repeat
    if output is "" then
        return "No events found matching '{query}'."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_calendar".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "delete" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_calendar")?);
                debug!(title = %title, "Deleting calendar event");
                let script = format!(
                    r#"tell application "Calendar"
    set found to false
    repeat with cal in calendars
        set matchingEvents to (every event of cal whose summary is "{title}")
        repeat with evt in matchingEvents
            delete evt
            set found to true
        end repeat
    end repeat
    if found then
        return "Event '{title}' deleted."
    else
        return "No event found with title '{title}'."
    end if
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_calendar".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_calendar".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid actions: list, create, delete, search",
                    other
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

// ── 2. Reminders Tool ───────────────────────────────────────────────────────

pub struct MacosRemindersTool;

#[async_trait]
impl Tool for MacosRemindersTool {
    fn name(&self) -> &str {
        "macos_reminders"
    }

    fn description(&self) -> &str {
        "Manage macOS Reminders. Actions: list (incomplete reminders), create (new reminder), \
         complete (mark as done), search (find by text). Uses the native Reminders.app."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "complete", "search"],
                    "description": "Action to perform"
                },
                "list": {
                    "type": "string",
                    "description": "Reminder list name (default: Reminders)"
                },
                "title": {
                    "type": "string",
                    "description": "Reminder title (required for create/complete/search)"
                },
                "due_date": {
                    "type": "string",
                    "description": "Due date in natural language or ISO 8601 (for create)"
                },
                "notes": {
                    "type": "string",
                    "description": "Notes for the reminder (for create)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_reminders")?;

        match action {
            "list" => {
                let list_name = args["list"].as_str().unwrap_or("Reminders");
                let list_safe = sanitize_applescript_string(list_name);
                debug!(list = %list_safe, "Listing reminders");
                let script = format!(
                    r#"tell application "Reminders"
    set output to ""
    try
        set targetList to list "{list_safe}"
        set incompleteReminders to (reminders of targetList whose completed is false)
        repeat with r in incompleteReminders
            set output to output & (name of r)
            if due date of r is not missing value then
                set output to output & " | Due: " & (due date of r as string)
            end if
            set output to output & linefeed
        end repeat
    end try
    if output is "" then
        return "No incomplete reminders."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_reminders".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "create" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_reminders")?);
                let list_name =
                    sanitize_applescript_string(args["list"].as_str().unwrap_or("Reminders"));
                let notes = args["notes"]
                    .as_str()
                    .map(sanitize_applescript_string)
                    .unwrap_or_default();

                debug!(title = %title, "Creating reminder");
                let due_clause = if let Some(due) = args["due_date"].as_str() {
                    let safe_due = sanitize_applescript_string(due);
                    format!(", due date:(date \"{safe_due}\")")
                } else {
                    String::new()
                };

                let script = format!(
                    r#"tell application "Reminders"
    set targetList to list "{list_name}"
    make new reminder at end of reminders of targetList with properties {{name:"{title}", body:"{notes}"{due_clause}}}
    return "Reminder '{title}' created."
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_reminders".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "complete" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_reminders")?);
                debug!(title = %title, "Completing reminder");
                let script = format!(
                    r#"tell application "Reminders"
    set found to false
    repeat with l in lists
        repeat with r in (reminders of l whose name is "{title}" and completed is false)
            set completed of r to true
            set found to true
        end repeat
    end repeat
    if found then
        return "Reminder '{title}' marked as complete."
    else
        return "No incomplete reminder found with title '{title}'."
    end if
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_reminders".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "search" => {
                let query =
                    sanitize_applescript_string(require_str(&args, "title", "macos_reminders")?);
                debug!(query = %query, "Searching reminders");
                let script = format!(
                    r#"tell application "Reminders"
    set output to ""
    repeat with l in lists
        set matches to (reminders of l whose name contains "{query}")
        repeat with r in matches
            set status to "incomplete"
            if completed of r then set status to "done"
            set output to output & (name of r) & " [" & status & "] in " & (name of l) & linefeed
        end repeat
    end repeat
    if output is "" then
        return "No reminders found matching '{query}'."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_reminders".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_reminders".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid actions: list, create, complete, search",
                    other
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

// ── 3. Notes Tool ───────────────────────────────────────────────────────────

pub struct MacosNotesTool;

#[async_trait]
impl Tool for MacosNotesTool {
    fn name(&self) -> &str {
        "macos_notes"
    }

    fn description(&self) -> &str {
        "Manage Apple Notes. Actions: list (recent notes), create (new note), \
         read (note content), search (find by title/body). Uses the native Notes.app."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create", "read", "search"],
                    "description": "Action to perform"
                },
                "folder": {
                    "type": "string",
                    "description": "Folder name (default: Notes)"
                },
                "title": {
                    "type": "string",
                    "description": "Note title (required for create/read)"
                },
                "body": {
                    "type": "string",
                    "description": "Note body content (for create)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (for search action)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum notes to return (default: 20)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_notes")?;

        match action {
            "list" => {
                let limit = args["limit"].as_u64().unwrap_or(20);
                debug!(limit = limit, "Listing notes");
                let script = format!(
                    r#"tell application "Notes"
    set output to ""
    set noteList to notes 1 thru (minimum of ({limit} as integer) and (count of notes))
    repeat with n in noteList
        set output to output & (name of n) & " | " & (modification date of n as string) & linefeed
    end repeat
    if output is "" then
        return "No notes found."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_notes".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "create" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_notes")?);
                let body = args["body"]
                    .as_str()
                    .map(sanitize_applescript_string)
                    .unwrap_or_default();
                let folder =
                    sanitize_applescript_string(args["folder"].as_str().unwrap_or("Notes"));

                debug!(title = %title, "Creating note");
                let html_body = format!("<h1>{title}</h1><br>{body}");
                let script = format!(
                    r#"tell application "Notes"
    set targetFolder to folder "{folder}"
    make new note at targetFolder with properties {{name:"{title}", body:"{html_body}"}}
    return "Note '{title}' created."
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_notes".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "read" => {
                let title =
                    sanitize_applescript_string(require_str(&args, "title", "macos_notes")?);
                debug!(title = %title, "Reading note");
                let script = format!(
                    r#"tell application "Notes"
    set matchingNotes to (notes whose name is "{title}")
    if (count of matchingNotes) is 0 then
        return "No note found with title '{title}'."
    end if
    set n to item 1 of matchingNotes
    return (plaintext of n)
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_notes".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "search" => {
                let query = sanitize_applescript_string(
                    args["query"]
                        .as_str()
                        .or_else(|| args["title"].as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: "macos_notes".to_string(),
                            reason: "missing 'query' or 'title' parameter for search".to_string(),
                        })?,
                );
                debug!(query = %query, "Searching notes");
                let script = format!(
                    r#"tell application "Notes"
    set output to ""
    set matches to (notes whose name contains "{query}")
    repeat with n in matches
        set output to output & (name of n) & " | " & (modification date of n as string) & linefeed
    end repeat
    if output is "" then
        return "No notes found matching '{query}'."
    end if
    return output
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_notes".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_notes".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid actions: list, create, read, search",
                    other
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

// ── 4. App Control Tool ─────────────────────────────────────────────────────

pub struct MacosAppControlTool;

#[async_trait]
impl Tool for MacosAppControlTool {
    fn name(&self) -> &str {
        "macos_app_control"
    }

    fn description(&self) -> &str {
        "Control macOS applications. Actions: open (launch app), quit (close app), \
         list_running (show running apps), activate (bring app to front)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["open", "quit", "list_running", "activate"],
                    "description": "Action to perform"
                },
                "app_name": {
                    "type": "string",
                    "description": "Application name (required for open/quit/activate)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_app_control")?;

        match action {
            "open" => {
                let app = require_str(&args, "app_name", "macos_app_control")?;
                let safe_app = sanitize_applescript_string(app);
                debug!(app = %safe_app, "Opening application");
                run_command("open", &["-a", app]).await.map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "macos_app_control".into(),
                        message: e,
                    }
                })?;
                Ok(ToolOutput::text(format!("Opened '{app}'.")))
            }
            "quit" => {
                let app = require_str(&args, "app_name", "macos_app_control")?;
                let safe_app = sanitize_applescript_string(app);
                debug!(app = %safe_app, "Quitting application");
                let script = format!(r#"tell application "{safe_app}" to quit"#);
                run_osascript(&script)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_app_control".into(),
                        message: e,
                    })?;
                Ok(ToolOutput::text(format!("Quit '{app}'.")))
            }
            "list_running" => {
                debug!("Listing running applications");
                let script = r#"tell application "System Events" to get name of every application process whose visible is true"#;
                let result =
                    run_osascript(script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_app_control".into(),
                            message: e,
                        })?;
                let apps: Vec<&str> = result.split(", ").collect();
                let mut output = format!("Running applications ({}):\n", apps.len());
                for app in &apps {
                    output.push_str(&format!("  - {app}\n"));
                }
                Ok(ToolOutput::text(output))
            }
            "activate" => {
                let app = require_str(&args, "app_name", "macos_app_control")?;
                let safe_app = sanitize_applescript_string(app);
                debug!(app = %safe_app, "Activating application");
                let script = format!(r#"tell application "{safe_app}" to activate"#);
                run_osascript(&script)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_app_control".into(),
                        message: e,
                    })?;
                Ok(ToolOutput::text(format!("Activated '{app}'.")))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_app_control".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid actions: open, quit, list_running, activate",
                    other
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

// ── 5. Notification Tool ────────────────────────────────────────────────────

pub struct MacosNotificationTool;

#[async_trait]
impl Tool for MacosNotificationTool {
    fn name(&self) -> &str {
        "macos_notification"
    }

    fn description(&self) -> &str {
        "Send a macOS system notification. Appears in Notification Center."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Notification body text"
                },
                "title": {
                    "type": "string",
                    "description": "Notification title (default: Rustant)"
                },
                "subtitle": {
                    "type": "string",
                    "description": "Optional subtitle"
                },
                "sound": {
                    "type": "string",
                    "description": "Sound name (default: 'default')"
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let message =
            sanitize_applescript_string(require_str(&args, "message", "macos_notification")?);
        let title = sanitize_applescript_string(args["title"].as_str().unwrap_or("Rustant"));
        let sound = args["sound"].as_str().unwrap_or("default");

        debug!(title = %title, "Sending notification");

        let mut script = format!(r#"display notification "{message}" with title "{title}""#);
        if let Some(subtitle) = args["subtitle"].as_str() {
            let safe_sub = sanitize_applescript_string(subtitle);
            script.push_str(&format!(r#" subtitle "{safe_sub}""#));
        }
        script.push_str(&format!(r#" sound name "{sound}""#));

        run_osascript(&script)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "macos_notification".into(),
                message: e,
            })?;
        Ok(ToolOutput::text(format!(
            "Notification sent: '{title}' — {message}"
        )))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// ── 6. Clipboard Tool ───────────────────────────────────────────────────────

pub struct MacosClipboardTool;

#[async_trait]
impl Tool for MacosClipboardTool {
    fn name(&self) -> &str {
        "macos_clipboard"
    }

    fn description(&self) -> &str {
        "Read from or write to the macOS clipboard. Actions: read (get clipboard contents), \
         write (set clipboard contents)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read", "write"],
                    "description": "Action: read or write"
                },
                "content": {
                    "type": "string",
                    "description": "Text to copy to clipboard (required for write)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_clipboard")?;

        match action {
            "read" => {
                debug!("Reading clipboard");
                let result =
                    run_command("pbpaste", &[])
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_clipboard".into(),
                            message: e,
                        })?;
                if result.is_empty() {
                    Ok(ToolOutput::text("Clipboard is empty."))
                } else {
                    Ok(ToolOutput::text(format!("Clipboard contents:\n{result}")))
                }
            }
            "write" => {
                let content = require_str(&args, "content", "macos_clipboard")?;
                debug!(len = content.len(), "Writing to clipboard");
                let mut child = tokio::process::Command::new("pbcopy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_clipboard".into(),
                        message: format!("Failed to spawn pbcopy: {e}"),
                    })?;

                use tokio::io::AsyncWriteExt;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(content.as_bytes()).await.map_err(|e| {
                        ToolError::ExecutionFailed {
                            name: "macos_clipboard".into(),
                            message: format!("Failed to write to pbcopy: {e}"),
                        }
                    })?;
                }
                child.wait().await.map_err(|e| ToolError::ExecutionFailed {
                    name: "macos_clipboard".into(),
                    message: format!("pbcopy failed: {e}"),
                })?;

                Ok(ToolOutput::text(format!(
                    "Copied {} characters to clipboard.",
                    content.len()
                )))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_clipboard".to_string(),
                reason: format!("unknown action '{}'. Valid actions: read, write", other),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// ── 7. Screenshot Tool ──────────────────────────────────────────────────────

pub struct MacosScreenshotTool;

#[async_trait]
impl Tool for MacosScreenshotTool {
    fn name(&self) -> &str {
        "macos_screenshot"
    }

    fn description(&self) -> &str {
        "Capture a screenshot on macOS. Modes: full (entire screen), window (front window), \
         region (specific rectangle). Saves to the specified path."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Output file path (default: ~/Desktop/screenshot.png)"
                },
                "mode": {
                    "type": "string",
                    "enum": ["full", "window", "region"],
                    "description": "Capture mode (default: full)"
                },
                "region": {
                    "type": "string",
                    "description": "Region as 'x,y,width,height' (for region mode)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let default_path = format!(
            "{}/Desktop/screenshot_{}.png",
            std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string()),
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        );
        let path = args["path"].as_str().unwrap_or(&default_path);
        let mode = args["mode"].as_str().unwrap_or("full");

        debug!(path = %path, mode = %mode, "Taking screenshot");

        let mut cmd_args = vec!["-x"]; // silent (no sound)
        match mode {
            "window" => cmd_args.push("-w"),
            "region" => {
                if let Some(region) = args["region"].as_str() {
                    cmd_args.push("-R");
                    cmd_args.push(region);
                }
            }
            _ => {} // full screen is default
        }
        cmd_args.push(path);

        run_command("screencapture", &cmd_args)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "macos_screenshot".into(),
                message: e,
            })?;

        Ok(ToolOutput::text(format!("Screenshot saved to: {path}")))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

// ── 8. System Info Tool ─────────────────────────────────────────────────────

pub struct MacosSystemInfoTool;

#[async_trait]
impl Tool for MacosSystemInfoTool {
    fn name(&self) -> &str {
        "macos_system_info"
    }

    fn description(&self) -> &str {
        "Get macOS system information. Types: all (everything), battery, disk, memory, \
         network, uptime, cpu, version."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "info_type": {
                    "type": "string",
                    "enum": ["all", "battery", "disk", "memory", "network", "uptime", "cpu", "version"],
                    "description": "Type of system info to retrieve (default: all)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let info_type = args["info_type"].as_str().unwrap_or("all");
        debug!(info_type = %info_type, "Getting system info");

        let mut sections = Vec::new();
        let exec_err = |e: String| ToolError::ExecutionFailed {
            name: "macos_system_info".into(),
            message: e,
        };

        if info_type == "all" || info_type == "version" {
            if let Ok(ver) = run_command("sw_vers", &[]).await {
                sections.push(format!("## macOS Version\n{ver}"));
            }
        }
        if info_type == "all" || info_type == "cpu" {
            if let Ok(cpu) = run_command("sysctl", &["-n", "machdep.cpu.brand_string"]).await {
                sections.push(format!("## CPU\n{cpu}"));
            }
        }
        if info_type == "all" || info_type == "memory" {
            if let Ok(mem) = run_command("sysctl", &["-n", "hw.memsize"]).await {
                let bytes: u64 = mem.trim().parse().unwrap_or(0);
                let gb = bytes as f64 / 1_073_741_824.0;
                sections.push(format!("## Memory\nTotal: {gb:.1} GB"));
            }
        }
        if info_type == "all" || info_type == "disk" {
            if let Ok(disk) = run_command("df", &["-h", "/"]).await {
                sections.push(format!("## Disk Usage\n{disk}"));
            }
        }
        if info_type == "all" || info_type == "battery" {
            if let Ok(batt) = run_command("pmset", &["-g", "batt"]).await {
                sections.push(format!("## Battery\n{batt}"));
            }
        }
        if info_type == "all" || info_type == "network" {
            let mut net_info = String::from("## Network\n");
            if let Ok(wifi) = run_command("networksetup", &["-getairportnetwork", "en0"]).await {
                net_info.push_str(&format!("{wifi}\n"));
            }
            if let Ok(ip) = run_command("ipconfig", &["getifaddr", "en0"]).await {
                net_info.push_str(&format!("IP: {ip}"));
            }
            sections.push(net_info);
        }
        if info_type == "all" || info_type == "uptime" {
            if let Ok(up) = run_command("uptime", &[]).await {
                sections.push(format!("## Uptime\n{up}"));
            }
        }

        if sections.is_empty() {
            return Err(exec_err(format!("Unknown info type: {info_type}")));
        }

        Ok(ToolOutput::text(sections.join("\n\n")))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

// ── 9. Spotlight Tool ───────────────────────────────────────────────────────

pub struct MacosSpotlightTool;

#[async_trait]
impl Tool for MacosSpotlightTool {
    fn name(&self) -> &str {
        "macos_spotlight"
    }

    fn description(&self) -> &str {
        "Search files using macOS Spotlight (mdfind). Supports Spotlight query syntax \
         (e.g., 'kind:pdf budget', 'kMDItemContentType == \"public.jpeg\"'). \
         Much faster than manual file traversal."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Spotlight search query"
                },
                "directory": {
                    "type": "string",
                    "description": "Limit search to this directory"
                },
                "name_only": {
                    "type": "boolean",
                    "description": "Search by filename only (default: false)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum results to return (default: 20)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = require_str(&args, "query", "macos_spotlight")?;
        let limit = args["limit"].as_u64().unwrap_or(20);
        let name_only = args["name_only"].as_bool().unwrap_or(false);

        debug!(query = %query, limit = limit, "Spotlight search");

        let mut cmd_args: Vec<String> = Vec::new();
        if let Some(dir) = args["directory"].as_str() {
            cmd_args.push("-onlyin".to_string());
            cmd_args.push(dir.to_string());
        }
        if name_only {
            cmd_args.push("-name".to_string());
        }
        cmd_args.push(query.to_string());

        let args_refs: Vec<&str> = cmd_args.iter().map(|s| s.as_str()).collect();
        let result =
            run_command("mdfind", &args_refs)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "macos_spotlight".into(),
                    message: e,
                })?;

        if result.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No files found matching '{query}'."
            )));
        }

        let lines: Vec<&str> = result.lines().take(limit as usize).collect();
        let total_count = result.lines().count();
        let mut output = format!(
            "Found {} result(s) (showing {}):\n\n",
            total_count,
            lines.len()
        );
        for line in &lines {
            output.push_str(&format!("  {line}\n"));
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(15)
    }
}

// ── 10. Finder Tool ─────────────────────────────────────────────────────────

pub struct MacosFinderTool;

#[async_trait]
impl Tool for MacosFinderTool {
    fn name(&self) -> &str {
        "macos_finder"
    }

    fn description(&self) -> &str {
        "Interact with macOS Finder. Actions: reveal (show file in Finder), \
         open_folder (open directory), get_selection (get currently selected files), \
         trash (move to Trash — reversible)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["reveal", "open_folder", "get_selection", "trash"],
                    "description": "Action to perform"
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path (required for reveal/open_folder/trash)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_finder")?;

        match action {
            "reveal" => {
                let path = require_str(&args, "path", "macos_finder")?;
                debug!(path = %path, "Revealing in Finder");
                run_command("open", &["-R", path]).await.map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "macos_finder".into(),
                        message: e,
                    }
                })?;
                Ok(ToolOutput::text(format!("Revealed '{path}' in Finder.")))
            }
            "open_folder" => {
                let path = require_str(&args, "path", "macos_finder")?;
                debug!(path = %path, "Opening folder in Finder");
                run_command("open", &[path])
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_finder".into(),
                        message: e,
                    })?;
                Ok(ToolOutput::text(format!("Opened '{path}' in Finder.")))
            }
            "get_selection" => {
                debug!("Getting Finder selection");
                let script = r#"tell application "Finder"
    set sel to selection as alias list
    if (count of sel) is 0 then
        return "No files selected in Finder."
    end if
    set output to ""
    repeat with f in sel
        set output to output & (POSIX path of f) & linefeed
    end repeat
    return output
end tell"#;
                let result =
                    run_osascript(script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_finder".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            "trash" => {
                let path = require_str(&args, "path", "macos_finder")?;
                let safe_path = sanitize_applescript_string(path);
                debug!(path = %path, "Moving to Trash");
                let script = format!(
                    r#"tell application "Finder"
    move POSIX file "{safe_path}" to trash
    return "Moved '{safe_path}' to Trash."
end tell"#
                );
                let result =
                    run_osascript(&script)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_finder".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }
            other => Err(ToolError::InvalidArguments {
                name: "macos_finder".to_string(),
                reason: format!(
                    "unknown action '{}'. Valid actions: reveal, open_folder, get_selection, trash",
                    other
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(10)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    }

    // ── Calendar Tool Tests ─────────────────────────────────────────────

    #[test]
    fn test_calendar_tool_name() {
        assert_eq!(MacosCalendarTool.name(), "macos_calendar");
    }

    #[test]
    fn test_calendar_tool_risk_level() {
        assert_eq!(MacosCalendarTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_calendar_tool_timeout() {
        assert_eq!(MacosCalendarTool.timeout(), Duration::from_secs(15));
    }

    #[test]
    fn test_calendar_schema_has_required_fields() {
        let schema = MacosCalendarTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_calendar_missing_action_returns_error() {
        let result = rt().block_on(MacosCalendarTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_calendar_invalid_action_returns_error() {
        let result = rt().block_on(MacosCalendarTool.execute(json!({"action": "invalid"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Reminders Tool Tests ────────────────────────────────────────────

    #[test]
    fn test_reminders_tool_name() {
        assert_eq!(MacosRemindersTool.name(), "macos_reminders");
    }

    #[test]
    fn test_reminders_tool_risk_level() {
        assert_eq!(MacosRemindersTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_reminders_schema_has_required_fields() {
        let schema = MacosRemindersTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_reminders_missing_action_returns_error() {
        let result = rt().block_on(MacosRemindersTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_reminders_invalid_action_returns_error() {
        let result = rt().block_on(MacosRemindersTool.execute(json!({"action": "invalid"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Notes Tool Tests ────────────────────────────────────────────────

    #[test]
    fn test_notes_tool_name() {
        assert_eq!(MacosNotesTool.name(), "macos_notes");
    }

    #[test]
    fn test_notes_tool_risk_level() {
        assert_eq!(MacosNotesTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_notes_schema_has_required_fields() {
        let schema = MacosNotesTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_notes_missing_action_returns_error() {
        let result = rt().block_on(MacosNotesTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_notes_invalid_action_returns_error() {
        let result = rt().block_on(MacosNotesTool.execute(json!({"action": "invalid"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── App Control Tool Tests ──────────────────────────────────────────

    #[test]
    fn test_app_control_tool_name() {
        assert_eq!(MacosAppControlTool.name(), "macos_app_control");
    }

    #[test]
    fn test_app_control_risk_level() {
        assert_eq!(MacosAppControlTool.risk_level(), RiskLevel::Execute);
    }

    #[test]
    fn test_app_control_schema_has_required_fields() {
        let schema = MacosAppControlTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_app_control_missing_action_returns_error() {
        let result = rt().block_on(MacosAppControlTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_app_control_invalid_action_returns_error() {
        let result = rt().block_on(MacosAppControlTool.execute(json!({"action": "destroy"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Notification Tool Tests ─────────────────────────────────────────

    #[test]
    fn test_notification_tool_name() {
        assert_eq!(MacosNotificationTool.name(), "macos_notification");
    }

    #[test]
    fn test_notification_risk_level() {
        assert_eq!(MacosNotificationTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_notification_schema_requires_message() {
        let schema = MacosNotificationTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("message")));
    }

    #[test]
    fn test_notification_missing_message_returns_error() {
        let result = rt().block_on(MacosNotificationTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Clipboard Tool Tests ────────────────────────────────────────────

    #[test]
    fn test_clipboard_tool_name() {
        assert_eq!(MacosClipboardTool.name(), "macos_clipboard");
    }

    #[test]
    fn test_clipboard_risk_level() {
        assert_eq!(MacosClipboardTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_clipboard_schema_has_required_fields() {
        let schema = MacosClipboardTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_clipboard_missing_action_returns_error() {
        let result = rt().block_on(MacosClipboardTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_clipboard_invalid_action_returns_error() {
        let result = rt().block_on(MacosClipboardTool.execute(json!({"action": "clear"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_clipboard_write_missing_content_returns_error() {
        let result = rt().block_on(MacosClipboardTool.execute(json!({"action": "write"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Screenshot Tool Tests ───────────────────────────────────────────

    #[test]
    fn test_screenshot_tool_name() {
        assert_eq!(MacosScreenshotTool.name(), "macos_screenshot");
    }

    #[test]
    fn test_screenshot_risk_level() {
        assert_eq!(MacosScreenshotTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_screenshot_schema() {
        let schema = MacosScreenshotTool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["mode"].is_object());
    }

    // ── System Info Tool Tests ──────────────────────────────────────────

    #[test]
    fn test_system_info_tool_name() {
        assert_eq!(MacosSystemInfoTool.name(), "macos_system_info");
    }

    #[test]
    fn test_system_info_risk_level() {
        assert_eq!(MacosSystemInfoTool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_system_info_schema() {
        let schema = MacosSystemInfoTool.parameters_schema();
        let info_type = &schema["properties"]["info_type"];
        let valid_types = info_type["enum"].as_array().unwrap();
        assert!(valid_types.contains(&json!("all")));
        assert!(valid_types.contains(&json!("battery")));
        assert!(valid_types.contains(&json!("disk")));
        assert!(valid_types.contains(&json!("memory")));
        assert!(valid_types.contains(&json!("cpu")));
        assert!(valid_types.contains(&json!("version")));
    }

    // ── Spotlight Tool Tests ────────────────────────────────────────────

    #[test]
    fn test_spotlight_tool_name() {
        assert_eq!(MacosSpotlightTool.name(), "macos_spotlight");
    }

    #[test]
    fn test_spotlight_risk_level() {
        assert_eq!(MacosSpotlightTool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_spotlight_schema_requires_query() {
        let schema = MacosSpotlightTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
    }

    #[test]
    fn test_spotlight_missing_query_returns_error() {
        let result = rt().block_on(MacosSpotlightTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Finder Tool Tests ───────────────────────────────────────────────

    #[test]
    fn test_finder_tool_name() {
        assert_eq!(MacosFinderTool.name(), "macos_finder");
    }

    #[test]
    fn test_finder_risk_level() {
        assert_eq!(MacosFinderTool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_finder_schema_has_required_fields() {
        let schema = MacosFinderTool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("action")));
    }

    #[test]
    fn test_finder_missing_action_returns_error() {
        let result = rt().block_on(MacosFinderTool.execute(json!({})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_finder_invalid_action_returns_error() {
        let result = rt().block_on(MacosFinderTool.execute(json!({"action": "copy"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[test]
    fn test_finder_trash_missing_path_returns_error() {
        let result = rt().block_on(MacosFinderTool.execute(json!({"action": "trash"})));
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    // ── Helper Tests ────────────────────────────────────────────────────

    #[test]
    fn test_sanitize_applescript_string() {
        assert_eq!(sanitize_applescript_string("hello"), "hello");
        assert_eq!(sanitize_applescript_string(r#"say "hi""#), r#"say \"hi\""#);
        assert_eq!(sanitize_applescript_string(r"path\to"), r"path\\to");
        // Prevents injection: "; do shell script "malicious"
        let malicious = r#""; do shell script "rm -rf /""#;
        let sanitized = sanitize_applescript_string(malicious);
        assert!(!sanitized.contains(r#"""#) || sanitized.contains(r#"\""#));
    }

    #[test]
    fn test_require_str_present() {
        let args = json!({"name": "test"});
        assert_eq!(require_str(&args, "name", "tool").unwrap(), "test");
    }

    #[test]
    fn test_require_str_missing() {
        let args = json!({});
        assert!(require_str(&args, "name", "tool").is_err());
    }

    #[test]
    fn test_require_str_wrong_type() {
        let args = json!({"name": 42});
        assert!(require_str(&args, "name", "tool").is_err());
    }
}
