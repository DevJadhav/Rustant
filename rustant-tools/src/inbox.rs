//! Inbox tool — quick capture for tasks, ideas, and notes.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InboxItem {
    id: usize,
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    created_at: DateTime<Utc>,
    #[serde(default)]
    done: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct InboxState {
    items: Vec<InboxItem>,
    next_id: usize,
}

pub struct InboxTool {
    workspace: PathBuf,
}

impl InboxTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("inbox")
            .join("items.json")
    }

    fn load_state(&self) -> InboxState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            InboxState {
                items: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &InboxState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "inbox".to_string(),
                message: format!("Failed to create dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "inbox".to_string(),
            message: format!("Serialize error: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "inbox".to_string(),
            message: format!("Write error: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "inbox".to_string(),
            message: format!("Rename error: {}", e),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for InboxTool {
    fn name(&self) -> &str {
        "inbox"
    }
    fn description(&self) -> &str {
        "Quick capture inbox for tasks, ideas, and notes. Actions: add, list, search, clear, tag, done."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["add", "list", "search", "clear", "tag", "done"],
                    "description": "Action to perform"
                },
                "text": { "type": "string", "description": "Item text (for add/search)" },
                "id": { "type": "integer", "description": "Item ID (for tag/done)" },
                "tag": { "type": "string", "description": "Tag name (for tag action)" }
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
            "add" => {
                let text = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if text.is_empty() {
                    return Ok(ToolOutput::text("Please provide text for the inbox item."));
                }
                let id = state.next_id;
                state.next_id += 1;
                state.items.push(InboxItem {
                    id,
                    text: text.to_string(),
                    tags: Vec::new(),
                    created_at: Utc::now(),
                    done: false,
                });
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!("Added to inbox (#{}).", id)))
            }
            "list" => {
                let active: Vec<&InboxItem> = state.items.iter().filter(|i| !i.done).collect();
                if active.is_empty() {
                    return Ok(ToolOutput::text("Inbox is empty."));
                }
                let lines: Vec<String> = active
                    .iter()
                    .map(|i| {
                        let tags = if i.tags.is_empty() {
                            String::new()
                        } else {
                            format!(" [{}]", i.tags.join(", "))
                        };
                        format!("  #{} — {}{}", i.id, i.text, tags)
                    })
                    .collect();
                Ok(ToolOutput::text(format!(
                    "Inbox ({} items):\n{}",
                    active.len(),
                    lines.join("\n")
                )))
            }
            "search" => {
                let query = args
                    .get("text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                let matches: Vec<String> = state
                    .items
                    .iter()
                    .filter(|i| {
                        i.text.to_lowercase().contains(&query)
                            || i.tags.iter().any(|t| t.to_lowercase().contains(&query))
                    })
                    .map(|i| {
                        format!(
                            "  #{} — {} {}",
                            i.id,
                            i.text,
                            if i.done { "(done)" } else { "" }
                        )
                    })
                    .collect();
                if matches.is_empty() {
                    Ok(ToolOutput::text(format!("No items matching '{}'.", query)))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Found {} items:\n{}",
                        matches.len(),
                        matches.join("\n")
                    )))
                }
            }
            "tag" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let tag = args.get("tag").and_then(|v| v.as_str()).unwrap_or("");
                if tag.is_empty() {
                    return Ok(ToolOutput::text("Please provide a tag."));
                }
                if let Some(item) = state.items.iter_mut().find(|i| i.id == id) {
                    if !item.tags.contains(&tag.to_string()) {
                        item.tags.push(tag.to_string());
                    }
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!("Tagged #{} with '{}'.", id, tag)))
                } else {
                    Ok(ToolOutput::text(format!("Item #{} not found.", id)))
                }
            }
            "done" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                if let Some(item) = state.items.iter_mut().find(|i| i.id == id) {
                    item.done = true;
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!("Marked #{} as done.", id)))
                } else {
                    Ok(ToolOutput::text(format!("Item #{} not found.", id)))
                }
            }
            "clear" => {
                let count = state.items.iter().filter(|i| i.done).count();
                state.items.retain(|i| !i.done);
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Cleared {} completed items.",
                    count
                )))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: add, list, search, clear, tag, done",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_inbox_add_list() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = InboxTool::new(workspace);

        tool.execute(json!({"action": "add", "text": "Buy groceries"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "add", "text": "Call dentist"}))
            .await
            .unwrap();

        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.content.contains("Buy groceries"));
        assert!(result.content.contains("Call dentist"));
        assert!(result.content.contains("2 items"));
    }

    #[tokio::test]
    async fn test_inbox_done_clear() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = InboxTool::new(workspace);

        tool.execute(json!({"action": "add", "text": "Task 1"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "done", "id": 1}))
            .await
            .unwrap();

        let result = tool.execute(json!({"action": "clear"})).await.unwrap();
        assert!(result.content.contains("Cleared 1"));
    }

    #[tokio::test]
    async fn test_inbox_search() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = InboxTool::new(workspace);

        tool.execute(json!({"action": "add", "text": "Review PR #42"}))
            .await
            .unwrap();
        tool.execute(json!({"action": "add", "text": "Fix bug in parser"}))
            .await
            .unwrap();

        let result = tool
            .execute(json!({"action": "search", "text": "PR"}))
            .await
            .unwrap();
        assert!(result.content.contains("Review PR"));
        assert!(!result.content.contains("parser"));
    }

    #[tokio::test]
    async fn test_inbox_schema() {
        let dir = TempDir::new().unwrap();
        let tool = InboxTool::new(dir.path().to_path_buf());
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }
}
