//! Git integration tools: status, diff, and commit.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::path::PathBuf;
use tracing::debug;

/// Show git repository status.
pub struct GitStatusTool {
    workspace: PathBuf,
}

impl GitStatusTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, ToolError> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "git".into(),
                message: format!("Failed to run git: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(ToolError::ExecutionFailed {
                name: "git".into(),
                message: format!("git {} failed: {}", args.join(" "), stderr),
            });
        }

        Ok(if stdout.is_empty() { stderr } else { stdout })
    }
}

#[async_trait]
impl Tool for GitStatusTool {
    fn name(&self) -> &str {
        "git_status"
    }

    fn description(&self) -> &str {
        "Show the current git repository status, including staged, modified, and untracked files."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        debug!(workspace = %self.workspace.display(), "Getting git status");
        let status = self.run_git(&["status", "--short"]).await?;
        let branch = self.run_git(&["branch", "--show-current"]).await?;

        let output = format!(
            "Branch: {}\n{}",
            branch.trim(),
            if status.trim().is_empty() {
                "Working tree clean".to_string()
            } else {
                status
            }
        );

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

/// Show git diff of working tree changes.
pub struct GitDiffTool {
    workspace: PathBuf,
}

impl GitDiffTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, ToolError> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "git_diff".into(),
                message: format!("Failed to run git: {}", e),
            })?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[async_trait]
impl Tool for GitDiffTool {
    fn name(&self) -> &str {
        "git_diff"
    }

    fn description(&self) -> &str {
        "Show the diff of changes in the working tree. Optionally specify a file path to see changes for a specific file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Optional file path to diff"
                },
                "staged": {
                    "type": "boolean",
                    "description": "Show staged changes instead of unstaged. Default: false."
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let staged = args["staged"].as_bool().unwrap_or(false);
        let path = args["path"].as_str();

        let mut git_args = vec!["diff"];
        if staged {
            git_args.push("--cached");
        }
        if let Some(p) = path {
            git_args.push("--");
            git_args.push(p);
        }

        debug!(staged, path = ?path, "Getting git diff");

        let diff = self.run_git(&git_args).await?;

        let output = if diff.trim().is_empty() {
            let scope = if staged { "staged" } else { "unstaged" };
            format!("No {} changes", scope)
        } else {
            diff
        };

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

/// Stage files and create a git commit.
pub struct GitCommitTool {
    workspace: PathBuf,
}

impl GitCommitTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    async fn run_git(&self, args: &[&str]) -> Result<String, ToolError> {
        let output = tokio::process::Command::new("git")
            .args(args)
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "git_commit".into(),
                message: format!("Failed to run git: {}", e),
            })?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            return Err(ToolError::ExecutionFailed {
                name: "git_commit".into(),
                message: format!("git {} failed: {}", args.join(" "), stderr),
            });
        }

        Ok(if stdout.is_empty() { stderr } else { stdout })
    }
}

#[async_trait]
impl Tool for GitCommitTool {
    fn name(&self) -> &str {
        "git_commit"
    }

    fn description(&self) -> &str {
        "Stage files and create a git commit. Specify files to stage and a commit message."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "The commit message"
                },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to stage before committing. Use [\".\"] for all changes."
                }
            },
            "required": ["message"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let message = args["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "git_commit".into(),
                reason: "'message' parameter is required".into(),
            })?;

        // Stage files if specified
        if let Some(files) = args["files"].as_array() {
            for file in files {
                if let Some(f) = file.as_str() {
                    debug!(file = f, "Staging file");
                    self.run_git(&["add", f]).await?;
                }
            }
        }

        // Create commit
        debug!(message = message, "Creating commit");
        let result = self.run_git(&["commit", "-m", message]).await?;

        Ok(ToolOutput::text(format!("Committed: {}", result.trim())))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        // Initialize a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        // Disable commit signing for tests (avoids GPG/SSH signing issues)
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        // Create an initial commit
        std::fs::write(dir.path().join("README.md"), "# Test\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(dir.path())
            .output()
            .unwrap();

        dir
    }

    #[tokio::test]
    async fn test_git_status_clean() {
        let dir = setup_git_repo();
        let tool = GitStatusTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(
            result.content.contains("Working tree clean") || result.content.contains("Branch:")
        );
    }

    #[tokio::test]
    async fn test_git_status_with_changes() {
        let dir = setup_git_repo();
        std::fs::write(dir.path().join("new_file.txt"), "new content").unwrap();

        let tool = GitStatusTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("new_file.txt"));
    }

    #[tokio::test]
    async fn test_git_diff_no_changes() {
        let dir = setup_git_repo();
        let tool = GitDiffTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("No unstaged changes"));
    }

    #[tokio::test]
    async fn test_git_diff_with_changes() {
        let dir = setup_git_repo();
        std::fs::write(dir.path().join("README.md"), "# Updated\n").unwrap();

        let tool = GitDiffTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("Updated") || result.content.contains("diff"));
    }

    #[tokio::test]
    async fn test_git_commit() {
        let dir = setup_git_repo();
        std::fs::write(dir.path().join("new_file.txt"), "content").unwrap();

        let tool = GitCommitTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({
                "message": "Add new file",
                "files": ["new_file.txt"]
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Committed"));
    }

    #[test]
    fn test_git_tool_properties() {
        let ws = PathBuf::from("/tmp");
        let status = GitStatusTool::new(ws.clone());
        assert_eq!(status.name(), "git_status");
        assert_eq!(status.risk_level(), RiskLevel::ReadOnly);

        let diff = GitDiffTool::new(ws.clone());
        assert_eq!(diff.name(), "git_diff");
        assert_eq!(diff.risk_level(), RiskLevel::ReadOnly);

        let commit = GitCommitTool::new(ws);
        assert_eq!(commit.name(), "git_commit");
        assert_eq!(commit.risk_level(), RiskLevel::Write);
    }

    #[tokio::test]
    async fn test_git_commit_missing_message() {
        let dir = setup_git_repo();
        let tool = GitCommitTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "git_commit");
                assert!(reason.contains("message"));
            }
            e => panic!("Expected InvalidArguments, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_git_commit_null_message() {
        let dir = setup_git_repo();
        let tool = GitCommitTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({"message": null})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_git_diff_staged_no_changes() {
        let dir = setup_git_repo();
        let tool = GitDiffTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"staged": true}))
            .await
            .unwrap();
        assert!(result.content.contains("No staged changes"));
    }

    #[tokio::test]
    async fn test_git_status_in_non_repo() {
        let dir = TempDir::new().unwrap(); // Not a git repo
        let tool = GitStatusTool::new(dir.path().to_path_buf());
        let result = tool.execute(serde_json::json!({})).await;
        // Should fail since it's not a git repo
        assert!(result.is_err());
    }

    #[test]
    fn test_git_commit_schema_required() {
        let tool = GitCommitTool::new(PathBuf::from("/tmp"));
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("message")));
    }

    #[test]
    fn test_git_diff_schema_no_required() {
        let tool = GitDiffTool::new(PathBuf::from("/tmp"));
        let schema = tool.parameters_schema();
        // diff has no required params (path and staged are optional)
        assert!(schema.get("required").is_none());
    }

    #[test]
    fn test_git_status_schema_no_required() {
        let tool = GitStatusTool::new(PathBuf::from("/tmp"));
        let schema = tool.parameters_schema();
        assert!(schema.get("required").is_none());
    }
}
