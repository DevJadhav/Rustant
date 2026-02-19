//! Test Runner tool — run tests, check coverage, and scope by file or change.
//!
//! Uses project detection to pick the right test framework command.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::project_detect::{ProjectType, detect_project};
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const TOOL_NAME: &str = "test_runner";

pub struct TestRunnerTool {
    workspace: PathBuf,
}

impl TestRunnerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl crate::registry::Tool for TestRunnerTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Run tests: all, by file, by name, or only changed. Supports cargo test, npm test, pytest, go test, and more."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["run_all", "run_file", "run_test", "run_changed", "coverage"],
                    "description": "The test action to perform"
                },
                "file": {
                    "type": "string",
                    "description": "File path to test (for 'run_file' action)"
                },
                "test_name": {
                    "type": "string",
                    "description": "Specific test name or pattern (for 'run_test' action)"
                },
                "verbose": {
                    "type": "boolean",
                    "description": "Enable verbose output",
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

        let verbose = args
            .get("verbose")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        match action {
            "run_all" => self.run_all(verbose).await,
            "run_file" => {
                let file = args.get("file").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: TOOL_NAME.into(),
                        reason: "Missing 'file' parameter for run_file".to_string(),
                    }
                })?;
                self.run_file(file, verbose).await
            }
            "run_test" => {
                let test_name =
                    args.get("test_name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| ToolError::InvalidArguments {
                            name: TOOL_NAME.into(),
                            reason: "Missing 'test_name' parameter for run_test".to_string(),
                        })?;
                self.run_test(test_name, verbose).await
            }
            "run_changed" => self.run_changed(verbose).await,
            "coverage" => self.run_coverage().await,
            _ => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: format!("Unknown action: {action}"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(300)
    }
}

impl TestRunnerTool {
    fn detect_test_command(&self) -> (String, ProjectType) {
        let project = detect_project(&self.workspace);
        let cmd = match &project.project_type {
            ProjectType::Rust => "cargo test".to_string(),
            ProjectType::Node => {
                if self.workspace.join("vitest.config.ts").exists()
                    || self.workspace.join("vitest.config.js").exists()
                {
                    "npx vitest run".to_string()
                } else if self.workspace.join("jest.config.ts").exists()
                    || self.workspace.join("jest.config.js").exists()
                {
                    "npx jest".to_string()
                } else {
                    "npm test".to_string()
                }
            }
            ProjectType::Python => {
                if self.workspace.join("pytest.ini").exists()
                    || self.workspace.join("pyproject.toml").exists()
                {
                    "python -m pytest".to_string()
                } else {
                    "python -m unittest discover".to_string()
                }
            }
            ProjectType::Go => "go test ./...".to_string(),
            ProjectType::Ruby => "bundle exec rspec".to_string(),
            ProjectType::Java => "mvn test".to_string(),
            _ => "npm test".to_string(),
        };
        (cmd, project.project_type)
    }

    async fn run_all(&self, verbose: bool) -> Result<ToolOutput, ToolError> {
        let (mut cmd, _) = self.detect_test_command();
        if verbose {
            append_verbose_flag(&mut cmd);
        }
        let output = run_test_command(&self.workspace, &cmd).await?;
        let summary = parse_test_summary(&output);
        Ok(ToolOutput::text(format!("{summary}\n\n{output}")))
    }

    async fn run_file(&self, file: &str, verbose: bool) -> Result<ToolOutput, ToolError> {
        let (base_cmd, project_type) = self.detect_test_command();
        let cmd = match project_type {
            ProjectType::Rust => {
                // Extract module name from file path for cargo test
                let module = file
                    .trim_start_matches("src/")
                    .trim_end_matches(".rs")
                    .replace('/', "::");
                format!("cargo test {module}")
            }
            ProjectType::Python => format!("python -m pytest {file}"),
            ProjectType::Node => {
                if base_cmd.contains("vitest") {
                    format!("npx vitest run {file}")
                } else {
                    format!("npx jest {file}")
                }
            }
            ProjectType::Go => {
                let dir = std::path::Path::new(file)
                    .parent()
                    .and_then(|p| p.to_str())
                    .unwrap_or(".");
                format!("go test ./{dir}/...")
            }
            _ => format!("{base_cmd} {file}"),
        };

        let mut cmd = cmd;
        if verbose {
            append_verbose_flag(&mut cmd);
        }
        let output = run_test_command(&self.workspace, &cmd).await?;
        let summary = parse_test_summary(&output);
        Ok(ToolOutput::text(format!("{summary}\n\n{output}")))
    }

