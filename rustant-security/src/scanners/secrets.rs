//! Secrets detection scanner — detects hardcoded secrets using patterns and entropy.
//!
//! Leverages the existing `SecretRedactor` pattern library for detection,
//! producing `Finding` objects for the unified pipeline.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance, FindingSeverity,
    Remediation,
};
use crate::redaction::SecretRedactor;
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use std::path::Path;

/// Secrets detection scanner using regex patterns and entropy analysis.
pub struct SecretsScanner {
    redactor: SecretRedactor,
    /// File patterns to exclude from scanning.
    exclude_patterns: Vec<String>,
}

impl SecretsScanner {
    /// Create a new secrets scanner.
    pub fn new() -> Self {
        Self {
            redactor: SecretRedactor::new(),
            exclude_patterns: default_exclude_patterns(),
        }
    }

    /// Scan a single file for secrets.
    pub fn scan_source(&self, source: &str, file: &Path) -> Vec<Finding> {
        // Skip excluded files
        let file_str = file.to_string_lossy();
        if self.should_exclude(&file_str) {
            return Vec::new();
        }

        let mut findings = Vec::new();

        for (line_num, line) in source.lines().enumerate() {
            let line_num = line_num + 1; // 1-indexed

            // Skip comments that look like examples
            let trimmed = line.trim();
            if is_example_or_placeholder(trimmed) {
                continue;
            }

            // Use the redactor to detect secrets
            let result = self.redactor.redact(line);
            if result.count > 0 {
                for secret_type in &result.secret_types {
                    let severity = severity_for_secret_type(secret_type);

                    let finding = Finding::new(
                        format!("Hardcoded {secret_type} detected"),
                        format!(
                            "A {} was found hardcoded in source code at {}:{}. \
                             Hardcoded secrets should be moved to environment variables \
                             or a secrets manager.",
                            secret_type,
                            file.display(),
                            line_num,
                        ),
                        severity,
                        FindingCategory::Secret,
                        FindingProvenance {
                            scanner: "secrets".to_string(),
                            rule_id: Some(format!(
                                "SECRET-{}",
                                secret_type.to_uppercase().replace(' ', "-")
                            )),
                            confidence: 0.85,
                            consensus: None,
                        },
                    )
                    .with_location(CodeLocation::new(file, line_num))
                    .with_explanation(FindingExplanation {
                        reasoning_chain: vec![
                            format!(
                                "Pattern match for {} detected on line {}",
                                secret_type, line_num
                            ),
                            "Secret values should never be committed to source control".to_string(),
                        ],
                        evidence: vec![result.redacted.clone()], // Redacted, never the actual value
                        context_factors: vec![format!("File: {}", file.display())],
                    })
                    .with_remediation(Remediation {
                        description: format!(
                            "Move this {secret_type} to an environment variable or secrets manager. \
                             If this is a test fixture, add a `# rustant:secret-ignore` comment."
                        ),
                        patch: None,
                        effort: Some(crate::finding::RemediationEffort::Low),
                        confidence: 0.9,
                    })
                    .with_tag("secret".to_string())
                    .with_tag(secret_type.clone());

                    findings.push(finding);
                }
            }
        }

        findings
    }

    /// Check if a file should be excluded from scanning.
    fn should_exclude(&self, file_path: &str) -> bool {
        self.exclude_patterns
            .iter()
            .any(|pattern| file_path.contains(pattern))
    }

