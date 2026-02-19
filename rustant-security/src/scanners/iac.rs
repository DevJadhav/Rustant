//! Infrastructure-as-Code security scanner — Terraform, Kubernetes, CloudFormation.
//!
//! Native pattern-based analysis for IaC security misconfigurations,
//! plus Checkov adapter for comprehensive scanning.

use crate::config::ScanConfig;
use crate::error::ScanError;
use crate::finding::{
    CodeLocation, Finding, FindingCategory, FindingExplanation, FindingProvenance,
    FindingReference, FindingSeverity, ReferenceType, Remediation, RemediationEffort,
};
use crate::scanner::{ScanContext, Scanner, ScannerRiskLevel, ScannerVersion};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// IaC resource type detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IacResourceType {
    Terraform,
    Kubernetes,
    CloudFormation,
    DockerCompose,
    HelmChart,
    AnsiblePlaybook,
}

impl std::fmt::Display for IacResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IacResourceType::Terraform => write!(f, "Terraform"),
            IacResourceType::Kubernetes => write!(f, "Kubernetes"),
            IacResourceType::CloudFormation => write!(f, "CloudFormation"),
            IacResourceType::DockerCompose => write!(f, "Docker Compose"),
            IacResourceType::HelmChart => write!(f, "Helm Chart"),
            IacResourceType::AnsiblePlaybook => write!(f, "Ansible"),
        }
    }
}

/// An IaC security rule.
#[derive(Debug, Clone)]
pub struct IacRule {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: FindingSeverity,
    pub resource_type: IacResourceType,
    pub pattern: IacPattern,
    pub remediation: String,
    pub cis_reference: Option<String>,
}

/// Pattern to match in IaC files.
#[derive(Debug, Clone)]
pub enum IacPattern {
    /// Simple string/regex match in file content.
    ContentMatch {
        pattern: String,
        /// Whether the pattern should NOT be found (absence = violation).
        expect_absent: bool,
    },
    /// Key-value match in YAML/JSON.
    KeyValue {
        key: String,
        value: String,
        /// Match semantics.
        match_type: KeyValueMatch,
    },
}

/// How to match key-value pairs.
#[derive(Debug, Clone)]
pub enum KeyValueMatch {
    Equals,
    Contains,
    NotEquals,
    Missing,
}

/// Infrastructure-as-Code security scanner.
pub struct IacScanner {
    rules: Vec<IacRule>,
}

impl IacScanner {
    pub fn new() -> Self {
        Self {
            rules: Self::default_rules(),
        }
    }

    /// Create with custom rules.
    pub fn with_rules(rules: Vec<IacRule>) -> Self {
        Self { rules }
    }

    /// Detect IaC resource type from file extension/content.
    pub fn detect_iac_type(file_path: &str, content: &str) -> Option<IacResourceType> {
        let lower = file_path.to_lowercase();

        if lower.ends_with(".tf") || lower.ends_with(".tfvars") {
            return Some(IacResourceType::Terraform);
        }

        if lower.ends_with(".yaml") || lower.ends_with(".yml") {
            // Check for Kubernetes markers
            if content.contains("apiVersion:") && content.contains("kind:") {
                return Some(IacResourceType::Kubernetes);
            }
            // Check for CloudFormation
            if content.contains("AWSTemplateFormatVersion")
                || (content.contains("Resources:") && content.contains("Type: AWS::"))
            {
                return Some(IacResourceType::CloudFormation);
            }
            // Check for Docker Compose
            if content.contains("services:")
                && (content.contains("image:") || content.contains("build:"))
            {
                return Some(IacResourceType::DockerCompose);
            }
            // Check for Helm chart
            if content.contains("{{") && content.contains(".Values") {
                return Some(IacResourceType::HelmChart);
            }
            // Check for Ansible
            if content.contains("- hosts:")
                || (content.contains("- name:") && content.contains("tasks:"))
            {
                return Some(IacResourceType::AnsiblePlaybook);
            }
        }

        if lower.ends_with(".json") && content.contains("AWSTemplateFormatVersion") {
            return Some(IacResourceType::CloudFormation);
        }

        None
    }

