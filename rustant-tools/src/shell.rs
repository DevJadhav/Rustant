//! Shell command execution tool with streaming output support.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{ProgressUpdate, RiskLevel, ToolOutput};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Execute shell commands within the workspace.
///
/// Supports optional streaming of stdout/stderr lines via a progress channel.
pub struct ShellExecTool {
    workspace: PathBuf,
    /// Optional channel for streaming progress updates (shell output lines).
    progress_tx: Option<mpsc::UnboundedSender<ProgressUpdate>>,
}

impl ShellExecTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            progress_tx: None,
        }
    }

    /// Create a shell tool with a progress sender for streaming output.
    pub fn with_progress(workspace: PathBuf, tx: mpsc::UnboundedSender<ProgressUpdate>) -> Self {
        Self {
            workspace,
            progress_tx: Some(tx),
        }
    }
}

#[async_trait]
impl Tool for ShellExecTool {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "Execute a shell command in the workspace directory. Returns stdout, stderr, and exit code."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Working directory (relative to workspace). Defaults to workspace root."
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let command = args["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "shell_exec".into(),
                reason: "'command' parameter is required".into(),
            })?;

        let working_dir = if let Some(dir) = args["working_dir"].as_str() {
            self.workspace.join(dir)
        } else {
            self.workspace.clone()
        };

        debug!(command = command, cwd = %working_dir.display(), "Executing shell command");

        // If we have a progress sender, stream output line by line
        if let Some(ref tx) = self.progress_tx {
            self.execute_streaming(command, &working_dir, tx).await
        } else {
            self.execute_buffered(command, &working_dir).await
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }
}

impl ShellExecTool {
    /// Execute a command with streaming output via the progress channel.
    async fn execute_streaming(
        &self,
        command: &str,
        working_dir: &PathBuf,
        tx: &mpsc::UnboundedSender<ProgressUpdate>,
    ) -> Result<ToolOutput, ToolError> {
        use tokio::process::Command;

        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "shell_exec".into(),
                message: format!("Failed to execute command: {}", e),
            })?;

        // Send initial progress
        let _ = tx.send(ProgressUpdate::ToolProgress {
            tool: "shell_exec".into(),
            stage: format!("running: {}", truncate_cmd(command, 50)),
            percent: None,
        });

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        let tx_stdout = tx.clone();
        let tx_stderr = tx.clone();

        // Spawn tasks to read stdout and stderr concurrently
        let stdout_task = tokio::spawn(async move {
            let mut lines = Vec::new();
            if let Some(pipe) = stdout_pipe {
                let reader = BufReader::new(pipe);
                let mut line_stream = reader.lines();
                while let Ok(Some(line)) = line_stream.next_line().await {
                    let _ = tx_stdout.send(ProgressUpdate::ShellOutput {
                        line: line.clone(),
                        is_stderr: false,
                    });
                    lines.push(line);
                }
            }
            lines
        });

        let stderr_task = tokio::spawn(async move {
            let mut lines = Vec::new();
            if let Some(pipe) = stderr_pipe {
                let reader = BufReader::new(pipe);
                let mut line_stream = reader.lines();
                while let Ok(Some(line)) = line_stream.next_line().await {
                    let _ = tx_stderr.send(ProgressUpdate::ShellOutput {
                        line: line.clone(),
                        is_stderr: true,
                    });
                    lines.push(line);
                }
            }
            lines
        });

        // Wait for the process to complete
        let status = child.wait().await.map_err(|e| ToolError::ExecutionFailed {
            name: "shell_exec".into(),
            message: format!("Failed to wait for command: {}", e),
        })?;

        // Collect output from tasks
        if let Ok(lines) = stdout_task.await {
            stdout_lines = lines;
        }
        if let Ok(lines) = stderr_task.await {
            stderr_lines = lines;
        }

        let exit_code = status.code().unwrap_or(-1);
        let stdout = stdout_lines.join("\n");
        let stderr = stderr_lines.join("\n");

        let result = format!(
            "Exit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
            exit_code,
            if stdout.is_empty() { "(empty)" } else { &stdout },
            if stderr.is_empty() { "(empty)" } else { &stderr }
        );

        if exit_code != 0 {
            warn!(
                command = command,
                exit_code, "Command exited with non-zero status"
            );
        }

        Ok(ToolOutput::text(result))
    }

    /// Execute a command with buffered output (no streaming).
    async fn execute_buffered(
        &self,
        command: &str,
        working_dir: &PathBuf,
    ) -> Result<ToolOutput, ToolError> {
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(working_dir)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "shell_exec".into(),
                message: format!("Failed to execute command: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let result = format!(
            "Exit code: {}\n\n--- stdout ---\n{}\n--- stderr ---\n{}",
            exit_code,
            if stdout.is_empty() {
                "(empty)"
            } else {
                &stdout
            },
            if stderr.is_empty() {
                "(empty)"
            } else {
                &stderr
            }
        );

        if exit_code != 0 {
            warn!(
                command = command,
                exit_code, "Command exited with non-zero status"
            );
        }

        Ok(ToolOutput::text(result))
    }
}