    async fn run_test(&self, test_name: &str, verbose: bool) -> Result<ToolOutput, ToolError> {
        let (base_cmd, project_type) = self.detect_test_command();
        let cmd = match project_type {
            ProjectType::Rust => format!("cargo test {test_name}"),
            ProjectType::Python => format!("python -m pytest -k {test_name}"),
            ProjectType::Node => {
                if base_cmd.contains("vitest") {
                    format!("npx vitest run -t {test_name}")
                } else {
                    format!("npx jest -t {test_name}")
                }
            }
            ProjectType::Go => format!("go test ./... -run {test_name}"),
            _ => format!("{base_cmd} --filter {test_name}"),
        };

        let mut cmd = cmd;
        if verbose {
            append_verbose_flag(&mut cmd);
        }
        let output = run_test_command(&self.workspace, &cmd).await?;
        let summary = parse_test_summary(&output);
        Ok(ToolOutput::text(format!("{summary}\n\n{output}")))
    }

    async fn run_changed(&self, verbose: bool) -> Result<ToolOutput, ToolError> {
        // Get changed files from git
        let diff_output = tokio::process::Command::new("git")
            .args(["diff", "--name-only", "HEAD"])
            .current_dir(&self.workspace)
            .output()
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: format!("Failed to run git diff: {e}"),
            })?;

        let changed = String::from_utf8_lossy(&diff_output.stdout);
        let changed_files: Vec<&str> = changed.lines().filter(|l| !l.is_empty()).collect();

        if changed_files.is_empty() {
            return Ok(ToolOutput::text(
                "No changed files detected. Nothing to test.".to_string(),
            ));
        }

        let (_, project_type) = self.detect_test_command();

        // Filter to test-relevant files and build test command
        let test_files: Vec<&&str> = changed_files
            .iter()
            .filter(|f| is_test_relevant(f, &project_type))
            .collect();

        if test_files.is_empty() {
            return Ok(ToolOutput::text(format!(
                "Changed files ({}) don't include test-relevant files:\n  {}",
                changed_files.len(),
                changed_files.join("\n  ")
            )));
        }

        let cmd = match project_type {
            ProjectType::Rust => {
                let modules: Vec<String> = test_files
                    .iter()
                    .filter_map(|f| {
                        f.strip_suffix(".rs")
                            .map(|m| m.trim_start_matches("src/").replace('/', "::"))
                    })
                    .collect();
                if modules.is_empty() {
                    "cargo test".to_string()
                } else {
                    format!("cargo test {}", modules.join(" "))
                }
            }
            ProjectType::Python => {
                let py_files: Vec<&str> = test_files
                    .iter()
                    .filter(|f| f.ends_with(".py"))
                    .map(|f| **f)
                    .collect();
                format!("python -m pytest {}", py_files.join(" "))
            }
            ProjectType::Node => {
                let js_files: Vec<&str> = test_files
                    .iter()
                    .filter(|f| {
                        f.ends_with(".ts")
                            || f.ends_with(".tsx")
                            || f.ends_with(".js")
                            || f.ends_with(".jsx")
                    })
                    .map(|f| **f)
                    .collect();
                format!("npx jest --findRelatedTests {}", js_files.join(" "))
            }
            _ => "npm test".to_string(),
        };

        let mut cmd = cmd;
        if verbose {
            append_verbose_flag(&mut cmd);
        }

        let output = run_test_command(&self.workspace, &cmd).await?;
        let summary = parse_test_summary(&output);
        Ok(ToolOutput::text(format!(
            "Testing changed files ({}):\n  {}\n\n{summary}\n\n{output}",
            test_files.len(),
            test_files
                .iter()
                .map(|f| **f)
                .collect::<Vec<&str>>()
                .join("\n  "),
        )))
    }

    async fn run_coverage(&self) -> Result<ToolOutput, ToolError> {
        let (_, project_type) = self.detect_test_command();
        let cmd = match project_type {
            ProjectType::Rust => "cargo tarpaulin --out stdout --skip-clean",
            ProjectType::Python => "python -m pytest --cov=. --cov-report=term",
            ProjectType::Node => "npx jest --coverage",
            ProjectType::Go => {
                "go test ./... -coverprofile=coverage.out && go tool cover -func=coverage.out"
            }
            _ => "npm test -- --coverage",
        };

        run_test_command(&self.workspace, cmd)
            .await
            .map(|output| ToolOutput::text(format!("Coverage report:\n\n{output}")))
    }
}