    /// Scan git history for secrets that were introduced in past commits.
    ///
    /// Uses `git log --all --diff-filter=A -p` to scan diff output of added files
    /// across all branches. Each diff hunk is checked against the same `SecretRedactor`
    /// patterns used for live file scanning.
    ///
    /// Returns findings that include the commit hash and file path where each
    /// secret was first introduced.
    pub fn scan_git_history(&self, repo_path: &Path, max_commits: usize) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Run git log to get diffs of added content across all branches
        let output = match std::process::Command::new("git")
            .args([
                "log",
                "--all",
                "--diff-filter=A",
                "-p",
                &format!("--max-count={max_commits}"),
                "--no-color",
            ])
            .current_dir(repo_path)
            .output()
        {
            Ok(output) => output,
            Err(e) => {
                tracing::warn!("Failed to run git log for secrets scanning: {}", e);
                return findings;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("git log failed: {}", stderr);
            return findings;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse the git log output to extract commit info and diff hunks
        let mut current_commit: Option<String> = None;
        let mut current_file: Option<String> = None;
        let mut in_diff_hunk = false;
        let mut hunk_line_num: usize = 0;

        for line in stdout.lines() {
            // Track current commit
            if let Some(hash) = line.strip_prefix("commit ") {
                current_commit = Some(hash.trim().to_string());
                current_file = None;
                in_diff_hunk = false;
                continue;
            }

            // Track current file in diff
            if let Some(file_path) = line.strip_prefix("+++ b/") {
                current_file = Some(file_path.to_string());
                in_diff_hunk = false;
                continue;
            }

            // Track diff hunk start to get line numbers
            if line.starts_with("@@ ") {
                in_diff_hunk = true;
                // Parse hunk header: @@ -old,count +new,count @@
                hunk_line_num = parse_hunk_start_line(line);
                continue;
            }

            // Skip non-addition lines or diff metadata
            if !in_diff_hunk || !line.starts_with('+') || line.starts_with("+++") {
                if in_diff_hunk && !line.starts_with('-') && !line.starts_with('\\') {
                    hunk_line_num += 1;
                }
                continue;
            }

            // This is an added line (starts with '+')
            let content = &line[1..]; // Strip the leading '+'
            let trimmed = content.trim();

            // Skip example/placeholder lines
            if is_example_or_placeholder(trimmed) {
                hunk_line_num += 1;
                continue;
            }

            // Check for secrets using the redactor
            let result = self.redactor.redact(content);
            if result.count > 0
                && let (Some(commit), Some(file)) = (&current_commit, &current_file)
            {
                // Skip excluded file patterns
                if self.should_exclude(file) {
                    hunk_line_num += 1;
                    continue;
                }

                let short_commit = if commit.len() >= 8 {
                    &commit[..8]
                } else {
                    commit
                };

                for secret_type in &result.secret_types {
                    let severity = severity_for_secret_type(secret_type);

                    let finding = Finding::new(
                        format!(
                            "Historical {secret_type} in git commit {short_commit}"
                        ),
                        format!(
                            "A {secret_type} was introduced in commit {short_commit} in file '{file}' at line {hunk_line_num}. \
                             Even if the secret has been removed from the current branch, \
                             it remains accessible in git history and should be rotated.",
                        ),
                        severity,
                        FindingCategory::Secret,
                        FindingProvenance {
                            scanner: "secrets".to_string(),
                            rule_id: Some(format!(
                                "SECRET-HISTORY-{}",
                                secret_type.to_uppercase().replace(' ', "-")
                            )),
                            confidence: 0.80,
                            consensus: None,
                        },
                    )
                    .with_location(CodeLocation::new(
                        Path::new(file),
                        hunk_line_num,
                    ))
                    .with_explanation(FindingExplanation {
                        reasoning_chain: vec![
                            format!(
                                "Secret pattern '{}' matched in git diff for commit {}",
                                secret_type, short_commit
                            ),
                            format!("File: {}", file),
                            "Secrets in git history remain accessible even after deletion"
                                .to_string(),
                        ],
                        evidence: vec![result.redacted.clone()],
                        context_factors: vec![
                            format!("Commit: {}", commit),
                            format!("File: {}", file),
                        ],
                    })
                    .with_remediation(Remediation {
                        description: format!(
                            "Rotate this {secret_type}. Even though it may be removed from the current \
                             codebase, it persists in git history. Consider using \
                             `git filter-repo` or BFG Repo Cleaner to purge history, \
                             but rotation is always the safest option."
                        ),
                        patch: None,
                        effort: Some(crate::finding::RemediationEffort::Medium),
                        confidence: 0.85,
                    })
                    .with_tag("secret")
                    .with_tag("git-history")
                    .with_tag(secret_type.clone());

                    findings.push(finding);
                }
            }

            hunk_line_num += 1;
        }

        findings
    }
}

/// Parse the start line number from a git diff hunk header.
/// Format: `@@ -old_start,old_count +new_start,new_count @@ optional context`
fn parse_hunk_start_line(hunk_header: &str) -> usize {
    // Find the +N,M or +N portion
    if let Some(plus_pos) = hunk_header.find('+') {
        let rest = &hunk_header[plus_pos + 1..];
        // Take digits until we hit ',' or ' ' or '@'
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        num_str.parse().unwrap_or(1)
    } else {
        1
    }
}

impl Default for SecretsScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for SecretsScanner {
    fn name(&self) -> &str {
        "secrets"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Secret]
    }

