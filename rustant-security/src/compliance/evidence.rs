//! Evidence collection automation — gathering artifacts for compliance frameworks.
//!
//! Automates the collection of evidence required by various compliance frameworks,
//! mapping scanner outputs to framework controls.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A piece of compliance evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    /// Evidence identifier.
    pub id: String,
    /// Framework control this evidence supports.
    pub control_id: String,
    /// Evidence type.
    pub evidence_type: EvidenceType,
    /// Description of what this evidence demonstrates.
    pub description: String,
    /// When this evidence was collected.
    pub collected_at: DateTime<Utc>,
    /// Source of the evidence.
    pub source: EvidenceSource,
    /// Status of the evidence.
    pub status: EvidenceStatus,
    /// Artifacts (file paths, URLs, or inline data).
    #[serde(default)]
    pub artifacts: Vec<EvidenceArtifact>,
    /// Validity period (how long this evidence is valid).
    pub valid_until: Option<DateTime<Utc>>,
}

/// Type of evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceType {
    /// Automated scan results.
    ScanResult,
    /// Configuration artifact.
    Configuration,
    /// Policy document.
    PolicyDocument,
    /// Test results.
    TestResult,
    /// Audit log.
    AuditLog,
    /// Review record.
    ReviewRecord,
    /// Remediation record.
    RemediationRecord,
}

impl std::fmt::Display for EvidenceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvidenceType::ScanResult => write!(f, "Scan Result"),
            EvidenceType::Configuration => write!(f, "Configuration"),
            EvidenceType::PolicyDocument => write!(f, "Policy Document"),
            EvidenceType::TestResult => write!(f, "Test Result"),
            EvidenceType::AuditLog => write!(f, "Audit Log"),
            EvidenceType::ReviewRecord => write!(f, "Review Record"),
            EvidenceType::RemediationRecord => write!(f, "Remediation Record"),
        }
    }
}

/// Source of evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceSource {
    /// Tool/scanner that produced this evidence.
    pub tool: String,
    /// Version of the tool.
    pub version: Option<String>,
    /// Whether this was automatically collected.
    pub automated: bool,
}

/// Evidence collection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceStatus {
    /// Evidence has been collected and is valid.
    Collected,
    /// Evidence is pending collection.
    Pending,
    /// Evidence has expired.
    Expired,
    /// Evidence collection failed.
    Failed,
}

impl std::fmt::Display for EvidenceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvidenceStatus::Collected => write!(f, "Collected"),
            EvidenceStatus::Pending => write!(f, "Pending"),
            EvidenceStatus::Expired => write!(f, "Expired"),
            EvidenceStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// An evidence artifact (file, URL, or inline data).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceArtifact {
    /// Artifact name.
    pub name: String,
    /// Artifact type.
    pub artifact_type: ArtifactType,
    /// Content or reference.
    pub content: String,
    /// SHA-256 hash for integrity verification.
    pub hash: Option<String>,
}

/// Artifact type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    /// Path to a file.
    FilePath,
    /// Inline JSON data.
    InlineJson,
    /// Inline text.
    InlineText,
    /// URL reference.
    Url,
}

/// Evidence collection plan for a framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCollectionPlan {
    /// Framework ID.
    pub framework_id: String,
    /// Required evidence items.
    pub required: Vec<EvidenceRequirement>,
}

/// A single evidence requirement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRequirement {
    /// Control ID this evidence supports.
    pub control_id: String,
    /// What evidence is needed.
    pub description: String,
    /// How to collect this evidence.
    pub collection_method: CollectionMethod,
    /// Frequency of collection.
    pub frequency: CollectionFrequency,
}

/// How to collect evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionMethod {
    /// Run a specific scanner.
    Scanner { scanner_name: String },
    /// Export audit trail.
    AuditExport,
    /// Generate compliance report.
    ComplianceReport,
    /// Manual collection required.
    Manual,
}

/// How often evidence should be collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionFrequency {
    /// Once per release/deployment.
    PerRelease,
    /// Daily.
    Daily,
    /// Weekly.
    Weekly,
    /// Monthly.
    Monthly,
    /// Quarterly.
    Quarterly,
    /// Annually.
    Annually,
    /// On demand.
    OnDemand,
}

