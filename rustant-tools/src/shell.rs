//! Shell command execution tool.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, warn};

/// Execute shell commands within the workspace.
pub struct ShellExecTool {
    workspace: PathBuf,
}

impl ShellExecTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
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

        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&working_dir)
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

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
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
}
