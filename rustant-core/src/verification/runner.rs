//! Verification runner — executes test, lint, and typecheck commands.

use super::{
    DiagnosticItem, DiagnosticSeverity, TestFailure, VerificationConfig, VerificationResult,
};
use std::path::Path;

/// Run the full verification pipeline.
pub async fn run_verification(workspace: &Path, config: &VerificationConfig) -> VerificationResult {
    let mut result = VerificationResult::passing();

    // Step 1: Lint check
    if config.auto_lint {
        let lint_output = run_lint(workspace).await;
        result.lint_errors.extend(lint_output);
    }

    // Step 2: Type check
    if config.auto_typecheck {
        let type_output = run_typecheck(workspace).await;
        result.type_errors.extend(type_output);
    }

    // Step 3: Test
    if config.auto_test {
        let test_output = run_tests(workspace).await;
        result.test_failures.extend(test_output);
    }

    result.passed = result
        .lint_errors
        .iter()
        .all(|d| d.severity != DiagnosticSeverity::Error)
        && result.test_failures.is_empty()
        && result
            .type_errors
            .iter()
            .all(|d| d.severity != DiagnosticSeverity::Error);

    result
}

/// Run lint check and parse output.
async fn run_lint(workspace: &Path) -> Vec<DiagnosticItem> {
    let project = crate::project_detect::detect_project(workspace);
    let cmd = match &project.project_type {
        crate::project_detect::ProjectType::Rust => "cargo clippy --message-format=short 2>&1",
        crate::project_detect::ProjectType::Node => "npx eslint . --format compact 2>&1",
        crate::project_detect::ProjectType::Python => "ruff check . 2>&1",
        _ => return Vec::new(),
    };

    let output = match tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(workspace)
        .output()
        .await
    {
        Ok(o) => format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) => return Vec::new(),
    };

    parse_diagnostics(&output)
}

/// Run type checking.
async fn run_typecheck(workspace: &Path) -> Vec<DiagnosticItem> {
    let project = crate::project_detect::detect_project(workspace);
    let cmd = match &project.project_type {
        crate::project_detect::ProjectType::Rust => "cargo check --message-format=short 2>&1",
        crate::project_detect::ProjectType::Node => "npx tsc --noEmit 2>&1",
        crate::project_detect::ProjectType::Python => "mypy . 2>&1",
        _ => return Vec::new(),
    };

    let output = match tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(workspace)
        .output()
        .await
    {
        Ok(o) => format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
        Err(_) => return Vec::new(),
    };

    parse_diagnostics(&output)
}

/// Run tests and parse output.
async fn run_tests(workspace: &Path) -> Vec<TestFailure> {
    let project = crate::project_detect::detect_project(workspace);
    let cmd = match &project.project_type {
        crate::project_detect::ProjectType::Rust => "cargo test 2>&1",
        crate::project_detect::ProjectType::Node => "npm test 2>&1",
        crate::project_detect::ProjectType::Python => "pytest -v 2>&1",
        crate::project_detect::ProjectType::Go => "go test ./... 2>&1",
        _ => return Vec::new(),
    };

    let output = match tokio::process::Command::new("sh")
        .args(["-c", cmd])
        .current_dir(workspace)
        .output()
        .await
    {
        Ok(o) => {
            if o.status.success() {
                return Vec::new();
            }
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        }
        Err(_) => return Vec::new(),
    };

    parse_test_failures(&output)
}

/// Parse diagnostic output into structured items.
fn parse_diagnostics(output: &str) -> Vec<DiagnosticItem> {
    let mut items = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let severity = if trimmed.contains("error") {
            DiagnosticSeverity::Error
        } else if trimmed.contains("warning") {
            DiagnosticSeverity::Warning
        } else {
            continue;
        };

        // Try to parse file:line patterns
        let (file, line_num) = if let Some(colon_pos) = trimmed.find(':') {
            let file_part = &trimmed[..colon_pos];
            let rest = &trimmed[colon_pos + 1..];
            let line_num = rest
                .split(':')
                .next()
                .and_then(|s| s.trim().parse::<usize>().ok());
            (file_part.to_string(), line_num)
        } else {
            (String::new(), None)
        };

        items.push(DiagnosticItem {
            file,
            line: line_num,
            severity,
            message: trimmed.to_string(),
        });
    }

    items
}

/// Parse test failure output.
fn parse_test_failures(output: &str) -> Vec<TestFailure> {
    let mut failures = Vec::new();

    for line in output.lines() {
        let trimmed = line.trim();

        // Rust test failures: "test foo::bar ... FAILED"
        if trimmed.starts_with("test ") && trimmed.ends_with("FAILED") {
            let name = trimmed
                .strip_prefix("test ")
                .unwrap_or(trimmed)
                .split(" ... ")
                .next()
                .unwrap_or(trimmed)
                .to_string();
            failures.push(TestFailure {
                test_name: name,
                message: trimmed.to_string(),
                file: None,
            });
        }
        // pytest failures: "FAILED tests/test_foo.py::test_bar"
        else if trimmed.starts_with("FAILED ") {
            let name = trimmed.strip_prefix("FAILED ").unwrap_or(trimmed);
            let file = name.split("::").next().map(String::from);
            failures.push(TestFailure {
                test_name: name.to_string(),
                message: trimmed.to_string(),
                file,
            });
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostics() {
        let output = "src/main.rs:10:5: error[E0308]: mismatched types\nwarning: unused import";
        let items = parse_diagnostics(output);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].severity, DiagnosticSeverity::Error);
        assert_eq!(items[1].severity, DiagnosticSeverity::Warning);
    }

    #[test]
    fn test_parse_test_failures_rust() {
        let output = "test foo::bar ... ok\ntest foo::baz ... FAILED\n";
        let failures = parse_test_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].test_name, "foo::baz");
    }

    #[test]
    fn test_parse_test_failures_pytest() {
        let output = "PASSED tests/test_a.py::test_ok\nFAILED tests/test_b.py::test_bad\n";
        let failures = parse_test_failures(output);
        assert_eq!(failures.len(), 1);
        assert!(failures[0].test_name.contains("test_bad"));
        assert_eq!(failures[0].file.as_deref(), Some("tests/test_b.py"));
    }

    #[test]
    fn test_parse_diagnostics_empty() {
        let items = parse_diagnostics("");
        assert!(items.is_empty());
    }
}
