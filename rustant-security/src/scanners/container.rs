//! Container security scanner — Trivy adapter for image vulnerability scanning.
//!
//! Provides container image scanning via sandboxed shell execution of Trivy,
//! plus native Dockerfile analysis and base image assessment.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance,
    FindingReference, FindingSeverity, ReferenceType, Remediation, RemediationEffort,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Container vulnerability entry from Trivy JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerVulnerability {
    /// CVE identifier.
    pub cve_id: String,
    /// Affected package name.
    pub package: String,
    /// Installed version.
    pub installed_version: String,
    /// Fixed version, if available.
    pub fixed_version: Option<String>,
    /// Severity level.
    pub severity: String,
    /// Description.
    pub description: String,
    /// Layer in which the vulnerable package was introduced.
    pub layer: Option<String>,
}

/// Container image metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageMetadata {
    /// Image name/tag.
    pub image: String,
    /// Base image if detectable.
    pub base_image: Option<String>,
    /// Total layers.
    pub layer_count: usize,
    /// Image size in bytes.
    pub size_bytes: Option<u64>,
    /// OS detected in image.
    pub os: Option<String>,
}

/// Result of a container security scan.
#[derive(Debug, Clone)]
pub struct ContainerScanResult {
    /// Image metadata.
    pub metadata: ImageMetadata,
    /// Vulnerabilities found.
    pub vulnerabilities: Vec<ContainerVulnerability>,
    /// Findings converted to unified schema.
    pub findings: Vec<Finding>,
    /// Summary statistics.
    pub summary: ContainerScanSummary,
}

/// Summary of container scan results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContainerScanSummary {
    pub total_vulnerabilities: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub fixable: usize,
}

/// Container security scanner using Trivy.
pub struct ContainerScanner {
    /// Path to trivy binary.
    _trivy_path: String,
    /// Whether trivy is available on the system.
    available: bool,
}

impl ContainerScanner {
    /// Create a new container scanner.
    pub fn new() -> Self {
        let available = Self::check_trivy_available("trivy");
        Self {
            _trivy_path: "trivy".to_string(),
            available,
        }
    }

    /// Create with a specific trivy path.
    pub fn with_trivy_path(path: &str) -> Self {
        let available = Self::check_trivy_available(path);
        Self {
            _trivy_path: path.to_string(),
            available,
        }
    }

    fn check_trivy_available(path: &str) -> bool {
        std::process::Command::new(path)
            .arg("--version")
            .output()
            .is_ok()
    }

    /// Parse Trivy JSON output into vulnerabilities.
    pub fn parse_trivy_output(json_str: &str) -> Result<Vec<ContainerVulnerability>, ScanError> {
        let value: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| ScanError::ScannerFailed {
                scanner: "container".into(),
                message: format!("Failed to parse Trivy output: {e}"),
            })?;

        let mut vulns = Vec::new();