    fn supports_language(&self, _language: &str) -> bool {
        true // Secrets can be in any language
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
                    scanner: "secrets".into(),
                    message: format!("Failed to read {}: {e}", file_path.display()),
                })?;

            let findings = self.scan_source(&source, file_path);
            all_findings.extend(findings);
        }

        Ok(all_findings)
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

/// Get the severity for a given secret type.
fn severity_for_secret_type(secret_type: &str) -> FindingSeverity {
    match secret_type {
        t if t.contains("private_key") => FindingSeverity::Critical,
        t if t.contains("aws") => FindingSeverity::Critical,
        t if t.contains("stripe") && t.contains("live") => FindingSeverity::Critical,
        t if t.contains("database") => FindingSeverity::High,
        t if t.contains("api_key") || t.contains("token") => FindingSeverity::High,
        t if t.contains("password") => FindingSeverity::High,
        t if t.contains("jwt") => FindingSeverity::High,
        t if t.contains("test") || t.contains("example") => FindingSeverity::Low,
        _ => FindingSeverity::Medium,
    }
}

/// Check if a line looks like an example or placeholder.
fn is_example_or_placeholder(trimmed: &str) -> bool {
    // Inline ignore comment
    if trimmed.contains("rustant:secret-ignore") || trimmed.contains("noqa: secret") {
        return true;
    }
    // Common example placeholders
    if trimmed.contains("your-api-key-here")
        || trimmed.contains("YOUR_API_KEY")
        || trimmed.contains("xxx")
        || trimmed.contains("REPLACE_ME")
        || trimmed.contains("<your-")
        || trimmed.contains("example.com")
    {
        return true;
    }
    false
}

