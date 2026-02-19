//! Compliance framework mappings — SOC 2, ISO 27001, NIST 800-53, PCI DSS,
//! OWASP ASVS, CIS Controls.
//!
//! Maps security findings and controls to compliance framework requirements.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A compliance framework definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceFramework {
    /// Framework identifier.
    pub id: String,
    /// Framework name.
    pub name: String,
    /// Framework version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Controls defined by this framework.
    pub controls: Vec<FrameworkControl>,
}

/// A control within a compliance framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkControl {
    /// Control identifier (e.g., "CC6.1", "A.12.6.1").
    pub id: String,
    /// Control title.
    pub title: String,
    /// Control description.
    pub description: String,
    /// Control category/domain.
    pub domain: String,
    /// Scanner types that provide evidence for this control.
    #[serde(default)]
    pub evidence_scanners: Vec<String>,
    /// CWE IDs that map to this control.
    #[serde(default)]
    pub mapped_cwes: Vec<String>,
    /// Whether this control is required (vs. recommended).
    #[serde(default = "default_true")]
    pub required: bool,
}

fn default_true() -> bool {
    true
}

/// Control compliance status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlStatus {
    /// Fully implemented and evidenced.
    Compliant,
    /// Partially implemented.
    PartiallyCompliant,
    /// Not implemented.
    NonCompliant,
    /// Not applicable to this project.
    NotApplicable,
    /// Not yet assessed.
    NotAssessed,
}

impl std::fmt::Display for ControlStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ControlStatus::Compliant => write!(f, "Compliant"),
            ControlStatus::PartiallyCompliant => write!(f, "Partially Compliant"),
            ControlStatus::NonCompliant => write!(f, "Non-Compliant"),
            ControlStatus::NotApplicable => write!(f, "Not Applicable"),
            ControlStatus::NotAssessed => write!(f, "Not Assessed"),
        }
    }
}

/// Assessment result for a single control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlAssessment {
    /// Control ID.
    pub control_id: String,
    /// Status.
    pub status: ControlStatus,
    /// Evidence collected.
    pub evidence: Vec<String>,
    /// Findings related to this control.
    pub related_findings: Vec<String>,
    /// Notes from assessor.
    pub notes: Option<String>,
}

/// Full compliance assessment for a framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceAssessment {
    /// Framework ID.
    pub framework_id: String,
    /// Framework name.
    pub framework_name: String,
    /// Overall compliance rate (0-100).
    pub compliance_rate: f64,
    /// Individual control assessments.
    pub assessments: Vec<ControlAssessment>,
    /// Summary statistics.
    pub summary: ComplianceSummary,
}

/// Summary statistics for a compliance assessment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComplianceSummary {
    pub total_controls: usize,
    pub compliant: usize,
    pub partially_compliant: usize,
    pub non_compliant: usize,
    pub not_applicable: usize,
    pub not_assessed: usize,
}

/// Framework registry for storing and retrieving compliance frameworks.
pub struct FrameworkRegistry {
    frameworks: HashMap<String, ComplianceFramework>,
}

impl FrameworkRegistry {
    pub fn new() -> Self {
        Self {
            frameworks: HashMap::new(),
        }
    }

