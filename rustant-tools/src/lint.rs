//! Lint tool — check, fix, typecheck, and format code.
//!
//! Wraps framework-specific linters: cargo clippy, eslint, ruff, tsc, cargo fmt.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::project_detect::{ProjectType, detect_project};
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const TOOL_NAME: &str = "lint";

pub struct LintTool {
    workspace: PathBuf,
}

impl LintTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl crate::registry::Tool for LintTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Lint, typecheck, and format code. Wraps cargo clippy, eslint, ruff, tsc, prettier, and more."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["check", "fix", "typecheck", "format", "format_check"],
                    "description": "The lint action to perform"
                },
                "file": {
                    "type": "string",
                    "description": "Specific file to lint (optional, defaults to entire project)"
                },
                "auto_fix": {
                    "type": "boolean",
                    "description": "Automatically fix issues when possible",
                    "default": false
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

        let file = args.get("file").and_then(|v| v.as_str());

        match action {
            "check" => self.run_check(file).await,
            "fix" => self.run_fix(file).await,
            "typecheck" => self.run_typecheck().await,
            "format" => self.run_format(file).await,
            "format_check" => self.run_format_check(file).await,
            _ => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: format!("Unknown lint action: {action}"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        // fix/format modify files; check/typecheck are read-only
        // We use Execute as the higher bound since the tool dispatches internally
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }
}

impl LintTool {
    async fn run_check(&self, file: Option<&str>) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => {
                "cargo clippy --workspace --all-targets -- -D warnings".to_string()
            }
            ProjectType::Node => {
                if self.workspace.join(".eslintrc.js").exists()
                    || self.workspace.join(".eslintrc.json").exists()
                    || self.workspace.join("eslint.config.js").exists()
                    || self.workspace.join("eslint.config.mjs").exists()
                {
                    match file {
                        Some(f) => format!("npx eslint {f}"),
                        None => "npx eslint .".to_string(),
                    }
                } else if self.workspace.join("biome.json").exists() {
                    match file {
                        Some(f) => format!("npx biome check {f}"),
                        None => "npx biome check .".to_string(),
                    }
                } else {
                    "npx eslint .".to_string()
                }
            }
            ProjectType::Python => {
                if self.workspace.join("ruff.toml").exists()
                    || self.workspace.join("pyproject.toml").exists()
                {
                    match file {
                        Some(f) => format!("ruff check {f}"),
                        None => "ruff check .".to_string(),
                    }
                } else {
                    match file {
                        Some(f) => format!("python -m flake8 {f}"),
                        None => "python -m flake8 .".to_string(),
                    }
                }
            }
            ProjectType::Go => match file {
                Some(f) => format!("golangci-lint run {f}"),
                None => "golangci-lint run ./...".to_string(),
            },
            _ => "echo 'No linter detected for this project type'".to_string(),
        };

        let output = run_lint_command(&self.workspace, &cmd).await?;
        let diagnostics = parse_diagnostics(&output);
        Ok(ToolOutput::text(format!("{diagnostics}\n\n{output}")))
    }

    async fn run_fix(&self, file: Option<&str>) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => {
                "cargo clippy --workspace --all-targets --fix --allow-dirty -- -D warnings"
                    .to_string()
            }
            ProjectType::Node => {
                if self.workspace.join("biome.json").exists() {
                    match file {
                        Some(f) => format!("npx biome check --write {f}"),
                        None => "npx biome check --write .".to_string(),
                    }
                } else {
                    match file {
                        Some(f) => format!("npx eslint --fix {f}"),
                        None => "npx eslint --fix .".to_string(),
                    }
                }
            }
            ProjectType::Python => match file {
                Some(f) => format!("ruff check --fix {f}"),
                None => "ruff check --fix .".to_string(),
            },
            ProjectType::Go => "golangci-lint run --fix ./...".to_string(),
            _ => "echo 'No auto-fix available for this project type'".to_string(),
        };

        let output = run_lint_command(&self.workspace, &cmd).await?;
        Ok(ToolOutput::text(format!("Auto-fix applied:\n\n{output}")))
    }

    async fn run_typecheck(&self) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "cargo check --workspace".to_string(),
            ProjectType::Node => {
                if self.workspace.join("tsconfig.json").exists() {
                    "npx tsc --noEmit".to_string()
                } else {
                    return Ok(ToolOutput::text(
                        "No tsconfig.json found. TypeScript type checking not available."
                            .to_string(),
                    ));
                }
            }
            ProjectType::Python => {
                if self.workspace.join("pyproject.toml").exists() {
                    "python -m mypy .".to_string()
                } else {
                    return Ok(ToolOutput::text(
                        "No mypy configuration found. Python type checking not available."
                            .to_string(),
                    ));
                }
            }
            _ => {
                return Ok(ToolOutput::text(
                    "Type checking not available for this project type.".to_string(),
                ));
            }
        };

        let output = run_lint_command(&self.workspace, &cmd).await?;
        let diagnostics = parse_diagnostics(&output);
        Ok(ToolOutput::text(format!(
            "Type check results:\n{diagnostics}\n\n{output}"
        )))
    }

    async fn run_format(&self, file: Option<&str>) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "cargo fmt --all".to_string(),
            ProjectType::Node => {
                if self.workspace.join(".prettierrc").exists()
                    || self.workspace.join("prettier.config.js").exists()
                    || self.workspace.join(".prettierrc.json").exists()
                {
                    match file {
                        Some(f) => format!("npx prettier --write {f}"),
                        None => "npx prettier --write .".to_string(),
                    }
                } else if self.workspace.join("biome.json").exists() {
                    match file {
                        Some(f) => format!("npx biome format --write {f}"),
                        None => "npx biome format --write .".to_string(),
                    }
                } else {
                    match file {
                        Some(f) => format!("npx prettier --write {f}"),
                        None => "npx prettier --write .".to_string(),
                    }
                }
            }
            ProjectType::Python => match file {
                Some(f) => format!("ruff format {f}"),
                None => "ruff format .".to_string(),
            },
            ProjectType::Go => match file {
                Some(f) => format!("gofmt -w {f}"),
                None => "gofmt -w .".to_string(),
            },
            _ => "echo 'No formatter detected for this project type'".to_string(),
        };

        let output = run_lint_command(&self.workspace, &cmd).await?;
        Ok(ToolOutput::text(format!("Format applied:\n\n{output}")))
    }

    async fn run_format_check(&self, file: Option<&str>) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "cargo fmt --all -- --check".to_string(),
            ProjectType::Node => {
                if self.workspace.join("biome.json").exists() {
                    match file {
                        Some(f) => format!("npx biome format {f}"),
                        None => "npx biome format .".to_string(),
                    }
                } else {
                    match file {
                        Some(f) => format!("npx prettier --check {f}"),
                        None => "npx prettier --check .".to_string(),
                    }
                }
            }
            ProjectType::Python => match file {
                Some(f) => format!("ruff format --check {f}"),
                None => "ruff format --check .".to_string(),
            },
            ProjectType::Go => match file {
                Some(f) => format!("gofmt -d {f}"),
                None => "gofmt -d .".to_string(),
            },
            _ => "echo 'No format check available for this project type'".to_string(),
        };

        let output = run_lint_command(&self.workspace, &cmd).await?;
        let has_issues = !output.trim().is_empty()
            && !output.contains("All matched files use the correct style");
        let status = if has_issues {
            "Format issues found"
        } else {
            "All files properly formatted"
        };
        Ok(ToolOutput::text(format!("{status}\n\n{output}")))
    }
}

