//! Unified Finding Schema — the canonical data model for ALL scanner outputs.
//!
//! Every finding includes provenance for interpretability and deduplication
//! support via content hashing.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use uuid::Uuid;

/// A security, quality, or compliance finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Unique identifier.
    pub id: Uuid,
    /// Short title describing the finding.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Severity classification.
    pub severity: FindingSeverity,
    /// CVSS 3.1 base score (0.0-10.0), if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cvss_score: Option<f32>,
    /// Finding category.
    pub category: FindingCategory,
    /// Code location where the finding was detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<CodeLocation>,
    /// SHA-256 hash of canonical content for deduplication.
    pub content_hash: String,
    /// Provenance: which scanner, rule, confidence, and consensus info.
    pub provenance: FindingProvenance,
    /// Human-readable explanation with reasoning chain.
    pub explanation: FindingExplanation,
    /// Suggested remediation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remediation: Option<Remediation>,
    /// External references (CWE, OWASP, CVE).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<FindingReference>,
    /// Suppression record if this finding was suppressed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppression: Option<SuppressionRecord>,
    /// When this finding was first detected.
    pub created_at: DateTime<Utc>,
    /// Current status.
    pub status: FindingStatus,
    /// Tags for categorization and retrieval.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Finding {
    /// Create a new finding with auto-generated ID, timestamp, and content hash.
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        severity: FindingSeverity,
        category: FindingCategory,
        provenance: FindingProvenance,
    ) -> Self {
        let title = title.into();
        let description = description.into();
        let content_hash = compute_content_hash(&title, &description, &provenance);

        Self {
            id: Uuid::new_v4(),
            title,
            description,
            severity,
            cvss_score: None,
            category,
            location: None,
            content_hash,
            provenance,
            explanation: FindingExplanation::default(),
            remediation: None,
            references: Vec::new(),
            suppression: None,
            created_at: Utc::now(),
            status: FindingStatus::Open,
            tags: Vec::new(),
        }
    }

    /// Set the code location.
    pub fn with_location(mut self, location: CodeLocation) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the CVSS score and auto-derive severity if not already set.
    pub fn with_cvss(mut self, score: f32) -> Self {
        self.cvss_score = Some(score);
        self.severity = FindingSeverity::from_cvss(score);
        self
    }

    /// Add a remediation suggestion.
    pub fn with_remediation(mut self, remediation: Remediation) -> Self {
        self.remediation = Some(remediation);
        self
    }

    /// Add a reference (CWE, CVE, OWASP).
    pub fn with_reference(mut self, reference: FindingReference) -> Self {
        self.references.push(reference);
        self
    }

    /// Add a tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the explanation.
    pub fn with_explanation(mut self, explanation: FindingExplanation) -> Self {
        self.explanation = explanation;
        self
    }

    /// Check if this finding is a duplicate of another (by content hash).
    pub fn is_duplicate_of(&self, other: &Finding) -> bool {
        self.content_hash == other.content_hash
    }

    /// Suppress this finding with a reason.
    pub fn suppress(&mut self, suppressed_by: impl Into<String>, reason: impl Into<String>) {
        self.suppression = Some(SuppressionRecord {
            suppressed_by: suppressed_by.into(),
            suppressed_at: Utc::now(),
            reason: reason.into(),
            expires_at: None,
        });
        self.status = FindingStatus::Suppressed;
    }

    /// Mark as resolved.
    pub fn resolve(&mut self) {
        self.status = FindingStatus::Resolved;
    }

    /// Mark as false positive.
    pub fn mark_false_positive(&mut self) {
        self.status = FindingStatus::FalsePositive;
    }
}

/// Severity levels for findings, aligned with CVSS 3.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl FindingSeverity {
    /// Derive severity from a CVSS 3.1 base score.
    pub fn from_cvss(score: f32) -> Self {
        match score {
            s if s >= 9.0 => FindingSeverity::Critical,
            s if s >= 7.0 => FindingSeverity::High,
            s if s >= 4.0 => FindingSeverity::Medium,
            s if s >= 0.1 => FindingSeverity::Low,
            _ => FindingSeverity::Info,
        }
    }

    /// Return the display name.
    pub fn as_str(&self) -> &'static str {
        match self {
            FindingSeverity::Info => "info",
            FindingSeverity::Low => "low",
            FindingSeverity::Medium => "medium",
            FindingSeverity::High => "high",
            FindingSeverity::Critical => "critical",
        }
    }
}

