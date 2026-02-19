//! OCSF export — Open Cybersecurity Schema Framework event format.
//!
//! Converts findings to OCSF Security Finding (class_uid 2001) events
//! for integration with SIEM and security analytics platforms.

use crate::finding::{Finding, FindingSeverity, FindingStatus};
use serde::Serialize;

/// OCSF Security Finding event (class_uid 2001).
#[derive(Debug, Serialize)]
pub struct OcsfSecurityFinding {
    /// OCSF class UID (2001 = Security Finding).
    pub class_uid: u32,
    /// Category UID (2 = Findings).
    pub category_uid: u32,
    /// Activity ID (1 = Create, 2 = Update).
    pub activity_id: u32,
    /// Severity ID (OCSF mapping).
    pub severity_id: u32,
    /// Status ID (1 = New, 2 = InProgress, 3 = Suppressed, 4 = Resolved).
    pub status_id: u32,
    /// Finding title.
    pub finding: OcsfFindingInfo,
    /// Metadata.
    pub metadata: OcsfMetadata,
    /// Resource affected.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<OcsfResource>,
    /// Time of detection (epoch milliseconds).
    pub time: i64,
    /// Unmapped data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmapped: Option<serde_json::Value>,
}

impl OcsfSecurityFinding {
    /// Serialize this single OCSF event to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

/// OCSF finding information.
#[derive(Debug, Serialize)]
pub struct OcsfFindingInfo {
    /// Finding title.
    pub title: String,
    /// Finding UID (our internal ID).
    pub uid: String,
    /// Finding description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desc: Option<String>,
    /// Type UID.
    pub type_uid: u32,
    /// Remediation information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<OcsfRemediation>,
}

/// OCSF remediation info.
#[derive(Debug, Serialize)]
pub struct OcsfRemediation {
    /// Description of the remediation action.
    pub desc: String,
    /// External references for the remediation (e.g., CWE links, documentation).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<String>,
}

/// OCSF metadata.
#[derive(Debug, Serialize)]
pub struct OcsfMetadata {
    /// OCSF version.
    pub version: String,
    /// Product information.
    pub product: OcsfProduct,
}

/// OCSF product info.
#[derive(Debug, Serialize)]
pub struct OcsfProduct {
    /// Product name.
    pub name: String,
    /// Vendor name.
    pub vendor_name: String,
    /// Product version.
    pub version: String,
}

/// OCSF resource.
#[derive(Debug, Serialize)]
pub struct OcsfResource {
    /// Resource UID (file path or unique identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uid: Option<String>,
    /// Resource type.
    #[serde(rename = "type")]
    pub resource_type: String,
    /// Resource name (file path).
    pub name: String,
}

/// Map FindingSeverity to OCSF severity_id.
/// 0=Unknown, 1=Informational, 2=Low, 3=Medium, 4=High, 5=Critical, 6=Fatal.
fn severity_to_ocsf(severity: FindingSeverity) -> u32 {
    match severity {
        FindingSeverity::Info => 1,     // Informational
        FindingSeverity::Low => 2,      // Low
        FindingSeverity::Medium => 3,   // Medium
        FindingSeverity::High => 4,     // High
        FindingSeverity::Critical => 5, // Critical
    }
}

/// Map FindingStatus to OCSF status_id.
/// 1=New, 2=InProgress, 3=Suppressed, 4=Resolved.
fn status_to_ocsf(status: FindingStatus) -> u32 {
    match status {
        FindingStatus::Open => 1,          // New
        FindingStatus::FalsePositive => 2, // InProgress (being reviewed)
        FindingStatus::Suppressed => 3,    // Suppressed
        FindingStatus::Resolved => 4,      // Resolved
    }
}

/// Map FindingStatus to OCSF activity_id.
/// 1=Create for new/open findings, 2=Update for status-changed findings.
fn activity_for_status(status: FindingStatus) -> u32 {
    match status {
        FindingStatus::Open => 1, // Create
        _ => 2,                   // Update (status changed from Open)
    }
}

/// Convert findings to OCSF Security Finding events.
///
/// Maps each `Finding` to an `OcsfSecurityFinding` with:
/// - Severity mapped via OCSF severity_id scale (0-6)
/// - Status mapped via OCSF status_id (1-4)
/// - Activity derived from finding status (Create vs Update)
/// - Code location mapped to OCSF resource entries
/// - Remediation with references when available
pub fn findings_to_ocsf(
    findings: &[Finding],
    tool_name: &str,
    tool_version: &str,
) -> Vec<OcsfSecurityFinding> {
    findings
        .iter()
        .map(|f| {
            let resources = if let Some(ref loc) = f.location {
                let file_path = loc.file.to_string_lossy().to_string();
                vec![OcsfResource {
                    uid: Some(file_path.clone()),
                    resource_type: "File".to_string(),
                    name: file_path,
                }]
            } else {
                Vec::new()
            };

            let remediation = f.remediation.as_ref().map(|r| {
                let references: Vec<String> = f
                    .references
                    .iter()
                    .filter_map(|rf| rf.url.clone())
                    .collect();
                OcsfRemediation {
                    desc: r.description.clone(),
                    references,
                }
            });

            OcsfSecurityFinding {
                class_uid: 2001,
                category_uid: 2,
                activity_id: activity_for_status(f.status),
                severity_id: severity_to_ocsf(f.severity),
                status_id: status_to_ocsf(f.status),
                finding: OcsfFindingInfo {
                    title: f.title.clone(),
                    uid: f.id.to_string(),
                    desc: Some(f.description.clone()),
                    type_uid: 200100 + activity_for_status(f.status), // 200101=Create, 200102=Update
                    remediation,
                },
                metadata: OcsfMetadata {
                    version: "1.1.0".to_string(),
                    product: OcsfProduct {
                        name: tool_name.to_string(),
                        vendor_name: "Rustant".to_string(),
                        version: tool_version.to_string(),
                    },
                },
                resources,
                time: f.created_at.timestamp_millis(),
                unmapped: None,
            }
        })
        .collect()
}

/// Serialize OCSF events to JSON.
pub fn to_json(events: &[OcsfSecurityFinding]) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{
        CodeLocation, FindingCategory, FindingProvenance, FindingReference, ReferenceType,
        Remediation,
    };

