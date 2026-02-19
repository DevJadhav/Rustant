//! Policy engine — TOML-based gate/warn/inform rules for security governance.
//!
//! Evaluates findings and actions against configurable policies.

use crate::finding::{Finding, FindingSeverity};
use serde::{Deserialize, Serialize};

/// Type of policy action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyType {
    /// Block on violation (hard gate).
    Gate,
    /// Warn but allow (soft gate).
    Warn,
    /// Log only (informational).
    Inform,
}

impl std::fmt::Display for PolicyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyType::Gate => write!(f, "gate"),
            PolicyType::Warn => write!(f, "warn"),
            PolicyType::Inform => write!(f, "inform"),
        }
    }
}

/// A single policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Unique rule identifier.
    pub id: String,
    /// Policy type (gate/warn/inform).
    pub policy_type: PolicyType,
    /// Scanner this rule applies to (or "all").
    #[serde(default = "default_all")]
    pub scanner: String,
    /// Minimum severity to trigger this rule.
    #[serde(default)]
    pub min_severity: Option<FindingSeverity>,
    /// Maximum allowed findings of this type.
    #[serde(default)]
    pub max_count: Option<usize>,
    /// Human-readable message.
    pub message: String,
    /// Scope: "pr", "repository", "branch".
    #[serde(default = "default_repository")]
    pub scope: String,
}

fn default_all() -> String {
    "all".into()
}

fn default_repository() -> String {
    "repository".into()
}

/// Result of evaluating a policy rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyEvaluation {
    /// The rule that was evaluated.
    pub rule_id: String,
    /// Whether the rule passed.
    pub passed: bool,
    /// Policy type.
    pub policy_type: PolicyType,
    /// Message (with context).
    pub message: String,
    /// Number of findings that triggered this rule.
    pub finding_count: usize,
}

/// Result of evaluating all policies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyReport {
    /// All evaluations.
    pub evaluations: Vec<PolicyEvaluation>,
    /// Whether any gate policy failed.
    pub has_gate_failures: bool,
    /// Whether any warn policy triggered.
    pub has_warnings: bool,
    /// Number of passed rules.
    pub passed: usize,
    /// Number of failed rules.
    pub failed: usize,
}

impl PolicyReport {
    /// Get all gate failures.
    pub fn gate_failures(&self) -> Vec<&PolicyEvaluation> {
        self.evaluations
            .iter()
            .filter(|e| !e.passed && e.policy_type == PolicyType::Gate)
            .collect()
    }

    /// Get all warnings.
    pub fn warnings(&self) -> Vec<&PolicyEvaluation> {
        self.evaluations
            .iter()
            .filter(|e| !e.passed && e.policy_type == PolicyType::Warn)
            .collect()
    }
}

