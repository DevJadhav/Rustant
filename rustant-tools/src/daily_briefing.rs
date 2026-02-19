//! Daily briefing tool — aggregates calendar, reminders, weather, and system
//! status into a structured note in Notes.app.
//!
//! macOS only.

use crate::macos::{run_command, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use chrono::Utc;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

/// Fetch today's calendar events.
async fn get_todays_events() -> Result<String, String> {
    let script = r#"tell application "Calendar"
    set output to ""
    set today to current date
    set tomorrow to today + (1 * days)
    repeat with cal in calendars
        set calEvents to (every event of cal whose start date >= today and start date < tomorrow)
        repeat with evt in calEvents
            set output to output & "- " & (summary of evt) & " at " & (start date of evt as string) & " (" & (name of cal) & ")" & linefeed
        end repeat
    end repeat
    if output is "" then
        return "No events scheduled for today."
    end if
    return output
end tell"#;
    run_osascript(script).await
}

/// Fetch pending reminders.
async fn get_pending_reminders() -> Result<String, String> {
    let script = r#"tell application "Reminders"
    set output to ""
    set pendingItems to (every reminder whose completed is false)
    set counter to 0
    repeat with r in pendingItems
        set counter to counter + 1
        if counter > 20 then exit repeat
        set dueStr to ""
        try
            set dueStr to " (due: " & (due date of r as string) & ")"
        end try
        set output to output & "- " & (name of r) & dueStr & linefeed
    end repeat
    if output is "" then
        return "No pending reminders."
    end if
    return output
end tell"#;
    run_osascript(script).await
}

/// Fetch weather from wttr.in (no API key required).
async fn get_weather(location: &str) -> Result<String, String> {
    let loc = if location.is_empty() {
        String::new()
    } else {
        format!("/{}", urlencoding_simple(location))
    };

    let url = format!("https://wttr.in{loc}?format=3");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Try up to 2 times with a brief delay between attempts
    let mut last_err = String::new();
    for attempt in 0..2 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        match client
            .get(&url)
            .header("User-Agent", "rustant/1.0")
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.text().await {
                        Ok(t) => return Ok(t.trim().to_string()),
                        Err(e) => last_err = format!("Failed to read weather response: {e}"),
                    }
                } else {
                    last_err = format!("Weather API returned {}", resp.status());
                }
            }
            Err(e) => {
                last_err = format!("Weather fetch failed: {e}");
            }
        }
    }

    // Return a graceful fallback instead of propagating the error
    Ok(format!("Weather unavailable ({last_err})"))
}

/// Simple URL encoding for location strings.
fn urlencoding_simple(s: &str) -> String {
    s.replace(' ', "+").replace('&', "%26").replace('?', "%3F")
}

/// Get basic system status (battery + disk).
async fn get_system_status() -> Result<String, String> {
    let mut output = String::new();

    // Battery
    if let Ok(battery) = run_command("pmset", &["-g", "batt"]).await
        && let Some(pct_line) = battery.lines().nth(1)
    {
        output.push_str(&format!("Battery: {}\n", pct_line.trim()));
    }

    // Disk usage
    if let Ok(disk) = run_command("df", &["-h", "/"]).await
        && let Some(usage_line) = disk.lines().nth(1)
    {
        let parts: Vec<&str> = usage_line.split_whitespace().collect();
        if parts.len() >= 5 {
            output.push_str(&format!(
                "Disk: {} used of {} ({})\n",
                parts[2], parts[1], parts[4]
            ));
        }
    }

    if output.is_empty() {
        Ok("System status unavailable.".to_string())
    } else {
        Ok(output.trim().to_string())
    }
}

