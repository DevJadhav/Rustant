//! Pomodoro timer tool — focus sessions with DND integration on macOS.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PomodoroSession {
    task: String,
    started_at: DateTime<Utc>,
    duration_mins: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    completed_at: Option<DateTime<Utc>>,
    completed: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PomodoroState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<PomodoroSession>,
    history: Vec<PomodoroSession>,
}

pub struct PomodoroTool {
    workspace: PathBuf,
}

impl PomodoroTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("pomodoro")
            .join("state.json")
    }

    fn load_state(&self) -> PomodoroState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            PomodoroState::default()
        }
    }

    fn save_state(&self, state: &PomodoroState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "pomodoro".to_string(),
                message: format!("Failed to create state dir: {e}"),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "pomodoro".to_string(),
            message: format!("Failed to serialize state: {e}"),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "pomodoro".to_string(),
            message: format!("Failed to write state: {e}"),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "pomodoro".to_string(),
            message: format!("Failed to rename state file: {e}"),
        })?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    fn toggle_dnd(enable: bool) {
        let script = if enable {
            r#"do shell script "defaults -currentHost write com.apple.notificationcenterui doNotDisturb -boolean true && killall NotificationCenter 2>/dev/null || true""#
        } else {
            r#"do shell script "defaults -currentHost write com.apple.notificationcenterui doNotDisturb -boolean false && killall NotificationCenter 2>/dev/null || true""#
        };
        let _ = std::process::Command::new("osascript")
            .args(["-e", script])
            .output();
    }

    #[cfg(not(target_os = "macos"))]
    fn toggle_dnd(_enable: bool) {
        // DND not available on non-macOS
    }
}

#[async_trait]
impl Tool for PomodoroTool {
    fn name(&self) -> &str {
        "pomodoro"
    }

    fn description(&self) -> &str {
        "Pomodoro focus timer with start/stop/status/history. Toggles Do Not Disturb on macOS."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "stop", "status", "history"],
                    "description": "Action to perform"
                },
                "task": {
                    "type": "string",
                    "description": "Task description (for start action)"
                },
                "duration_mins": {
                    "type": "integer",
                    "description": "Focus duration in minutes (default: 25)",
                    "default": 25
                }
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
            "start" => {
                if state.active.is_some() {
                    return Ok(ToolOutput::text(
                        "A pomodoro session is already active. Stop it first.",
                    ));
                }
                let task = args
                    .get("task")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Focus session");
                let duration = args
                    .get("duration_mins")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(25) as u32;

                let session = PomodoroSession {
                    task: task.to_string(),
                    started_at: Utc::now(),
                    duration_mins: duration,
                    completed_at: None,
                    completed: false,
                };
                state.active = Some(session);
                self.save_state(&state)?;
                Self::toggle_dnd(true);

                Ok(ToolOutput::text(format!(
                    "Pomodoro started: '{task}' ({duration} minutes). DND enabled."
                )))
            }
            "stop" => {
                if let Some(mut session) = state.active.take() {
                    session.completed_at = Some(Utc::now());
                    session.completed = true;
                    let elapsed = (Utc::now() - session.started_at).num_minutes();
                    state.history.push(session.clone());
                    // Keep last 100 entries
                    if state.history.len() > 100 {
                        state.history.drain(0..state.history.len() - 100);
                    }
                    self.save_state(&state)?;
                    Self::toggle_dnd(false);

                    Ok(ToolOutput::text(format!(
                        "Pomodoro complete: '{}' after {} minutes. DND disabled.",
                        session.task, elapsed
                    )))
                } else {
                    Ok(ToolOutput::text("No active pomodoro session to stop."))
                }
            }
            "status" => {
                if let Some(ref session) = state.active {
                    let elapsed = (Utc::now() - session.started_at).num_minutes();
                    let remaining = session.duration_mins as i64 - elapsed;
                    Ok(ToolOutput::text(format!(
                        "Active: '{}' — {} min elapsed, {} min remaining",
                        session.task,
                        elapsed,
                        remaining.max(0)
                    )))
                } else {
                    Ok(ToolOutput::text("No active pomodoro session."))
                }
            }
            "history" => {
                if state.history.is_empty() {
                    return Ok(ToolOutput::text("No pomodoro history."));
                }
                let recent: Vec<String> = state
                    .history
                    .iter()
                    .rev()
                    .take(10)
                    .map(|s| {
                        let date = s.started_at.format("%Y-%m-%d %H:%M");
                        format!("  {} — {} ({} min)", date, s.task, s.duration_mins)
                    })
                    .collect();
                Ok(ToolOutput::text(format!(
                    "Recent sessions:\n{}",
                    recent.join("\n")
                )))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {action}. Use: start, stop, status, history"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_pomodoro_start_stop() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = PomodoroTool::new(workspace);

        let result = tool
            .execute(json!({"action": "start", "task": "coding", "duration_mins": 25}))
            .await
            .unwrap();
        assert!(result.content.contains("started"));

        let result = tool.execute(json!({"action": "status"})).await.unwrap();
        assert!(result.content.contains("coding"));

        let result = tool.execute(json!({"action": "stop"})).await.unwrap();
        assert!(result.content.contains("complete") || result.content.contains("Complete"));
    }

    #[tokio::test]
    async fn test_pomodoro_double_start() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = PomodoroTool::new(workspace);

        tool.execute(json!({"action": "start", "task": "task1"}))
            .await
            .unwrap();
        let result = tool
            .execute(json!({"action": "start", "task": "task2"}))
            .await
            .unwrap();
        assert!(result.content.contains("already active"));
    }

    #[tokio::test]
    async fn test_pomodoro_history_empty() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = PomodoroTool::new(workspace);

        let result = tool.execute(json!({"action": "history"})).await.unwrap();
        assert!(result.content.contains("No pomodoro history"));
    }

    #[tokio::test]
    async fn test_pomodoro_schema() {
        let dir = TempDir::new().unwrap();
        let tool = PomodoroTool::new(dir.path().to_path_buf());
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        assert!(schema["properties"]["action"].get("enum").is_some());
    }
}
