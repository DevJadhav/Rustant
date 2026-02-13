//! macOS Contacts.app tool — search, read, and manage contacts via AppleScript.
//!
//! Provides full access to the macOS address book for searching contacts,
//! reading details, creating new contacts, and listing groups.
//! macOS only.

use crate::macos::{require_str, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

const TOOL_NAME: &str = "macos_contacts";

pub struct MacosContactsTool;

#[async_trait]
impl Tool for MacosContactsTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Manage macOS Contacts.app. Actions: search (find contacts by name/email/phone), \
         get_details (full contact card), create (add new contact), list_groups (contact groups)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search", "get_details", "create", "list_groups"],
                    "description": "Action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "Search query (name, email, or phone)"
                },
                "name": {
                    "type": "string",
                    "description": "Contact name for get_details"
                },
                "first_name": {
                    "type": "string",
                    "description": "First name for creating a contact"
                },
                "last_name": {
                    "type": "string",
                    "description": "Last name for creating a contact"
                },
                "email": {
                    "type": "string",
                    "description": "Email address for creating a contact"
                },
                "phone": {
                    "type": "string",
                    "description": "Phone number for creating a contact"
                },
                "company": {
                    "type": "string",
                    "description": "Company name for creating a contact"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", TOOL_NAME)?;

        match action {
            "search" => execute_search(&args).await,
            "get_details" => execute_get_details(&args).await,
            "create" => execute_create(&args).await,
            "list_groups" => execute_list_groups().await,
            other => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid: search, get_details, create, list_groups"
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

async fn execute_search(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let query = sanitize_applescript_string(require_str(args, "query", TOOL_NAME)?);
    debug!(query = %query, "Searching contacts");

    let script = format!(
        r#"
tell application "Contacts"
    set matchList to {{}}
    set searchResults to (every person whose name contains "{query}")
    repeat with p in searchResults
        set pName to name of p
        set pEmail to ""
        try
            set pEmail to value of first email of p
        end try
        set pPhone to ""
        try
            set pPhone to value of first phone of p
        end try
        set pCompany to ""
        try
            set pCompany to organization of p
        end try
        set pInfo to pName
        if pEmail is not "" then set pInfo to pInfo & " <" & pEmail & ">"
        if pPhone is not "" then set pInfo to pInfo & " (" & pPhone & ")"
        if pCompany is not "" then set pInfo to pInfo & " @ " & pCompany
        set end of matchList to pInfo
    end repeat

    -- Also search by email if no name matches
    if (count of matchList) is 0 then
        set emailResults to (every person whose value of emails contains "{query}")
        repeat with p in emailResults
            set pName to name of p
            set pEmail to value of first email of p
            set end of matchList to pName & " <" & pEmail & ">"
        end repeat
    end if

    if (count of matchList) is 0 then
        return "No contacts found matching '{query}'."
    end if

    set output to ""
    repeat with i from 1 to count of matchList
        set output to output & (item i of matchList) & linefeed
    end repeat
    return output
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

async fn execute_get_details(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let name = sanitize_applescript_string(require_str(args, "name", TOOL_NAME)?);
    debug!(name = %name, "Getting contact details");

    let script = format!(
        r#"
tell application "Contacts"
    set matchList to (every person whose name is "{name}")
    if (count of matchList) is 0 then
        set matchList to (every person whose name contains "{name}")
    end if

    if (count of matchList) is 0 then
        return "No contact found matching '{name}'."
    end if

    set p to item 1 of matchList
    set output to "Name: " & name of p

    try
        set output to output & linefeed & "Company: " & organization of p
    end try

    try
        set output to output & linefeed & "Job Title: " & job title of p
    end try

    -- Emails
    set emailList to emails of p
    if (count of emailList) > 0 then
        set output to output & linefeed & "Emails:"
        repeat with e in emailList
            set output to output & linefeed & "  - " & (label of e) & ": " & (value of e)
        end repeat
    end if

    -- Phones
    set phoneList to phones of p
    if (count of phoneList) > 0 then
        set output to output & linefeed & "Phones:"
        repeat with ph in phoneList
            set output to output & linefeed & "  - " & (label of ph) & ": " & (value of ph)
        end repeat
    end if

    -- Addresses
    set addrList to addresses of p
    if (count of addrList) > 0 then
        set output to output & linefeed & "Addresses:"
        repeat with a in addrList
            try
                set addrStr to (street of a) & ", " & (city of a) & ", " & (state of a) & " " & (zip of a)
                set output to output & linefeed & "  - " & (label of a) & ": " & addrStr
            end try
        end repeat
    end if

    -- Birthday
    try
        set bday to birth date of p
        set output to output & linefeed & "Birthday: " & (bday as string)
    end try

    -- Notes
    try
        set pNote to note of p
        if pNote is not missing value and pNote is not "" then
            set output to output & linefeed & "Notes: " & pNote
        end if
    end try

    return output
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

async fn execute_create(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let first = sanitize_applescript_string(require_str(args, "first_name", TOOL_NAME)?);
    let last = args["last_name"]
        .as_str()
        .map(sanitize_applescript_string)
        .unwrap_or_default();
    debug!(first = %first, last = %last, "Creating contact");

    let mut props = format!(r#"{{first name:"{first}""#);
    if !last.is_empty() {
        props.push_str(&format!(r#", last name:"{last}""#));
    }
    if let Some(company) = args["company"].as_str() {
        let safe = sanitize_applescript_string(company);
        props.push_str(&format!(r#", organization:"{safe}""#));
    }
    props.push('}');

    let mut script = format!(
        r#"
tell application "Contacts"
    set newPerson to make new person with properties {props}
"#
    );

    if let Some(email) = args["email"].as_str() {
        let safe_email = sanitize_applescript_string(email);
        script.push_str(&format!(
            r#"    make new email at end of emails of newPerson with properties {{label:"work", value:"{safe_email}"}}
"#
        ));
    }

    if let Some(phone) = args["phone"].as_str() {
        let safe_phone = sanitize_applescript_string(phone);
        script.push_str(&format!(
            r#"    make new phone at end of phones of newPerson with properties {{label:"mobile", value:"{safe_phone}"}}
"#
        ));
    }

    script.push_str(
        r#"    save
    return "Contact created: " & name of newPerson
end tell
"#,
    );

    let result = run_osascript(&script)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: e,
        })?;

    Ok(ToolOutput::text(result))
}

async fn execute_list_groups() -> Result<ToolOutput, ToolError> {
    debug!("Listing contact groups");

    let script = r#"
tell application "Contacts"
    set groupList to {}
    repeat with g in groups
        set groupInfo to name of g & " (" & (count of people of g) & " contacts)"
        set end of groupList to groupInfo
    end repeat

    if (count of groupList) is 0 then
        return "No contact groups found."
    end if

    set output to "Contact groups:" & linefeed
    repeat with i from 1 to count of groupList
        set output to output & "  - " & (item i of groupList) & linefeed
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

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contacts_name() {
        let tool = MacosContactsTool;
        assert_eq!(tool.name(), "macos_contacts");
    }

    #[test]
    fn test_contacts_risk_level() {
        let tool = MacosContactsTool;
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    #[test]
    fn test_contacts_timeout() {
        let tool = MacosContactsTool;
        assert_eq!(tool.timeout(), Duration::from_secs(15));
    }

    #[test]
    fn test_contacts_schema() {
        let tool = MacosContactsTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("query"));
        assert!(props.contains_key("name"));
        assert!(props.contains_key("first_name"));
        assert!(props.contains_key("last_name"));
        assert!(props.contains_key("email"));
        assert!(props.contains_key("phone"));
    }

    #[tokio::test]
    async fn test_contacts_missing_action() {
        let tool = MacosContactsTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_contacts");
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_contacts_invalid_action() {
        let tool = MacosContactsTool;
        let result = tool.execute(json!({"action": "bad"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_contacts");
                assert!(reason.contains("bad"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }
}
