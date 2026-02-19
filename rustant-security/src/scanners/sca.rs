//! SCA vulnerability scanner — software composition analysis.
//!
//! Scans dependencies for known vulnerabilities using the dependency graph
//! and advisory databases.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance,
    FindingReference, FindingSeverity, Remediation,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use std::path::Path;

/// A known vulnerability advisory.
#[derive(Debug, Clone)]
pub struct Advisory {
    /// Advisory ID (e.g., "GHSA-xxx", "CVE-2024-1234").
    pub id: String,
    /// Affected package name.
    pub package: String,
    /// Affected version range (semver).
    pub affected_range: String,
    /// Fixed version, if available.
    pub fixed_version: Option<String>,
    /// Severity.
    pub severity: FindingSeverity,
    /// CVSS score.
    pub cvss_score: Option<f32>,
    /// Description.
    pub description: String,
    /// CWE reference.
    pub cwe: Option<String>,
    /// URL for more info.
    pub url: Option<String>,
}

/// SCA scanner for dependency vulnerability detection.
pub struct ScaScanner {
    /// Local advisory database (loaded from cache or fetched).
    advisories: Vec<Advisory>,
}

impl ScaScanner {
    /// Create a new SCA scanner with an empty advisory database.
    pub fn new() -> Self {
        Self {
            advisories: Vec::new(),
        }
    }

    /// Create with pre-loaded advisories.
    pub fn with_advisories(advisories: Vec<Advisory>) -> Self {
        Self { advisories }
    }

    /// Add an advisory to the local database.
    pub fn add_advisory(&mut self, advisory: Advisory) {
        self.advisories.push(advisory);
    }

    /// Check a specific package/version against advisories.
    pub fn check_package(&self, package: &str, version: &str, lockfile: &Path) -> Vec<Finding> {
        let mut findings = Vec::new();

        for advisory in &self.advisories {
            if advisory.package != package {
                continue;
            }

            // Simple version range check (in production, use semver crate)
            if !version_in_range(version, &advisory.affected_range) {
                continue;
            }

            let mut finding = Finding::new(
                format!(
                    "Vulnerable dependency: {} {} ({})",
                    package, version, advisory.id
                ),
                format!(
                    "Package {} version {} is affected by {}.\n\n{}",
                    package, version, advisory.id, advisory.description
                ),
                advisory.severity,
                FindingCategory::Dependency,
                FindingProvenance {
                    scanner: "sca".to_string(),
                    rule_id: Some(advisory.id.clone()),
                    confidence: 0.95,
                    consensus: None,
                },
            )
            .with_location(CodeLocation::new(lockfile, 1))
            .with_explanation(FindingExplanation {
                reasoning_chain: vec![
                    format!("Package {} version {} found in lockfile", package, version),
                    format!(
                        "Advisory {} affects versions {}",
                        advisory.id, advisory.affected_range
                    ),
                ],
                evidence: vec![format!("{}@{}", package, version)],
                context_factors: vec![format!("Lockfile: {}", lockfile.display())],
            })
            .with_tag("sca".to_string())
            .with_tag("dependency".to_string());

            if let Some(ref cwe) = advisory.cwe {
                finding = finding.with_reference(FindingReference {
                    ref_type: crate::finding::ReferenceType::Cwe,
                    id: cwe.clone(),
                    url: None,
                });
            }

            if let Some(ref url) = advisory.url {
                finding = finding.with_reference(FindingReference {
                    ref_type: crate::finding::ReferenceType::Other,
                    id: advisory.id.clone(),
                    url: Some(url.clone()),
                });
            }

            if let Some(cvss) = advisory.cvss_score {
                finding = finding.with_cvss(cvss);
            }

            if let Some(ref fixed) = advisory.fixed_version {
                finding = finding.with_remediation(Remediation {
                    description: format!("Upgrade {package} to version {fixed} or later."),
                    patch: None,
                    effort: Some(crate::finding::RemediationEffort::Low),
                    confidence: 0.95,
                });
            }

            findings.push(finding);
        }

        findings
    }
}

