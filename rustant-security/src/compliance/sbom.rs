//! SBOM generation — CycloneDX 1.5 and SPDX 2.3 compatible output.
//!
//! Generates Software Bill of Materials from dependency graph data.

use crate::dep_graph::DepNode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// SBOM output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SbomFormat {
    /// CycloneDX 1.5 JSON.
    CycloneDx,
    /// SPDX 2.3 JSON.
    Spdx,
    /// Simplified CSV.
    Csv,
}

impl std::fmt::Display for SbomFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SbomFormat::CycloneDx => write!(f, "CycloneDX 1.5"),
            SbomFormat::Spdx => write!(f, "SPDX 2.3"),
            SbomFormat::Csv => write!(f, "CSV"),
        }
    }
}

/// A component in the SBOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomComponent {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Ecosystem/package type (npm, cargo, pypi, etc.).
    pub purl_type: String,
    /// Package URL (purl) identifier.
    pub purl: String,
    /// SPDX license identifier.
    pub license: Option<String>,
    /// Whether this is a direct dependency.
    pub is_direct: bool,
    /// Whether this is dev-only.
    pub is_dev: bool,
    /// Component scope.
    pub scope: ComponentScope,
    /// Known vulnerabilities (CVE IDs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vulnerabilities: Vec<String>,
    /// Component hash (SHA-256 if available).
    pub hash: Option<String>,
}

/// Component scope classification.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComponentScope {
    #[default]
    Required,
    Optional,
    Excluded,
}

/// A complete SBOM document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sbom {
    /// SBOM format version.
    pub format: SbomFormat,
    /// Spec version string.
    pub spec_version: String,
    /// Serial number / document namespace.
    pub serial_number: String,
    /// Tool that generated this SBOM.
    pub tool_name: String,
    /// Tool version.
    pub tool_version: String,
    /// When this SBOM was generated.
    pub created_at: DateTime<Utc>,
    /// Subject component (the project itself).
    pub subject: SbomSubject,
    /// All components (dependencies).
    pub components: Vec<SbomComponent>,
    /// Summary statistics.
    pub summary: SbomSummary,
}

/// The subject (top-level project) of the SBOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomSubject {
    /// Project name.
    pub name: String,
    /// Project version.
    pub version: String,
    /// Project description.
    pub description: Option<String>,
}

/// SBOM summary statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SbomSummary {
    pub total_components: usize,
    pub direct_dependencies: usize,
    pub transitive_dependencies: usize,
    pub dev_dependencies: usize,
    pub with_known_vulnerabilities: usize,
    pub ecosystems: HashMap<String, usize>,
    pub licenses: HashMap<String, usize>,
}

/// Result of comparing two SBOMs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomDiff {
    /// Components added in the new SBOM.
    pub added: Vec<SbomComponent>,
    /// Components removed from the old SBOM.
    pub removed: Vec<SbomComponent>,
    /// Components with version changes.
    pub changed: Vec<SbomVersionChange>,
    /// Summary of changes.
    pub summary: SbomDiffSummary,
}

/// A version change between two SBOMs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomVersionChange {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub ecosystem: String,
}

/// Summary of SBOM diff.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SbomDiffSummary {
    pub added_count: usize,
    pub removed_count: usize,
    pub changed_count: usize,
    pub total_changes: usize,
}

/// SBOM generator.
pub struct SbomGenerator {
    tool_name: String,
    tool_version: String,
}

impl SbomGenerator {
    /// Create a new generator.
    pub fn new(tool_name: &str, tool_version: &str) -> Self {
        Self {
            tool_name: tool_name.to_string(),
            tool_version: tool_version.to_string(),
        }
    }

