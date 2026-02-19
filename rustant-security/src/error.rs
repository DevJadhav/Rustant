//! Error types for the security crate.

use thiserror::Error;

/// Top-level security crate error.
#[derive(Debug, Error)]
pub enum SecurityError {
    #[error("scan error: {0}")]
    Scan(#[from] ScanError),
    #[error("review error: {0}")]
    Review(#[from] ReviewError),
    #[error("compliance error: {0}")]
    Compliance(#[from] ComplianceError),
    #[error("AST error: {0}")]
    Ast(#[from] AstError),
    #[error("dependency graph error: {0}")]
    DepGraph(#[from] DepGraphError),
    #[error("configuration error: {0}")]
    Config(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

/// Errors from security scanners.
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("scanner '{scanner}' failed: {message}")]
    ScannerFailed { scanner: String, message: String },
    #[error("scanner '{0}' not found")]
    ScannerNotFound(String),
    #[error("scanner '{0}' is not available")]
    ScannerUnavailable(String),
    #[error("scan timed out after {0}s")]
    Timeout(u64),
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("rule parse error in '{rule_id}': {message}")]
    RuleParseError { rule_id: String, message: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// Errors from code review operations.
#[derive(Debug, Error)]
pub enum ReviewError {
    #[error("diff analysis failed: {0}")]
    DiffAnalysis(String),
    #[error("review generation failed: {0}")]
    ReviewGeneration(String),
    #[error("fix generation failed: {0}")]
    FixGeneration(String),
    #[error("fix application failed: {0}")]
    FixApplication(String),
    #[error("quality scoring failed: {0}")]
    QualityScoring(String),
    #[error("git error: {0}")]
    Git(String),
    #[error("{0}")]
    Other(String),
}

/// Errors from compliance operations.
#[derive(Debug, Error)]
pub enum ComplianceError {
    #[error("license check failed: {0}")]
    LicenseCheck(String),
    #[error("SBOM generation failed: {0}")]
    SbomGeneration(String),
    #[error("policy evaluation failed: {0}")]
    PolicyEvaluation(String),
    #[error("framework '{0}' not supported")]
    UnsupportedFramework(String),
    #[error("evidence collection failed: {0}")]
    EvidenceCollection(String),
    #[error("{0}")]
    Other(String),
}

/// Errors from the AST engine.
#[derive(Debug, Error)]
pub enum AstError {
    #[error("parse error for '{file}': {message}")]
    ParseError { file: String, message: String },
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("query error for pattern '{pattern}': {message}")]
    QueryError { pattern: String, message: String },
    #[error("language grammar not compiled: {0} (enable the corresponding sast-* feature)")]
    GrammarNotCompiled(String),
}

/// Errors from the dependency graph engine.
#[derive(Debug, Error)]
pub enum DepGraphError {
    #[error("lockfile parse error for '{file}': {message}")]
    LockfileParse { file: String, message: String },
    #[error("no lockfile found for ecosystem: {0}")]
    NoLockfile(String),
    #[error("package '{0}' not found in graph")]
    PackageNotFound(String),
    #[error("cycle detected in dependency graph")]
    CycleDetected,
    #[error("registry API error: {0}")]
    RegistryError(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