        // Trivy JSON format: { "Results": [ { "Vulnerabilities": [...] } ] }
        if let Some(results) = value.get("Results").and_then(|r| r.as_array()) {
            for result in results {
                if let Some(vulnerabilities) =
                    result.get("Vulnerabilities").and_then(|v| v.as_array())
                {
                    for vuln in vulnerabilities {
                        let cve_id = vuln
                            .get("VulnerabilityID")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let package = vuln
                            .get("PkgName")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let installed_version = vuln
                            .get("InstalledVersion")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let fixed_version = vuln
                            .get("FixedVersion")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let severity = vuln
                            .get("Severity")
                            .and_then(|v| v.as_str())
                            .unwrap_or("UNKNOWN")
                            .to_string();
                        let description = vuln
                            .get("Description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let layer = vuln
                            .get("Layer")
                            .and_then(|l| l.get("DiffID"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        vulns.push(ContainerVulnerability {
                            cve_id,
                            package,
                            installed_version,
                            fixed_version,
                            severity,
                            description,
                            layer,
                        });
                    }
                }
            }
        }

        Ok(vulns)
    }

    /// Convert a container vulnerability to a unified Finding.
    pub fn vuln_to_finding(vuln: &ContainerVulnerability, image: &str) -> Finding {
        let severity = match vuln.severity.to_uppercase().as_str() {
            "CRITICAL" => FindingSeverity::Critical,
            "HIGH" => FindingSeverity::High,
            "MEDIUM" => FindingSeverity::Medium,
            "LOW" => FindingSeverity::Low,
            _ => FindingSeverity::Info,
        };

        let mut finding = Finding::new(
            format!("{}: {} in {}", vuln.cve_id, vuln.package, image),
            vuln.description.clone(),
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "container".to_string(),
                rule_id: Some(vuln.cve_id.clone()),
                confidence: 0.95,
                consensus: None,
            },
        );

        finding = finding.with_reference(FindingReference {
            ref_type: ReferenceType::Cve,
            id: vuln.cve_id.clone(),
            url: Some(format!("https://nvd.nist.gov/vuln/detail/{}", vuln.cve_id)),
        });

        if let Some(ref fixed) = vuln.fixed_version {
            finding = finding.with_remediation(Remediation {
                description: format!(
                    "Upgrade {} from {} to {}",
                    vuln.package, vuln.installed_version, fixed
                ),
                patch: None,
                effort: Some(RemediationEffort::Low),
                confidence: 0.9,
            });
        }

        finding = finding.with_explanation(FindingExplanation {
            reasoning_chain: vec![
                format!(
                    "Package {} version {} is installed in image {}",
                    vuln.package, vuln.installed_version, image
                ),
                format!("Vulnerability {} affects this version", vuln.cve_id),
            ],
            evidence: vec![format!(
                "Installed: {}@{}, Severity: {}",
                vuln.package, vuln.installed_version, vuln.severity
            )],
            context_factors: vec![if vuln.fixed_version.is_some() {
                "A fix is available".to_string()
            } else {
                "No fix available yet".to_string()
            }],
        });

        finding.with_tag("container")
    }

    /// Summarize vulnerabilities.
    pub fn summarize(vulns: &[ContainerVulnerability]) -> ContainerScanSummary {
        let mut summary = ContainerScanSummary {
            total_vulnerabilities: vulns.len(),
            ..Default::default()
        };

        for v in vulns {
            match v.severity.to_uppercase().as_str() {
                "CRITICAL" => summary.critical += 1,
                "HIGH" => summary.high += 1,
                "MEDIUM" => summary.medium += 1,
                "LOW" => summary.low += 1,
                _ => {}
            }
            if v.fixed_version.is_some() {
                summary.fixable += 1;
            }
        }

        summary
    }

    /// Recommend a hardened base image alternative.
    pub fn recommend_base_image(current_base: &str) -> Option<String> {
        let recommendations: HashMap<&str, &str> = HashMap::from([
            ("ubuntu", "ubuntu:22.04-minimal or distroless"),
            ("debian", "debian:bookworm-slim or distroless"),
            ("alpine", "alpine:3.19 (already minimal)"),
            ("node", "node:20-alpine or node:20-slim"),
            ("python", "python:3.12-slim or python:3.12-alpine"),
            (
                "golang",
                "golang:1.22-alpine (for build), scratch/distroless (for runtime)",
            ),
            ("ruby", "ruby:3.3-slim or ruby:3.3-alpine"),
            (
                "openjdk",
                "eclipse-temurin:21-jre-alpine or distroless/java",
            ),
            ("nginx", "nginx:1.25-alpine"),
            ("postgres", "postgres:16-alpine"),
            ("redis", "redis:7-alpine"),
        ]);

        let lower = current_base.to_lowercase();
        for (base, rec) in &recommendations {
            if lower.contains(base) {
                return Some(rec.to_string());
            }
        }
        None
    }
}

impl Default for ContainerScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for ContainerScanner {
    fn name(&self) -> &str {
        "container"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Security, FindingCategory::Dependency]
    }

