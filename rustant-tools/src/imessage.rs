//! iMessage tools — contact lookup and message sending via macOS Messages.app.
//!
//! These tools expose the iMessage channel functionality as agent tools,
//! allowing the LLM to search contacts by name and send iMessages directly.
//! macOS only.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

// ── Contact Search Tool ────────────────────────────────────────────────────

/// Tool that searches macOS Contacts by name and returns matching entries
/// with phone numbers and email addresses.
pub struct IMessageContactsTool;

#[async_trait]
impl Tool for IMessageContactsTool {
    fn name(&self) -> &str {
        "imessage_contacts"
    }

    fn description(&self) -> &str {
        "Search macOS Contacts by name. Returns matching contacts with phone numbers \
         and email addresses. Use this to find the correct recipient before sending \
         an iMessage."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Name or partial name to search for (e.g. 'John', 'Chaitu')"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "imessage_contacts".to_string(),
                reason: "missing required 'query' parameter".to_string(),
            })?;

        debug!(query = query, "Searching contacts");

        let contacts =
            search_contacts_applescript(query)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "imessage_contacts".into(),
                    message: e,
                })?;

        if contacts.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No contacts found matching '{}'.",
                query
            )));
        }

        let mut output = format!(
            "Found {} contact(s) matching '{}':\n\n",
            contacts.len(),
            query
        );
        for (i, contact) in contacts.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, contact.name));
            if let Some(ref phone) = contact.phone {
                output.push_str(&format!("   Phone: {}\n", phone));
            }
            if let Some(ref email) = contact.email {
                output.push_str(&format!("   Email: {}\n", email));
            }
            output.push('\n');
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

// ── Send Message Tool ──────────────────────────────────────────────────────

/// Tool that sends an iMessage to a recipient via Messages.app.
pub struct IMessageSendTool;

#[async_trait]
impl Tool for IMessageSendTool {
    fn name(&self) -> &str {
        "imessage_send"
    }

    fn description(&self) -> &str {
        "Send an iMessage to a recipient via macOS Messages.app. The recipient \
         should be a phone number (e.g. '+1234567890') or Apple ID email. Use \
         imessage_contacts first to find the correct phone number or email for \
         a contact name."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "recipient": {
                    "type": "string",
                    "description": "Phone number (e.g. '+1234567890') or Apple ID email of the recipient"
                },
                "message": {
                    "type": "string",
                    "description": "The text message to send"
                }
            },
            "required": ["recipient", "message"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let recipient = args["recipient"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "imessage_send".to_string(),
                reason: "missing required 'recipient' parameter".to_string(),
            })?;

        let message = args["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "imessage_send".to_string(),
                reason: "missing required 'message' parameter".to_string(),
            })?;

        debug!(recipient = recipient, "Sending iMessage");

        send_imessage_applescript(recipient, message)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "imessage_send".into(),
                message: e,
            })?;

        Ok(ToolOutput::text(format!(
            "iMessage sent successfully to {}.",
            recipient
        )))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

// ── Read Messages Tool ─────────────────────────────────────────────────────

/// Tool that reads recent incoming iMessages from the Messages database.
pub struct IMessageReadTool;

#[async_trait]
impl Tool for IMessageReadTool {
    fn name(&self) -> &str {
        "imessage_read"
    }