impl std::fmt::Display for FindingSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Finding categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingCategory {
    Security,
    Quality,
    Compliance,
    Dependency,
    Secret,
    Configuration,
    Performance,
}

/// Code location for a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeLocation {
    /// File path relative to workspace root.
    pub file: PathBuf,
    /// Start line (1-indexed).
    pub start_line: usize,
    /// End line (1-indexed, inclusive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    /// Start column (0-indexed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    /// End column (0-indexed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    /// The function or method containing this location, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
}

impl CodeLocation {
    pub fn new(file: impl Into<PathBuf>, line: usize) -> Self {
        Self {
            file: file.into(),
            start_line: line,
            end_line: None,
            start_column: None,
            end_column: None,
            function_name: None,
        }
    }

    pub fn with_range(mut self, end_line: usize) -> Self {
        self.end_line = Some(end_line);
        self
    }

    pub fn with_columns(mut self, start: usize, end: usize) -> Self {
        self.start_column = Some(start);
        self.end_column = Some(end);
        self
    }

    pub fn with_function(mut self, name: impl Into<String>) -> Self {
        self.function_name = Some(name.into());
        self
    }
}

impl std::fmt::Display for CodeLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file.display(), self.start_line)?;
        if let Some(end) = self.end_line {
            write!(f, "-{end}")?;
        }
        Ok(())
    }
}

/// Provenance information: where did this finding come from?
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingProvenance {
    /// Which scanner produced this finding.
    pub scanner: String,
    /// Rule identifier (e.g., "CWE-89", "CVE-2024-1234").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Confidence score (0.0-1.0).
    pub confidence: f32,
    /// Multi-model consensus information, if used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consensus: Option<ConsensusProvenance>,
}

impl FindingProvenance {
    pub fn new(scanner: impl Into<String>, confidence: f32) -> Self {
        Self {
            scanner: scanner.into(),
            rule_id: None,
            confidence,
            consensus: None,
        }
    }

    pub fn with_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_id = Some(rule_id.into());
        self
    }

    pub fn with_consensus(mut self, consensus: ConsensusProvenance) -> Self {
        self.consensus = Some(consensus);
        self
    }
}

/// Multi-model consensus provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusProvenance {
    /// Models that were queried.
    pub models_queried: Vec<String>,
    /// Models that agreed with the finding.
    pub models_agreed: Vec<String>,
    /// Models that disagreed.
    pub models_disagreed: Vec<String>,
    /// Agreement ratio (0.0-1.0).
    pub agreement_ratio: f32,
}

/// Human-readable explanation of a finding.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindingExplanation {
    /// Step-by-step reasoning chain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasoning_chain: Vec<String>,
    /// Evidence snippets (code, patterns matched). MUST be redacted of secrets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    /// Contextual factors that influenced the finding.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_factors: Vec<String>,
}

impl FindingExplanation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reasoning(mut self, step: impl Into<String>) -> Self {
        self.reasoning_chain.push(step.into());
        self
    }

    pub fn with_evidence(mut self, evidence: impl Into<String>) -> Self {
        self.evidence.push(evidence.into());
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context_factors.push(context.into());
        self
    }
}

/// Remediation suggestion for a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Remediation {
    /// Description of the suggested fix.
    pub description: String,
    /// Unified diff patch, if available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,
    /// Estimated effort.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<RemediationEffort>,
    /// Confidence in the fix (0.0-1.0).
    pub confidence: f32,
}

/// Estimated effort to remediate a finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemediationEffort {
    Trivial,
    Low,
    Medium,
    High,
    Significant,
}

/// External reference for a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindingReference {
    /// Reference type.
    pub ref_type: ReferenceType,
    /// Identifier (e.g., "CWE-89", "CVE-2024-1234").
    pub id: String,
    /// URL to the reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Types of external references.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceType {
    Cwe,
    Cve,
    Owasp,
    Ghsa,
    Url,
    Other,
}