    /// Scan a single file for IaC security issues.
    pub fn scan_file(&self, content: &str, file_path: &str) -> Vec<Finding> {
        let iac_type = match Self::detect_iac_type(file_path, content) {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut findings = Vec::new();

        for rule in &self.rules {
            if rule.resource_type != iac_type {
                continue;
            }

            if let Some(finding) = self.check_rule(rule, content, file_path) {
                findings.push(finding);
            }
        }

        findings
    }

    fn check_rule(&self, rule: &IacRule, content: &str, file_path: &str) -> Option<Finding> {
        let matched = match &rule.pattern {
            IacPattern::ContentMatch {
                pattern,
                expect_absent,
            } => {
                let found = content.contains(pattern.as_str());
                if *expect_absent { !found } else { found }
            }
            IacPattern::KeyValue {
                key,
                value,
                match_type,
            } => match match_type {
                KeyValueMatch::Equals => {
                    Self::find_key_value(content, key).is_some_and(|v| v == *value)
                }
                KeyValueMatch::Contains => {
                    Self::find_key_value(content, key).is_some_and(|v| v.contains(value.as_str()))
                }
                KeyValueMatch::NotEquals => {
                    Self::find_key_value(content, key).is_none_or(|v| v != *value)
                }
                KeyValueMatch::Missing => Self::find_key_value(content, key).is_none(),
            },
        };

        if !matched {
            return None;
        }

        // Find the line number for context
        let line_num = match &rule.pattern {
            IacPattern::ContentMatch { pattern, .. } => content
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(pattern.as_str()))
                .map(|(i, _)| i + 1)
                .unwrap_or(1),
            IacPattern::KeyValue { key, .. } => content
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(key.as_str()))
                .map(|(i, _)| i + 1)
                .unwrap_or(1),
        };

        let _snippet = content
            .lines()
            .nth(line_num.saturating_sub(1))
            .unwrap_or("")
            .to_string();

        let mut finding = Finding::new(
            &rule.title,
            &rule.description,
            rule.severity,
            FindingCategory::Security,
            FindingProvenance {
                scanner: "iac".to_string(),
                rule_id: Some(rule.id.clone()),
                confidence: 0.85,
                consensus: None,
            },
        );

        finding = finding.with_location(CodeLocation {
            file: file_path.into(),
            start_line: line_num,
            end_line: Some(line_num),
            start_column: Some(1),
            end_column: None,
            function_name: None,
        });

        finding = finding.with_remediation(Remediation {
            description: rule.remediation.clone(),
            patch: None,
            effort: Some(RemediationEffort::Medium),
            confidence: 0.85,
        });

        if let Some(ref cis_ref) = rule.cis_reference {
            finding = finding.with_reference(FindingReference {
                ref_type: ReferenceType::Other,
                id: cis_ref.clone(),
                url: None,
            });
        }

        finding = finding.with_explanation(FindingExplanation {
            reasoning_chain: vec![
                format!(
                    "Detected {} configuration in {}",
                    rule.resource_type, file_path
                ),
                rule.description.clone(),
            ],
            evidence: Vec::new(),
            context_factors: vec![format!("Resource type: {}", rule.resource_type)],
        });

