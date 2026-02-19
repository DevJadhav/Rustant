//! Dockerfile linter — native static analysis for Dockerfile best practices.
//!
//! Analyzes Dockerfiles for security issues, best practices violations,
//! and optimization opportunities without requiring external tools.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance, FindingSeverity,
    Remediation, RemediationEffort,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A parsed Dockerfile instruction.
#[derive(Debug, Clone)]
pub struct DockerInstruction {
    /// Line number (1-based).
    pub line: usize,
    /// Instruction keyword (FROM, RUN, COPY, etc.).
    pub keyword: String,
    /// Arguments to the instruction.
    pub arguments: String,
    /// Original line text.
    pub raw: String,
}

/// Dockerfile lint rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintRule {
    /// Rule identifier.
    pub id: String,
    /// Rule description.
    pub description: String,
    /// Severity.
    pub severity: String,
    /// Category tag.
    pub category: String,
}

/// Parsed Dockerfile structure.
#[derive(Debug, Clone)]
pub struct ParsedDockerfile {
    /// All instructions.
    pub instructions: Vec<DockerInstruction>,
    /// Build stages (FROM instruction indices).
    pub stages: Vec<usize>,
    /// Whether it's a multi-stage build.
    pub is_multi_stage: bool,
}

/// Dockerfile linter.
pub struct DockerfileLinter {
    /// Max allowed layers for optimization warning.
    _max_layers: usize,
}

impl DockerfileLinter {
    pub fn new() -> Self {
        Self { _max_layers: 20 }
    }

    /// Parse a Dockerfile into structured instructions.
    pub fn parse(content: &str) -> ParsedDockerfile {
        let mut instructions = Vec::new();
        let mut stages = Vec::new();
        let mut continuation = String::new();
        let mut cont_start_line = 0;

        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Skip empty lines and comments
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Handle line continuations
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                if continuation.is_empty() {
                    cont_start_line = idx;
                }
                continuation.push_str(stripped);
                continuation.push(' ');
                continue;
            }

            let full_line = if !continuation.is_empty() {
                let mut full = continuation.clone();
                full.push_str(trimmed);
                continuation.clear();
                (cont_start_line, full)
            } else {
                (idx, trimmed.to_string())
            };

            let (line_num, text) = full_line;

