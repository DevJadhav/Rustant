//! SAST rule engine — static application security testing via pattern matching.
//!
//! Provides rule-based vulnerability detection using regex patterns and
//! tree-sitter AST queries when available.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance,
    FindingReference, FindingSeverity, Remediation,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use regex::Regex;
use std::path::Path;

/// A SAST detection rule.
#[derive(Debug, Clone)]
pub struct SastRule {
    /// Unique rule identifier (e.g., "CWE-89", "RUST-001").
    pub id: String,
    /// Human-readable title.
    pub title: String,
    /// Detailed description.
    pub description: String,
    /// Severity level.
    pub severity: FindingSeverity,
    /// Languages this rule applies to.
    pub languages: Vec<String>,
    /// Regex pattern to detect the vulnerability.
    pub pattern: Regex,
    /// CWE reference (e.g., "CWE-89").
    pub cwe: Option<String>,
    /// OWASP reference (e.g., "A03:2021").
    pub owasp: Option<String>,
    /// Suggested fix description.
    pub fix: Option<String>,
}

/// Built-in SAST scanner using regex-based pattern matching.
pub struct SastScanner {
    rules: Vec<SastRule>,
}

impl SastScanner {
    /// Create a new SAST scanner with built-in rules.
    pub fn new() -> Self {
        Self {
            rules: build_default_rules(),
        }
    }

    /// Create with custom rules.
    pub fn with_rules(rules: Vec<SastRule>) -> Self {
        Self { rules }
    }

    /// Add a rule.
    pub fn add_rule(&mut self, rule: SastRule) {
        self.rules.push(rule);
    }

    /// Scan a single file's content against all rules.
    pub fn scan_source(&self, source: &str, file: &Path, language: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for rule in &self.rules {
            if !rule.languages.is_empty() && !rule.languages.iter().any(|l| l == language) {
                continue;
            }

            for mat in rule.pattern.find_iter(source) {
                let line = source[..mat.start()].lines().count();
                let matched_text = mat.as_str();

                let mut finding = Finding::new(
                    &rule.title,
                    format!(
                        "{}\n\nMatched: `{}`",
                        rule.description,
                        truncate(matched_text, 200)
                    ),
                    rule.severity,
                    FindingCategory::Security,
                    FindingProvenance {
                        scanner: "sast".to_string(),
                        rule_id: Some(rule.id.clone()),
                        confidence: 0.7,
                        consensus: None,
                    },
                )
                .with_location(CodeLocation::new(file, line.max(1)))
                .with_explanation(FindingExplanation {
                    reasoning_chain: vec![
                        format!("Rule {} matched pattern in source", rule.id),
                        format!("Found at line {} in {}", line.max(1), file.display()),
                    ],
                    evidence: vec![truncate(matched_text, 500)],
                    context_factors: vec![format!("Language: {language}")],
                })
                .with_tag("sast".to_string());

                if let Some(ref cwe) = rule.cwe {
                    finding = finding.with_reference(FindingReference {
                        ref_type: crate::finding::ReferenceType::Cwe,
                        id: cwe.clone(),
                        url: Some(format!(
                            "https://cwe.mitre.org/data/definitions/{}.html",
                            cwe.strip_prefix("CWE-").unwrap_or(cwe)
                        )),
                    });
                }

                if let Some(ref owasp) = rule.owasp {
                    finding = finding.with_reference(FindingReference {
                        ref_type: crate::finding::ReferenceType::Owasp,
                        id: owasp.clone(),
                        url: None,
                    });
                }

                if let Some(ref fix) = rule.fix {
                    finding = finding.with_remediation(Remediation {
                        description: fix.clone(),
                        patch: None,
                        effort: Some(crate::finding::RemediationEffort::Low),
                        confidence: 0.8,
                    });
                }

                findings.push(finding);
            }
        }

        findings
    }
}