    /// Create with built-in compliance frameworks.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.add(Self::soc2());
        registry.add(Self::iso27001());
        registry.add(Self::nist_800_53());
        registry.add(Self::pci_dss());
        registry.add(Self::owasp_asvs());
        registry.add(Self::cis_controls());
        registry
    }

    /// Add a framework.
    pub fn add(&mut self, framework: ComplianceFramework) {
        self.frameworks.insert(framework.id.clone(), framework);
    }

    /// Get a framework by ID.
    pub fn get(&self, id: &str) -> Option<&ComplianceFramework> {
        self.frameworks.get(id)
    }

    /// List all available frameworks.
    pub fn list(&self) -> Vec<&ComplianceFramework> {
        self.frameworks.values().collect()
    }

    /// Find controls that map to a given CWE.
    pub fn controls_for_cwe(&self, cwe_id: &str) -> Vec<(&ComplianceFramework, &FrameworkControl)> {
        let mut results = Vec::new();
        for framework in self.frameworks.values() {
            for control in &framework.controls {
                if control.mapped_cwes.iter().any(|c| c == cwe_id) {
                    results.push((framework, control));
                }
            }
        }
        results
    }

    /// Assess compliance based on scanner results.
    pub fn assess(
        &self,
        framework_id: &str,
        scanners_run: &[String],
        finding_cwes: &[String],
    ) -> Option<ComplianceAssessment> {
        let framework = self.frameworks.get(framework_id)?;

        let mut assessments = Vec::new();
        let mut summary = ComplianceSummary {
            total_controls: framework.controls.len(),
            ..Default::default()
        };

        for control in &framework.controls {
            let has_evidence = control
                .evidence_scanners
                .iter()
                .any(|s| scanners_run.contains(s));

            let has_violations = control
                .mapped_cwes
                .iter()
                .any(|cwe| finding_cwes.contains(cwe));

            let status = if !has_evidence && control.evidence_scanners.is_empty() {
                ControlStatus::NotApplicable
            } else if !has_evidence {
                ControlStatus::NotAssessed
            } else if has_violations {
                ControlStatus::NonCompliant
            } else {
                ControlStatus::Compliant
            };

            match status {
                ControlStatus::Compliant => summary.compliant += 1,
                ControlStatus::PartiallyCompliant => summary.partially_compliant += 1,
                ControlStatus::NonCompliant => summary.non_compliant += 1,
                ControlStatus::NotApplicable => summary.not_applicable += 1,
                ControlStatus::NotAssessed => summary.not_assessed += 1,
            }

            assessments.push(ControlAssessment {
                control_id: control.id.clone(),
                status,
                evidence: if has_evidence {
                    vec!["Automated scan evidence available".to_string()]
                } else {
                    Vec::new()
                },
                related_findings: if has_violations {
                    control.mapped_cwes.clone()
                } else {
                    Vec::new()
                },
                notes: None,
            });
        }

        let assessable = summary.total_controls - summary.not_applicable - summary.not_assessed;
        let compliance_rate = if assessable > 0 {
            (summary.compliant as f64 / assessable as f64) * 100.0
        } else {
            100.0
        };

        Some(ComplianceAssessment {
            framework_id: framework.id.clone(),
            framework_name: framework.name.clone(),
            compliance_rate,
            assessments,
            summary,
        })
    }

    // Built-in framework definitions

    fn soc2() -> ComplianceFramework {
        ComplianceFramework {
            id: "soc2".into(),
            name: "SOC 2 Type II".into(),
            version: "2017".into(),
            description: "Service Organization Control 2 — Trust Services Criteria".into(),
            controls: vec![
                FrameworkControl {
                    id: "CC6.1".into(),
                    title: "Logical and Physical Access Controls".into(),
                    description: "Implements logical access security software and policies".into(),
                    domain: "Common Criteria".into(),
                    evidence_scanners: vec!["sast".into(), "secrets".into()],
                    mapped_cwes: vec!["CWE-798".into(), "CWE-259".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "CC6.6".into(),
                    title: "System Operations Security".into(),
                    description: "Restricts access to system components and data".into(),
                    domain: "Common Criteria".into(),
                    evidence_scanners: vec!["sast".into(), "iac".into()],
                    mapped_cwes: vec!["CWE-284".into(), "CWE-285".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "CC7.1".into(),
                    title: "Change Management".into(),
                    description: "Monitors infrastructure and software for vulnerabilities".into(),
                    domain: "Common Criteria".into(),
                    evidence_scanners: vec!["sca".into(), "container".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "CC8.1".into(),
                    title: "Change Management Process".into(),
                    description: "Changes to software undergo testing and approval".into(),
                    domain: "Common Criteria".into(),
                    evidence_scanners: vec!["sast".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
            ],
        }
    }

    fn iso27001() -> ComplianceFramework {
        ComplianceFramework {
            id: "iso27001".into(),
            name: "ISO/IEC 27001:2022".into(),
            version: "2022".into(),
            description: "Information security management systems".into(),
            controls: vec![
                FrameworkControl {
                    id: "A.8.25".into(),
                    title: "Secure Development Life Cycle".into(),
                    description: "Rules for the development of software shall be established"
                        .into(),
                    domain: "Technology".into(),
                    evidence_scanners: vec!["sast".into(), "sca".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "A.8.26".into(),
                    title: "Application Security Requirements".into(),
                    description: "Security requirements shall be identified and specified".into(),
                    domain: "Technology".into(),
                    evidence_scanners: vec!["sast".into()],
                    mapped_cwes: vec!["CWE-89".into(), "CWE-79".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "A.8.28".into(),
                    title: "Secure Coding".into(),
                    description: "Secure coding practices shall be applied".into(),
                    domain: "Technology".into(),
                    evidence_scanners: vec!["sast".into(), "secrets".into()],
                    mapped_cwes: vec!["CWE-798".into(), "CWE-502".into()],
                    required: true,
                },
            ],
        }
    }

    fn nist_800_53() -> ComplianceFramework {
        ComplianceFramework {
            id: "nist-800-53".into(),
            name: "NIST SP 800-53 Rev. 5".into(),
            version: "Rev. 5".into(),
            description: "Security and Privacy Controls for Information Systems".into(),
            controls: vec![
                FrameworkControl {
                    id: "SA-11".into(),
                    title: "Developer Testing and Evaluation".into(),
                    description: "Developer creates a security and privacy assessment plan".into(),
                    domain: "System and Services Acquisition".into(),
                    evidence_scanners: vec!["sast".into(), "sca".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "SI-10".into(),
                    title: "Information Input Validation".into(),
                    description: "Check validity of information inputs".into(),
                    domain: "System and Information Integrity".into(),
                    evidence_scanners: vec!["sast".into()],
                    mapped_cwes: vec!["CWE-89".into(), "CWE-79".into(), "CWE-78".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "RA-5".into(),
                    title: "Vulnerability Monitoring and Scanning".into(),
                    description: "Monitor and scan for vulnerabilities".into(),
                    domain: "Risk Assessment".into(),
                    evidence_scanners: vec!["sca".into(), "container".into(), "iac".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
            ],
        }
    }

    fn pci_dss() -> ComplianceFramework {
        ComplianceFramework {
            id: "pci-dss".into(),
            name: "PCI DSS v4.0".into(),
            version: "4.0".into(),
            description: "Payment Card Industry Data Security Standard".into(),
            controls: vec![
                FrameworkControl {
                    id: "6.2.4".into(),
                    title: "Secure Software Development".into(),
                    description: "Software engineering techniques prevent common vulnerabilities"
                        .into(),
                    domain: "Secure Systems and Software".into(),
                    evidence_scanners: vec!["sast".into()],
                    mapped_cwes: vec!["CWE-89".into(), "CWE-79".into(), "CWE-22".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "6.3.2".into(),
                    title: "Custom Software Inventory".into(),
                    description: "Inventory of bespoke and custom software components".into(),
                    domain: "Secure Systems and Software".into(),
                    evidence_scanners: vec!["sca".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "11.3.1".into(),
                    title: "Vulnerability Scans".into(),
                    description: "Internal vulnerability scans performed quarterly".into(),
                    domain: "Regular Testing".into(),
                    evidence_scanners: vec!["sast".into(), "sca".into(), "container".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
            ],
        }
    }

    fn owasp_asvs() -> ComplianceFramework {
        ComplianceFramework {
            id: "owasp-asvs".into(),
            name: "OWASP ASVS v4.0".into(),
            version: "4.0.3".into(),
            description: "Application Security Verification Standard".into(),
            controls: vec![
                FrameworkControl {
                    id: "V5.3".into(),
                    title: "Output Encoding and Injection Prevention".into(),
                    description: "Verify output encoding for the context".into(),
                    domain: "Validation".into(),
                    evidence_scanners: vec!["sast".into()],
                    mapped_cwes: vec!["CWE-79".into(), "CWE-89".into(), "CWE-78".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "V2.10".into(),
                    title: "Service Authentication".into(),
                    description: "Verify service authentication secrets are not hardcoded".into(),
                    domain: "Authentication".into(),
                    evidence_scanners: vec!["secrets".into()],
                    mapped_cwes: vec!["CWE-798".into()],
                    required: true,
                },
                FrameworkControl {
                    id: "V14.2".into(),
                    title: "Dependency".into(),
                    description: "Verify all components are up to date".into(),
                    domain: "Configuration".into(),
                    evidence_scanners: vec!["sca".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
            ],
        }
    }

    fn cis_controls() -> ComplianceFramework {
        ComplianceFramework {
            id: "cis-controls".into(),
            name: "CIS Controls v8".into(),
            version: "8".into(),
            description: "Center for Internet Security Critical Security Controls".into(),
            controls: vec![
                FrameworkControl {
                    id: "CIS-2".into(),
                    title: "Inventory and Control of Software Assets".into(),
                    description: "Actively manage all software on the network".into(),
                    domain: "Basic".into(),
                    evidence_scanners: vec!["sca".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "CIS-7".into(),
                    title: "Continuous Vulnerability Management".into(),
                    description: "Continuously assess and remediate vulnerabilities".into(),
                    domain: "Basic".into(),
                    evidence_scanners: vec!["sast".into(), "sca".into(), "container".into()],
                    mapped_cwes: Vec::new(),
                    required: true,
                },
                FrameworkControl {
                    id: "CIS-16".into(),
                    title: "Application Software Security".into(),
                    description: "Manage security throughout the software lifecycle".into(),
                    domain: "Organizational".into(),
                    evidence_scanners: vec!["sast".into(), "secrets".into()],
                    mapped_cwes: vec!["CWE-89".into(), "CWE-79".into(), "CWE-798".into()],
                    required: true,
                },
            ],
        }
    }
}

impl Default for FrameworkRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_frameworks() {
        let registry = FrameworkRegistry::with_defaults();
        assert_eq!(registry.list().len(), 6);
        assert!(registry.get("soc2").is_some());
        assert!(registry.get("iso27001").is_some());
        assert!(registry.get("nist-800-53").is_some());
        assert!(registry.get("pci-dss").is_some());
        assert!(registry.get("owasp-asvs").is_some());
        assert!(registry.get("cis-controls").is_some());
    }

    #[test]
    fn test_controls_for_cwe() {
        let registry = FrameworkRegistry::with_defaults();
        let results = registry.controls_for_cwe("CWE-89");
        assert!(
            !results.is_empty(),
            "SQL injection should map to multiple frameworks"
        );
    }

    #[test]
    fn test_assess_compliant() {
        let registry = FrameworkRegistry::with_defaults();
        let assessment = registry
            .assess(
                "owasp-asvs",
                &["sast".into(), "secrets".into(), "sca".into()],
                &[], // No findings
            )
            .unwrap();

        assert_eq!(assessment.framework_id, "owasp-asvs");
        assert!(assessment.compliance_rate > 0.0);
        assert!(assessment.summary.compliant > 0);
    }

    #[test]
    fn test_assess_non_compliant() {
        let registry = FrameworkRegistry::with_defaults();
        let assessment = registry
            .assess(
                "owasp-asvs",
                &["sast".into(), "secrets".into(), "sca".into()],
                &["CWE-89".into()], // SQL injection found
            )
            .unwrap();

        assert!(assessment.summary.non_compliant > 0);
        assert!(assessment.compliance_rate < 100.0);
    }

    #[test]
    fn test_assess_not_assessed() {
        let registry = FrameworkRegistry::with_defaults();
        let assessment = registry
            .assess(
                "soc2",
                &[], // No scanners run
                &[],
            )
            .unwrap();

        assert!(assessment.summary.not_assessed > 0);
    }

    #[test]
    fn test_assess_unknown_framework() {
        let registry = FrameworkRegistry::with_defaults();
        assert!(registry.assess("nonexistent", &[], &[]).is_none());
    }

    #[test]
    fn test_control_status_display() {
        assert_eq!(ControlStatus::Compliant.to_string(), "Compliant");
        assert_eq!(ControlStatus::NonCompliant.to_string(), "Non-Compliant");
        assert_eq!(
            ControlStatus::PartiallyCompliant.to_string(),
            "Partially Compliant"
        );
    }

    #[test]
    fn test_framework_structure() {
        let registry = FrameworkRegistry::with_defaults();
        let soc2 = registry.get("soc2").unwrap();
        assert!(!soc2.controls.is_empty());

        let first = &soc2.controls[0];
        assert!(!first.id.is_empty());
        assert!(!first.domain.is_empty());
        assert!(first.required);
    }
}
