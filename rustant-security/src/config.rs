//! Configuration types for the security crate.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level security configuration, added as `security: Option<SecurityConfig>` in AgentConfig.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Master enable/disable for the security subsystem.
    pub enabled: bool,
    /// Scanning configuration.
    pub scanning: ScanConfig,
    /// Code review configuration.
    pub review: ReviewConfig,
    /// Compliance configuration.
    pub compliance: ComplianceConfig,
    /// Consensus configuration for multi-model validation.
    pub consensus: ConsensusConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scanning: ScanConfig::default(),
            review: ReviewConfig::default(),
            compliance: ComplianceConfig::default(),
            consensus: ConsensusConfig::default(),
        }
    }
}

/// Configuration for security scanning operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScanConfig {
    /// Enable scanning subsystem.
    pub enabled: bool,
    /// Maximum number of scanners to run in parallel.
    pub parallel_scanners: usize,
    /// Global timeout for scan operations (seconds).
    pub timeout_secs: u64,
    /// Minimum severity to report (findings below this are suppressed).
    pub severity_threshold: SeverityThreshold,
    /// SAST-specific configuration.
    pub sast: SastConfig,
    /// SCA-specific configuration.
    pub sca: ScaConfig,
    /// Secrets detection configuration.
    pub secrets: SecretsConfig,
    /// Container scanning configuration.
    pub containers: ContainerConfig,
    /// Infrastructure-as-code scanning configuration.
    pub iac: IacConfig,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            parallel_scanners: 4,
            timeout_secs: 300,
            severity_threshold: SeverityThreshold::Medium,
            sast: SastConfig::default(),
            sca: ScaConfig::default(),
            secrets: SecretsConfig::default(),
            containers: ContainerConfig::default(),
            iac: IacConfig::default(),
        }
    }
}

/// SAST scanner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SastConfig {
    pub enabled: bool,
    /// Languages to scan (empty = auto-detect all supported).
    pub languages: Vec<String>,
    /// Path to custom rule definitions (YAML).
    pub custom_rules_dir: Option<PathBuf>,
}

impl Default for SastConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            languages: Vec::new(),
            custom_rules_dir: None,
        }
    }
}

/// SCA vulnerability scanner configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScaConfig {
    pub enabled: bool,
    /// Vulnerability databases to query.
    pub vuln_databases: Vec<String>,
    /// Enable reachability analysis for prioritization.
    pub reachability_analysis: bool,
    /// Hours between automatic vulnerability database updates.
    pub auto_update_interval_hours: u64,
}

impl Default for ScaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            vuln_databases: vec!["osv".into(), "ghsa".into()],
            reachability_analysis: true,
            auto_update_interval_hours: 24,
        }
    }
}

/// Secrets detection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SecretsConfig {
    pub enabled: bool,
    /// Scan git history for leaked secrets.
    pub history_scan: bool,
    /// Attempt live validation of detected secrets (network access required).
    pub live_validation: bool,
    /// Path to custom pattern definitions (YAML).
    pub custom_patterns_file: Option<PathBuf>,
    /// Maximum number of git commits to scan for history mode.
    pub max_history_commits: usize,
    /// Shannon entropy threshold for high-entropy string detection.
    pub entropy_threshold: f64,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            history_scan: false,
            live_validation: false,
            custom_patterns_file: None,
            max_history_commits: 1000,
            entropy_threshold: 4.5,
        }
    }
}

/// Container scanning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContainerConfig {
    pub enabled: bool,
    /// Container registries to scan.
    pub registries: Vec<String>,
}

impl Default for ContainerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            registries: vec!["docker.io".into()],
        }
    }
}

/// IaC scanning configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IacConfig {
    pub enabled: bool,
    /// IaC frameworks to scan for.
    pub frameworks: Vec<String>,
}

impl Default for IacConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            frameworks: vec![
                "terraform".into(),
                "kubernetes".into(),
                "cloudformation".into(),
            ],
        }
    }
}

/// Code review configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    pub enabled: bool,
    /// Minimum confidence for auto-fix application (0.0-1.0).
    pub auto_fix_confidence: f32,
    /// Minimum severity to include in reviews.
    pub severity_threshold: SeverityThreshold,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_fix_confidence: 0.85,
            severity_threshold: SeverityThreshold::Low,
        }
    }
}

/// Compliance engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ComplianceConfig {
    /// Compliance frameworks to check against.
    pub frameworks: Vec<String>,
    /// SBOM output format.
    pub sbom_format: SbomFormat,
    /// License policy settings.
    pub license_policy: LicensePolicyConfig,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            frameworks: Vec::new(),
            sbom_format: SbomFormat::CycloneDx,
            license_policy: LicensePolicyConfig::default(),
        }
    }
}

/// SBOM output format.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SbomFormat {
    CycloneDx,
    Spdx,
    Csv,
}

/// License policy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LicensePolicyConfig {
    /// Allowed SPDX license identifiers (supports glob patterns).
    pub allowed: Vec<String>,
    /// Denied SPDX license identifiers.
    pub denied: Vec<String>,
    /// Licenses that require manual review.
    pub review_required: Vec<String>,
    /// Per-package overrides (package name -> "approved" | "denied").
    pub overrides: std::collections::HashMap<String, String>,
}

impl Default for LicensePolicyConfig {
    fn default() -> Self {
        Self {
            allowed: vec![
                "MIT".into(),
                "Apache-2.0".into(),
                "BSD-2-Clause".into(),
                "BSD-3-Clause".into(),
                "ISC".into(),
            ],
            denied: Vec::new(),
            review_required: Vec::new(),
            overrides: std::collections::HashMap::new(),
        }
    }
}

/// Multi-model consensus configuration for security findings validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConsensusConfig {
    pub enabled: bool,
    /// Agreement threshold (e.g., "2-of-3", "majority", "unanimous").
    pub threshold: String,
    /// LLM providers to use for consensus.
    pub providers: Vec<String>,
    /// Timeout per consensus call (seconds).
    pub timeout_secs: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: "2-of-3".into(),
            providers: vec!["openai".into(), "anthropic".into(), "gemini".into()],
            timeout_secs: 30,
        }
    }
}

/// Minimum severity threshold for filtering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum SeverityThreshold {
    Info,
    Low,
    Medium,
    High,
    Critical,
}
