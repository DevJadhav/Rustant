//! VEX (Vulnerability Exploitability eXchange) — statement generation for
//! vulnerability status communication.
//!
//! Generates VEX statements that document the exploitability status of
//! vulnerabilities in the context of a specific product or project.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// VEX document containing one or more vulnerability statements.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VexDocument {
    /// Document identifier.
    pub id: String,
    /// Document version.
    pub version: String,
    /// Author of the VEX document.
    pub author: String,
    /// When this document was created.
    pub timestamp: DateTime<Utc>,
    /// Last updated timestamp.
    pub last_updated: DateTime<Utc>,
    /// Product this VEX applies to.
    pub product: VexProduct,
    /// Vulnerability statements.
    pub statements: Vec<VexStatement>,
}

/// Product identified in a VEX document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VexProduct {
    /// Product name.
    pub name: String,
    /// Product version.
    pub version: String,
    /// Package URL (purl) if applicable.
    pub purl: Option<String>,
    /// Supplier/vendor.
    pub supplier: Option<String>,
}

/// A VEX statement about a specific vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VexStatement {
    /// Vulnerability identifier (CVE, GHSA, etc.).
    pub vulnerability_id: String,
    /// Exploitability status.
    pub status: VexStatus,
    /// Justification for the status.
    pub justification: Option<VexJustification>,
    /// Impact description.
    pub impact_statement: Option<String>,
    /// Action statement (what the user should do).
    pub action_statement: Option<String>,
    /// When this statement was made.
    pub timestamp: DateTime<Utc>,
    /// Additional notes.
    #[serde(default)]
    pub notes: Vec<String>,
    /// Sub-components affected.
    #[serde(default)]
    pub affected_components: Vec<String>,
}

/// VEX exploitability status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VexStatus {
    /// The vulnerability does not affect this product.
    NotAffected,
    /// The vulnerability is present but has been fixed.
    Fixed,
    /// The vulnerability is present and being investigated.
    UnderInvestigation,
    /// The vulnerability affects this product.
    Affected,
}

impl std::fmt::Display for VexStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VexStatus::NotAffected => write!(f, "Not Affected"),
            VexStatus::Fixed => write!(f, "Fixed"),
            VexStatus::UnderInvestigation => write!(f, "Under Investigation"),
            VexStatus::Affected => write!(f, "Affected"),
        }
    }
}

/// Justification for a "Not Affected" VEX status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VexJustification {
    /// The component is not present in the product.
    ComponentNotPresent,
    /// The vulnerable code is not reachable.
    VulnerableCodeNotPresent,
    /// The vulnerable code cannot be controlled by an attacker.
    VulnerableCodeNotInExecutePath,
    /// Inline mitigations already in place.
    InlineMitigationsAlreadyExist,
}

impl std::fmt::Display for VexJustification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VexJustification::ComponentNotPresent => write!(f, "Component not present"),
            VexJustification::VulnerableCodeNotPresent => write!(f, "Vulnerable code not present"),
            VexJustification::VulnerableCodeNotInExecutePath => {
                write!(f, "Vulnerable code not in execute path")
            }
            VexJustification::InlineMitigationsAlreadyExist => {
                write!(f, "Inline mitigations already exist")
            }
        }
    }
}

/// VEX document generator.
pub struct VexGenerator {
    author: String,
}

impl VexGenerator {
    pub fn new(author: &str) -> Self {
        Self {
            author: author.to_string(),
        }
    }

    /// Create a new VEX document.
    pub fn create_document(&self, product: VexProduct) -> VexDocument {
        let now = Utc::now();
        VexDocument {
            id: format!("VEX-{}", now.format("%Y%m%d%H%M%S")),
            version: "1.0".to_string(),
            author: self.author.clone(),
            timestamp: now,
            last_updated: now,
            product,
            statements: Vec::new(),
        }
    }

    /// Add a "Not Affected" statement.
    pub fn not_affected(
        &self,
        vuln_id: &str,
        justification: VexJustification,
        impact: Option<&str>,
    ) -> VexStatement {
        VexStatement {
            vulnerability_id: vuln_id.to_string(),
            status: VexStatus::NotAffected,
            justification: Some(justification),
            impact_statement: impact.map(|s| s.to_string()),
            action_statement: None,
            timestamp: Utc::now(),
            notes: Vec::new(),
            affected_components: Vec::new(),
        }
    }

    /// Add a "Fixed" statement.
    pub fn fixed(&self, vuln_id: &str, action: &str) -> VexStatement {
        VexStatement {
            vulnerability_id: vuln_id.to_string(),
            status: VexStatus::Fixed,
            justification: None,
            impact_statement: None,
            action_statement: Some(action.to_string()),
            timestamp: Utc::now(),
            notes: Vec::new(),
            affected_components: Vec::new(),
        }
    }

    /// Add an "Under Investigation" statement.
    pub fn under_investigation(&self, vuln_id: &str, notes: Vec<String>) -> VexStatement {
        VexStatement {
            vulnerability_id: vuln_id.to_string(),
            status: VexStatus::UnderInvestigation,
            justification: None,
            impact_statement: None,
            action_statement: None,
            timestamp: Utc::now(),
            notes,
            affected_components: Vec::new(),
        }
    }