    /// Generate SBOM from dependency nodes.
    pub fn generate(
        &self,
        format: SbomFormat,
        subject_name: &str,
        subject_version: &str,
        deps: &[DepNode],
    ) -> Sbom {
        let mut components = Vec::new();
        let mut summary = SbomSummary::default();

        for dep in deps {
            let purl = format!("pkg:{}/{}/{}", dep.ecosystem, dep.name, dep.version);
            let scope = if dep.is_dev {
                ComponentScope::Optional
            } else {
                ComponentScope::Required
            };

            components.push(SbomComponent {
                name: dep.name.clone(),
                version: dep.version.clone(),
                purl_type: dep.ecosystem.clone(),
                purl,
                license: dep.license.clone(),
                is_direct: dep.is_direct,
                is_dev: dep.is_dev,
                scope,
                vulnerabilities: Vec::new(),
                hash: None,
            });

            // Update summary
            summary.total_components += 1;
            if dep.is_direct {
                summary.direct_dependencies += 1;
            } else {
                summary.transitive_dependencies += 1;
            }
            if dep.is_dev {
                summary.dev_dependencies += 1;
            }
            *summary.ecosystems.entry(dep.ecosystem.clone()).or_insert(0) += 1;
            if let Some(ref license) = dep.license {
                *summary.licenses.entry(license.clone()).or_insert(0) += 1;
            }
        }

        let spec_version = match format {
            SbomFormat::CycloneDx => "1.5".to_string(),
            SbomFormat::Spdx => "SPDX-2.3".to_string(),
            SbomFormat::Csv => "1.0".to_string(),
        };

        Sbom {
            format,
            spec_version,
            serial_number: uuid::Uuid::new_v4().to_string(),
            tool_name: self.tool_name.clone(),
            tool_version: self.tool_version.clone(),
            created_at: Utc::now(),
            subject: SbomSubject {
                name: subject_name.to_string(),
                version: subject_version.to_string(),
                description: None,
            },
            components,
            summary,
        }
    }

    /// Export SBOM to CycloneDX JSON format.
    pub fn to_cyclonedx_json(&self, sbom: &Sbom) -> Result<String, serde_json::Error> {
        let cdx = serde_json::json!({
            "bomFormat": "CycloneDX",
            "specVersion": "1.5",
            "serialNumber": format!("urn:uuid:{}", sbom.serial_number),
            "version": 1,
            "metadata": {
                "timestamp": sbom.created_at.to_rfc3339(),
                "tools": [{
                    "vendor": "Rustant",
                    "name": sbom.tool_name,
                    "version": sbom.tool_version,
                }],
                "component": {
                    "type": "application",
                    "name": sbom.subject.name,
                    "version": sbom.subject.version,
                }
            },
            "components": sbom.components.iter().map(|c| {
                serde_json::json!({
                    "type": "library",
                    "name": c.name,
                    "version": c.version,
                    "purl": c.purl,
                    "scope": if c.is_dev { "excluded" } else { "required" },
                    "licenses": c.license.as_ref().map(|l| vec![
                        serde_json::json!({"license": {"id": l}})
                    ]).unwrap_or_default(),
                })
            }).collect::<Vec<_>>(),
        });
        serde_json::to_string_pretty(&cdx)
    }

    /// Export SBOM to CSV format.
    pub fn to_csv(&self, sbom: &Sbom) -> String {
        let mut csv = String::from("name,version,ecosystem,license,direct,dev,purl\n");
        for c in &sbom.components {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{}\n",
                c.name,
                c.version,
                c.purl_type,
                c.license.as_deref().unwrap_or("unknown"),
                c.is_direct,
                c.is_dev,
                c.purl,
            ));
        }
        csv
    }
}