            // Parse keyword and arguments
            if let Some(space_pos) = text.find(|c: char| c.is_whitespace()) {
                let keyword = text[..space_pos].to_uppercase();
                let arguments = text[space_pos..].trim().to_string();

                if keyword == "FROM" {
                    stages.push(instructions.len());
                }

                instructions.push(DockerInstruction {
                    line: line_num + 1,
                    keyword,
                    arguments,
                    raw: text,
                });
            } else {
                instructions.push(DockerInstruction {
                    line: line_num + 1,
                    keyword: text.to_uppercase(),
                    arguments: String::new(),
                    raw: text,
                });
            }
        }

        let is_multi_stage = stages.len() > 1;
        ParsedDockerfile {
            instructions,
            stages,
            is_multi_stage,
        }
    }

    /// Run all lint rules on a parsed Dockerfile.
    pub fn lint(&self, content: &str, file_path: &str) -> Vec<Finding> {
        let parsed = Self::parse(content);
        let mut findings = Vec::new();

        findings.extend(self.check_pinned_versions(&parsed, file_path));
        findings.extend(self.check_user_directive(&parsed, file_path));
        findings.extend(self.check_add_vs_copy(&parsed, file_path));
        findings.extend(self.check_run_consolidation(&parsed, file_path));
        findings.extend(self.check_apt_cleanup(&parsed, file_path));
        findings.extend(self.check_healthcheck(&parsed, file_path));
        findings.extend(self.check_workdir(&parsed, file_path));

        findings
    }

    fn check_pinned_versions(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for instr in &parsed.instructions {
            if instr.keyword == "FROM" {
                let image = instr.arguments.split_whitespace().next().unwrap_or("");
                // Skip scratch and build stage references
                if image == "scratch" {
                    continue;
                }

                if !image.contains(':') || image.ends_with(":latest") {
                    findings.push(self.make_finding(
                        "DL-001",
                        "Pin base image version",
                        &format!("Base image '{image}' is not pinned to a specific version. Use a specific tag for reproducible builds."),
                        FindingSeverity::Medium,
                        file_path,
                        instr,
                        Some("Use a specific version tag, e.g., 'ubuntu:22.04' instead of 'ubuntu' or 'ubuntu:latest'"),
                        RemediationEffort::Trivial,
                    ));
                }
            }
        }

        findings
    }

    fn check_user_directive(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        // Check each stage for USER directive
        for (stage_idx, &stage_start) in parsed.stages.iter().enumerate() {
            let stage_end = parsed
                .stages
                .get(stage_idx + 1)
                .copied()
                .unwrap_or(parsed.instructions.len());
            let stage_instrs = &parsed.instructions[stage_start..stage_end];

            // Skip intermediate build stages in multi-stage builds
            if parsed.is_multi_stage && stage_idx < parsed.stages.len() - 1 {
                continue;
            }

            let has_user = stage_instrs.iter().any(|i| i.keyword == "USER");
            let has_cmd = stage_instrs
                .iter()
                .any(|i| i.keyword == "CMD" || i.keyword == "ENTRYPOINT");

            if has_cmd
                && !has_user
                && let Some(cmd_instr) = stage_instrs
                    .iter()
                    .find(|i| i.keyword == "CMD" || i.keyword == "ENTRYPOINT")
            {
                findings.push(self.make_finding(
                    "DL-002",
                    "Add USER directive",
                    "Final stage has no USER directive. Container will run as root by default.",
                    FindingSeverity::High,
                    file_path,
                    cmd_instr,
                    Some("Add 'USER nonroot' or 'USER 1001' before CMD/ENTRYPOINT"),
                    RemediationEffort::Low,
                ));
            }
        }

        findings
    }

    fn check_add_vs_copy(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for instr in &parsed.instructions {
            if instr.keyword == "ADD" {
                let args = &instr.arguments;
                // ADD is ok for URLs and tar files
                if !args.contains("http://")
                    && !args.contains("https://")
                    && !args.contains(".tar")
                    && !args.contains(".gz")
                    && !args.contains(".bz2")
                    && !args.contains(".xz")
                {
                    findings.push(self.make_finding(
                        "DL-003",
                        "Use COPY instead of ADD",
                        "ADD has implicit tar extraction and URL fetching. Use COPY for simple file copies.",
                        FindingSeverity::Low,
                        file_path,
                        instr,
                        Some("Replace ADD with COPY for non-archive, non-URL sources"),
                        RemediationEffort::Trivial,
                    ));
                }
            }
        }

        findings
    }

    fn check_run_consolidation(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut consecutive_runs = 0;
        let mut first_run_instr = None;

        for instr in &parsed.instructions {
            if instr.keyword == "RUN" {
                consecutive_runs += 1;
                if consecutive_runs == 1 {
                    first_run_instr = Some(instr);
                }
            } else {
                if consecutive_runs >= 3
                    && let Some(first) = first_run_instr
                {
                    findings.push(self.make_finding(
                        "DL-004",
                        "Consolidate RUN instructions",
                        &format!("{consecutive_runs} consecutive RUN instructions create unnecessary layers. Combine with &&."),
                        FindingSeverity::Info,
                        file_path,
                        first,
                        Some("Combine RUN instructions using && to reduce layers"),
                        RemediationEffort::Low,
                    ));
                }
                consecutive_runs = 0;
                first_run_instr = None;
            }
        }

        // Check trailing consecutive runs
        if consecutive_runs >= 3
            && let Some(first) = first_run_instr
        {
            findings.push(self.make_finding(
                "DL-004",
                "Consolidate RUN instructions",
                &format!(
                    "{consecutive_runs} consecutive RUN instructions create unnecessary layers."
                ),
                FindingSeverity::Info,
                file_path,
                first,
                Some("Combine RUN instructions using && to reduce layers"),
                RemediationEffort::Low,
            ));
        }

        findings
    }

    fn check_apt_cleanup(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for instr in &parsed.instructions {
            if instr.keyword == "RUN" {
                let args = &instr.arguments;
                if (args.contains("apt-get install") || args.contains("apt install"))
                    && !args.contains("rm -rf /var/lib/apt/lists")
                    && !args.contains("apt-get clean")
                {
                    findings.push(self.make_finding(
                        "DL-005",
                        "Clean apt cache after install",
                        "apt-get install without cleanup leaves cache in the layer, increasing image size.",
                        FindingSeverity::Info,
                        file_path,
                        instr,
                        Some("Add '&& rm -rf /var/lib/apt/lists/*' after apt-get install"),
                        RemediationEffort::Trivial,
                    ));
                }
            }
        }

        findings
    }

    fn check_healthcheck(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let has_healthcheck = parsed
            .instructions
            .iter()
            .any(|i| i.keyword == "HEALTHCHECK");
        let has_cmd = parsed
            .instructions
            .iter()
            .any(|i| i.keyword == "CMD" || i.keyword == "ENTRYPOINT");

        if has_cmd
            && !has_healthcheck
            && let Some(last) = parsed.instructions.last()
        {
            return vec![self.make_finding(
                "DL-006",
                "Add HEALTHCHECK instruction",
                "No HEALTHCHECK defined. Container orchestrators cannot determine container health.",
                FindingSeverity::Info,
                file_path,
                last,
                Some("Add HEALTHCHECK --interval=30s CMD curl -f http://localhost/ || exit 1"),
                RemediationEffort::Low,
            )];
        }

        Vec::new()
    }

    fn check_workdir(&self, parsed: &ParsedDockerfile, file_path: &str) -> Vec<Finding> {
        let mut findings = Vec::new();

        for instr in &parsed.instructions {
            if instr.keyword == "WORKDIR" && !instr.arguments.starts_with('/') {
                findings.push(self.make_finding(
                    "DL-007",
                    "Use absolute WORKDIR path",
                    "WORKDIR should use absolute paths for clarity and reliability.",
                    FindingSeverity::Low,
                    file_path,
                    instr,
                    Some("Use absolute path like '/app' instead of relative path"),
                    RemediationEffort::Trivial,
                ));
            }
        }

        findings
    }

    #[allow(clippy::too_many_arguments)]
    fn make_finding(
        &self,
        rule_id: &str,
        title: &str,
        description: &str,
        severity: FindingSeverity,
        file_path: &str,
        instr: &DockerInstruction,
        remediation: Option<&str>,
        effort: RemediationEffort,
    ) -> Finding {
        let mut finding = Finding::new(
            title,
            description,
            severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "dockerfile".to_string(),
                rule_id: Some(rule_id.to_string()),
                confidence: 0.90,
                consensus: None,
            },
        );

        finding = finding.with_location(CodeLocation {
            file: file_path.into(),
            start_line: instr.line,
            end_line: Some(instr.line),
            start_column: Some(1),
            end_column: None,
            function_name: None,
        });

        if let Some(rem) = remediation {
            finding = finding.with_remediation(Remediation {
                description: rem.to_string(),
                patch: None,
                effort: Some(effort),
                confidence: 0.9,
            });
        }

        finding = finding.with_explanation(FindingExplanation {
            reasoning_chain: vec![
                format!("Found {} instruction at line {}", instr.keyword, instr.line),
                description.to_string(),
            ],
            evidence: vec![instr.raw.clone()],
            context_factors: Vec::new(),
        });

        finding.with_tag("dockerfile")
    }
}