/// Policy engine for evaluating findings against rules.
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    /// Create a new policy engine with the given rules.
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self { rules }
    }

    /// Create with default security policies.
    pub fn with_defaults() -> Self {
        Self::new(vec![
            PolicyRule {
                id: "no-critical-vulns".into(),
                policy_type: PolicyType::Gate,
                scanner: "all".into(),
                min_severity: Some(FindingSeverity::Critical),
                max_count: Some(0),
                message: "Critical vulnerabilities must be resolved before merge".into(),
                scope: "pr".into(),
            },
            PolicyRule {
                id: "max-high-per-pr".into(),
                policy_type: PolicyType::Warn,
                scanner: "all".into(),
                min_severity: Some(FindingSeverity::High),
                max_count: Some(3),
                message: "More than 3 high-severity findings detected".into(),
                scope: "pr".into(),
            },
            PolicyRule {
                id: "no-hardcoded-secrets".into(),
                policy_type: PolicyType::Gate,
                scanner: "secrets".into(),
                min_severity: None,
                max_count: Some(0),
                message: "Hardcoded secrets must be removed".into(),
                scope: "pr".into(),
            },
        ])
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: PolicyRule) {
        self.rules.push(rule);
    }

    /// Evaluate all policies against a set of findings.
    pub fn evaluate(&self, findings: &[Finding]) -> PolicyReport {
        let mut evaluations = Vec::new();
        let mut has_gate_failures = false;
        let mut has_warnings = false;
        let mut passed = 0;
        let mut failed = 0;

        for rule in &self.rules {
            let matching_findings = self.matching_findings(findings, rule);
            let count = matching_findings.len();

            let rule_passed = match rule.max_count {
                Some(max) => count <= max,
                None => count == 0, // If no max_count, any match is a failure
            };

            if rule_passed {
                passed += 1;
            } else {
                failed += 1;
                match rule.policy_type {
                    PolicyType::Gate => has_gate_failures = true,
                    PolicyType::Warn => has_warnings = true,
                    PolicyType::Inform => {}
                }
            }

            evaluations.push(PolicyEvaluation {
                rule_id: rule.id.clone(),
                passed: rule_passed,
                policy_type: rule.policy_type,
                message: if rule_passed {
                    format!("Rule '{}' passed ({} findings)", rule.id, count)
                } else {
                    format!(
                        "{} ({} findings, max allowed: {})",
                        rule.message,
                        count,
                        rule.max_count.map_or("0".to_string(), |m| m.to_string())
                    )
                },
                finding_count: count,
            });
        }

        PolicyReport {
            evaluations,
            has_gate_failures,
            has_warnings,
            passed,
            failed,
        }
    }

    /// Get findings matching a rule's criteria.
    fn matching_findings<'a>(
        &self,
        findings: &'a [Finding],
        rule: &PolicyRule,
    ) -> Vec<&'a Finding> {
        findings
            .iter()
            .filter(|f| {
                // Scanner filter
                if rule.scanner != "all" && f.provenance.scanner != rule.scanner {
                    return false;
                }
                // Severity filter
                if let Some(min_sev) = rule.min_severity
                    && f.severity < min_sev
                {
                    return false;
                }
                true
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{FindingCategory, FindingProvenance};

    fn make_finding(severity: FindingSeverity, scanner: &str) -> Finding {
        Finding::new(
            "Test finding",
            "Test description",
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: scanner.into(),
                rule_id: None,
                confidence: 0.9,
                consensus: None,
            },
        )
    }

    #[test]
    fn test_default_policies() {
        let engine = PolicyEngine::with_defaults();
        let findings = vec![make_finding(FindingSeverity::Critical, "sast")];

        let report = engine.evaluate(&findings);
        assert!(
            report.has_gate_failures,
            "Critical finding should trigger gate"
        );
    }

    #[test]
    fn test_no_findings_all_pass() {
        let engine = PolicyEngine::with_defaults();
        let report = engine.evaluate(&[]);
        assert!(!report.has_gate_failures);
        assert!(!report.has_warnings);
        assert_eq!(report.passed, 3);
        assert_eq!(report.failed, 0);
    }

    #[test]
    fn test_high_severity_warning() {
        let engine = PolicyEngine::with_defaults();
        let findings = vec![
            make_finding(FindingSeverity::High, "sast"),
            make_finding(FindingSeverity::High, "sast"),
            make_finding(FindingSeverity::High, "sast"),
            make_finding(FindingSeverity::High, "sast"),
        ];

        let report = engine.evaluate(&findings);
        assert!(
            report.has_warnings,
            "4 high findings should trigger warning"
        );
    }

    #[test]
    fn test_scanner_filter() {
        let engine = PolicyEngine::with_defaults();
        // Secrets from SAST scanner shouldn't trigger secrets policy
        let findings = vec![make_finding(FindingSeverity::Medium, "sast")];
        let report = engine.evaluate(&findings);

        let secrets_eval = report
            .evaluations
            .iter()
            .find(|e| e.rule_id == "no-hardcoded-secrets")
            .unwrap();
        assert!(
            secrets_eval.passed,
            "SAST finding shouldn't trigger secrets rule"
        );
    }

    #[test]
    fn test_custom_policy() {
        let engine = PolicyEngine::new(vec![PolicyRule {
            id: "max-medium".into(),
            policy_type: PolicyType::Gate,
            scanner: "all".into(),
            min_severity: Some(FindingSeverity::Medium),
            max_count: Some(2),
            message: "Too many medium+ findings".into(),
            scope: "pr".into(),
        }]);

        let findings = vec![
            make_finding(FindingSeverity::Medium, "sast"),
            make_finding(FindingSeverity::High, "sca"),
            make_finding(FindingSeverity::Medium, "sast"),
        ];

        let report = engine.evaluate(&findings);
        assert!(
            report.has_gate_failures,
            "3 medium+ findings exceeds max of 2"
        );
    }

    #[test]
    fn test_gate_failures_helper() {
        let engine = PolicyEngine::with_defaults();
        let findings = vec![make_finding(FindingSeverity::Critical, "sast")];
        let report = engine.evaluate(&findings);
        assert!(!report.gate_failures().is_empty());
    }

    #[test]
    fn test_policy_type_display() {
        assert_eq!(PolicyType::Gate.to_string(), "gate");
        assert_eq!(PolicyType::Warn.to_string(), "warn");
        assert_eq!(PolicyType::Inform.to_string(), "inform");
    }
}
