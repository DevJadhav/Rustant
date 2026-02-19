//! SARIF 2.1.0 export — Static Analysis Results Interchange Format.
//!
//! Converts findings to SARIF JSON for integration with GitHub Code Scanning,
//! Azure DevOps, and other SARIF-compatible tools.

use crate::finding::{Finding, FindingSeverity};
use serde::Serialize;

/// A complete SARIF log.
#[derive(Debug, Serialize)]
pub struct SarifLog {
    /// SARIF schema URL.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// SARIF version.
    pub version: String,
    /// Analysis runs.
    pub runs: Vec<SarifRun>,
}

/// A single analysis run in SARIF.
#[derive(Debug, Serialize)]
pub struct SarifRun {
    /// Tool information.
    pub tool: SarifTool,
    /// Results (findings).
    pub results: Vec<SarifResult>,
}

/// Tool information in SARIF.
#[derive(Debug, Serialize)]
pub struct SarifTool {
    pub driver: SarifDriver,
}

/// Tool driver information.
#[derive(Debug, Serialize)]
pub struct SarifDriver {
    pub name: String,
    pub version: String,
    #[serde(rename = "informationUri", skip_serializing_if = "Option::is_none")]
    pub information_uri: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<SarifRule>,
}

/// A SARIF rule definition.
#[derive(Debug, Serialize)]
pub struct SarifRule {
    pub id: String,
    #[serde(rename = "shortDescription")]
    pub short_description: SarifMessage,
    #[serde(rename = "fullDescription", skip_serializing_if = "Option::is_none")]
    pub full_description: Option<SarifMessage>,
    #[serde(
        rename = "defaultConfiguration",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_configuration: Option<SarifRuleConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<SarifMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<SarifProperty>,
}

/// SARIF rule configuration.
#[derive(Debug, Serialize)]
pub struct SarifRuleConfiguration {
    pub level: String,
}

/// A message in SARIF.
#[derive(Debug, Serialize)]
pub struct SarifMessage {
    pub text: String,
}

/// A property bag entry.
#[derive(Debug, Serialize)]
pub struct SarifProperty {
    pub key: String,
    pub value: String,
}

/// A SARIF result (finding).
#[derive(Debug, Serialize)]
pub struct SarifResult {
    /// Rule ID.
    #[serde(rename = "ruleId")]
    pub rule_id: String,
    /// Severity level.
    pub level: String,
    /// Result message.
    pub message: SarifMessage,
    /// Locations.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<SarifLocation>,
    /// Fix suggestions.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<SarifFix>,
    /// Fingerprint for deduplication.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprints: Option<SarifFingerprints>,
}

/// SARIF location.
#[derive(Debug, Serialize)]
pub struct SarifLocation {
    #[serde(rename = "physicalLocation")]
    pub physical_location: SarifPhysicalLocation,
}

/// SARIF physical location.
#[derive(Debug, Serialize)]
pub struct SarifPhysicalLocation {
    #[serde(rename = "artifactLocation")]
    pub artifact_location: SarifArtifactLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<SarifRegion>,
}

/// SARIF artifact location.
#[derive(Debug, Serialize)]
pub struct SarifArtifactLocation {
    pub uri: String,
}

/// SARIF region (line/column span).
#[derive(Debug, Serialize)]
pub struct SarifRegion {
    #[serde(rename = "startLine")]
    pub start_line: usize,
    #[serde(rename = "endLine", skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(rename = "startColumn", skip_serializing_if = "Option::is_none")]
    pub start_column: Option<usize>,
    #[serde(rename = "endColumn", skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
}

/// SARIF fix suggestion.
#[derive(Debug, Serialize)]
pub struct SarifFix {
    pub description: SarifMessage,
}

/// SARIF fingerprints for deduplication.
#[derive(Debug, Serialize)]
pub struct SarifFingerprints {
    #[serde(rename = "contentHash/sha256")]
    pub content_hash: String,
}

/// Convert a severity to SARIF level string.
fn severity_to_sarif_level(severity: FindingSeverity) -> &'static str {
    match severity {
        FindingSeverity::Critical | FindingSeverity::High => "error",
        FindingSeverity::Medium => "warning",
        FindingSeverity::Low => "note",
        FindingSeverity::Info => "none",
    }
}