/// Fetch tomorrow's calendar preview.
async fn get_tomorrow_preview() -> Result<String, String> {
    let script = r#"tell application "Calendar"
    set output to ""
    set tomorrow to (current date) + (1 * days)
    set dayAfter to tomorrow + (1 * days)
    repeat with cal in calendars
        set calEvents to (every event of cal whose start date >= tomorrow and start date < dayAfter)
        repeat with evt in calEvents
            set output to output & "- " & (summary of evt) & " at " & (start date of evt as string) & " (" & (name of cal) & ")" & linefeed
        end repeat
    end repeat
    if output is "" then
        return "No events scheduled for tomorrow."
    end if
    return output
end tell"#;
    run_osascript(script).await
}

/// Save briefing to Notes.app.
async fn save_briefing_note(title: &str, body: &str, folder: &str) -> Result<String, String> {
    let title_safe = sanitize_applescript_string(title);
    let body_safe = sanitize_applescript_string(body);
    let folder_safe = sanitize_applescript_string(folder);

    // Ensure folder exists
    let ensure_script = format!(
        r#"tell application "Notes"
    try
        set targetFolder to folder "{folder_safe}"
    on error
        make new folder with properties {{name:"{folder_safe}"}}
    end try
end tell"#
    );
    run_osascript(&ensure_script).await.ok();

    let script = format!(
        r#"tell application "Notes"
    set targetFolder to folder "{folder_safe}"
    make new note at targetFolder with properties {{name:"{title_safe}", body:"{body_safe}"}}
    return "Briefing saved to Notes.app: {title_safe}"
end tell"#
    );
    run_osascript(&script).await
}

pub struct MacosDailyBriefingTool;

#[async_trait]
impl Tool for MacosDailyBriefingTool {
    fn name(&self) -> &str {
        "macos_daily_briefing"
    }