    fn sample_finding() -> Finding {
        Finding::new(
            "XSS Vulnerability",
            "Cross-site scripting via user input",
            FindingSeverity::High,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "sast".into(),
                rule_id: Some("CWE-79".into()),
                confidence: 0.85,
                consensus: None,
            },
        )
    }

    #[test]
    fn test_findings_to_ocsf() {
        let findings = vec![sample_finding()];
        let events = findings_to_ocsf(&findings, "rustant-security", "1.0.0");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].class_uid, 2001);
        assert_eq!(events[0].category_uid, 2);
        assert_eq!(events[0].severity_id, 4); // High
        assert_eq!(events[0].finding.title, "XSS Vulnerability");
    }

    #[test]
    fn test_ocsf_metadata() {
        let events = findings_to_ocsf(&[sample_finding()], "rustant-security", "1.0.0");
        assert_eq!(events[0].metadata.version, "1.1.0");
        assert_eq!(events[0].metadata.product.name, "rustant-security");
        assert_eq!(events[0].metadata.product.vendor_name, "Rustant");
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(severity_to_ocsf(FindingSeverity::Info), 1);
        assert_eq!(severity_to_ocsf(FindingSeverity::Low), 2);
        assert_eq!(severity_to_ocsf(FindingSeverity::Medium), 3);
        assert_eq!(severity_to_ocsf(FindingSeverity::High), 4);
        assert_eq!(severity_to_ocsf(FindingSeverity::Critical), 5);
    }

    #[test]
    fn test_ocsf_json_serialization() {
        let events = findings_to_ocsf(&[sample_finding()], "rustant-security", "1.0.0");
        let json = to_json(&events).unwrap();
        assert!(json.contains("\"class_uid\": 2001"));
        assert!(json.contains("\"category_uid\": 2"));
        assert!(json.contains("XSS Vulnerability"));
    }

    #[test]
    fn test_empty_findings() {
        let events = findings_to_ocsf(&[], "rustant-security", "1.0.0");
        assert!(events.is_empty());
    }

    #[test]
    fn test_status_mapping() {
        assert_eq!(status_to_ocsf(FindingStatus::Open), 1);
        assert_eq!(status_to_ocsf(FindingStatus::FalsePositive), 2);
        assert_eq!(status_to_ocsf(FindingStatus::Suppressed), 3);
        assert_eq!(status_to_ocsf(FindingStatus::Resolved), 4);
    }

    #[test]
    fn test_activity_id_for_status() {
        assert_eq!(activity_for_status(FindingStatus::Open), 1); // Create
        assert_eq!(activity_for_status(FindingStatus::Resolved), 2); // Update
        assert_eq!(activity_for_status(FindingStatus::Suppressed), 2); // Update
        assert_eq!(activity_for_status(FindingStatus::FalsePositive), 2); // Update
    }

    #[test]
    fn test_resolved_finding_maps_to_update() {
        let mut finding = sample_finding();
        finding.resolve();

        let events = findings_to_ocsf(&[finding], "rustant-security", "1.0.0");
        assert_eq!(events[0].activity_id, 2); // Update
        assert_eq!(events[0].status_id, 4); // Resolved
        assert_eq!(events[0].finding.type_uid, 200102); // Security Finding / Update
    }

    #[test]
    fn test_ocsf_resource_with_location() {
        let finding =
            sample_finding().with_location(CodeLocation::new("src/handler.rs", 42).with_range(50));

        let events = findings_to_ocsf(&[finding], "rustant-security", "1.0.0");
        assert_eq!(events[0].resources.len(), 1);
        assert_eq!(events[0].resources[0].resource_type, "File");
        assert_eq!(events[0].resources[0].name, "src/handler.rs");
        assert_eq!(
            events[0].resources[0].uid.as_deref(),
            Some("src/handler.rs")
        );
    }

    #[test]
    fn test_ocsf_remediation_with_references() {
        let finding = sample_finding()
            .with_remediation(Remediation {
                description: "Sanitize user input".into(),
                patch: None,
                effort: None,
                confidence: 0.9,
            })
            .with_reference(FindingReference {
                ref_type: ReferenceType::Cwe,
                id: "CWE-79".into(),
                url: Some("https://cwe.mitre.org/data/definitions/79.html".into()),
            });

        let events = findings_to_ocsf(&[finding], "rustant-security", "1.0.0");
        let remediation = events[0].finding.remediation.as_ref().unwrap();
        assert_eq!(remediation.desc, "Sanitize user input");
        assert_eq!(remediation.references.len(), 1);
        assert!(remediation.references[0].contains("cwe.mitre.org"));
    }

    #[test]
    fn test_single_event_to_json() {
        let finding = sample_finding();
        let events = findings_to_ocsf(&[finding], "rustant-security", "1.0.0");
        let json = events[0].to_json().unwrap();
        assert!(json.contains("\"class_uid\": 2001"));
        assert!(json.contains("XSS Vulnerability"));
        assert!(json.contains("\"severity_id\": 4"));
    }
}