    fn description(&self) -> &str {
        "Read recent incoming iMessages. Returns the latest messages received \
         in the past N minutes (default 5). Useful for checking replies."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "minutes": {
                    "type": "integer",
                    "description": "How far back to look in minutes (default: 5, max: 60)",
                    "default": 5
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of messages to return (default: 20)",
                    "default": 20
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let minutes = args["minutes"].as_u64().unwrap_or(5).min(60);
        let limit = args["limit"].as_u64().unwrap_or(20).min(100);

        debug!(minutes = minutes, limit = limit, "Reading recent iMessages");

        let messages = read_recent_imessages(minutes, limit).await.map_err(|e| {
            ToolError::ExecutionFailed {
                name: "imessage_read".into(),
                message: e,
            }
        })?;

        if messages.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No incoming messages in the last {} minute(s).",
                minutes
            )));
        }

        let mut output = format!(
            "Recent iMessages (last {} minute(s), {} message(s)):\n\n",
            minutes,
            messages.len()
        );
        for msg in &messages {
            output.push_str(&format!("From: {}\n", msg.sender));
            output.push_str(&format!("Text: {}\n\n", msg.text));
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

// ── AppleScript helpers ────────────────────────────────────────────────────

/// A contact result from AppleScript Contacts search.
#[derive(Debug)]
struct ContactResult {
    name: String,
    phone: Option<String>,
    email: Option<String>,
}

/// An incoming iMessage from the database.
#[derive(Debug)]
struct IncomingMessage {
    sender: String,
    text: String,
}

/// Search macOS Contacts via AppleScript.
async fn search_contacts_applescript(query: &str) -> Result<Vec<ContactResult>, String> {
    let escaped = query.replace('"', "\\\"");
    let script = format!(
        r#"tell application "Contacts"
    set matchingPeople to every person whose name contains "{query}"
    set output to ""
    repeat with p in matchingPeople
        set pName to name of p
        set pPhone to ""
        set pEmail to ""
        try
            set pPhone to value of phone 1 of p
        end try
        try
            set pEmail to value of email 1 of p
        end try
        set output to output & pName & "||" & pPhone & "||" & pEmail & "%%"
    end repeat
    return output
end tell"#,
        query = escaped
    );

    let output = tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .await
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Contacts lookup failed: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let contacts = stdout
        .trim()
        .split("%%")
        .filter(|s| !s.is_empty())
        .filter_map(|entry| {
            let parts: Vec<&str> = entry.split("||").collect();
            if parts.is_empty() {
                return None;
            }
            let name = parts[0].trim().to_string();
            if name.is_empty() {
                return None;
            }
            let phone = parts.get(1).and_then(|p| {
                let p = p.trim();
                if p.is_empty() {
                    None
                } else {
                    Some(p.to_string())
                }
            });
            let email = parts.get(2).and_then(|e| {
                let e = e.trim();
                if e.is_empty() {
                    None
                } else {
                    Some(e.to_string())
                }
            });
            Some(ContactResult { name, phone, email })
        })
        .collect();

    Ok(contacts)
}

/// Send an iMessage via AppleScript.
async fn send_imessage_applescript(recipient: &str, text: &str) -> Result<(), String> {
    let escaped_recipient = recipient.replace('"', "\\\"");
    let escaped_text = text.replace('"', "\\\"");
    let script = format!(
        "tell application \"Messages\"\n\
         \tset targetService to 1st service whose service type = iMessage\n\
         \tset targetBuddy to buddy \"{}\" of targetService\n\
         \tsend \"{}\" to targetBuddy\n\
         end tell",
        escaped_recipient, escaped_text,
    );

    let output = tokio::process::Command::new("osascript")
        .args(["-e", &script])
        .output()
        .await
        .map_err(|e| format!("Failed to run osascript: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to send iMessage: {}", stderr));
    }

    Ok(())
}

/// Read recent incoming iMessages using sqlite3 on the Messages database.
async fn read_recent_imessages(minutes: u64, limit: u64) -> Result<Vec<IncomingMessage>, String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let db_path = format!("{}/Library/Messages/chat.db", home);

    // macOS Messages uses Apple's Core Data epoch (2001-01-01) in nanoseconds.
    // We compute seconds-ago × 1e9 for the date comparison.
    let seconds = minutes * 60;
    let query = format!(
        "SELECT m.text, h.id as sender \
         FROM message m \
         JOIN handle h ON m.handle_id = h.ROWID \
         WHERE m.is_from_me = 0 \
         AND m.text IS NOT NULL \
         AND m.date > (strftime('%s', 'now') - 978307200 - {seconds}) * 1000000000 \
         ORDER BY m.date DESC \
         LIMIT {limit};",
        seconds = seconds,
        limit = limit,
    );

    let output = tokio::process::Command::new("sqlite3")
        .args([&db_path, "-json", &query])
        .output()
        .await
        .map_err(|e| format!("Failed to read Messages DB: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Full Disk Access may be needed
        return Err(format!(
            "Cannot read Messages database: {}. \
             Ensure your terminal has Full Disk Access in System Settings.",
            stderr
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(vec![]);
    }

    let rows: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).map_err(|e| format!("JSON parse error: {e}"))?;

    let messages = rows
        .iter()
        .filter_map(|r| {
            let sender = r["sender"].as_str()?.to_string();
            let text = r["text"].as_str().unwrap_or("").to_string();
            if text.is_empty() {
                return None;
            }
            Some(IncomingMessage { sender, text })
        })
        .collect();

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imessage_contacts_tool_definition() {
        let tool = IMessageContactsTool;
        assert_eq!(tool.name(), "imessage_contacts");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["query"].is_object());
    }

    #[test]
    fn test_imessage_send_tool_definition() {
        let tool = IMessageSendTool;
        assert_eq!(tool.name(), "imessage_send");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["recipient"].is_object());
        assert!(schema["properties"]["message"].is_object());
    }

    #[test]
    fn test_imessage_read_tool_definition() {
        let tool = IMessageReadTool;
        assert_eq!(tool.name(), "imessage_read");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["minutes"].is_object());
        assert!(schema["properties"]["limit"].is_object());
    }

    #[tokio::test]
    async fn test_imessage_contacts_missing_query() {
        let tool = IMessageContactsTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "imessage_contacts");
                assert!(reason.contains("query"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[tokio::test]
    async fn test_imessage_send_missing_recipient() {
        let tool = IMessageSendTool;
        let result = tool.execute(json!({"message": "hello"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "imessage_send");
                assert!(reason.contains("recipient"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }

    #[tokio::test]
    async fn test_imessage_send_missing_message() {
        let tool = IMessageSendTool;
        let result = tool.execute(json!({"recipient": "+1234567890"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "imessage_send");
                assert!(reason.contains("message"));
            }
            _ => panic!("Expected InvalidArguments error"),
        }
    }
}