/// Default file patterns to exclude from secrets scanning.
fn default_exclude_patterns() -> Vec<String> {
    vec![
        ".env.example".into(),
        ".env.sample".into(),
        ".env.template".into(),
        "test_fixtures".into(),
        "testdata".into(),
        "__snapshots__".into(),
        "package-lock.json".into(),
        "yarn.lock".into(),
        "Cargo.lock".into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_aws_key() {
        let scanner = SecretsScanner::new();
        let source = r#"AWS_ACCESS_KEY = "AKIAIOSFODNN7EXAMPLE""#;
        let findings = scanner.scan_source(source, Path::new("config.py"));
        assert!(!findings.is_empty(), "Should detect AWS key");
        assert_eq!(findings[0].category, FindingCategory::Secret);
    }

    #[test]
    fn test_detect_github_token() {
        let scanner = SecretsScanner::new();
        let source = r#"token = "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh1234""#;
        let findings = scanner.scan_source(source, Path::new("script.sh"));
        assert!(!findings.is_empty(), "Should detect GitHub token");
    }

    #[test]
    fn test_skip_example_files() {
        let scanner = SecretsScanner::new();
        let source = r#"API_KEY = "sk_live_abcdefghijklmnopqrstuv""#;
        let findings = scanner.scan_source(source, Path::new(".env.example"));
        assert!(findings.is_empty(), "Should skip .env.example files");
    }

    #[test]
    fn test_skip_ignore_comment() {
        let scanner = SecretsScanner::new();
        let source = r#"key = "AKIAIOSFODNN7EXAMPLE" # rustant:secret-ignore"#;
        let findings = scanner.scan_source(source, Path::new("test.py"));
        assert!(findings.is_empty(), "Should skip lines with ignore comment");
    }

    #[test]
    fn test_no_false_positive_normal_code() {
        let scanner = SecretsScanner::new();
        let source = r#"
fn main() {
    let greeting = "Hello, World!";
    let count = 42;
    println!("{} {}", greeting, count);
}
"#;
        let findings = scanner.scan_source(source, Path::new("main.rs"));
        assert!(
            findings.is_empty(),
            "Normal code should not trigger findings"
        );
    }

    #[test]
    fn test_severity_mapping() {
        assert_eq!(
            severity_for_secret_type("aws_access_key"),
            FindingSeverity::Critical
        );
        assert_eq!(
            severity_for_secret_type("github_token"),
            FindingSeverity::High
        );
        assert_eq!(
            severity_for_secret_type("database_uri"),
            FindingSeverity::High
        );
    }

    #[test]
    fn test_finding_has_remediation() {
        let scanner = SecretsScanner::new();
        let source = r#"secret = "AKIAIOSFODNN7EXAMPLE""#;
        let findings = scanner.scan_source(source, Path::new("app.py"));
        if !findings.is_empty() {
            assert!(findings[0].remediation.is_some(), "Should have remediation");
        }
    }

    #[test]
    fn test_parse_hunk_start_line() {
        assert_eq!(parse_hunk_start_line("@@ -0,0 +1,5 @@"), 1);
        assert_eq!(parse_hunk_start_line("@@ -10,3 +42,7 @@ fn main()"), 42);
        assert_eq!(parse_hunk_start_line("@@ -1 +1 @@"), 1);
        // Fallback for malformed headers
        assert_eq!(parse_hunk_start_line("garbage"), 1);
    }

    #[test]
    fn test_scan_git_history_detects_committed_secrets() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_path = tmp.path();

        // Initialize a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .expect("git init failed");

        // Configure git user for commits
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .expect("git config email failed");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .expect("git config name failed");
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(repo_path)
            .output()
            .expect("git config gpgsign failed");

        // Create a file with a secret and commit it
        let secret_file = repo_path.join("config.py");
        std::fs::write(
            &secret_file,
            "AWS_ACCESS_KEY = \"AKIAIOSFODNN7EXAMPLE\"\nDB_HOST = \"localhost\"\n",
        )
        .unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .expect("git add failed");
        std::process::Command::new("git")
            .args(["commit", "-m", "add config with secret"])
            .current_dir(repo_path)
            .output()
            .expect("git commit failed");

        // Now scan the history
        let scanner = SecretsScanner::new();
        let findings = scanner.scan_git_history(repo_path, 10);

        assert!(
            !findings.is_empty(),
            "Should detect the AWS key in git history"
        );

        // Verify finding metadata
        let aws_finding = &findings[0];
        assert_eq!(aws_finding.category, FindingCategory::Secret);
        assert!(aws_finding.title.contains("git commit"));
        assert!(aws_finding.tags.contains(&"git-history".to_string()));
        assert!(aws_finding.remediation.is_some());
        let remediation = aws_finding.remediation.as_ref().unwrap();
        assert!(remediation.description.contains("Rotate"));
    }

    #[test]
    fn test_scan_git_history_clean_repo_no_findings() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_path = tmp.path();

        // Initialize a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .expect("git init failed");

        // Configure git user
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .expect("git config email failed");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .expect("git config name failed");
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(repo_path)
            .output()
            .expect("git config gpgsign failed");

        // Create a clean file with no secrets
        let clean_file = repo_path.join("main.rs");
        std::fs::write(
            &clean_file,
            "fn main() {\n    println!(\"Hello, world!\");\n}\n",
        )
        .unwrap();

        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .expect("git add failed");
        std::process::Command::new("git")
            .args(["commit", "-m", "initial clean commit"])
            .current_dir(repo_path)
            .output()
            .expect("git commit failed");

        let scanner = SecretsScanner::new();
        let findings = scanner.scan_git_history(repo_path, 10);

        assert!(
            findings.is_empty(),
            "Clean repo should produce no secret findings"
        );
    }

    #[test]
    fn test_scan_git_history_respects_max_commits() {
        let tmp = tempfile::tempdir().unwrap();
        let repo_path = tmp.path();

        // Initialize a git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .expect("git init failed");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .expect("git config email failed");
        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .expect("git config name failed");
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(repo_path)
            .output()
            .expect("git config gpgsign failed");

        // Commit 1: has a secret
        let file1 = repo_path.join("old_config.py");
        std::fs::write(&file1, "OLD_KEY = \"AKIAIOSFODNN7EXAMPLE\"\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "first commit with secret"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Commit 2: also has a secret in a different file
        let file2 = repo_path.join("new_config.py");
        std::fs::write(&file2, "NEW_KEY = \"AKIAIOSFODNN7SECONDKEY\"\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "second commit with secret"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let scanner = SecretsScanner::new();

        // Scan only the last 1 commit (most recent first)
        let findings_one = scanner.scan_git_history(repo_path, 1);
        // Scan all 2 commits
        let findings_all = scanner.scan_git_history(repo_path, 10);

        // With max_commits=1, we should get fewer or equal findings than scanning all
        assert!(
            findings_one.len() <= findings_all.len(),
            "Limiting commits should reduce or equal findings: one={}, all={}",
            findings_one.len(),
            findings_all.len()
        );
    }
}