impl Default for DockerfileLinter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for DockerfileLinter {
    fn name(&self) -> &str {
        "dockerfile"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Security, FindingCategory::Quality]
    }

    fn supports_language(&self, language: &str) -> bool {
        language == "dockerfile" || language == "docker"
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        let mut findings = Vec::new();

        for file in &context.files {
            let filename = file.file_name().and_then(|f| f.to_str()).unwrap_or("");
            if filename == "Dockerfile"
                || filename.starts_with("Dockerfile.")
                || filename.ends_with(".dockerfile")
            {
                let content =
                    std::fs::read_to_string(file).map_err(|e| ScanError::ScannerFailed {
                        scanner: "dockerfile".into(),
                        message: format!("Failed to read {}: {}", file.display(), e),
                    })?;
                findings.extend(self.lint(&content, &file.display().to_string()));
            }
        }

        Ok(findings)
    }

    fn risk_level(&self) -> ScannerRiskLevel {
        ScannerRiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dockerfile() {
        let content = "FROM ubuntu:22.04\nRUN apt-get update\nCOPY . /app\nCMD [\"/app\"]\n";
        let parsed = DockerfileLinter::parse(content);
        assert_eq!(parsed.instructions.len(), 4);
        assert_eq!(parsed.stages.len(), 1);
        assert!(!parsed.is_multi_stage);
    }

    #[test]
    fn test_parse_multistage() {
        let content = "FROM golang:1.22 AS builder\nRUN go build\nFROM alpine:3.19\nCOPY --from=builder /app /app\nCMD [\"/app\"]\n";
        let parsed = DockerfileLinter::parse(content);
        assert_eq!(parsed.stages.len(), 2);
        assert!(parsed.is_multi_stage);
    }

    #[test]
    fn test_lint_unpinned_version() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu\nCMD [\"/bin/bash\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(findings.iter().any(|f| f.title.contains("Pin base image")));
    }

    #[test]
    fn test_lint_no_user() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nRUN echo hello\nCMD [\"/bin/bash\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(findings.iter().any(|f| f.title.contains("USER")));
    }

    #[test]
    fn test_lint_with_user() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nUSER 1001\nCMD [\"/bin/bash\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(!findings.iter().any(|f| f.title.contains("Add USER")));
    }

    #[test]
    fn test_lint_add_vs_copy() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nADD app.py /app/\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(
            findings
                .iter()
                .any(|f| f.title.contains("COPY instead of ADD"))
        );
    }

    #[test]
    fn test_lint_add_tar_ok() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nADD archive.tar.gz /app/\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(
            !findings
                .iter()
                .any(|f| f.title.contains("COPY instead of ADD"))
        );
    }

    #[test]
    fn test_lint_apt_cleanup() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nRUN apt-get install -y curl\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(findings.iter().any(|f| f.title.contains("apt cache")));
    }

    #[test]
    fn test_lint_consecutive_runs() {
        let linter = DockerfileLinter::new();
        let content =
            "FROM ubuntu:22.04\nRUN echo a\nRUN echo b\nRUN echo c\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(findings.iter().any(|f| f.title.contains("Consolidate")));
    }

    #[test]
    fn test_lint_relative_workdir() {
        let linter = DockerfileLinter::new();
        let content = "FROM ubuntu:22.04\nWORKDIR app\nUSER 1001\nCMD [\"/app\"]\n";
        let findings = linter.lint(content, "Dockerfile");
        assert!(
            findings
                .iter()
                .any(|f| f.title.contains("absolute WORKDIR"))
        );
    }

    #[test]
    fn test_lint_line_continuation() {
        let content = "FROM ubuntu:22.04\nRUN apt-get update && \\\n    apt-get install -y curl && \\\n    rm -rf /var/lib/apt/lists/*\nUSER 1001\nCMD [\"/app\"]\n";
        let parsed = DockerfileLinter::parse(content);
        // The RUN with continuations should be parsed as one instruction
        let run_count = parsed
            .instructions
            .iter()
            .filter(|i| i.keyword == "RUN")
            .count();
        assert_eq!(run_count, 1);
    }

    #[test]
    fn test_scanner_metadata() {
        let linter = DockerfileLinter::new();
        assert_eq!(linter.name(), "dockerfile");
        assert_eq!(linter.risk_level(), ScannerRiskLevel::ReadOnly);
        assert!(linter.supports_language("dockerfile"));
        assert!(!linter.supports_language("python"));
    }
}
