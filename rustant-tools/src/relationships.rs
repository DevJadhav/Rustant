//! Relationships tool — track contacts and interaction history.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Contact {
    id: usize,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    phone: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    tags: Vec<String>,
    interactions: Vec<Interaction>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Interaction {
    date: DateTime<Utc>,
    kind: String, // "call", "email", "meeting", "message", etc.
    note: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RelationshipsState {
    contacts: Vec<Contact>,
    next_id: usize,
}

pub struct RelationshipsTool {
    workspace: PathBuf,
}

impl RelationshipsTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("relationships")
            .join("contacts.json")
    }

    fn load_state(&self) -> RelationshipsState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            RelationshipsState {
                contacts: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &RelationshipsState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "relationships".to_string(),
                message: format!("Failed to create dir: {e}"),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "relationships".to_string(),
            message: format!("Serialize error: {e}"),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "relationships".to_string(),
            message: format!("Write error: {e}"),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "relationships".to_string(),
            message: format!("Rename error: {e}"),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for RelationshipsTool {
    fn name(&self) -> &str {
        "relationships"
    }
    fn description(&self) -> &str {
        "Track contacts and interaction history. Actions: add_contact, update, search, list, log_interaction."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add_contact", "update", "search", "list", "log_interaction"],
                    "description": "Action to perform"
                },
                "name": { "type": "string", "description": "Contact name" },
                "email": { "type": "string", "description": "Email address" },
                "phone": { "type": "string", "description": "Phone number" },
                "notes": { "type": "string", "description": "Notes about the contact" },
                "id": { "type": "integer", "description": "Contact ID" },
                "kind": { "type": "string", "description": "Interaction type (call, email, meeting, message)" },
                "note": { "type": "string", "description": "Interaction note" },
                "query": { "type": "string", "description": "Search query" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "add_contact" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if name.is_empty() {
                    return Ok(ToolOutput::text("Please provide a contact name."));
                }
                let id = state.next_id;
                state.next_id += 1;
                state.contacts.push(Contact {
                    id,
                    name: name.to_string(),
                    email: args
                        .get("email")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    phone: args
                        .get("phone")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    notes: args
                        .get("notes")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    tags: Vec::new(),
                    interactions: Vec::new(),
                    created_at: Utc::now(),
                });
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!("Added contact '{name}' (#{id}).")))
            }
            "update" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(contact) = state.contacts.iter_mut().find(|c| c.id == id) {
                    if let Some(name) = args.get("name").and_then(|v| v.as_str()) {
                        contact.name = name.to_string();
                    }
                    if let Some(email) = args.get("email").and_then(|v| v.as_str()) {
                        contact.email = Some(email.to_string());
                    }
                    if let Some(phone) = args.get("phone").and_then(|v| v.as_str()) {
                        contact.phone = Some(phone.to_string());
                    }
                    if let Some(notes) = args.get("notes").and_then(|v| v.as_str()) {
                        contact.notes = notes.to_string();
                    }
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!("Updated contact #{id}.")))
                } else {
                    Ok(ToolOutput::text(format!("Contact #{id} not found.")))
                }
            }
            "search" => {
                let query = args
                    .get("query")
                    .and_then(|v| v.as_str())
                    .or_else(|| args.get("name").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .to_lowercase();
                let matches: Vec<String> = state
                    .contacts
                    .iter()
                    .filter(|c| {
                        c.name.to_lowercase().contains(&query)
                            || c.email
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(&query)
                            || c.notes.to_lowercase().contains(&query)
                    })
                    .map(|c| {
                        let email = c.email.as_deref().unwrap_or("N/A");
                        format!(
                            "  #{} — {} ({}) — {} interactions",
                            c.id,
                            c.name,
                            email,
                            c.interactions.len()
                        )
                    })
                    .collect();
                if matches.is_empty() {
                    Ok(ToolOutput::text(format!("No contacts matching '{query}'.")))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Found {}:\n{}",
                        matches.len(),
                        matches.join("\n")
                    )))
                }
            }
            "list" => {
                if state.contacts.is_empty() {
                    return Ok(ToolOutput::text("No contacts yet."));
                }
                let lines: Vec<String> = state
                    .contacts
                    .iter()
                    .map(|c| {
                        let last = c
                            .interactions
                            .last()
                            .map(|i| i.date.format("%Y-%m-%d").to_string())
                            .unwrap_or_else(|| "never".to_string());
                        format!("  #{} — {} (last contact: {})", c.id, c.name, last)
                    })
                    .collect();
                Ok(ToolOutput::text(format!(
                    "Contacts ({}):\n{}",
                    state.contacts.len(),
                    lines.join("\n")
                )))
            }
            "log_interaction" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let kind = args.get("kind").and_then(|v| v.as_str()).unwrap_or("note");
                let note = args.get("note").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(contact) = state.contacts.iter_mut().find(|c| c.id == id) {
                    let contact_name = contact.name.clone();
                    contact.interactions.push(Interaction {
                        date: Utc::now(),
                        kind: kind.to_string(),
                        note: note.to_string(),
                    });
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!(
                        "Logged {kind} interaction for '{contact_name}'."
                    )))
                } else {
                    Ok(ToolOutput::text(format!("Contact #{id} not found.")))
                }
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {action}. Use: add_contact, update, search, list, log_interaction"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_relationships_add_search() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = RelationshipsTool::new(workspace);

        tool.execute(
            json!({"action": "add_contact", "name": "Alice", "email": "alice@example.com"}),
        )
        .await
        .unwrap();
        let result = tool
            .execute(json!({"action": "search", "query": "alice"}))
            .await
            .unwrap();
        assert!(result.content.contains("Alice"));
    }

    #[tokio::test]
    async fn test_relationships_log_interaction() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = RelationshipsTool::new(workspace);

        tool.execute(json!({"action": "add_contact", "name": "Bob"}))
            .await
            .unwrap();
        let result = tool.execute(json!({"action": "log_interaction", "id": 1, "kind": "call", "note": "Discussed project"})).await.unwrap();
        assert!(result.content.contains("Logged call"));
    }

    #[tokio::test]
    async fn test_relationships_list_empty() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = RelationshipsTool::new(workspace);

        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.content.contains("No contacts"));
    }

    #[tokio::test]
    async fn test_relationships_schema() {
        let dir = TempDir::new().unwrap();
        let tool = RelationshipsTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "relationships");
        assert!(tool.parameters_schema().get("properties").is_some());
    }
}