/// Compare two SBOMs and produce a diff.
pub fn diff_sboms(old: &Sbom, new: &Sbom) -> SbomDiff {
    let old_map: HashMap<&str, &SbomComponent> = old
        .components
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let new_map: HashMap<&str, &SbomComponent> = new
        .components
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();

    // Find added and changed
    for (name, new_comp) in &new_map {
        if let Some(old_comp) = old_map.get(name) {
            if old_comp.version != new_comp.version {
                changed.push(SbomVersionChange {
                    name: name.to_string(),
                    old_version: old_comp.version.clone(),
                    new_version: new_comp.version.clone(),
                    ecosystem: new_comp.purl_type.clone(),
                });
            }
        } else {
            added.push((*new_comp).clone());
        }
    }

    // Find removed
    for (name, old_comp) in &old_map {
        if !new_map.contains_key(name) {
            removed.push((*old_comp).clone());
        }
    }

    let summary = SbomDiffSummary {
        added_count: added.len(),
        removed_count: removed.len(),
        changed_count: changed.len(),
        total_changes: added.len() + removed.len() + changed.len(),
    };

    SbomDiff {
        added,
        removed,
        changed,
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_deps() -> Vec<DepNode> {
        vec![
            DepNode {
                name: "serde".into(),
                version: "1.0.200".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("MIT OR Apache-2.0".into()),
                source: None,
            },
            DepNode {
                name: "tokio".into(),
                version: "1.37.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("MIT".into()),
                source: None,
            },
            DepNode {
                name: "proptest".into(),
                version: "1.4.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: true,
                license: Some("MIT OR Apache-2.0".into()),
                source: None,
            },
            DepNode {
                name: "serde_derive".into(),
                version: "1.0.200".into(),
                ecosystem: "cargo".into(),
                is_direct: false,
                is_dev: false,
                license: Some("MIT OR Apache-2.0".into()),
                source: None,
            },
        ]
    }

    #[test]
    fn test_generate_sbom() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let sbom = generator.generate(SbomFormat::CycloneDx, "my-project", "0.1.0", &sample_deps());

        assert_eq!(sbom.format, SbomFormat::CycloneDx);
        assert_eq!(sbom.spec_version, "1.5");
        assert_eq!(sbom.components.len(), 4);
        assert_eq!(sbom.summary.total_components, 4);
        assert_eq!(sbom.summary.direct_dependencies, 3);
        assert_eq!(sbom.summary.transitive_dependencies, 1);
        assert_eq!(sbom.summary.dev_dependencies, 1);
    }

    #[test]
    fn test_purl_generation() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.0", &sample_deps());

        assert_eq!(sbom.components[0].purl, "pkg:cargo/serde/1.0.200");
        assert_eq!(sbom.components[1].purl, "pkg:cargo/tokio/1.37.0");
    }

    #[test]
    fn test_cyclonedx_json() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.0", &sample_deps());
        let json = generator.to_cyclonedx_json(&sbom).unwrap();

        assert!(json.contains("\"bomFormat\": \"CycloneDX\""));
        assert!(json.contains("\"specVersion\": \"1.5\""));
        assert!(json.contains("serde"));
        assert!(json.contains("tokio"));
    }

    #[test]
    fn test_csv_export() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let sbom = generator.generate(SbomFormat::Csv, "test", "1.0.0", &sample_deps());
        let csv = generator.to_csv(&sbom);

        assert!(csv.starts_with("name,version,ecosystem,license,direct,dev,purl\n"));
        assert!(csv.contains("serde,1.0.200,cargo,MIT OR Apache-2.0,true,false"));
        assert!(csv.contains("proptest,1.4.0,cargo,MIT OR Apache-2.0,true,true"));
    }

    #[test]
    fn test_sbom_diff() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let old_deps = sample_deps();
        let mut new_deps = sample_deps();
        // Change serde version
        new_deps[0].version = "1.0.201".into();
        // Remove proptest
        new_deps.remove(2);
        // Add new dep
        new_deps.push(DepNode {
            name: "anyhow".into(),
            version: "1.0.0".into(),
            ecosystem: "cargo".into(),
            is_direct: true,
            is_dev: false,
            license: Some("MIT OR Apache-2.0".into()),
            source: None,
        });

        let old_sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.0", &old_deps);
        let new_sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.1", &new_deps);

        let diff = diff_sboms(&old_sbom, &new_sbom);
        assert_eq!(diff.summary.added_count, 1);
        assert_eq!(diff.summary.removed_count, 1);
        assert_eq!(diff.summary.changed_count, 1);
        assert_eq!(diff.added[0].name, "anyhow");
        assert_eq!(diff.removed[0].name, "proptest");
        assert_eq!(diff.changed[0].name, "serde");
    }

    #[test]
    fn test_sbom_format_display() {
        assert_eq!(SbomFormat::CycloneDx.to_string(), "CycloneDX 1.5");
        assert_eq!(SbomFormat::Spdx.to_string(), "SPDX 2.3");
        assert_eq!(SbomFormat::Csv.to_string(), "CSV");
    }

    #[test]
    fn test_empty_sbom() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let sbom = generator.generate(SbomFormat::CycloneDx, "empty", "0.1.0", &[]);

        assert_eq!(sbom.components.len(), 0);
        assert_eq!(sbom.summary.total_components, 0);
    }

    #[test]
    fn test_ecosystem_summary() {
        let generator = SbomGenerator::new("rustant-security", "1.0.0");
        let mut deps = sample_deps();
        deps.push(DepNode {
            name: "express".into(),
            version: "4.18.0".into(),
            ecosystem: "npm".into(),
            is_direct: true,
            is_dev: false,
            license: Some("MIT".into()),
            source: None,
        });

        let sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.0", &deps);
        assert_eq!(sbom.summary.ecosystems.get("cargo"), Some(&4));
        assert_eq!(sbom.summary.ecosystems.get("npm"), Some(&1));
    }
}