    fn description(&self) -> &str {
        "Generate a daily briefing combining calendar events, reminders, weather, \
         and system status. Saves to Notes.app. Actions: morning (today's briefing), \
         evening (end-of-day summary with tomorrow preview), custom (select components)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["morning", "evening", "custom"],
                    "description": "Briefing type"
                },
                "include_weather": {
                    "type": "boolean",
                    "description": "Include weather forecast (default: true)"
                },
                "include_system": {
                    "type": "boolean",
                    "description": "Include system status (default: false)"
                },
                "location": {
                    "type": "string",
                    "description": "Location for weather (default: auto-detect by IP)"
                },
                "folder": {
                    "type": "string",
                    "description": "Notes.app folder name (default: Daily Briefings)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "macos_daily_briefing".to_string(),
                reason: "missing required 'action' parameter".to_string(),
            })?;

        let include_weather = args["include_weather"].as_bool().unwrap_or(true);
        let include_system = args["include_system"].as_bool().unwrap_or(false);
        let location = args["location"].as_str().unwrap_or("");
        let folder = args["folder"].as_str().unwrap_or("Daily Briefings");
        let date_str = Utc::now().format("%Y-%m-%d").to_string();

        match action {
            "morning" => {
                debug!("Generating morning briefing");
                let mut sections = Vec::new();

                // Calendar events
                let events = get_todays_events()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch calendar: {e}"));
                sections.push(format!(
                    "<h2>Today's Schedule</h2><p>{}</p>",
                    events.replace('\n', "<br>")
                ));

                // Reminders
                let reminders = get_pending_reminders()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch reminders: {e}"));
                sections.push(format!(
                    "<h2>Pending Reminders</h2><p>{}</p>",
                    reminders.replace('\n', "<br>")
                ));

                // Weather
                if include_weather {
                    let weather = get_weather(location)
                        .await
                        .unwrap_or_else(|e| format!("Weather unavailable: {e}"));
                    sections.push(format!("<h2>Weather</h2><p>{weather}</p>"));
                }

                // System status
                if include_system {
                    let status = get_system_status()
                        .await
                        .unwrap_or_else(|e| format!("System status unavailable: {e}"));
                    sections.push(format!(
                        "<h2>System Status</h2><p>{}</p>",
                        status.replace('\n', "<br>")
                    ));
                }

                let body = format!(
                    "<h1>Morning Briefing - {date_str}</h1>{}",
                    sections.join("")
                );
                let title = format!("Morning Briefing - {date_str}");

                // Save to Notes
                let save_result = save_briefing_note(&title, &body, folder)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_daily_briefing".into(),
                        message: e,
                    })?;

                // Also return the content as text
                let plain_text = format!(
                    "=== Morning Briefing - {date_str} ===\n\n\
                     Schedule:\n{events}\n\
                     Reminders:\n{reminders}\n\
                     {weather_section}\
                     {system_section}\
                     {save_result}",
                    weather_section = if include_weather {
                        format!(
                            "Weather: {}\n\n",
                            get_weather(location).await.unwrap_or_default()
                        )
                    } else {
                        String::new()
                    },
                    system_section = if include_system {
                        format!(
                            "System: {}\n\n",
                            get_system_status().await.unwrap_or_default()
                        )
                    } else {
                        String::new()
                    },
                );
                Ok(ToolOutput::text(plain_text))
            }

            "evening" => {
                debug!("Generating evening summary");
                let mut sections = Vec::new();

                // Tomorrow preview
                let tomorrow = get_tomorrow_preview()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch tomorrow's events: {e}"));
                sections.push(format!(
                    "<h2>Tomorrow's Schedule</h2><p>{}</p>",
                    tomorrow.replace('\n', "<br>")
                ));

                // Pending reminders
                let reminders = get_pending_reminders()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch reminders: {e}"));
                sections.push(format!(
                    "<h2>Outstanding Reminders</h2><p>{}</p>",
                    reminders.replace('\n', "<br>")
                ));

                let body = format!("<h1>Evening Summary - {date_str}</h1>{}", sections.join(""));
                let title = format!("Evening Summary - {date_str}");

                let save_result = save_briefing_note(&title, &body, folder)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_daily_briefing".into(),
                        message: e,
                    })?;

                let plain_text = format!(
                    "=== Evening Summary - {date_str} ===\n\n\
                     Tomorrow:\n{tomorrow}\n\
                     Outstanding Reminders:\n{reminders}\n\
                     {save_result}"
                );
                Ok(ToolOutput::text(plain_text))
            }

            "custom" => {
                debug!("Generating custom briefing");
                let mut parts = Vec::new();

                if include_weather {
                    let weather = get_weather(location)
                        .await
                        .unwrap_or_else(|e| format!("Weather unavailable: {e}"));
                    parts.push(format!("Weather: {weather}"));
                }

                if include_system {
                    let status = get_system_status()
                        .await
                        .unwrap_or_else(|e| format!("System status unavailable: {e}"));
                    parts.push(format!("System:\n{status}"));
                }

                // Always include calendar and reminders for custom
                let events = get_todays_events()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch calendar: {e}"));
                parts.push(format!("Today's Events:\n{events}"));

                let reminders = get_pending_reminders()
                    .await
                    .unwrap_or_else(|e| format!("Could not fetch reminders: {e}"));
                parts.push(format!("Reminders:\n{reminders}"));

                Ok(ToolOutput::text(parts.join("\n\n")))
            }

            other => Err(ToolError::InvalidArguments {
                name: "macos_daily_briefing".to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid actions: morning, evening, custom"
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let tool = MacosDailyBriefingTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .len()
                >= 3
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("action"))
        );
    }

    #[test]
    fn test_tool_metadata() {
        let tool = MacosDailyBriefingTool;
        assert_eq!(tool.name(), "macos_daily_briefing");
        assert!(tool.description().contains("briefing"));
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), Duration::from_secs(30));
    }

    #[test]
    fn test_urlencoding_simple() {
        assert_eq!(urlencoding_simple("New York"), "New+York");
        assert_eq!(urlencoding_simple("London"), "London");
        assert_eq!(urlencoding_simple("a&b"), "a%26b");
    }

    #[tokio::test]
    async fn test_invalid_action_returns_error() {
        let tool = MacosDailyBriefingTool;
        let args = json!({"action": "invalid"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_missing_action_returns_error() {
        let tool = MacosDailyBriefingTool;
        let args = json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