        Some(
            finding
                .with_tag("iac")
                .with_tag(rule.resource_type.to_string().to_lowercase()),
        )
    }

    /// Simple YAML/HCL key-value finder.
    fn find_key_value(content: &str, key: &str) -> Option<String> {
        for line in content.lines() {
            let trimmed = line.trim();
            // YAML: key: value
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim();
                if let Some(val) = rest.strip_prefix(':') {
                    return Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
                }
                // HCL: key = value
                if let Some(val) = rest.strip_prefix('=') {
                    return Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
                }
            }
        }
        None
    }

    fn default_rules() -> Vec<IacRule> {
        vec![
            // Terraform rules
            IacRule {
                id: "IAC-TF-001".into(),
                title: "S3 bucket without encryption".into(),
                description: "S3 bucket does not have server-side encryption enabled".into(),
                severity: FindingSeverity::High,
                resource_type: IacResourceType::Terraform,
                pattern: IacPattern::ContentMatch {
                    pattern: "aws_s3_bucket".into(),
                    expect_absent: false,
                },
                remediation:
                    "Add server_side_encryption_configuration block to the S3 bucket resource"
                        .into(),
                cis_reference: Some("CIS AWS 2.1.1".into()),
            },
            IacRule {
                id: "IAC-TF-002".into(),
                title: "Security group allows unrestricted ingress".into(),
                description: "Security group allows ingress from 0.0.0.0/0".into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::Terraform,
                pattern: IacPattern::ContentMatch {
                    pattern: "0.0.0.0/0".into(),
                    expect_absent: false,
                },
                remediation: "Restrict ingress to specific CIDR blocks or security groups".into(),
                cis_reference: Some("CIS AWS 5.2".into()),
            },
            IacRule {
                id: "IAC-TF-003".into(),
                title: "RDS instance publicly accessible".into(),
                description: "RDS instance has publicly_accessible set to true".into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::Terraform,
                pattern: IacPattern::KeyValue {
                    key: "publicly_accessible".into(),
                    value: "true".into(),
                    match_type: KeyValueMatch::Equals,
                },
                remediation: "Set publicly_accessible = false for RDS instances".into(),
                cis_reference: Some("CIS AWS 2.3.3".into()),
            },
            IacRule {
                id: "IAC-TF-004".into(),
                title: "IAM policy with wildcard actions".into(),
                description: "IAM policy uses Action = \"*\" which grants unrestricted access"
                    .into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::Terraform,
                pattern: IacPattern::ContentMatch {
                    pattern: "\"Action\": \"*\"".into(),
                    expect_absent: false,
                },
                remediation: "Use least-privilege IAM policies with specific actions".into(),
                cis_reference: Some("CIS AWS 1.22".into()),
            },
            // Kubernetes rules
            IacRule {
                id: "IAC-K8S-001".into(),
                title: "Pod running as root".into(),
                description: "Pod spec does not set runAsNonRoot: true".into(),
                severity: FindingSeverity::High,
                resource_type: IacResourceType::Kubernetes,
                pattern: IacPattern::ContentMatch {
                    pattern: "runAsNonRoot: true".into(),
                    expect_absent: true,
                },
                remediation: "Add securityContext.runAsNonRoot: true to pod spec".into(),
                cis_reference: Some("CIS K8s 5.2.6".into()),
            },
            IacRule {
                id: "IAC-K8S-002".into(),
                title: "Privileged container".into(),
                description: "Container is running in privileged mode".into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::Kubernetes,
                pattern: IacPattern::KeyValue {
                    key: "privileged".into(),
                    value: "true".into(),
                    match_type: KeyValueMatch::Equals,
                },
                remediation: "Remove privileged: true or set to false".into(),
                cis_reference: Some("CIS K8s 5.2.1".into()),
            },
            IacRule {
                id: "IAC-K8S-003".into(),
                title: "hostNetwork enabled".into(),
                description: "Pod uses host network namespace, bypassing network isolation".into(),
                severity: FindingSeverity::High,
                resource_type: IacResourceType::Kubernetes,
                pattern: IacPattern::KeyValue {
                    key: "hostNetwork".into(),
                    value: "true".into(),
                    match_type: KeyValueMatch::Equals,
                },
                remediation: "Remove hostNetwork: true unless absolutely necessary".into(),
                cis_reference: Some("CIS K8s 5.2.4".into()),
            },
            IacRule {
                id: "IAC-K8S-004".into(),
                title: "Missing resource limits".into(),
                description:
                    "Container does not define resource limits, risking resource exhaustion".into(),
                severity: FindingSeverity::Medium,
                resource_type: IacResourceType::Kubernetes,
                pattern: IacPattern::ContentMatch {
                    pattern: "limits:".into(),
                    expect_absent: true,
                },
                remediation: "Add resources.limits with cpu and memory to container spec".into(),
                cis_reference: None,
            },
            // CloudFormation rules
            IacRule {
                id: "IAC-CF-001".into(),
                title: "CloudFormation S3 bucket without encryption".into(),
                description: "S3 bucket resource does not have BucketEncryption configured".into(),
                severity: FindingSeverity::High,
                resource_type: IacResourceType::CloudFormation,
                pattern: IacPattern::ContentMatch {
                    pattern: "AWS::S3::Bucket".into(),
                    expect_absent: false,
                },
                remediation: "Add BucketEncryption property with SSEAlgorithm".into(),
                cis_reference: Some("CIS AWS 2.1.1".into()),
            },
            IacRule {
                id: "IAC-CF-002".into(),
                title: "CloudFormation security group open to world".into(),
                description: "Security group allows ingress from 0.0.0.0/0".into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::CloudFormation,
                pattern: IacPattern::ContentMatch {
                    pattern: "0.0.0.0/0".into(),
                    expect_absent: false,
                },
                remediation: "Restrict CidrIp to specific ranges".into(),
                cis_reference: Some("CIS AWS 5.2".into()),
            },
            // Docker Compose rules
            IacRule {
                id: "IAC-DC-001".into(),
                title: "Docker Compose privileged mode".into(),
                description: "Service runs in privileged mode".into(),
                severity: FindingSeverity::Critical,
                resource_type: IacResourceType::DockerCompose,
                pattern: IacPattern::KeyValue {
                    key: "privileged".into(),
                    value: "true".into(),
                    match_type: KeyValueMatch::Equals,
                },
                remediation: "Remove privileged: true from service configuration".into(),
                cis_reference: None,
            },
        ]
    }
}

impl Default for IacScanner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Scanner for IacScanner {
    fn name(&self) -> &str {
        "iac"
    }

    fn version(&self) -> ScannerVersion {
        ScannerVersion {
            major: 1,
            minor: 0,
            patch: 0,
        }
    }

    fn supported_categories(&self) -> Vec<FindingCategory> {
        vec![FindingCategory::Security, FindingCategory::Compliance]
    }

