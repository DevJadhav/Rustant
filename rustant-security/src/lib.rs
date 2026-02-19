//! Rustant Security — code review, security scanning, and compliance engine.
//!
//! This crate provides comprehensive security capabilities for the Rustant agent:
//!
//! - **Phase 1 (Foundation):** Finding schema, secret redaction, AST engine,
//!   dependency graph, consensus validation, scanner plugin interface
//! - **Phase 2 (Code Review):** Diff analysis, AI review comments, auto-fix,
//!   quality scoring, tech debt tracking
//! - **Phase 3 (Security Scanning):** SAST, SCA, secrets detection, container
//!   security, IaC scanning, supply chain security
//! - **Phase 4 (Compliance):** License compliance, SBOM generation, policy engine,
//!   compliance frameworks, risk scoring
//! - **Phase 5 (Advanced):** Threat detection, incident response, alert management,
//!   production learning loop

// Phase 1: Foundation Infrastructure
pub mod ast;
pub mod config;
pub mod consensus;
pub mod dep_graph;
pub mod error;
pub mod finding;
pub mod memory_bridge;
pub mod redaction;
pub mod scanner;

// Phase 2+: Stub modules (to be implemented)
pub mod compliance;
pub mod incident;
pub mod report;
pub mod review;
pub mod scanners;
pub mod tools;

// Re-exports for convenience
pub use config::SecurityConfig;
pub use error::{AstError, ComplianceError, DepGraphError, ReviewError, ScanError, SecurityError};
pub use finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance, FindingSeverity,
    FindingStatus, Remediation,
};
pub use memory_bridge::{FindingMemoryBridge, RedactedFact};
pub use redaction::SecretRedactor;
pub use scanner::{ScanContext, Scanner, ScannerRegistry};

use rustant_tools::registry::{Tool, ToolRegistry};
use std::sync::Arc;

/// Register all security tools with the given ToolRegistry.
///
/// This function should be called after `register_builtin_tools()` to add
/// the 33 security-specific tools (code review, SAST, SCA, compliance, etc.)
/// to the agent's tool set.
pub fn register_security_tools(registry: &mut ToolRegistry) {
    let security_tools: Vec<Arc<dyn Tool>> = vec![
        // Phase 2: Code Review & Quality (9 tools)
        Arc::new(tools::AnalyzeDiffTool),
        Arc::new(tools::ApplyFixTool),
        Arc::new(tools::CodeReviewTool),
        Arc::new(tools::ComplexityCheckTool),
        Arc::new(tools::DeadCodeDetectTool),
        Arc::new(tools::DuplicateDetectTool),
        Arc::new(tools::QualityScoreTool),
        Arc::new(tools::SuggestFixTool),
        Arc::new(tools::TechDebtReportTool),
        // Phase 3: Security Scanning (12 tools)
        Arc::new(tools::ContainerScanTool),
        Arc::new(tools::DockerfileLintTool),
        Arc::new(tools::IacScanTool),
        Arc::new(tools::K8sLintTool),
        Arc::new(tools::SastScanTool),
        Arc::new(tools::ScaScanTool),
        Arc::new(tools::SecretsScanTool),
        Arc::new(tools::SecretsValidateTool),
        Arc::new(tools::SecurityScanTool),
        Arc::new(tools::SupplyChainCheckTool),
        Arc::new(tools::TerraformCheckTool),
        Arc::new(tools::VulnerabilityCheckTool),
        // Phase 4: Compliance & Risk (7 tools)
        Arc::new(tools::AuditExportTool),
        Arc::new(tools::ComplianceReportTool),
        Arc::new(tools::LicenseCheckTool),
        Arc::new(tools::PolicyCheckTool),
        Arc::new(tools::RiskScoreTool),
        Arc::new(tools::SbomDiffTool),
        Arc::new(tools::SbomGenerateTool),
        // Phase 5: Incident Response & Alert Management (5 tools)
        Arc::new(tools::AlertStatusTool),
        Arc::new(tools::AlertTriageTool),
        Arc::new(tools::IncidentRespondTool),
        Arc::new(tools::LogAnalyzeTool),
        Arc::new(tools::ThreatDetectTool),
    ];

    for tool in security_tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register security tool: {}", e);
        }
    }
}