/// Convert findings to a SARIF log.
pub fn findings_to_sarif(findings: &[Finding], tool_name: &str, tool_version: &str) -> SarifLog {
    // Collect unique rules
    let mut rules = Vec::new();
    let mut seen_rules = std::collections::HashSet::new();

    for finding in findings {
        if let Some(ref rule_id) = finding.provenance.rule_id
            && seen_rules.insert(rule_id.clone())
        {
            rules.push(SarifRule {
                id: rule_id.clone(),
                short_description: SarifMessage {
                    text: finding.title.clone(),
                },
                full_description: Some(SarifMessage {
                    text: finding.description.clone(),
                }),
                default_configuration: Some(SarifRuleConfiguration {
                    level: severity_to_sarif_level(finding.severity).to_string(),
                }),
                help: finding.remediation.as_ref().map(|r| SarifMessage {
                    text: r.description.clone(),
                }),
                properties: Vec::new(),
            });
        }
    }

    let results: Vec<SarifResult> = findings
        .iter()
        .map(|f| {
            let locations = if let Some(ref loc) = f.location {
                vec![SarifLocation {
                    physical_location: SarifPhysicalLocation {
                        artifact_location: SarifArtifactLocation {
                            uri: loc.file.to_string_lossy().to_string(),
                        },
                        region: Some(SarifRegion {
                            start_line: loc.start_line,
                            end_line: loc.end_line,
                            start_column: loc.start_column,
                            end_column: loc.end_column,
                        }),
                    },
                }]
            } else {
                Vec::new()
            };

            let fixes = if let Some(ref rem) = f.remediation {
                vec![SarifFix {
                    description: SarifMessage {
                        text: rem.description.clone(),
                    },
                }]
            } else {
                Vec::new()
            };

            SarifResult {
                rule_id: f
                    .provenance
                    .rule_id
                    .clone()
                    .unwrap_or_else(|| f.provenance.scanner.clone()),
                level: severity_to_sarif_level(f.severity).to_string(),
                message: SarifMessage {
                    text: f.title.clone(),
                },
                locations,
                fixes,
                fingerprints: Some(SarifFingerprints {
                    content_hash: f.content_hash.clone(),
                }),
            }
        })
        .collect();

    SarifLog {
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/main/sarif-2.1/schema/sarif-schema-2.1.0.json".to_string(),
        version: "2.1.0".to_string(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifDriver {
                    name: tool_name.to_string(),
                    version: tool_version.to_string(),
                    information_uri: None,
                    rules,
                },
            },
            results,
        }],
    }
}

/// Serialize a SARIF log to JSON string.
pub fn to_json(sarif: &SarifLog) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(sarif)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{CodeLocation, FindingProvenance};

    fn sample_finding() -> Finding {
        Finding::new(
            "SQL Injection",
            "SQL injection via string concatenation",
            FindingSeverity::Critical,
            crate::finding::FindingCategory::Security,
            FindingProvenance {
                scanner: "sast".into(),
                rule_id: Some("CWE-89".into()),
                confidence: 0.9,
                consensus: None,
            },
        )
        .with_location(CodeLocation::new("src/db.py", 42).with_range(44))
    }

    #[test]
    fn test_findings_to_sarif() {
        let findings = vec![sample_finding()];
        let sarif = findings_to_sarif(&findings, "rustant-security", "1.0.0");

        assert_eq!(sarif.version, "2.1.0");
        assert_eq!(sarif.runs.len(), 1);
        assert_eq!(sarif.runs[0].results.len(), 1);
        assert_eq!(sarif.runs[0].results[0].level, "error");
        assert_eq!(sarif.runs[0].results[0].rule_id, "CWE-89");
    }

    #[test]
    fn test_sarif_has_location() {
        let findings = vec![sample_finding()];
        let sarif = findings_to_sarif(&findings, "rustant-security", "1.0.0");
        let result = &sarif.runs[0].results[0];

        assert_eq!(result.locations.len(), 1);
        let loc = &result.locations[0].physical_location;
        assert_eq!(loc.artifact_location.uri, "src/db.py");
        assert_eq!(loc.region.as_ref().unwrap().start_line, 42);
    }

    #[test]
    fn test_sarif_json_serialization() {
        let findings = vec![sample_finding()];
        let sarif = findings_to_sarif(&findings, "rustant-security", "1.0.0");
        let json = to_json(&sarif).unwrap();
        assert!(json.contains("\"$schema\""));
        assert!(json.contains("\"version\": \"2.1.0\""));
        assert!(json.contains("CWE-89"));
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(severity_to_sarif_level(FindingSeverity::Critical), "error");
        assert_eq!(severity_to_sarif_level(FindingSeverity::High), "error");
        assert_eq!(severity_to_sarif_level(FindingSeverity::Medium), "warning");
        assert_eq!(severity_to_sarif_level(FindingSeverity::Low), "note");
        assert_eq!(severity_to_sarif_level(FindingSeverity::Info), "none");
    }

    #[test]
    fn test_empty_findings() {
        let sarif = findings_to_sarif(&[], "rustant-security", "1.0.0");
        assert_eq!(sarif.runs[0].results.len(), 0);
        assert_eq!(sarif.runs[0].tool.driver.rules.len(), 0);
    }

    #[test]
    fn test_sarif_rules_dedup() {
        let f1 = sample_finding();
        let f2 = sample_finding(); // Same rule ID
        let sarif = findings_to_sarif(&[f1, f2], "rustant-security", "1.0.0");
        assert_eq!(
            sarif.runs[0].tool.driver.rules.len(),
            1,
            "Duplicate rules should be deduped"
        );
        assert_eq!(sarif.runs[0].results.len(), 2);
    }
}