    /// Add an "Affected" statement with action.
    pub fn affected(
        &self,
        vuln_id: &str,
        impact: &str,
        action: &str,
        components: Vec<String>,
    ) -> VexStatement {
        VexStatement {
            vulnerability_id: vuln_id.to_string(),
            status: VexStatus::Affected,
            justification: None,
            impact_statement: Some(impact.to_string()),
            action_statement: Some(action.to_string()),
            timestamp: Utc::now(),
            notes: Vec::new(),
            affected_components: components,
        }
    }

    /// Export VEX document to JSON.
    pub fn to_json(doc: &VexDocument) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(doc)
    }

    /// Generate a summary of VEX statements.
    pub fn summarize(doc: &VexDocument) -> VexSummary {
        let mut by_status: HashMap<String, usize> = HashMap::new();

        for stmt in &doc.statements {
            *by_status.entry(stmt.status.to_string()).or_insert(0) += 1;
        }

        VexSummary {
            total_statements: doc.statements.len(),
            by_status,
            product_name: doc.product.name.clone(),
            product_version: doc.product.version.clone(),
        }
    }
}

/// VEX document summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VexSummary {
    pub total_statements: usize,
    pub by_status: HashMap<String, usize>,
    pub product_name: String,
    pub product_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_product() -> VexProduct {
        VexProduct {
            name: "test-app".to_string(),
            version: "1.0.0".to_string(),
            purl: Some("pkg:cargo/test-app@1.0.0".to_string()),
            supplier: Some("Test Corp".to_string()),
        }
    }

    #[test]
    fn test_create_document() {
        let generator = VexGenerator::new("security-team");
        let doc = generator.create_document(test_product());
        assert!(doc.id.starts_with("VEX-"));
        assert_eq!(doc.author, "security-team");
        assert!(doc.statements.is_empty());
    }

    #[test]
    fn test_not_affected_statement() {
        let generator = VexGenerator::new("analyst");
        let stmt = generator.not_affected(
            "CVE-2024-1234",
            VexJustification::VulnerableCodeNotPresent,
            Some("The vulnerable function is not called"),
        );

        assert_eq!(stmt.status, VexStatus::NotAffected);
        assert_eq!(
            stmt.justification,
            Some(VexJustification::VulnerableCodeNotPresent)
        );
        assert!(stmt.impact_statement.is_some());
    }

    #[test]
    fn test_fixed_statement() {
        let generator = VexGenerator::new("analyst");
        let stmt = generator.fixed("CVE-2024-5678", "Upgraded to version 2.0.0");

        assert_eq!(stmt.status, VexStatus::Fixed);
        assert_eq!(
            stmt.action_statement.as_deref(),
            Some("Upgraded to version 2.0.0")
        );
    }

    #[test]
    fn test_affected_statement() {
        let generator = VexGenerator::new("analyst");
        let stmt = generator.affected(
            "CVE-2024-9999",
            "Remote code execution possible",
            "Upgrade to version 3.0",
            vec!["web-handler".to_string()],
        );

        assert_eq!(stmt.status, VexStatus::Affected);
        assert_eq!(stmt.affected_components.len(), 1);
    }

    #[test]
    fn test_under_investigation() {
        let generator = VexGenerator::new("analyst");
        let stmt =
            generator.under_investigation("CVE-2024-0000", vec!["Reviewing impact".to_string()]);

        assert_eq!(stmt.status, VexStatus::UnderInvestigation);
        assert_eq!(stmt.notes.len(), 1);
    }

    #[test]
    fn test_full_document_json() {
        let generator = VexGenerator::new("security-team");
        let mut doc = generator.create_document(test_product());

        doc.statements.push(generator.not_affected(
            "CVE-2024-1234",
            VexJustification::ComponentNotPresent,
            None,
        ));
        doc.statements
            .push(generator.fixed("CVE-2024-5678", "Patched in v1.0.1"));
        doc.statements.push(generator.affected(
            "CVE-2024-9999",
            "Data leak possible",
            "Upgrade ASAP",
            Vec::new(),
        ));

        let json = VexGenerator::to_json(&doc).unwrap();
        assert!(json.contains("CVE-2024-1234"));
        assert!(json.contains("not_affected"));
        assert!(json.contains("fixed"));
    }

    #[test]
    fn test_summarize() {
        let generator = VexGenerator::new("analyst");
        let mut doc = generator.create_document(test_product());

        doc.statements.push(generator.not_affected(
            "CVE-1",
            VexJustification::ComponentNotPresent,
            None,
        ));
        doc.statements.push(generator.not_affected(
            "CVE-2",
            VexJustification::VulnerableCodeNotPresent,
            None,
        ));
        doc.statements.push(generator.fixed("CVE-3", "Fixed"));

        let summary = VexGenerator::summarize(&doc);
        assert_eq!(summary.total_statements, 3);
        assert_eq!(summary.by_status.get("Not Affected"), Some(&2));
        assert_eq!(summary.by_status.get("Fixed"), Some(&1));
    }

    #[test]
    fn test_vex_status_display() {
        assert_eq!(VexStatus::NotAffected.to_string(), "Not Affected");
        assert_eq!(VexStatus::Fixed.to_string(), "Fixed");
        assert_eq!(
            VexStatus::UnderInvestigation.to_string(),
            "Under Investigation"
        );
        assert_eq!(VexStatus::Affected.to_string(), "Affected");
    }

    #[test]
    fn test_justification_display() {
        assert_eq!(
            VexJustification::ComponentNotPresent.to_string(),
            "Component not present"
        );
    }
}