impl std::fmt::Display for CollectionFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectionFrequency::PerRelease => write!(f, "Per Release"),
            CollectionFrequency::Daily => write!(f, "Daily"),
            CollectionFrequency::Weekly => write!(f, "Weekly"),
            CollectionFrequency::Monthly => write!(f, "Monthly"),
            CollectionFrequency::Quarterly => write!(f, "Quarterly"),
            CollectionFrequency::Annually => write!(f, "Annually"),
            CollectionFrequency::OnDemand => write!(f, "On Demand"),
        }
    }
}

/// Evidence collector for automated evidence gathering.
pub struct EvidenceCollector {
    evidence: Vec<Evidence>,
    next_id: usize,
}

impl EvidenceCollector {
    pub fn new() -> Self {
        Self {
            evidence: Vec::new(),
            next_id: 1,
        }
    }

    /// Record evidence from a scan result.
    pub fn record_scan_evidence(
        &mut self,
        control_id: &str,
        scanner: &str,
        description: &str,
        artifacts: Vec<EvidenceArtifact>,
    ) -> &Evidence {
        let id = format!("EV-{:04}", self.next_id);
        self.next_id += 1;

        let evidence = Evidence {
            id: id.clone(),
            control_id: control_id.to_string(),
            evidence_type: EvidenceType::ScanResult,
            description: description.to_string(),
            collected_at: Utc::now(),
            source: EvidenceSource {
                tool: scanner.to_string(),
                version: None,
                automated: true,
            },
            status: EvidenceStatus::Collected,
            artifacts,
            valid_until: None,
        };

        self.evidence.push(evidence);
        self.evidence.last().unwrap()
    }

    /// Record evidence from an audit log export.
    pub fn record_audit_evidence(
        &mut self,
        control_id: &str,
        description: &str,
        audit_data: &str,
    ) -> &Evidence {
        let id = format!("EV-{:04}", self.next_id);
        self.next_id += 1;

        let evidence = Evidence {
            id,
            control_id: control_id.to_string(),
            evidence_type: EvidenceType::AuditLog,
            description: description.to_string(),
            collected_at: Utc::now(),
            source: EvidenceSource {
                tool: "rustant-audit".to_string(),
                version: None,
                automated: true,
            },
            status: EvidenceStatus::Collected,
            artifacts: vec![EvidenceArtifact {
                name: "audit_log.json".to_string(),
                artifact_type: ArtifactType::InlineJson,
                content: audit_data.to_string(),
                hash: None,
            }],
            valid_until: None,
        };

        self.evidence.push(evidence);
        self.evidence.last().unwrap()
    }

    /// Get all evidence for a specific control.
    pub fn for_control(&self, control_id: &str) -> Vec<&Evidence> {
        self.evidence
            .iter()
            .filter(|e| e.control_id == control_id)
            .collect()
    }

    /// Get all collected evidence.
    pub fn all(&self) -> &[Evidence] {
        &self.evidence
    }

    /// Get evidence count.
    pub fn count(&self) -> usize {
        self.evidence.len()
    }

    /// Get summary of evidence collection status.
    pub fn summary(&self) -> EvidenceCollectionSummary {
        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut by_type: HashMap<String, usize> = HashMap::new();
        let mut by_control: HashMap<String, usize> = HashMap::new();

        for ev in &self.evidence {
            *by_status.entry(ev.status.to_string()).or_insert(0) += 1;
            *by_type.entry(ev.evidence_type.to_string()).or_insert(0) += 1;
            *by_control.entry(ev.control_id.clone()).or_insert(0) += 1;
        }

        EvidenceCollectionSummary {
            total: self.evidence.len(),
            by_status,
            by_type,
            controls_covered: by_control.len(),
        }
    }