    fn supports_language(&self, _language: &str) -> bool {
        true // Container scanning is language-agnostic
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        // Scan Dockerfiles in workspace for issues
        let mut findings = Vec::new();

        for file in &context.files {
            let filename = file.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if (filename == "Dockerfile" || filename.starts_with("Dockerfile."))
                && let Ok(content) = std::fs::read_to_string(file)
            {
                findings.extend(analyze_dockerfile_security(&content, file));
            }
        }

        Ok(findings)
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::Execute
    }
}

/// Analyze a Dockerfile for security issues (native, no external tool).
fn analyze_dockerfile_security(content: &str, path: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        // Check for FROM latest
        if trimmed.starts_with("FROM ")
            && (trimmed.ends_with(":latest") || !trimmed.contains(':'))
            && (!trimmed.contains(" AS ")
                || trimmed
                    .split(" AS ")
                    .next()
                    .is_some_and(|base| base.ends_with(":latest") || !base.contains(':')))
        {
            let mut finding = Finding::new(
                "Dockerfile uses unpinned base image",
                "Using ':latest' or an untagged base image leads to non-reproducible builds and may introduce unexpected vulnerabilities.",
                FindingSeverity::Medium,
                FindingCategory::Security,
                FindingProvenance {
                    scanner: "dockerfile".to_string(),
                    rule_id: Some("DF-001".to_string()),
                    confidence: 0.95,
                    consensus: None,
                },
            );
            finding = finding.with_location(CodeLocation {
                file: path.to_path_buf(),
                start_line: line_num + 1,
                end_line: Some(line_num + 1),
                start_column: Some(1),
                end_column: None,
                function_name: None,
            });
            finding = finding.with_remediation(Remediation {
                description: "Pin the base image to a specific version tag and digest".to_string(),
                patch: None,
                effort: Some(RemediationEffort::Trivial),
                confidence: 0.9,
            });
            findings.push(finding.with_tag("dockerfile"));
        }

        // Check for running as root (no USER directive after last FROM)
        if trimmed.starts_with("CMD ") || trimmed.starts_with("ENTRYPOINT ") {
            let preceding: Vec<&str> = content.lines().take(line_num).collect();
            let has_user = preceding
                .iter()
                .rev()
                .take_while(|l| !l.trim().starts_with("FROM "))
                .any(|l| l.trim().starts_with("USER "));
            if !has_user {
                let mut finding = Finding::new(
                    "Container runs as root",
                    "No USER directive found before CMD/ENTRYPOINT. Container will run as root, which is a security risk.",
                    FindingSeverity::High,
                    FindingCategory::Security,
                    FindingProvenance {
                        scanner: "dockerfile".to_string(),
                        rule_id: Some("DF-002".to_string()),
                        confidence: 0.90,
                        consensus: None,
                    },
                );
                finding = finding.with_location(CodeLocation {
                    file: path.to_path_buf(),
                    start_line: line_num + 1,
                    end_line: Some(line_num + 1),
                    start_column: Some(1),
                    end_column: None,
                    function_name: None,
                });
                finding = finding.with_remediation(Remediation {
                    description: "Add a USER directive to run as a non-root user".to_string(),
                    patch: Some("USER 1001".to_string()),
                    effort: Some(RemediationEffort::Low),
                    confidence: 0.9,
                });
                findings.push(finding.with_tag("dockerfile"));
            }
        }

        // Check for ADD instead of COPY
        if trimmed.starts_with("ADD ")
            && !trimmed.contains("http://")
            && !trimmed.contains("https://")
            && !trimmed.contains(".tar")
        {
            let mut finding = Finding::new(
                "Use COPY instead of ADD",
                "ADD has extra functionality (URL download, tar extraction) that can be a security risk. Use COPY for simple file copies.",
                FindingSeverity::Low,
                FindingCategory::Quality,
                FindingProvenance {
                    scanner: "dockerfile".to_string(),
                    rule_id: Some("DF-003".to_string()),
                    confidence: 0.85,
                    consensus: None,
                },
            );
            finding = finding.with_location(CodeLocation {
                file: path.to_path_buf(),
                start_line: line_num + 1,
                end_line: Some(line_num + 1),
                start_column: Some(1),
                end_column: None,
                function_name: None,
            });
            findings.push(finding.with_tag("dockerfile"));
        }

        // Check for secrets in ENV or ARG
        let lower = trimmed.to_lowercase();
        if (trimmed.starts_with("ENV ") || trimmed.starts_with("ARG "))
            && (lower.contains("password")
                || lower.contains("secret")
                || lower.contains("api_key")
                || lower.contains("token"))
        {
            let mut finding = Finding::new(
                "Potential secret in Dockerfile ENV/ARG",
                "Secrets in ENV or ARG directives are visible in image metadata and build history.",
                FindingSeverity::High,
                FindingCategory::Security,
                FindingProvenance {
                    scanner: "dockerfile".to_string(),
                    rule_id: Some("DF-004".to_string()),
                    confidence: 0.80,
                    consensus: None,
                },
            );
            finding = finding.with_location(CodeLocation {
                file: path.to_path_buf(),
                start_line: line_num + 1,
                end_line: Some(line_num + 1),
                start_column: Some(1),
                end_column: None,
                function_name: None,
            });
            finding = finding.with_remediation(Remediation {
                description: "Use Docker secrets or build-time secret mounts instead".to_string(),
                patch: Some("RUN --mount=type=secret,id=mysecret".to_string()),
                effort: Some(RemediationEffort::Medium),
                confidence: 0.8,
            });
            findings.push(finding.with_tag("dockerfile"));
        }

        // Check for exposed privileged ports
        if trimmed.starts_with("EXPOSE ")
            && let Some(port_str) = trimmed.strip_prefix("EXPOSE ")
        {
            for port_part in port_str.split_whitespace() {
                let port_num = port_part.split('/').next().unwrap_or("");
                if let Ok(port) = port_num.parse::<u16>()
                    && port < 1024
                    && port != 80
                    && port != 443
                {
                    let mut finding = Finding::new(
                        format!("Exposing privileged port {port}"),
                        format!(
                            "Port {port} requires root privileges. Consider using a higher port and mapping it."
                        ),
                        FindingSeverity::Low,
                        FindingCategory::Security,
                        FindingProvenance {
                            scanner: "dockerfile".to_string(),
                            rule_id: Some("DF-005".to_string()),
                            confidence: 0.75,
                            consensus: None,
                        },
                    );
                    finding = finding.with_location(CodeLocation {
                        file: path.to_path_buf(),
                        start_line: line_num + 1,
                        end_line: Some(line_num + 1),
                        start_column: Some(1),
                        end_column: None,
                        function_name: None,
                    });
                    findings.push(finding.with_tag("dockerfile"));
                }
            }
        }
    }

    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_trivy_output() {
        let json = r#"{
            "Results": [{
                "Vulnerabilities": [
                    {
                        "VulnerabilityID": "CVE-2024-1234",
                        "PkgName": "openssl",
                        "InstalledVersion": "1.1.1",
                        "FixedVersion": "1.1.1w",
                        "Severity": "CRITICAL",
                        "Description": "Buffer overflow in openssl"
                    },
                    {
                        "VulnerabilityID": "CVE-2024-5678",
                        "PkgName": "zlib",
                        "InstalledVersion": "1.2.11",
                        "Severity": "MEDIUM",
                        "Description": "Integer overflow in zlib"
                    }
                ]
            }]
        }"#;

        let vulns = ContainerScanner::parse_trivy_output(json).unwrap();
        assert_eq!(vulns.len(), 2);
        assert_eq!(vulns[0].cve_id, "CVE-2024-1234");
        assert_eq!(vulns[0].package, "openssl");
        assert!(vulns[0].fixed_version.is_some());
        assert!(vulns[1].fixed_version.is_none());
    }

    #[test]
    fn test_vuln_to_finding() {
        let vuln = ContainerVulnerability {
            cve_id: "CVE-2024-1234".to_string(),
            package: "openssl".to_string(),
            installed_version: "1.1.1".to_string(),
            fixed_version: Some("1.1.1w".to_string()),
            severity: "CRITICAL".to_string(),
            description: "Buffer overflow".to_string(),
            layer: None,
        };

        let finding = ContainerScanner::vuln_to_finding(&vuln, "myapp:latest");
        assert_eq!(finding.severity, FindingSeverity::Critical);
        assert!(finding.title.contains("CVE-2024-1234"));
        assert!(finding.remediation.is_some());
    }

    #[test]
    fn test_summarize() {
        let vulns = vec![
            ContainerVulnerability {
                cve_id: "CVE-1".into(),
                package: "a".into(),
                installed_version: "1.0".into(),
                fixed_version: Some("1.1".into()),
                severity: "CRITICAL".into(),
                description: String::new(),
                layer: None,
            },
            ContainerVulnerability {
                cve_id: "CVE-2".into(),
                package: "b".into(),
                installed_version: "2.0".into(),
                fixed_version: None,
                severity: "HIGH".into(),
                description: String::new(),
                layer: None,
            },
            ContainerVulnerability {
                cve_id: "CVE-3".into(),
                package: "c".into(),
                installed_version: "3.0".into(),
                fixed_version: Some("3.1".into()),
                severity: "LOW".into(),
                description: String::new(),
                layer: None,
            },
        ];

        let summary = ContainerScanner::summarize(&vulns);
        assert_eq!(summary.total_vulnerabilities, 3);
        assert_eq!(summary.critical, 1);
        assert_eq!(summary.high, 1);
        assert_eq!(summary.low, 1);
        assert_eq!(summary.fixable, 2);
    }

    #[test]
    fn test_dockerfile_analysis_unpinned() {
        let content = "FROM ubuntu\nRUN apt-get update\nCMD [\"/app\"]\n";
        let findings = analyze_dockerfile_security(content, Path::new("Dockerfile"));
        assert!(
            findings.iter().any(|f| f.title.contains("unpinned")),
            "Should detect unpinned base image"
        );
    }

    #[test]
    fn test_dockerfile_analysis_no_user() {
        let content = "FROM ubuntu:22.04\nRUN apt-get update\nCMD [\"/app\"]\n";
        let findings = analyze_dockerfile_security(content, Path::new("Dockerfile"));
        assert!(
            findings.iter().any(|f| f.title.contains("root")),
            "Should detect running as root"
        );
    }

    #[test]
    fn test_dockerfile_analysis_add() {
        let content = "FROM ubuntu:22.04\nADD app.py /app/\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = analyze_dockerfile_security(content, Path::new("Dockerfile"));
        assert!(
            findings
                .iter()
                .any(|f| f.title.contains("COPY instead of ADD")),
            "Should detect ADD usage"
        );
    }

    #[test]
    fn test_dockerfile_analysis_secret_in_env() {
        let content = "FROM node:20\nENV API_KEY=abc123\nUSER node\nCMD [\"node\", \"app.js\"]\n";
        let findings = analyze_dockerfile_security(content, Path::new("Dockerfile"));
        assert!(
            findings.iter().any(|f| f.title.contains("secret")),
            "Should detect secret in ENV"
        );
    }

    #[test]
    fn test_recommend_base_image() {
        assert!(ContainerScanner::recommend_base_image("ubuntu:20.04").is_some());
        assert!(ContainerScanner::recommend_base_image("node:18").is_some());
        assert!(ContainerScanner::recommend_base_image("custom-image:1.0").is_none());
    }

    #[test]
    fn test_scanner_metadata() {
        let scanner = ContainerScanner::new();
        assert_eq!(scanner.name(), "container");
        assert_eq!(scanner.risk_level(), ScannerRiskLevel::Execute);
    }
}