impl Default for ScaScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for ScaScanner {
    fn name(&self) -> &str {
        "sca"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Dependency]
    }

    fn supports_language(&self, _language: &str) -> bool {
        true
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        let mut all_findings = Vec::new();

        // Try to build dependency graph from workspace
        let graph = crate::dep_graph::DependencyGraph::build(&context.workspace);

        if let Ok(graph) = graph {
            for node in graph.all_packages() {
                let findings = self.check_package(
                    &node.name,
                    &node.version,
                    &context.workspace.join("Cargo.lock"), // simplified
                );
                all_findings.extend(findings);
            }
        }

        Ok(all_findings)
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

/// Check whether `version` falls within `range`.
///
/// Uses the `semver` crate for proper semantic version comparison when possible,
/// falling back to naive numeric comparison for non-semver strings.
fn version_in_range(version: &str, range: &str) -> bool {
    // Handle exact version match
    if version == range {
        return true;
    }

    // Handle "*" (all versions)
    if range == "*" {
        return true;
    }

    // Handle ">= X.Y.Z, < Y.Z.W" compound ranges (split on comma, all must match)
    if range.contains(',') {
        let parts: Vec<&str> = range.split(',').map(|s| s.trim()).collect();
        return parts.iter().all(|part| version_in_range(version, part));
    }

    // Try semver-based comparison first
    if let Some(result) = semver_in_range(version, range) {
        return result;
    }

    // Fallback to naive numeric comparison for non-semver strings
    if let Some(max) = range.strip_prefix("< ") {
        return version_lt(version, max);
    }
    if let Some(max) = range.strip_prefix("<= ") {
        return version == max || version_lt(version, max);
    }
    if let Some(min) = range.strip_prefix(">= ") {
        return !version_lt(version, min);
    }
    if let Some(min) = range.strip_prefix("> ") {
        return version != min && !version_lt(version, min);
    }

    false
}

/// Attempt semver-based range check. Returns `None` if parsing fails.
fn semver_in_range(version: &str, range: &str) -> Option<bool> {
    let ver = semver::Version::parse(version).ok()?;

    // Handle "< X.Y.Z" format
    if let Some(max) = range.strip_prefix("< ") {
        let max_ver = semver::Version::parse(max).ok()?;
        return Some(ver < max_ver);
    }

    // Handle "<= X.Y.Z" format
    if let Some(max) = range.strip_prefix("<= ") {
        let max_ver = semver::Version::parse(max).ok()?;
        return Some(ver <= max_ver);
    }

    // Handle ">= X.Y.Z" format
    if let Some(min) = range.strip_prefix(">= ") {
        let min_ver = semver::Version::parse(min).ok()?;
        return Some(ver >= min_ver);
    }

    // Handle "> X.Y.Z" format
    if let Some(min) = range.strip_prefix("> ") {
        let min_ver = semver::Version::parse(min).ok()?;
        return Some(ver > min_ver);
    }

    // Handle exact match
    let range_ver = semver::Version::parse(range).ok()?;
    Some(ver == range_ver)
}

/// Simple version less-than comparison for non-semver strings.
fn version_lt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> { v.split('.').filter_map(|s| s.parse().ok()).collect() };
    let va = parse(a);
    let vb = parse(b);
    va < vb
}

// ---------------------------------------------------------------------------
// OSV.dev client — builds requests and parses responses for the OSV API.
// ---------------------------------------------------------------------------

/// The OSV.dev API endpoint for vulnerability queries.
pub const OSV_API_URL: &str = "https://api.osv.dev/v1/query";

/// Maps internal ecosystem names (as used in `DepNode.ecosystem`) to the
/// ecosystem identifiers expected by the OSV.dev API.
pub fn ecosystem_to_osv(ecosystem: &str) -> Option<&'static str> {
    match ecosystem {
        "cargo" | "crates.io" => Some("crates.io"),
        "npm" => Some("npm"),
        "pypi" => Some("PyPI"),
        "go" => Some("Go"),
        "maven" => Some("Maven"),
        "rubygems" => Some("RubyGems"),
        "packagist" => Some("Packagist"),
        "pub" => Some("Pub"),
        "hex" => Some("Hex"),
        "swift" => Some("SwiftURL"),
        "nuget" => Some("NuGet"),
        _ => None,
    }
}

