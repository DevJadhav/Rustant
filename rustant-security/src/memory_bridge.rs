//! Memory bridge — converts Findings into redacted Fact entries for long-term memory.
//!
//! All code snippets and evidence pass through SecretRedactor before storage.

use crate::finding::{Finding, FindingSeverity};
use crate::redaction::SecretRedactor;

/// A redacted fact entry ready for storage in long-term memory.
#[derive(Debug, Clone)]
pub struct RedactedFact {
    /// The fact content (redacted of any secrets).
    pub content: String,
    /// Tags for categorization and retrieval.
    pub tags: Vec<String>,
}

/// Converts findings into redacted facts suitable for long-term memory.
pub struct FindingMemoryBridge {
    redactor: SecretRedactor,
}

impl FindingMemoryBridge {
    pub fn new() -> Self {
        Self {
            redactor: SecretRedactor::new(),
        }
    }

    pub fn with_redactor(redactor: SecretRedactor) -> Self {
        Self { redactor }
    }

    /// Convert a finding into a redacted fact for long-term memory.
    pub fn to_fact(&self, finding: &Finding) -> RedactedFact {
        // Build a summary string (never include raw evidence directly)
        let summary = format!(
            "{}: {} in {} (confidence: {:.0}%)",
            finding.provenance.scanner.to_uppercase(),
            finding.title,
            finding
                .location
                .as_ref()
                .map(|l| l.to_string())
                .unwrap_or_else(|| "unknown location".into()),
            finding.provenance.confidence * 100.0,
        );

        // Redact the summary (in case the title or location contains secrets)
        let result = self.redactor.redact(&summary);

        // Build tags
        let mut tags = vec![
            "security_finding".to_string(),
            finding.severity.as_str().to_string(),
            finding.provenance.scanner.clone(),
        ];
        if let Some(ref rule_id) = finding.provenance.rule_id {
            tags.push(rule_id.clone());
        }
        tags.extend(finding.tags.clone());

        RedactedFact {
            content: result.redacted,
            tags,
        }
    }

    /// Convert a finding suppression into a correction entry for learning.
    pub fn to_correction(&self, finding: &Finding) -> Option<RedactedFact> {
        if let Some(ref suppression) = finding.suppression {
            let content = format!(
                "False positive: {} ({}) - {}",
                finding.title,
                finding.provenance.rule_id.as_deref().unwrap_or("no rule"),
                suppression.reason,
            );
            let result = self.redactor.redact(&content);
            Some(RedactedFact {
                content: result.redacted,
                tags: vec![
                    "security_correction".to_string(),
                    "false_positive".to_string(),
                    finding.provenance.scanner.clone(),
                ],
            })
        } else {
            None
        }
    }

    /// Convert multiple findings into facts, filtering by severity threshold.
    pub fn batch_to_facts(
        &self,
        findings: &[Finding],
        min_severity: FindingSeverity,
    ) -> Vec<RedactedFact> {
        findings
            .iter()
            .filter(|f| f.severity >= min_severity)
            .map(|f| self.to_fact(f))
            .collect()
    }
}

impl Default for FindingMemoryBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::*;

    #[test]
    fn test_finding_to_fact() {
        let bridge = FindingMemoryBridge::new();
        let finding = Finding::new(
            "SQL Injection",
            "Potential SQL injection via string concatenation",
            FindingSeverity::Critical,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.92).with_rule("CWE-89"),
        )
        .with_location(CodeLocation::new("src/db.rs", 42));

        let fact = bridge.to_fact(&finding);
        assert!(fact.content.contains("SAST"));
        assert!(fact.content.contains("SQL Injection"));
        assert!(fact.content.contains("src/db.rs:42"));
        assert!(fact.tags.contains(&"security_finding".to_string()));
        assert!(fact.tags.contains(&"critical".to_string()));
        assert!(fact.tags.contains(&"CWE-89".to_string()));
    }

    #[test]
    fn test_finding_with_secret_in_title() {
        let bridge = FindingMemoryBridge::new();
        let finding = Finding::new(
            "Hardcoded AWS key AKIAIOSFODNN7EXAMPLE found",
            "AWS access key in source code",
            FindingSeverity::Critical,
            FindingCategory::Secret,
            FindingProvenance::new("secrets", 1.0),
        );

        let fact = bridge.to_fact(&finding);
        assert!(!fact.content.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(fact.content.contains("[REDACTED"));
    }

    #[test]
    fn test_suppression_to_correction() {
        let bridge = FindingMemoryBridge::new();
        let mut finding = Finding::new(
            "Unused import",
            "Import is used in test code",
            FindingSeverity::Low,
            FindingCategory::Quality,
            FindingProvenance::new("quality", 0.6),
        );
        finding.suppress("dev", "Used in test code, not dead code");

        let correction = bridge.to_correction(&finding);
        assert!(correction.is_some());
        let correction = correction.unwrap();
        assert!(correction.tags.contains(&"false_positive".to_string()));
    }

    #[test]
    fn test_batch_severity_filter() {
        let bridge = FindingMemoryBridge::new();
        let findings = vec![
            Finding::new(
                "Info",
                "d",
                FindingSeverity::Info,
                FindingCategory::Quality,
                FindingProvenance::new("q", 0.5),
            ),
            Finding::new(
                "Low",
                "d",
                FindingSeverity::Low,
                FindingCategory::Quality,
                FindingProvenance::new("q", 0.5),
            ),
            Finding::new(
                "High",
                "d",
                FindingSeverity::High,
                FindingCategory::Security,
                FindingProvenance::new("s", 0.9),
            ),
        ];

        let facts = bridge.batch_to_facts(&findings, FindingSeverity::Medium);
        assert_eq!(facts.len(), 1);
        assert!(facts[0].content.contains("High"));
    }
}
