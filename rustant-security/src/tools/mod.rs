//! Tool trait implementations for security scanning, code review, and compliance.
//!
//! Each tool implements `rustant_tools::registry::Tool` and is registered
//! in the tool registry for agent invocation.

// Phase 2: Code review and quality tools
pub mod analyze_diff;
pub mod apply_fix;
pub mod code_review;
pub mod complexity_check;
pub mod dead_code_detect;
pub mod duplicate_detect;
pub mod quality_score;
pub mod suggest_fix;
pub mod tech_debt_report;

pub use analyze_diff::AnalyzeDiffTool;
pub use apply_fix::ApplyFixTool;
pub use code_review::CodeReviewTool;
pub use complexity_check::ComplexityCheckTool;
pub use dead_code_detect::DeadCodeDetectTool;
pub use duplicate_detect::DuplicateDetectTool;
pub use quality_score::QualityScoreTool;
pub use suggest_fix::SuggestFixTool;
pub use tech_debt_report::TechDebtReportTool;

// Phase 3: Security scanning tools
pub mod container_scan;
pub mod dockerfile_lint;
pub mod iac_scan;
pub mod k8s_lint;
pub mod sast_scan;
pub mod sca_scan;
pub mod secrets_scan;
pub mod secrets_validate;
pub mod security_scan;
pub mod supply_chain_check;
pub mod terraform_check;
pub mod vulnerability_check;

pub use container_scan::ContainerScanTool;
pub use dockerfile_lint::DockerfileLintTool;
pub use iac_scan::IacScanTool;
pub use k8s_lint::K8sLintTool;
pub use sast_scan::SastScanTool;
pub use sca_scan::ScaScanTool;
pub use secrets_scan::SecretsScanTool;
pub use secrets_validate::SecretsValidateTool;
pub use security_scan::SecurityScanTool;
pub use supply_chain_check::SupplyChainCheckTool;
pub use terraform_check::TerraformCheckTool;
pub use vulnerability_check::VulnerabilityCheckTool;

// Phase 4: Compliance & Risk
pub mod audit_export;
pub mod compliance_report;
pub mod license_check;
pub mod policy_check;
pub mod risk_score;
pub mod sbom_diff;
pub mod sbom_generate;

pub use audit_export::AuditExportTool;
pub use compliance_report::ComplianceReportTool;
pub use license_check::LicenseCheckTool;
pub use policy_check::PolicyCheckTool;
pub use risk_score::RiskScoreTool;
pub use sbom_diff::SbomDiffTool;
pub use sbom_generate::SbomGenerateTool;

// Phase 5: Incident Response & Alert Management
pub mod alert_status;
pub mod alert_triage;
pub mod incident_respond;
pub mod log_analyze;
pub mod threat_detect;

pub use alert_status::AlertStatusTool;
pub use alert_triage::AlertTriageTool;
pub use incident_respond::IncidentRespondTool;
pub use log_analyze::LogAnalyzeTool;
pub use threat_detect::ThreatDetectTool;