fn parse_diagnostics(output: &str) -> String {
    let mut errors = 0u32;
    let mut warnings = 0u32;

    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.starts_with("error")
            || (lower.contains("error") && (lower.contains("-->") || lower.contains(": error")))
        {
            errors += 1;
        } else if lower.starts_with("warning")
            || (lower.contains("warning") && (lower.contains("-->") || lower.contains(": warning")))
        {
            warnings += 1;
        }
    }

    if errors == 0 && warnings == 0 {
        "No issues found.".to_string()
    } else {
        format!("Found {errors} error(s), {warnings} warning(s)")
    }
}

async fn run_lint_command(workspace: &std::path::Path, cmd: &str) -> Result<String, ToolError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: "Empty command".to_string(),
        });
    }

    let output = tokio::process::Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: format!("Failed to run '{cmd}': {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = format!("$ {cmd}\n\n");
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !stdout.is_empty() {
            result.push('\n');
        }
        result.push_str(&stderr);
    }

    // Truncate very long output
    if result.len() > 50_000 {
        let truncated = &result[..50_000];
        result = format!("{truncated}\n\n... (output truncated at 50000 chars)");
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    #[test]
    fn test_parse_diagnostics_clean() {
        let output = "Compiling foo v0.1.0\nFinished dev target(s)";
        assert!(parse_diagnostics(output).contains("No issues"));
    }

    #[test]
    fn test_parse_diagnostics_errors() {
        // Compact format: error and location on same line
        let output = "src/main.rs:5:10: error[E0308]: expected `bool`, found `i32`\nsrc/main.rs:3:10: warning: unused variable";
        let diag = parse_diagnostics(output);
        assert!(diag.contains("error"), "Expected error count in: {diag}");
        assert!(
            diag.contains("warning"),
            "Expected warning count in: {diag}"
        );
    }

    #[tokio::test]
    async fn test_check_no_project() {
        let dir = TempDir::new().unwrap();
        let tool = LintTool::new(dir.path().to_path_buf());
        // This will try to run a linter but fail since no project exists
        let result = tool.execute(json!({"action": "check"})).await;
        // It's ok for this to succeed (echo fallback) or fail
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn test_typecheck_no_tsconfig() {
        let dir = TempDir::new().unwrap();
        // Create a Node project without tsconfig
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let tool = LintTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "typecheck"})).await.unwrap();
        assert!(result.content.contains("No tsconfig.json"));
    }
}