impl Default for SastScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for SastScanner {
    fn name(&self) -> &str {
        "sast"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Security]
    }

    fn supports_language(&self, language: &str) -> bool {
        self.rules
            .iter()
            .any(|r| r.languages.is_empty() || r.languages.iter().any(|l| l == language))
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        let mut all_findings = Vec::new();

        for file_path in &context.files {
            let source =
                std::fs::read_to_string(file_path).map_err(|e| ScanError::ScannerFailed {
                    scanner: "sast".into(),
                    message: format!("Failed to read {}: {e}", file_path.display()),
                })?;

            let language = crate::ast::Language::from_path(file_path);
            let lang_str = format!("{language}");

            let findings = self.scan_source(&source, file_path, &lang_str);
            all_findings.extend(findings);
        }

        Ok(all_findings)
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

/// Truncate a string to max length, adding "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Build the default set of SAST rules.
#[allow(clippy::vec_init_then_push)]
fn build_default_rules() -> Vec<SastRule> {
    let mut rules = Vec::new();

    // SQL Injection patterns
    rules.push(SastRule {
        id: "CWE-89-001".into(),
        title: "Potential SQL injection via string formatting".into(),
        description: "SQL query constructed using string formatting or concatenation may be vulnerable to SQL injection.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"(?i)(?:execute|cursor\.execute|query)\s*\(\s*(?:f"|f'|%s|\.format\()"#).unwrap(),
        cwe: Some("CWE-89".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Use parameterized queries instead of string formatting.".into()),
    });

    rules.push(SastRule {
        id: "CWE-89-002".into(),
        title: "Potential SQL injection via string concatenation".into(),
        description: "SQL query built with string concatenation is vulnerable to injection.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["javascript".into(), "typescript".into()],
        pattern: Regex::new(
            r#"(?i)(?:query|execute)\s*\(\s*[`"'](?:SELECT|INSERT|UPDATE|DELETE|DROP)\b.*\+\s*"#,
        )
        .unwrap(),
        cwe: Some("CWE-89".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Use parameterized queries or prepared statements.".into()),
    });

    // Command Injection
    rules.push(SastRule {
        id: "CWE-78-001".into(),
        title: "Potential command injection".into(),
        description: "Shell command executed with user-controlled input.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"(?:os\.system|os\.popen|subprocess\.call|subprocess\.run|subprocess\.Popen)\s*\(\s*(?:f"|f'|%|\.format)"#).unwrap(),
        cwe: Some("CWE-78".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Use subprocess with a list of arguments instead of shell=True.".into()),
    });

    rules.push(SastRule {
        id: "CWE-78-002".into(),
        title: "Potential command injection via eval".into(),
        description: "Use of eval() with potentially untrusted input.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["python".into(), "javascript".into(), "typescript".into()],
        pattern: Regex::new(r#"\beval\s*\("#).unwrap(),
        cwe: Some("CWE-78".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Avoid eval(). Use safe alternatives like JSON.parse() for JSON data.".into()),
    });

    // XSS patterns
    rules.push(SastRule {
        id: "CWE-79-001".into(),
        title: "Potential XSS via dangerouslySetInnerHTML".into(),
        description: "React dangerouslySetInnerHTML can lead to XSS if input is not sanitized."
            .into(),
        severity: FindingSeverity::High,
        languages: vec!["javascript".into(), "typescript".into()],
        pattern: Regex::new(r#"dangerouslySetInnerHTML"#).unwrap(),
        cwe: Some("CWE-79".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Sanitize HTML input with DOMPurify or similar library.".into()),
    });

    // Unsafe deserialization
    rules.push(SastRule {
        id: "CWE-502-001".into(),
        title: "Unsafe deserialization (pickle)".into(),
        description: "pickle.loads() on untrusted data can lead to arbitrary code execution.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"pickle\.(?:loads?|Unpickler)"#).unwrap(),
        cwe: Some("CWE-502".into()),
        owasp: Some("A08:2021".into()),
        fix: Some("Use json or msgpack for serialization. If pickle is required, only unpickle trusted data.".into()),
    });

    rules.push(SastRule {
        id: "CWE-502-002".into(),
        title: "Unsafe YAML loading".into(),
        description: "yaml.load() without SafeLoader can execute arbitrary code.".into(),
        severity: FindingSeverity::High,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"yaml\.load\s*\("#).unwrap(),
        cwe: Some("CWE-502".into()),
        owasp: Some("A08:2021".into()),
        fix: Some("Use yaml.safe_load() or yaml.load(data, Loader=yaml.SafeLoader).".into()),
    });

    // Hardcoded credentials
    rules.push(SastRule {
        id: "CWE-798-001".into(),
        title: "Potential hardcoded password".into(),
        description: "Password appears to be hardcoded in source code.".into(),
        severity: FindingSeverity::High,
        languages: Vec::new(), // all languages
        pattern: Regex::new(r#"(?i)(?:password|passwd|pwd|secret)\s*[:=]\s*["'][^"']{8,}["']"#)
            .unwrap(),
        cwe: Some("CWE-798".into()),
        owasp: Some("A07:2021".into()),
        fix: Some("Use environment variables or a secrets manager for credentials.".into()),
    });

    // Rust-specific: unsafe blocks
    rules.push(SastRule {
        id: "RUST-001".into(),
        title: "Unsafe block usage".into(),
        description: "Unsafe block found. Review for memory safety violations.".into(),
        severity: FindingSeverity::Medium,
        languages: vec!["rust".into()],
        pattern: Regex::new(r#"\bunsafe\s*\{"#).unwrap(),
        cwe: None,
        owasp: None,
        fix: Some("Minimize unsafe blocks. Document safety invariants.".into()),
    });

    // Path traversal
    rules.push(SastRule {
        id: "CWE-22-001".into(),
        title: "Potential path traversal".into(),
        description: "File path constructed from user input without sanitization.".into(),
        severity: FindingSeverity::High,
        languages: vec!["python".into(), "javascript".into(), "typescript".into()],
        pattern: Regex::new(r#"(?:open|readFile|readFileSync|Path)\s*\(\s*(?:f"|f'|`|\+)"#).unwrap(),
        cwe: Some("CWE-22".into()),
        owasp: Some("A01:2021".into()),
        fix: Some("Validate and sanitize file paths. Use path.resolve() and check against a base directory.".into()),
    });

    // SSRF
    rules.push(SastRule {
        id: "CWE-918-001".into(),
        title: "Potential SSRF via user-controlled URL".into(),
        description: "HTTP request with URL from user input may be exploitable for SSRF.".into(),
        severity: FindingSeverity::High,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"requests\.(?:get|post|put|delete|head)\s*\(\s*(?:f"|f'|%|\+)"#)
            .unwrap(),
        cwe: Some("CWE-918".into()),
        owasp: Some("A10:2021".into()),
        fix: Some("Validate URLs against an allowlist of trusted domains.".into()),
    });

    // Go-specific: SQL string formatting
    rules.push(SastRule {
        id: "CWE-89-003".into(),
        title: "Potential SQL injection via fmt.Sprintf".into(),
        description: "SQL query built with fmt.Sprintf is vulnerable to injection.".into(),
        severity: FindingSeverity::Critical,
        languages: vec!["go".into()],
        pattern: Regex::new(r#"fmt\.Sprintf\s*\(\s*"(?:SELECT|INSERT|UPDATE|DELETE)"#).unwrap(),
        cwe: Some("CWE-89".into()),
        owasp: Some("A03:2021".into()),
        fix: Some("Use query parameters (?) instead of string formatting.".into()),
    });

    // JWT without verification
    rules.push(SastRule {
        id: "CWE-347-001".into(),
        title: "JWT decoded without verification".into(),
        description: "JWT token decoded without signature verification.".into(),
        severity: FindingSeverity::High,
        languages: vec!["python".into(), "javascript".into(), "typescript".into()],
        pattern: Regex::new(r#"jwt\.decode\s*\([^)]*verify\s*=\s*False"#).unwrap(),
        cwe: Some("CWE-347".into()),
        owasp: Some("A02:2021".into()),
        fix: Some("Always verify JWT signatures with the appropriate algorithm and key.".into()),
    });

    // Weak cryptography
    rules.push(SastRule {
        id: "CWE-327-001".into(),
        title: "Weak cryptographic algorithm".into(),
        description: "Use of MD5 or SHA1 for security purposes is cryptographically weak.".into(),
        severity: FindingSeverity::Medium,
        languages: Vec::new(),
        pattern: Regex::new(r#"(?i)\b(?:md5|sha1)\s*\("#).unwrap(),
        cwe: Some("CWE-327".into()),
        owasp: Some("A02:2021".into()),
        fix: Some("Use SHA-256 or stronger hash algorithms for security purposes.".into()),
    });

    // Insecure random
    rules.push(SastRule {
        id: "CWE-330-001".into(),
        title: "Insecure random number generator".into(),
        description: "Non-cryptographic random used where security is needed.".into(),
        severity: FindingSeverity::Medium,
        languages: vec!["python".into()],
        pattern: Regex::new(r#"\brandom\.(random|randint|choice|shuffle)\s*\("#).unwrap(),
        cwe: Some("CWE-330".into()),
        owasp: Some("A02:2021".into()),
        fix: Some("Use secrets module for security-sensitive randomness.".into()),
    });

    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_injection_detection() {
        let scanner = SastScanner::new();
        let source = r#"
def get_user(name):
    cursor.execute(f"SELECT * FROM users WHERE name='{name}'")
"#;
        let findings = scanner.scan_source(source, Path::new("app.py"), "python");
        assert!(!findings.is_empty(), "Should detect SQL injection");
        assert_eq!(findings[0].severity, FindingSeverity::Critical);
    }

    #[test]
    fn test_eval_detection() {
        let scanner = SastScanner::new();
        let source = "const result = eval(userInput);";
        let findings = scanner.scan_source(source, Path::new("app.js"), "javascript");
        assert!(!findings.is_empty(), "Should detect eval usage");
    }

    #[test]
    fn test_unsafe_rust_detection() {
        let scanner = SastScanner::new();
        let source = "fn foo() { unsafe { std::ptr::null::<i32>(); } }";
        let findings = scanner.scan_source(source, Path::new("lib.rs"), "rust");
        assert!(!findings.is_empty(), "Should detect unsafe block");
        assert_eq!(findings[0].severity, FindingSeverity::Medium);
    }

    #[test]
    fn test_hardcoded_password() {
        let scanner = SastScanner::new();
        let source = r#"password = "super_secret_password123""#;
        let findings = scanner.scan_source(source, Path::new("config.py"), "python");
        assert!(!findings.is_empty(), "Should detect hardcoded password");
    }

    #[test]
    fn test_no_false_positive_on_clean_code() {
        let scanner = SastScanner::new();
        let source = r#"
fn main() {
    let x = 42;
    println!("Hello, world!");
}
"#;
        let findings = scanner.scan_source(source, Path::new("main.rs"), "rust");
        assert!(findings.is_empty(), "Clean code should produce no findings");
    }

    #[test]
    fn test_language_filtering() {
        let scanner = SastScanner::new();
        // Python rule should not match Go code
        let source = r#"result := os.system("ls")"#;
        let findings = scanner.scan_source(source, Path::new("main.go"), "go");
        // The os.system pattern is Python-specific, shouldn't trigger for Go
        let python_findings: Vec<_> = findings
            .iter()
            .filter(|f| {
                f.provenance
                    .rule_id
                    .as_ref()
                    .is_some_and(|id| id == "CWE-78-001")
            })
            .collect();
        assert!(python_findings.is_empty());
    }

    #[test]
    fn test_rule_count() {
        let scanner = SastScanner::new();
        assert!(
            scanner.rules.len() >= 15,
            "Should have at least 15 built-in rules"
        );
    }

    #[test]
    fn test_finding_has_references() {
        let scanner = SastScanner::new();
        let source = r#"cursor.execute(f"SELECT * FROM users WHERE id={user_id}")"#;
        let findings = scanner.scan_source(source, Path::new("app.py"), "python");
        assert!(!findings.is_empty());
        let finding = &findings[0];
        assert!(
            !finding.references.is_empty(),
            "Finding should have CWE reference"
        );
    }
}