/// Error type for OSV client operations.
#[derive(Debug, thiserror::Error)]
pub enum OsvError {
    #[error("unsupported ecosystem: {0}")]
    UnsupportedEcosystem(String),
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),
    #[error("missing field in response: {0}")]
    MissingField(String),
}

/// JSON body sent to the OSV.dev `/v1/query` endpoint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OsvQueryRequest {
    pub package: OsvPackage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Package identifier inside an OSV query.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OsvPackage {
    pub name: String,
    pub ecosystem: String,
}

/// A vulnerability record parsed from an OSV.dev response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OsvVulnerability {
    /// Vulnerability identifier (e.g. "GHSA-xxx", "RUSTSEC-2024-0001").
    pub id: String,
    /// One-line summary.
    #[serde(default)]
    pub summary: String,
    /// Detailed description (may be empty).
    #[serde(default)]
    pub details: String,
    /// Severity string extracted from the response (CVSS vector or textual).
    #[serde(default)]
    pub severity: Option<String>,
    /// CVSS score extracted from the severity array, if present.
    #[serde(default)]
    pub cvss_score: Option<f32>,
    /// Affected version ranges as human-readable strings.
    #[serde(default)]
    pub affected_ranges: Vec<String>,
    /// Fixed version, if any (first `fixed` event found).
    #[serde(default)]
    pub fixed_version: Option<String>,
    /// Reference URLs from the advisory.
    #[serde(default)]
    pub references: Vec<String>,
}

/// Stateless client for building OSV.dev API requests and parsing responses.
///
/// This struct intentionally does **not** make HTTP calls. The caller is
/// responsible for sending the request (e.g. via `reqwest`) and passing the
/// raw JSON response back here for parsing.
pub struct OsvClient;

impl OsvClient {
    /// Build the JSON request body for querying a specific package version.
    ///
    /// `ecosystem` should be the internal name (e.g. `"cargo"`, `"npm"`) — it
    /// will be mapped to the OSV API identifier automatically.
    pub fn query_package(
        ecosystem: &str,
        name: &str,
        version: &str,
    ) -> Result<OsvQueryRequest, OsvError> {
        let osv_ecosystem = ecosystem_to_osv(ecosystem)
            .ok_or_else(|| OsvError::UnsupportedEcosystem(ecosystem.to_string()))?;

        Ok(OsvQueryRequest {
            package: OsvPackage {
                name: name.to_string(),
                ecosystem: osv_ecosystem.to_string(),
            },
            version: Some(version.to_string()),
        })
    }

    /// Serialize the query request to a JSON string suitable for an HTTP body.
    pub fn query_to_json(request: &OsvQueryRequest) -> Result<String, OsvError> {
        Ok(serde_json::to_string(request)?)
    }

    /// Parse a raw JSON response from the OSV.dev `/v1/query` endpoint into a
    /// list of [`OsvVulnerability`] records.
    ///
    /// The OSV response envelope looks like:
    /// ```json
    /// { "vulns": [ { "id": "...", "summary": "...", ... }, ... ] }
    /// ```
    /// An empty `vulns` array (or missing key) returns an empty `Vec`.
    pub fn parse_response(json: &str) -> Result<Vec<OsvVulnerability>, OsvError> {
        let root: serde_json::Value = serde_json::from_str(json)?;

        let vulns_array = match root.get("vulns") {
            Some(serde_json::Value::Array(arr)) => arr,
            _ => return Ok(Vec::new()),
        };

        let mut results = Vec::with_capacity(vulns_array.len());
        for entry in vulns_array {
            results.push(Self::parse_vuln(entry)?);
        }
        Ok(results)
    }

    /// Parse a single vulnerability object from the OSV JSON.
    fn parse_vuln(value: &serde_json::Value) -> Result<OsvVulnerability, OsvError> {
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OsvError::MissingField("id".to_string()))?
            .to_string();