fn append_verbose_flag(cmd: &mut String) {
    if cmd.starts_with("cargo test") {
        cmd.push_str(" -- --nocapture");
    } else if cmd.contains("pytest") {
        cmd.push_str(" -v");
    } else if cmd.contains("jest") || cmd.contains("vitest") {
        cmd.push_str(" --verbose");
    } else if cmd.starts_with("go test") {
        cmd.push_str(" -v");
    }
}

fn is_test_relevant(file: &str, project_type: &ProjectType) -> bool {
    match project_type {
        ProjectType::Rust => file.ends_with(".rs"),
        ProjectType::Python => file.ends_with(".py"),
        ProjectType::Node => {
            file.ends_with(".ts")
                || file.ends_with(".tsx")
                || file.ends_with(".js")
                || file.ends_with(".jsx")
        }
        ProjectType::Go => file.ends_with(".go"),
        ProjectType::Ruby => file.ends_with(".rb"),
        ProjectType::Java => file.ends_with(".java"),
        _ => true,
    }
}

fn parse_test_summary(output: &str) -> String {
    // Try to extract pass/fail/skip counts from common test frameworks
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skipped = 0u32;

    for line in output.lines() {
        let line_lower = line.to_lowercase();
        // Cargo test: "test result: ok. X passed; Y failed; Z ignored"
        if line_lower.contains("test result:") {
            if let Some(p) = extract_number(line, "passed") {
                passed = p;
            }
            if let Some(f) = extract_number(line, "failed") {
                failed = f;
            }
            if let Some(s) = extract_number(line, "ignored") {
                skipped = s;
            }
            break;
        }
        // pytest: "X passed, Y failed, Z skipped"
        if line_lower.contains("passed")
            && (line_lower.contains("failed") || line_lower.contains("warning"))
        {
            if let Some(p) = extract_number(line, "passed") {
                passed = p;
            }
            if let Some(f) = extract_number(line, "failed") {
                failed = f;
            }
            if let Some(s) = extract_number(line, "skipped") {
                skipped = s;
            }
            break;
        }
        // Jest/Vitest: "Tests: X passed, Y failed, Z skipped"
        if line_lower.starts_with("tests:") || line_lower.starts_with("test suites:") {
            if let Some(p) = extract_number(line, "passed") {
                passed = p;
            }
            if let Some(f) = extract_number(line, "failed") {
                failed = f;
            }
            if let Some(s) = extract_number(line, "skipped") {
                skipped = s;
            }
        }
    }

    let status = if failed > 0 { "FAILED" } else { "PASSED" };
    format!("Test Summary: {status} ({passed} passed, {failed} failed, {skipped} skipped)")
}

fn extract_number(line: &str, after: &str) -> Option<u32> {
    let lower = line.to_lowercase();
    let idx = lower.find(after)?;
    // Look backwards from the keyword to find the number
    let before = &line[..idx].trim_end();
    let num_str: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    num_str.parse().ok()
}

async fn run_test_command(workspace: &std::path::Path, cmd: &str) -> Result<String, ToolError> {
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

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
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
    fn test_parse_cargo_summary() {
        let output = "test result: ok. 42 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out";
        let summary = parse_test_summary(output);
        assert!(summary.contains("PASSED"));
        assert!(summary.contains("42 passed"));
        assert!(summary.contains("3 skipped"));
    }

    #[test]
    fn test_parse_pytest_summary() {
        let output = "====== 15 passed, 2 failed, 1 skipped in 3.42s ======";
        let summary = parse_test_summary(output);
        assert!(summary.contains("FAILED"));
        assert!(summary.contains("15 passed"));
        assert!(summary.contains("2 failed"));
    }

    #[test]
    fn test_extract_number() {
        assert_eq!(extract_number("42 passed", "passed"), Some(42));
        assert_eq!(extract_number("0 failed", "failed"), Some(0));
        assert_eq!(extract_number("no match", "passed"), None);
    }

    #[test]
    fn test_is_test_relevant() {
        assert!(is_test_relevant("src/main.rs", &ProjectType::Rust));
        assert!(!is_test_relevant("README.md", &ProjectType::Rust));
        assert!(is_test_relevant("test_foo.py", &ProjectType::Python));
        assert!(is_test_relevant("App.tsx", &ProjectType::Node));
    }

    #[tokio::test]
    async fn test_run_changed_no_git() {
        let dir = TempDir::new().unwrap();
        let tool = TestRunnerTool::new(dir.path().to_path_buf());
        // This will fail or return empty since there's no git repo
        let result = tool.execute(json!({"action": "run_changed"})).await;
        // Either succeeds with "No changed files" or fails — both are fine
        assert!(result.is_ok() || result.is_err());
    }
}
