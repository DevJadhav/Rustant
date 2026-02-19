//! Verification feedback — formats failures into messages for the agent's conversation.

use super::{DiagnosticSeverity, VerificationResult};

/// Format a verification result into a feedback message for the agent.
///
/// This message is injected as an observation to trigger self-healing.
pub fn format_feedback(result: &VerificationResult) -> String {
    if result.passed {
        return "Verification passed: all lint, type, and test checks are green.".to_string();
    }

    let mut feedback = String::from("## Verification Failed\n\n");

    // Lint errors
    let lint_errors: Vec<_> = result
        .lint_errors
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .collect();
    if !lint_errors.is_empty() {
        feedback.push_str(&format!("### Lint Errors ({})\n", lint_errors.len()));
        for (i, err) in lint_errors.iter().enumerate().take(10) {
            let location = if let Some(line) = err.line {
                format!("{}:{}", err.file, line)
            } else if !err.file.is_empty() {
                err.file.clone()
            } else {
                "unknown".to_string()
            };
            feedback.push_str(&format!("{}. [{}] {}\n", i + 1, location, err.message));
        }
        if lint_errors.len() > 10 {
            feedback.push_str(&format!("... and {} more\n", lint_errors.len() - 10));
        }
        feedback.push('\n');
    }

    // Type errors
    let type_errors: Vec<_> = result
        .type_errors
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .collect();
    if !type_errors.is_empty() {
        feedback.push_str(&format!("### Type Errors ({})\n", type_errors.len()));
        for (i, err) in type_errors.iter().enumerate().take(10) {
            let location = if let Some(line) = err.line {
                format!("{}:{}", err.file, line)
            } else if !err.file.is_empty() {
                err.file.clone()
            } else {
                "unknown".to_string()
            };
            feedback.push_str(&format!("{}. [{}] {}\n", i + 1, location, err.message));
        }
        if type_errors.len() > 10 {
            feedback.push_str(&format!("... and {} more\n", type_errors.len() - 10));
        }
        feedback.push('\n');
    }

    // Test failures
    if !result.test_failures.is_empty() {
        feedback.push_str(&format!(
            "### Test Failures ({})\n",
            result.test_failures.len()
        ));
        for (i, fail) in result.test_failures.iter().enumerate().take(10) {
            feedback.push_str(&format!(
                "{}. {} — {}\n",
                i + 1,
                fail.test_name,
                fail.message
            ));
        }
        if result.test_failures.len() > 10 {
            feedback.push_str(&format!(
                "... and {} more\n",
                result.test_failures.len() - 10
            ));
        }
        feedback.push('\n');
    }

    feedback.push_str("Please fix these issues and try again.\n");
    feedback
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verification::{DiagnosticItem, TestFailure};

    #[test]
    fn test_format_feedback_passing() {
        let result = VerificationResult::passing();
        let feedback = format_feedback(&result);
        assert!(feedback.contains("passed"));
    }

    #[test]
    fn test_format_feedback_with_errors() {
        let result = VerificationResult {
            passed: false,
            lint_errors: vec![DiagnosticItem {
                file: "main.rs".into(),
                line: Some(10),
                severity: DiagnosticSeverity::Error,
                message: "unused variable `x`".into(),
            }],
            test_failures: vec![TestFailure {
                test_name: "test_login".into(),
                message: "assertion failed".into(),
                file: Some("tests/auth.rs".into()),
            }],
            type_errors: vec![],
            fix_attempts: 0,
        };
        let feedback = format_feedback(&result);
        assert!(feedback.contains("Lint Errors"));
        assert!(feedback.contains("unused variable"));
        assert!(feedback.contains("Test Failures"));
        assert!(feedback.contains("test_login"));
        assert!(feedback.contains("fix these issues"));
    }

    #[test]
    fn test_format_feedback_truncation() {
        let errors: Vec<DiagnosticItem> = (0..15)
            .map(|i| DiagnosticItem {
                file: format!("file{i}.rs"),
                line: Some(i),
                severity: DiagnosticSeverity::Error,
                message: format!("error {i}"),
            })
            .collect();

        let result = VerificationResult {
            passed: false,
            lint_errors: errors,
            test_failures: vec![],
            type_errors: vec![],
            fix_attempts: 0,
        };
        let feedback = format_feedback(&result);
        assert!(feedback.contains("... and 5 more"));
    }
}