        let summary = value
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let details = value
            .get("details")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Extract severity: try `severity` array first (CVSS vectors), then
        // `database_specific.severity`.
        let (severity_str, cvss_score) = Self::extract_severity(value);

        // Extract affected ranges and fixed version.
        let (affected_ranges, fixed_version) = Self::extract_affected(value);

        // Extract reference URLs.
        let references = Self::extract_references(value);

        Ok(OsvVulnerability {
            id,
            summary,
            details,
            severity: severity_str,
            cvss_score,
            affected_ranges,
            fixed_version,
            references,
        })
    }

    /// Extract severity information from the OSV JSON.
    ///
    /// Tries the top-level `severity` array first. Each entry may have:
    /// - `"type": "CVSS_V3"` with a `"score"` field containing the vector string.
    ///
    /// Falls back to `database_specific.severity` (a plain string like "HIGH").
    fn extract_severity(value: &serde_json::Value) -> (Option<String>, Option<f32>) {
        // Try top-level severity array (CVSS vectors)
        if let Some(serde_json::Value::Array(arr)) = value.get("severity") {
            for item in arr {
                if let Some(score_str) = item.get("score").and_then(|s| s.as_str()) {
                    let cvss = Self::parse_cvss_score(score_str);
                    return (Some(score_str.to_string()), cvss);
                }
            }
        }

        // Fallback: database_specific.severity
        if let Some(db_severity) = value
            .pointer("/database_specific/severity")
            .and_then(|v| v.as_str())
        {
            let cvss = Self::severity_text_to_score(db_severity);
            return (Some(db_severity.to_string()), cvss);
        }

        (None, None)
    }

    /// Try to extract a numeric CVSS base score from a CVSS v3 vector string.
    ///
    /// CVSS vectors look like: `CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H`
    /// Some responses append a `/score:X.X` component; we look for that first.
    /// Otherwise we return `None` — full CVSS calculation is out of scope.
    fn parse_cvss_score(vector: &str) -> Option<f32> {
        // Some OSV entries embed the score at the end (non-standard but common)
        for part in vector.split('/') {
            let lower = part.to_lowercase();
            if lower.starts_with("score:")
                && let Ok(score) = lower.trim_start_matches("score:").parse::<f32>()
            {
                return Some(score);
            }
        }
        None
    }

    /// Map a textual severity label to an approximate CVSS score.
    fn severity_text_to_score(label: &str) -> Option<f32> {
        match label.to_uppercase().as_str() {
            "CRITICAL" => Some(9.5),
            "HIGH" => Some(7.5),
            "MODERATE" | "MEDIUM" => Some(5.5),
            "LOW" => Some(2.5),
            _ => None,
        }
    }

    /// Extract affected version ranges and the first fixed version from the
    /// `affected` array in an OSV entry.
    fn extract_affected(value: &serde_json::Value) -> (Vec<String>, Option<String>) {
        let mut ranges = Vec::new();
        let mut fixed_version: Option<String> = None;

        let affected = match value.get("affected") {
            Some(serde_json::Value::Array(arr)) => arr,
            _ => return (ranges, fixed_version),
        };

        for affected_entry in affected {
            if let Some(serde_json::Value::Array(range_arr)) = affected_entry.get("ranges") {
                for range_obj in range_arr {
                    let range_type = range_obj
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("UNSPECIFIED");

                    if let Some(serde_json::Value::Array(events)) = range_obj.get("events") {
                        let mut introduced: Option<&str> = None;
                        let mut fixed: Option<&str> = None;

                        for event in events {
                            if let Some(v) = event.get("introduced").and_then(|v| v.as_str()) {
                                introduced = Some(v);
                            }
                            if let Some(v) = event.get("fixed").and_then(|v| v.as_str()) {
                                fixed = Some(v);
                                if fixed_version.is_none() {
                                    fixed_version = Some(v.to_string());
                                }
                            }
                        }

                        let range_desc = match (introduced, fixed) {
                            (Some(i), Some(f)) => {
                                format!("{range_type}: >= {i}, < {f}")
                            }
                            (Some(i), None) => {
                                format!("{range_type}: >= {i}")
                            }
                            (None, Some(f)) => {
                                format!("{range_type}: < {f}")
                            }
                            (None, None) => continue,
                        };
                        ranges.push(range_desc);
                    }
                }
            }

            // Also check versions list for explicitly enumerated affected versions
            if let Some(serde_json::Value::Array(versions)) = affected_entry.get("versions") {
                for v in versions {
                    if let Some(s) = v.as_str() {
                        ranges.push(format!("= {s}"));
                    }
                }
            }
        }

        (ranges, fixed_version)
    }

    /// Extract reference URLs from the `references` array.
    fn extract_references(value: &serde_json::Value) -> Vec<String> {
        let mut urls = Vec::new();
        if let Some(serde_json::Value::Array(refs)) = value.get("references") {
            for r in refs {
                if let Some(url) = r.get("url").and_then(|v| v.as_str()) {
                    urls.push(url.to_string());
                }
            }
        }
        urls
    }

    /// Convert an [`OsvVulnerability`] into an [`Advisory`] record that the
    /// existing [`ScaScanner`] can consume.
    pub fn to_advisory(vuln: &OsvVulnerability, package: &str) -> Advisory {
        let severity = match vuln.cvss_score {
            Some(score) => FindingSeverity::from_cvss(score),
            None => match vuln
                .severity
                .as_deref()
                .map(|s| s.to_uppercase())
                .as_deref()
            {
                Some("CRITICAL") => FindingSeverity::Critical,
                Some("HIGH") => FindingSeverity::High,
                Some("MODERATE") | Some("MEDIUM") => FindingSeverity::Medium,
                Some("LOW") => FindingSeverity::Low,
                _ => FindingSeverity::Medium, // default when unknown
            },
        };

        // Build the affected range string from the OSV ranges.
        let affected_range = if vuln.affected_ranges.is_empty() {
            "*".to_string()
        } else {
            // Pick the first SEMVER or ECOSYSTEM range for the advisory.
            vuln.affected_ranges
                .iter()
                .find(|r| r.starts_with("SEMVER:") || r.starts_with("ECOSYSTEM:"))
                .or(vuln.affected_ranges.first())
                .map(|r| {
                    // Strip the type prefix (e.g. "SEMVER: >= 1.0.0, < 2.0.0" -> ">= 1.0.0, < 2.0.0")
                    if let Some(pos) = r.find(": ") {
                        r[pos + 2..].to_string()
                    } else {
                        r.clone()
                    }
                })
                .unwrap_or_else(|| "*".to_string())
        };

        // Try to extract a CWE from the id or details.
        let cwe =
            extract_cwe_from_text(&vuln.details).or_else(|| extract_cwe_from_text(&vuln.summary));

        // Pick the first reference URL.
        let url = vuln.references.first().cloned();

        Advisory {
            id: vuln.id.clone(),
            package: package.to_string(),
            affected_range,
            fixed_version: vuln.fixed_version.clone(),
            severity,
            cvss_score: vuln.cvss_score,
            description: if vuln.details.is_empty() {
                vuln.summary.clone()
            } else {
                vuln.details.clone()
            },
            cwe,
            url,
        }
    }
}