    /// Generate a collection plan for a framework.
    pub fn plan_for_soc2() -> EvidenceCollectionPlan {
        EvidenceCollectionPlan {
            framework_id: "soc2".to_string(),
            required: vec![
                EvidenceRequirement {
                    control_id: "CC6.1".to_string(),
                    description: "Evidence of access controls and credential management"
                        .to_string(),
                    collection_method: CollectionMethod::Scanner {
                        scanner_name: "secrets".to_string(),
                    },
                    frequency: CollectionFrequency::PerRelease,
                },
                EvidenceRequirement {
                    control_id: "CC7.1".to_string(),
                    description: "Evidence of vulnerability monitoring".to_string(),
                    collection_method: CollectionMethod::Scanner {
                        scanner_name: "sca".to_string(),
                    },
                    frequency: CollectionFrequency::Weekly,
                },
                EvidenceRequirement {
                    control_id: "CC8.1".to_string(),
                    description: "Evidence of change management testing".to_string(),
                    collection_method: CollectionMethod::Scanner {
                        scanner_name: "sast".to_string(),
                    },
                    frequency: CollectionFrequency::PerRelease,
                },
            ],
        }
    }
}

impl Default for EvidenceCollector {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of evidence collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCollectionSummary {
    pub total: usize,
    pub by_status: HashMap<String, usize>,
    pub by_type: HashMap<String, usize>,
    pub controls_covered: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_scan_evidence() {
        let mut collector = EvidenceCollector::new();
        let ev = collector.record_scan_evidence(
            "CC6.1",
            "secrets",
            "No hardcoded secrets found",
            vec![EvidenceArtifact {
                name: "scan_report.json".to_string(),
                artifact_type: ArtifactType::InlineJson,
                content: "{}".to_string(),
                hash: None,
            }],
        );

        assert_eq!(ev.id, "EV-0001");
        assert_eq!(ev.control_id, "CC6.1");
        assert_eq!(ev.status, EvidenceStatus::Collected);
        assert_eq!(ev.artifacts.len(), 1);
    }

    #[test]
    fn test_record_audit_evidence() {
        let mut collector = EvidenceCollector::new();
        collector.record_audit_evidence("CC8.1", "Audit trail for changes", "{\"events\": []}");
        assert_eq!(collector.count(), 1);
    }

    #[test]
    fn test_for_control() {
        let mut collector = EvidenceCollector::new();
        collector.record_scan_evidence("CC6.1", "secrets", "Test 1", Vec::new());
        collector.record_scan_evidence("CC6.1", "sast", "Test 2", Vec::new());
        collector.record_scan_evidence("CC7.1", "sca", "Test 3", Vec::new());

        assert_eq!(collector.for_control("CC6.1").len(), 2);
        assert_eq!(collector.for_control("CC7.1").len(), 1);
        assert_eq!(collector.for_control("CC99").len(), 0);
    }

    #[test]
    fn test_summary() {
        let mut collector = EvidenceCollector::new();
        collector.record_scan_evidence("CC6.1", "secrets", "Test", Vec::new());
        collector.record_audit_evidence("CC8.1", "Audit", "{}");

        let summary = collector.summary();
        assert_eq!(summary.total, 2);
        assert_eq!(summary.controls_covered, 2);
        assert_eq!(summary.by_status.get("Collected"), Some(&2));
    }

    #[test]
    fn test_collection_plan() {
        let plan = EvidenceCollector::plan_for_soc2();
        assert_eq!(plan.framework_id, "soc2");
        assert!(!plan.required.is_empty());
    }

    #[test]
    fn test_evidence_type_display() {
        assert_eq!(EvidenceType::ScanResult.to_string(), "Scan Result");
        assert_eq!(EvidenceType::AuditLog.to_string(), "Audit Log");
    }

    #[test]
    fn test_evidence_status_display() {
        assert_eq!(EvidenceStatus::Collected.to_string(), "Collected");
        assert_eq!(EvidenceStatus::Pending.to_string(), "Pending");
    }

    #[test]
    fn test_collection_frequency_display() {
        assert_eq!(CollectionFrequency::PerRelease.to_string(), "Per Release");
        assert_eq!(CollectionFrequency::Quarterly.to_string(), "Quarterly");
    }

    #[test]
    fn test_sequential_ids() {
        let mut collector = EvidenceCollector::new();
        collector.record_scan_evidence("A", "s", "1", Vec::new());
        collector.record_scan_evidence("B", "s", "2", Vec::new());

        assert_eq!(collector.all()[0].id, "EV-0001");
        assert_eq!(collector.all()[1].id, "EV-0002");
    }
}
