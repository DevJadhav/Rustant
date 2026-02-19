//! Dev Server tool — start, stop, restart, and monitor development servers.
//!
//! Uses project detection to pick the right command (npm run dev, cargo run, etc.).
//! Manages background process with PID tracking.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::project_detect::{ProjectType, detect_project};
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const TOOL_NAME: &str = "dev_server";

/// State persisted to `.rustant/dev_server/state.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevServerState {
    pid: Option<u32>,
    command: String,
    port: u16,
    started_at: String,
}

pub struct DevServerTool {
    workspace: PathBuf,
}

impl DevServerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("dev_server")
            .join("state.json")
    }

    async fn load_state(&self) -> Option<DevServerState> {
        let path = self.state_path();
        let data = tokio::fs::read_to_string(&path).await.ok()?;
        serde_json::from_str(&data).ok()
    }

    async fn save_state(&self, state: &DevServerState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: format!("Failed to create state dir: {e}"),
                })?;
        }
        let data = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: format!("Failed to serialize state: {e}"),
        })?;
        // Atomic write
        let tmp_path = path.with_extension("tmp");
        tokio::fs::write(&tmp_path, &data)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: format!("Failed to write state: {e}"),
            })?;
        tokio::fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: format!("Failed to rename state file: {e}"),
            })?;
        Ok(())
    }

    async fn clear_state(&self) {
        let _ = tokio::fs::remove_file(self.state_path()).await;
    }
}

#[async_trait]
impl crate::registry::Tool for DevServerTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Start, stop, restart, or check status of a development server. Auto-detects the right command based on project type."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "stop", "restart", "status", "logs"],
                    "description": "The dev server action to perform"
                },
                "port": {
                    "type": "integer",
                    "description": "Port to run on (default: auto-detect from project)"
                },
                "command": {
                    "type": "string",
                    "description": "Override the server command (optional)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        let port = args
            .get("port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16)
            .unwrap_or(3000);

        match action {
            "start" => self.start(port, &args).await,
            "stop" => self.stop().await,
            "restart" => {
                let _ = self.stop().await;
                self.start(port, &args).await
            }
            "status" => self.status().await,
            "logs" => self.logs().await,
            _ => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: format!("Unknown dev_server action: {action}"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

impl DevServerTool {
    async fn start(&self, port: u16, args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
        // Check if already running
        if let Some(state) = self.load_state().await
            && let Some(pid) = state.pid
            && is_process_running(pid)
        {
            return Ok(ToolOutput::text(format!(
                "Dev server already running (PID: {pid}, port: {}, command: {})",
                state.port, state.command
            )));
        }

        let command = if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
            cmd.to_string()
        } else {
            detect_dev_command(&self.workspace, port)
        };

        // Start the process in background
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: "Empty command".to_string(),
            });
        }

        let child = tokio::process::Command::new(parts[0])
            .args(&parts[1..])
            .current_dir(&self.workspace)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: format!("Failed to start dev server: {e}"),
            })?;

        let pid = child.id().unwrap_or(0);

        let state = DevServerState {
            pid: Some(pid),
            command: command.clone(),
            port,
            started_at: chrono::Utc::now().to_rfc3339(),
        };
        self.save_state(&state).await?;

        // Don't await the child — it runs in background
        std::mem::forget(child);

        Ok(ToolOutput::text(format!(
            "Dev server started\n  PID: {pid}\n  Port: {port}\n  Command: {command}\n\n\
             Use 'dev_server stop' to stop, 'dev_server status' to check."
        )))
    }

    async fn stop(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state().await;

        if let Some(state) = state {
            if let Some(pid) = state.pid
                && is_process_running(pid)
            {
                kill_process(pid);
                self.clear_state().await;
                return Ok(ToolOutput::text(format!("Dev server stopped (PID: {pid})")));
            }
            self.clear_state().await;
            Ok(ToolOutput::text(
                "Dev server was not running (cleared stale state)".to_string(),
            ))
        } else {
            Ok(ToolOutput::text("No dev server is running".to_string()))
        }
    }

    async fn status(&self) -> Result<ToolOutput, ToolError> {
        if let Some(state) = self.load_state().await {
            if let Some(pid) = state.pid {
                let running = is_process_running(pid);
                Ok(ToolOutput::text(format!(
                    "Dev server status:\n  Running: {running}\n  PID: {pid}\n  \
                     Port: {}\n  Command: {}\n  Started: {}",
                    state.port, state.command, state.started_at
                )))
            } else {
                Ok(ToolOutput::text(
                    "Dev server state found but no PID recorded".to_string(),
                ))
            }
        } else {
            Ok(ToolOutput::text("No dev server is running".to_string()))
        }
    }

    async fn logs(&self) -> Result<ToolOutput, ToolError> {
        let log_files = [
            ".rustant/dev_server/server.log",
            ".next/trace",
            "npm-debug.log",
        ];

        for log_file in &log_files {
            let path = self.workspace.join(log_file);
            if path.exists() {
                let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
                let lines: Vec<&str> = content.lines().collect();
                let start = lines.len().saturating_sub(100);
                let tail: String = lines[start..].join("\n");
                return Ok(ToolOutput::text(format!(
                    "Last logs from {log_file}:\n\n{tail}"
                )));
            }
        }

        Ok(ToolOutput::text(
            "No log files found. Server output goes to stdout/stderr of the spawned process."
                .to_string(),
        ))
    }
}

fn detect_dev_command(workspace: &std::path::Path, port: u16) -> String {
    let project = detect_project(workspace);
    match project.project_type {
        ProjectType::Node => {
            if workspace.join("next.config.js").exists()
                || workspace.join("next.config.mjs").exists()
                || workspace.join("next.config.ts").exists()
            {
                format!("npx next dev -p {port}")
            } else if workspace.join("vite.config.ts").exists()
                || workspace.join("vite.config.js").exists()
            {
                format!("npx vite --port {port}")
            } else {
                "npm run dev".to_string()
            }
        }
        ProjectType::Rust => "cargo run".to_string(),
        ProjectType::Python => {
            if workspace.join("manage.py").exists() {
                format!("python manage.py runserver 0.0.0.0:{port}")
            } else {
                format!("uvicorn app.main:app --reload --port {port}")
            }
        }
        ProjectType::Go => "go run .".to_string(),
        ProjectType::Ruby => format!("bundle exec rails server -p {port}"),
        _ => "npm run dev".to_string(),
    }
}

fn is_process_running(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

fn kill_process(pid: u32) {
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_status_no_server() {
        let dir = TempDir::new().unwrap();
        let tool = DevServerTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "status"})).await.unwrap();
        assert!(result.content.contains("No dev server"));
    }

    #[tokio::test]
    async fn test_stop_no_server() {
        let dir = TempDir::new().unwrap();
        let tool = DevServerTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "stop"})).await.unwrap();
        assert!(result.content.contains("No dev server"));
    }

    #[tokio::test]
    async fn test_logs_no_files() {
        let dir = TempDir::new().unwrap();
        let tool = DevServerTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "logs"})).await.unwrap();
        assert!(result.content.contains("No log files found"));
    }

    #[test]
    fn test_detect_dev_command_node() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let cmd = detect_dev_command(dir.path(), 3000);
        assert_eq!(cmd, "npm run dev");
    }

    #[test]
    fn test_detect_dev_command_vite() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("vite.config.ts"), "").unwrap();
        let cmd = detect_dev_command(dir.path(), 5173);
        assert!(cmd.contains("vite"));
    }
}