/// Extract a CWE identifier (e.g. "CWE-79") from free-form text.
fn extract_cwe_from_text(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"CWE-\d+").ok()?;
    re.find(text).map(|m| m.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_advisory() -> Advisory {
        Advisory {
            id: "CVE-2024-1234".into(),
            package: "vulnerable-pkg".into(),
            affected_range: "< 2.0.0".into(),
            fixed_version: Some("2.0.0".into()),
            severity: FindingSeverity::High,
            cvss_score: Some(7.5),
            description: "Remote code execution vulnerability".into(),
            cwe: Some("CWE-94".into()),
            url: Some("https://example.com/advisory/CVE-2024-1234".into()),
        }
    }

    #[test]
    fn test_check_vulnerable_package() {
        let scanner = ScaScanner::with_advisories(vec![test_advisory()]);
        let findings = scanner.check_package("vulnerable-pkg", "1.5.0", Path::new("Cargo.lock"));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, FindingSeverity::High);
        assert!(findings[0].remediation.is_some());
    }

    #[test]
    fn test_check_fixed_package() {
        let scanner = ScaScanner::with_advisories(vec![test_advisory()]);
        let findings = scanner.check_package("vulnerable-pkg", "2.1.0", Path::new("Cargo.lock"));
        assert!(findings.is_empty(), "Fixed version should have no findings");
    }

    #[test]
    fn test_check_unrelated_package() {
        let scanner = ScaScanner::with_advisories(vec![test_advisory()]);
        let findings = scanner.check_package("safe-pkg", "1.0.0", Path::new("Cargo.lock"));
        assert!(findings.is_empty());
    }

    #[test]
    fn test_version_range_check() {
        assert!(version_in_range("1.0.0", "< 2.0.0"));
        assert!(!version_in_range("2.1.0", "< 2.0.0"));
        assert!(version_in_range("1.0.0", "*"));
        assert!(version_in_range("1.5.0", "1.5.0"));
    }

    #[test]
    fn test_version_lt() {
        assert!(version_lt("1.0.0", "2.0.0"));
        assert!(version_lt("1.0.0", "1.1.0"));
        assert!(!version_lt("2.0.0", "1.0.0"));
        assert!(!version_lt("1.0.0", "1.0.0"));
    }

    // -----------------------------------------------------------------------
    // OSV client tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_osv_query_building() {
        // Build a query for a Cargo package
        let req = OsvClient::query_package("cargo", "serde", "1.0.197").unwrap();
        assert_eq!(req.package.name, "serde");
        assert_eq!(req.package.ecosystem, "crates.io");
        assert_eq!(req.version.as_deref(), Some("1.0.197"));

        // Serializes to expected JSON structure
        let json = OsvClient::query_to_json(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["package"]["name"], "serde");
        assert_eq!(parsed["package"]["ecosystem"], "crates.io");
        assert_eq!(parsed["version"], "1.0.197");

        // npm ecosystem
        let req_npm = OsvClient::query_package("npm", "lodash", "4.17.20").unwrap();
        assert_eq!(req_npm.package.ecosystem, "npm");

        // PyPI
        let req_pypi = OsvClient::query_package("pypi", "requests", "2.28.0").unwrap();
        assert_eq!(req_pypi.package.ecosystem, "PyPI");

        // Unsupported ecosystem returns error
        let err = OsvClient::query_package("unknown_eco", "pkg", "1.0.0");
        assert!(err.is_err());
        assert!(
            matches!(err.unwrap_err(), OsvError::UnsupportedEcosystem(e) if e == "unknown_eco")
        );
    }

    #[test]
    fn test_osv_response_parsing() {
        let response_json = r#"{
            "vulns": [
                {
                    "id": "GHSA-abcd-1234-efgh",
                    "summary": "XSS in template engine",
                    "details": "A cross-site scripting vulnerability (CWE-79) exists in versions < 3.0.0.",
                    "severity": [
                        {"type": "CVSS_V3", "score": "CVSS:3.1/AV:N/AC:L/PR:N/UI:R/S:C/C:L/I:L/A:N/score:6.1"}
                    ],
                    "affected": [
                        {
                            "package": {"name": "template-engine", "ecosystem": "npm"},
                            "ranges": [
                                {
                                    "type": "SEMVER",
                                    "events": [
                                        {"introduced": "1.0.0"},
                                        {"fixed": "3.0.0"}
                                    ]
                                }
                            ],
                            "versions": ["1.0.0", "2.0.0", "2.5.0"]
                        }
                    ],
                    "references": [
                        {"type": "ADVISORY", "url": "https://github.com/advisories/GHSA-abcd-1234-efgh"},
                        {"type": "WEB", "url": "https://example.com/blog/disclosure"}
                    ]
                },
                {
                    "id": "RUSTSEC-2024-0001",
                    "summary": "Buffer overflow in parser",
                    "details": "",
                    "database_specific": {
                        "severity": "HIGH"
                    },
                    "affected": [
                        {
                            "package": {"name": "parser-lib", "ecosystem": "crates.io"},
                            "ranges": [
                                {
                                    "type": "ECOSYSTEM",
                                    "events": [
                                        {"introduced": "0"},
                                        {"fixed": "2.1.0"}
                                    ]
                                }
                            ]
                        }
                    ],
                    "references": []
                }
            ]
        }"#;

        let vulns = OsvClient::parse_response(response_json).unwrap();
        assert_eq!(vulns.len(), 2);

        // First vuln: full CVSS severity
        let v1 = &vulns[0];
        assert_eq!(v1.id, "GHSA-abcd-1234-efgh");
        assert_eq!(v1.summary, "XSS in template engine");
        assert!(v1.details.contains("CWE-79"));
        assert_eq!(v1.cvss_score, Some(6.1));
        assert!(v1.severity.as_ref().unwrap().starts_with("CVSS:3.1/"));
        assert_eq!(v1.fixed_version.as_deref(), Some("3.0.0"));
        assert_eq!(v1.references.len(), 2);
        assert!(v1.references[0].contains("github.com"));
        // Affected ranges include both SEMVER range and enumerated versions
        assert!(v1.affected_ranges.iter().any(|r| r.contains("SEMVER")));
        assert!(v1.affected_ranges.iter().any(|r| r.contains("= 2.5.0")));

        // Second vuln: database_specific severity, no CVSS vector
        let v2 = &vulns[1];
        assert_eq!(v2.id, "RUSTSEC-2024-0001");
        assert_eq!(v2.severity.as_deref(), Some("HIGH"));
        assert_eq!(v2.cvss_score, Some(7.5)); // mapped from "HIGH"
        assert_eq!(v2.fixed_version.as_deref(), Some("2.1.0"));
        assert!(v2.references.is_empty());

        // Empty response
        let empty_vulns = OsvClient::parse_response(r#"{"vulns": []}"#).unwrap();
        assert!(empty_vulns.is_empty());

        // Missing vulns key
        let no_key = OsvClient::parse_response(r#"{}"#).unwrap();
        assert!(no_key.is_empty());
    }

    #[test]
    fn test_semver_comparison() {
        // Proper semver: pre-release versions are less than release
        assert!(version_in_range("1.0.0", "< 2.0.0"));
        assert!(!version_in_range("2.0.0", "< 2.0.0"));
        assert!(!version_in_range("3.0.0", "< 2.0.0"));

        // Less-than-or-equal
        assert!(version_in_range("2.0.0", "<= 2.0.0"));
        assert!(version_in_range("1.9.9", "<= 2.0.0"));
        assert!(!version_in_range("2.0.1", "<= 2.0.0"));

        // Greater-than-or-equal
        assert!(version_in_range("2.0.0", ">= 2.0.0"));
        assert!(version_in_range("3.0.0", ">= 2.0.0"));
        assert!(!version_in_range("1.9.0", ">= 2.0.0"));

        // Greater-than
        assert!(version_in_range("2.0.1", "> 2.0.0"));
        assert!(!version_in_range("2.0.0", "> 2.0.0"));
        assert!(!version_in_range("1.0.0", "> 2.0.0"));

        // Compound range: >= 1.0.0, < 2.0.0
        assert!(version_in_range("1.5.0", ">= 1.0.0, < 2.0.0"));
        assert!(version_in_range("1.0.0", ">= 1.0.0, < 2.0.0"));
        assert!(!version_in_range("2.0.0", ">= 1.0.0, < 2.0.0"));
        assert!(!version_in_range("0.9.0", ">= 1.0.0, < 2.0.0"));

        // Exact match
        assert!(version_in_range("1.2.3", "1.2.3"));
        assert!(!version_in_range("1.2.4", "1.2.3"));

        // Patch-level precision
        assert!(version_in_range("1.0.1", "< 1.0.2"));
        assert!(!version_in_range("1.0.2", "< 1.0.2"));
    }

    #[test]
    fn test_ecosystem_mapping() {
        // Internal ecosystem names -> OSV API identifiers
        assert_eq!(ecosystem_to_osv("cargo"), Some("crates.io"));
        assert_eq!(ecosystem_to_osv("crates.io"), Some("crates.io"));
        assert_eq!(ecosystem_to_osv("npm"), Some("npm"));
        assert_eq!(ecosystem_to_osv("pypi"), Some("PyPI"));
        assert_eq!(ecosystem_to_osv("go"), Some("Go"));
        assert_eq!(ecosystem_to_osv("maven"), Some("Maven"));
        assert_eq!(ecosystem_to_osv("rubygems"), Some("RubyGems"));
        assert_eq!(ecosystem_to_osv("packagist"), Some("Packagist"));
        assert_eq!(ecosystem_to_osv("pub"), Some("Pub"));
        assert_eq!(ecosystem_to_osv("hex"), Some("Hex"));
        assert_eq!(ecosystem_to_osv("swift"), Some("SwiftURL"));
        assert_eq!(ecosystem_to_osv("nuget"), Some("NuGet"));

        // Unknown ecosystem returns None
        assert_eq!(ecosystem_to_osv("homebrew"), None);
        assert_eq!(ecosystem_to_osv(""), None);
    }

    #[test]
    fn test_osv_to_advisory_conversion() {
        let vuln = OsvVulnerability {
            id: "GHSA-test-0001".into(),
            summary: "Prototype pollution (CWE-1321) in deep-merge".into(),
            details: "Allows CWE-1321 prototype pollution via crafted input.".into(),
            severity: Some("CVSS:3.1/AV:N/AC:L/PR:N/UI:N/S:U/C:H/I:H/A:H/score:9.8".into()),
            cvss_score: Some(9.8),
            affected_ranges: vec!["SEMVER: >= 1.0.0, < 3.2.1".into()],
            fixed_version: Some("3.2.1".into()),
            references: vec!["https://github.com/advisories/GHSA-test-0001".into()],
        };

        let advisory = OsvClient::to_advisory(&vuln, "deep-merge");
        assert_eq!(advisory.id, "GHSA-test-0001");
        assert_eq!(advisory.package, "deep-merge");
        assert_eq!(advisory.severity, FindingSeverity::Critical); // 9.8 -> Critical
        assert_eq!(advisory.cvss_score, Some(9.8));
        assert_eq!(advisory.fixed_version.as_deref(), Some("3.2.1"));
        assert_eq!(advisory.affected_range, ">= 1.0.0, < 3.2.1");
        assert_eq!(advisory.cwe.as_deref(), Some("CWE-1321"));
        assert!(advisory.url.as_ref().unwrap().contains("github.com"));
        assert!(advisory.description.contains("prototype pollution"));
    }

    #[test]
    fn test_osv_to_advisory_with_text_severity() {
        // When there is no CVSS score, fall back to textual severity
        let vuln = OsvVulnerability {
            id: "RUSTSEC-2024-0099".into(),
            summary: "Memory safety issue".into(),
            details: "".into(),
            severity: Some("HIGH".into()),
            cvss_score: None,
            affected_ranges: vec![],
            fixed_version: None,
            references: vec![],
        };

        let advisory = OsvClient::to_advisory(&vuln, "unsafe-lib");
        assert_eq!(advisory.severity, FindingSeverity::High);
        assert_eq!(advisory.affected_range, "*"); // no ranges -> wildcard
        assert!(advisory.fixed_version.is_none());
        assert!(advisory.cwe.is_none());
        // When details is empty, summary is used as description
        assert_eq!(advisory.description, "Memory safety issue");
    }

    #[test]
    fn test_extract_cwe_from_text() {
        assert_eq!(
            extract_cwe_from_text("This is a CWE-79 vulnerability"),
            Some("CWE-79".to_string())
        );
        assert_eq!(
            extract_cwe_from_text("Related to CWE-1321 prototype pollution"),
            Some("CWE-1321".to_string())
        );
        assert_eq!(extract_cwe_from_text("No CWE reference here"), None);
        assert_eq!(extract_cwe_from_text(""), None);
    }
}