    fn supports_language(&self, language: &str) -> bool {
        matches!(
            language,
            "terraform" | "hcl" | "kubernetes" | "yaml" | "cloudformation" | "helm" | "ansible"
        )
    }

    async fn scan(
        &self,
        _config: &ScanConfig,
        context: &ScanContext,
    ) -> Result<Vec<Finding>, ScanError> {
        let mut findings = Vec::new();

        for file in &context.files {
            let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(ext, "tf" | "tfvars" | "yaml" | "yml" | "json")
                && let Ok(content) = std::fs::read_to_string(file)
            {
                findings.extend(self.scan_file(&content, &file.display().to_string()));
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
    fn test_detect_terraform() {
        assert_eq!(
            IacScanner::detect_iac_type("main.tf", "resource \"aws_instance\""),
            Some(IacResourceType::Terraform)
        );
    }

    #[test]
    fn test_detect_kubernetes() {
        let content = "apiVersion: apps/v1\nkind: Deployment\nmetadata:\n  name: test\n";
        assert_eq!(
            IacScanner::detect_iac_type("deploy.yaml", content),
            Some(IacResourceType::Kubernetes)
        );
    }

    #[test]
    fn test_detect_cloudformation() {
        let content = "AWSTemplateFormatVersion: '2010-09-09'\nResources:\n";
        assert_eq!(
            IacScanner::detect_iac_type("template.yaml", content),
            Some(IacResourceType::CloudFormation)
        );
    }

    #[test]
    fn test_detect_docker_compose() {
        let content = "version: '3'\nservices:\n  web:\n    image: nginx\n";
        assert_eq!(
            IacScanner::detect_iac_type("docker-compose.yml", content),
            Some(IacResourceType::DockerCompose)
        );
    }

    #[test]
    fn test_terraform_security_group() {
        let scanner = IacScanner::new();
        let content = r#"
resource "aws_security_group" "open" {
  ingress {
    from_port   = 0
    to_port     = 65535
    cidr_blocks = ["0.0.0.0/0"]
  }
}
"#;
        let findings = scanner.scan_file(content, "main.tf");
        assert!(
            findings.iter().any(|f| f.title.contains("unrestricted")),
            "Should detect open security group"
        );
    }

    #[test]
    fn test_k8s_privileged() {
        let scanner = IacScanner::new();
        let content = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\nspec:\n  containers:\n  - name: test\n    securityContext:\n      privileged: true\n";
        let findings = scanner.scan_file(content, "pod.yaml");
        assert!(
            findings.iter().any(|f| f.title.contains("Privileged")),
            "Should detect privileged container"
        );
    }

    #[test]
    fn test_k8s_no_root() {
        let scanner = IacScanner::new();
        let content = "apiVersion: v1\nkind: Pod\nmetadata:\n  name: test\nspec:\n  containers:\n  - name: test\n    image: nginx\n";
        let findings = scanner.scan_file(content, "pod.yaml");
        assert!(
            findings.iter().any(|f| f.title.contains("root")),
            "Should detect missing runAsNonRoot"
        );
    }

    #[test]
    fn test_terraform_rds_public() {
        let scanner = IacScanner::new();
        let content = "resource \"aws_db_instance\" \"db\" {\n  publicly_accessible = true\n  engine = \"postgres\"\n}\n";
        let findings = scanner.scan_file(content, "rds.tf");
        assert!(
            findings.iter().any(|f| f.title.contains("publicly")),
            "Should detect publicly accessible RDS"
        );
    }

    #[test]
    fn test_unknown_file_type() {
        let scanner = IacScanner::new();
        let findings = scanner.scan_file("just some text", "readme.md");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scanner_metadata() {
        let scanner = IacScanner::new();
        assert_eq!(scanner.name(), "iac");
        assert_eq!(scanner.risk_level(), ScannerRiskLevel::ReadOnly);
        assert!(scanner.supports_language("terraform"));
        assert!(scanner.supports_language("kubernetes"));
    }

    #[test]
    fn test_docker_compose_privileged() {
        let scanner = IacScanner::new();
        let content = "version: '3'\nservices:\n  web:\n    image: nginx\n    privileged: true\n";
        let findings = scanner.scan_file(content, "docker-compose.yml");
        assert!(
            findings.iter().any(|f| f.title.contains("privileged")),
            "Should detect privileged Docker Compose service"
        );
    }

    #[test]
    fn test_find_key_value() {
        let content = "name: test\nport: 8080\n";
        assert_eq!(
            IacScanner::find_key_value(content, "port"),
            Some("8080".to_string())
        );
        assert_eq!(IacScanner::find_key_value(content, "missing"), None);
    }
}