/// Truncate a command string for display.
fn truncate_cmd(cmd: &str, max: usize) -> String {
    if cmd.len() <= max {
        cmd.to_string()
    } else {
        format!("{}..", &cmd[..max.saturating_sub(2)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("test.txt"), "hello world").unwrap();
        dir
    }

    #[tokio::test]
    async fn test_shell_exec_basic() {
        let dir = setup_workspace();
        let tool = ShellExecTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"command": "echo hello"}))
            .await
            .unwrap();

        assert!(result.content.contains("hello"));
        assert!(result.content.contains("Exit code: 0"));
    }

    #[tokio::test]
    async fn test_shell_exec_with_cwd() {
        let dir = setup_workspace();
        std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("subdir/file.txt"), "sub content").unwrap();

        let tool = ShellExecTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "command": "cat file.txt",
                "working_dir": "subdir"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("sub content"));
    }

    #[tokio::test]
    async fn test_shell_exec_nonzero_exit() {
        let dir = setup_workspace();
        let tool = ShellExecTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"command": "exit 42"}))
            .await
            .unwrap();

        assert!(result.content.contains("Exit code: 42"));
    }

    #[tokio::test]
    async fn test_shell_exec_stderr() {
        let dir = setup_workspace();
        let tool = ShellExecTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"command": "echo error >&2"}))
            .await
            .unwrap();

        assert!(result.content.contains("error"));
        assert!(result.content.contains("stderr"));
    }

    #[tokio::test]
    async fn test_shell_exec_missing_command() {
        let dir = setup_workspace();
        let tool = ShellExecTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, .. } => assert_eq!(name, "shell_exec"),
            e => panic!("Expected InvalidArguments, got: {:?}", e),
        }
    }

    #[test]
    fn test_shell_exec_properties() {
        let tool = ShellExecTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "shell_exec");
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
        assert_eq!(tool.timeout(), Duration::from_secs(120));
    }

    #[tokio::test]
    async fn test_shell_exec_streaming() {
        let dir = setup_workspace();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tool = ShellExecTool::with_progress(dir.path().to_path_buf(), tx);

        let result = tool
            .execute(serde_json::json!({"command": "echo line1 && echo line2"}))
            .await
            .unwrap();

        assert!(result.content.contains("line1"));
        assert!(result.content.contains("line2"));
        assert!(result.content.contains("Exit code: 0"));

        // Should have received progress updates
        let mut progress_count = 0;
        while let Ok(update) = rx.try_recv() {
            progress_count += 1;
            match update {
                ProgressUpdate::ToolProgress { tool, .. } => {
                    assert_eq!(tool, "shell_exec");
                }
                ProgressUpdate::ShellOutput { is_stderr, .. } => {
                    assert!(!is_stderr);
                }
                _ => {}
            }
        }
        // At least the initial ToolProgress + 2 stdout lines
        assert!(progress_count >= 3, "Expected at least 3 progress updates, got {}", progress_count);
    }

    #[tokio::test]
    async fn test_shell_exec_streaming_stderr() {
        let dir = setup_workspace();
        let (tx, mut rx) = mpsc::unbounded_channel();
        let tool = ShellExecTool::with_progress(dir.path().to_path_buf(), tx);

        let result = tool
            .execute(serde_json::json!({"command": "echo err >&2"}))
            .await
            .unwrap();

        assert!(result.content.contains("err"));

        let mut has_stderr = false;
        while let Ok(update) = rx.try_recv() {
            if let ProgressUpdate::ShellOutput { is_stderr, .. } = update {
                if is_stderr {
                    has_stderr = true;
                }
            }
        }
        assert!(has_stderr, "Expected at least one stderr progress update");
    }

    #[test]
    fn test_truncate_cmd() {
        assert_eq!(truncate_cmd("echo hello", 20), "echo hello");
        assert_eq!(truncate_cmd("a very long command that should be truncated", 20), "a very long comman..");
    }
}
