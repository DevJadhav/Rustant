//! Verification Engine — auto-test + lint + typecheck with self-healing loop.
//!
//! After code generation, runs verification checks and feeds failures
//! back into the agent's conversation for automatic correction.

pub mod feedback;
pub mod runner;

use serde::{Deserialize, Serialize};

/// Configuration for the verification engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    /// Run tests automatically after code changes.
    pub auto_test: bool,
    /// Run lint checks automatically after code changes.
    pub auto_lint: bool,
    /// Run type checking automatically after code changes.
    pub auto_typecheck: bool,
    /// Maximum number of fix attempts before giving up.
    pub max_fix_attempts: u32,
    /// Whether to trigger verification on every file write.
    pub run_on_file_write: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            auto_test: true,
            auto_lint: true,
            auto_typecheck: true,
            max_fix_attempts: 3,
            run_on_file_write: false,
        }
    }
}

/// Result of a verification run.
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether all checks passed.
    pub passed: bool,
    /// Lint errors found.
    pub lint_errors: Vec<DiagnosticItem>,
    /// Test failures.
    pub test_failures: Vec<TestFailure>,
    /// Type errors.
    pub type_errors: Vec<DiagnosticItem>,
    /// Number of fix attempts so far.
    pub fix_attempts: u32,
}

/// A diagnostic item (lint error or type error).
#[derive(Debug, Clone)]
pub struct DiagnosticItem {
    pub file: String,
    pub line: Option<usize>,
    pub severity: DiagnosticSeverity,
    pub message: String,
}

/// Severity level for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

/// A test failure.
#[derive(Debug, Clone)]
pub struct TestFailure {
    pub test_name: String,
    pub message: String,
    pub file: Option<String>,
}

impl VerificationResult {
    /// Create a passing result.
    pub fn passing() -> Self {
        Self {
            passed: true,
            lint_errors: Vec::new(),
            test_failures: Vec::new(),
            type_errors: Vec::new(),
            fix_attempts: 0,
        }
    }

    /// Count total errors.
    pub fn error_count(&self) -> usize {
        self.lint_errors
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
            + self.test_failures.len()
            + self
                .type_errors
                .iter()
                .filter(|d| d.severity == DiagnosticSeverity::Error)
                .count()
    }

    /// Get a summary string.
    pub fn summary(&self) -> String {
        if self.passed {
            return "All checks passed.".to_string();
        }

        let mut parts = Vec::new();
        let lint_errors = self
            .lint_errors
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count();
        if lint_errors > 0 {
            parts.push(format!("{lint_errors} lint error(s)"));
        }
        if !self.test_failures.is_empty() {
            parts.push(format!("{} test failure(s)", self.test_failures.len()));
        }
        let type_errors = self
            .type_errors
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count();
        if type_errors > 0 {
            parts.push(format!("{type_errors} type error(s)"));
        }

        format!("Verification failed: {}", parts.join(", "))
    }
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagnosticSeverity::Error => write!(f, "error"),
            DiagnosticSeverity::Warning => write!(f, "warning"),
            DiagnosticSeverity::Info => write!(f, "info"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert!(config.auto_test);
        assert!(config.auto_lint);
        assert!(config.auto_typecheck);
        assert_eq!(config.max_fix_attempts, 3);
        assert!(!config.run_on_file_write);
    }

    #[test]
    fn test_verification_result_passing() {
        let result = VerificationResult::passing();
        assert!(result.passed);
        assert_eq!(result.error_count(), 0);
        assert_eq!(result.summary(), "All checks passed.");
    }

    #[test]
    fn test_verification_result_with_errors() {
        let result = VerificationResult {
            passed: false,
            lint_errors: vec![DiagnosticItem {
                file: "main.rs".into(),
                line: Some(10),
                severity: DiagnosticSeverity::Error,
                message: "unused variable".into(),
            }],
            test_failures: vec![TestFailure {
                test_name: "test_something".into(),
                message: "assertion failed".into(),
                file: Some("test.rs".into()),
            }],
            type_errors: vec![],
            fix_attempts: 1,
        };
        assert!(!result.passed);
        assert_eq!(result.error_count(), 2);
        assert!(result.summary().contains("lint error"));
        assert!(result.summary().contains("test failure"));
    }
}