/// Record of a finding suppression.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuppressionRecord {
    /// Who suppressed the finding.
    pub suppressed_by: String,
    /// When it was suppressed.
    pub suppressed_at: DateTime<Utc>,
    /// Reason for suppression.
    pub reason: String,
    /// Expiration of suppression (None = permanent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

/// Finding status lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindingStatus {
    Open,
    Suppressed,
    Resolved,
    FalsePositive,
}

/// Compute a deterministic content hash for deduplication.
fn compute_content_hash(title: &str, description: &str, provenance: &FindingProvenance) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"|");
    hasher.update(description.as_bytes());
    hasher.update(b"|");
    hasher.update(provenance.scanner.as_bytes());
    if let Some(ref rule) = provenance.rule_id {
        hasher.update(b"|");
        hasher.update(rule.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Trait for normalizing scanner-specific outputs into the unified Finding schema.
pub trait FindingNormalizer: Send + Sync {
    /// Convert scanner-specific output into Finding objects.
    fn normalize(
        &self,
        raw_output: &serde_json::Value,
    ) -> Result<Vec<Finding>, crate::error::ScanError>;
}

/// Deduplication engine that filters duplicate findings by content hash.
pub struct DeduplicationEngine {
    seen_hashes: std::collections::HashSet<String>,
}

impl DeduplicationEngine {
    pub fn new() -> Self {
        Self {
            seen_hashes: std::collections::HashSet::new(),
        }
    }

    /// Deduplicate a list of findings, returning only unique ones.
    pub fn deduplicate(&mut self, findings: Vec<Finding>) -> Vec<Finding> {
        let mut unique = Vec::new();
        for finding in findings {
            if self.seen_hashes.insert(finding.content_hash.clone()) {
                unique.push(finding);
            }
        }
        unique
    }

    /// Reset the deduplication state.
    pub fn reset(&mut self) {
        self.seen_hashes.clear();
    }
}

impl Default for DeduplicationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finding_creation() {
        let finding = Finding::new(
            "SQL Injection",
            "Potential SQL injection in query builder",
            FindingSeverity::Critical,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.95),
        );

        assert_eq!(finding.title, "SQL Injection");
        assert_eq!(finding.severity, FindingSeverity::Critical);
        assert_eq!(finding.status, FindingStatus::Open);
        assert!(!finding.content_hash.is_empty());
    }

    #[test]
    fn test_severity_from_cvss() {
        assert_eq!(FindingSeverity::from_cvss(9.5), FindingSeverity::Critical);
        assert_eq!(FindingSeverity::from_cvss(7.5), FindingSeverity::High);
        assert_eq!(FindingSeverity::from_cvss(5.0), FindingSeverity::Medium);
        assert_eq!(FindingSeverity::from_cvss(2.0), FindingSeverity::Low);
        assert_eq!(FindingSeverity::from_cvss(0.0), FindingSeverity::Info);
    }

    #[test]
    fn test_deduplication() {
        let mut dedup = DeduplicationEngine::new();
        let f1 = Finding::new(
            "XSS",
            "desc",
            FindingSeverity::High,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.9),
        );
        let f2 = Finding::new(
            "XSS",
            "desc",
            FindingSeverity::High,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.9),
        );
        let f3 = Finding::new(
            "SQLi",
            "different",
            FindingSeverity::Critical,
            FindingCategory::Security,
            FindingProvenance::new("sast", 0.8),
        );

        let results = dedup.deduplicate(vec![f1, f2, f3]);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_finding_suppression() {
        let mut finding = Finding::new(
            "Test",
            "desc",
            FindingSeverity::Low,
            FindingCategory::Quality,
            FindingProvenance::new("quality", 0.7),
        );
        finding.suppress("admin", "False positive in test code");
        assert_eq!(finding.status, FindingStatus::Suppressed);
        assert!(finding.suppression.is_some());
    }

    #[test]
    fn test_severity_ordering() {
        assert!(FindingSeverity::Critical > FindingSeverity::High);
        assert!(FindingSeverity::High > FindingSeverity::Medium);
        assert!(FindingSeverity::Medium > FindingSeverity::Low);
        assert!(FindingSeverity::Low > FindingSeverity::Info);
    }

    #[test]
    fn test_code_location_display() {
        let loc = CodeLocation::new("src/main.rs", 42).with_range(50);
        assert_eq!(loc.to_string(), "src/main.rs:42-50");
    }
}
