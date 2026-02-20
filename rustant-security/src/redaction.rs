//! SecretRedactor — CRITICAL security component.
//!
//! Detects and redacts secrets from text before it reaches:
//! 1. LLM context (brain.rs)
//! 2. Long-term memory (Facts)
//! 3. Audit trail (TraceEvent payloads)
//! 4. Log output
//! 5. MCP output
//!
//! Original secret values are NEVER stored.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Result of a redaction operation.
#[derive(Debug, Clone)]
pub struct RedactionResult {
    /// The text with secrets replaced by `[REDACTED:<type>]`.
    pub redacted: String,
    /// Number of secrets found and redacted.
    pub count: usize,
    /// Types of secrets that were redacted.
    pub secret_types: Vec<String>,
}

/// A compiled secret detection pattern.
struct CompiledSecretPattern {
    /// Human-readable name for this pattern type.
    name: String,
    /// Compiled regex.
    regex: Regex,
}

/// Detects and redacts secrets from text and JSON values.
pub struct SecretRedactor {
    patterns: Vec<CompiledSecretPattern>,
    /// Shannon entropy threshold for detecting high-entropy strings.
    entropy_threshold: f64,
    /// Total count of redactions performed (for metrics).
    redaction_count: AtomicUsize,
}

impl SecretRedactor {
    /// Create a new redactor with all built-in patterns.
    pub fn new() -> Self {
        Self::with_entropy_threshold(4.5)
    }

    /// Create a new redactor with a custom entropy threshold.
    pub fn with_entropy_threshold(threshold: f64) -> Self {
        Self {
            patterns: build_patterns(),
            entropy_threshold: threshold,
            redaction_count: AtomicUsize::new(0),
        }
    }

    /// Redact all detected secrets from text.
    pub fn redact(&self, text: &str) -> RedactionResult {
        let mut redacted = text.to_string();
        let mut count = 0;
        let mut secret_types = Vec::new();

        for pattern in &self.patterns {
            let matches: Vec<_> = pattern.regex.find_iter(&redacted).collect();
            if !matches.is_empty() {
                count += matches.len();
                if !secret_types.contains(&pattern.name) {
                    secret_types.push(pattern.name.clone());
                }
                redacted = pattern
                    .regex
                    .replace_all(&redacted, format!("[REDACTED:{}]", pattern.name))
                    .into_owned();
            }
        }

        // High-entropy string detection for remaining unmatched secrets
        redacted = self.redact_high_entropy(&redacted, &mut count, &mut secret_types);

        self.redaction_count.fetch_add(count, Ordering::Relaxed);

        RedactionResult {
            redacted,
            count,
            secret_types,
        }
    }

    /// Recursively redact secrets from a JSON value tree.
    pub fn redact_json(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => {
                let result = self.redact(s);
                if result.count > 0 {
                    *s = result.redacted;
                }
            }
            serde_json::Value::Object(map) => {
                // Check for sensitive key names
                let sensitive_keys: Vec<String> = map
                    .keys()
                    .filter(|k| is_sensitive_key(k))
                    .cloned()
                    .collect();

                for key in sensitive_keys {
                    if let Some(serde_json::Value::String(s)) = map.get_mut(&key)
                        && !s.is_empty()
                        && !s.starts_with("[REDACTED")
                    {
                        *s = format!("[REDACTED:{key}]");
                        self.redaction_count.fetch_add(1, Ordering::Relaxed);
                    }
                }

                for val in map.values_mut() {
                    self.redact_json(val);
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr {
                    self.redact_json(val);
                }
            }
            _ => {}
        }
    }

    /// Get the total number of redactions performed.
    pub fn total_redactions(&self) -> usize {
        self.redaction_count.load(Ordering::Relaxed)
    }

    /// Detect and redact high-entropy strings that may be secrets.
    fn redact_high_entropy(
        &self,
        text: &str,
        count: &mut usize,
        secret_types: &mut Vec<String>,
    ) -> String {
        // Match quoted strings or unbroken tokens of 20+ hex/base64 chars
        let high_entropy_re = Regex::new(r#"[A-Za-z0-9+/=_\-]{20,}"#).unwrap();

        let mut result = text.to_string();
        let matches: Vec<_> = high_entropy_re.find_iter(text).collect();

        // Process in reverse to preserve offsets
        for m in matches.into_iter().rev() {
            let candidate = m.as_str();
            // Skip already-redacted text
            if candidate.starts_with("REDACTED") {
                continue;
            }
            let entropy = shannon_entropy(candidate);
            if entropy > self.entropy_threshold && candidate.len() >= 20 {
                let replacement = "[REDACTED:high_entropy]";
                result.replace_range(m.range(), replacement);
                *count += 1;
                let label = "high_entropy".to_string();
                if !secret_types.contains(&label) {
                    secret_types.push(label);
                }
            }
        }

        result
    }
}

impl Default for SecretRedactor {
    fn default() -> Self {
        Self::new()
    }
}

/// Metadata about a redacted secret for auditing (never contains the actual value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactedSecretMeta {
    /// Type of secret detected.
    pub secret_type: String,
    /// File where it was found.
    pub file_path: Option<String>,
    /// Line range (start, end).
    pub line_range: Option<(usize, usize)>,
}

/// Calculate Shannon entropy of a string.
pub fn shannon_entropy(s: &str) -> f64 {
    if s.is_empty() {
        return 0.0;
    }
    let mut freq = [0u32; 256];
    for &b in s.as_bytes() {
        freq[b as usize] += 1;
    }
    let len = s.len() as f64;
    freq.iter()
        .filter(|&&c| c > 0)
        .map(|&c| {
            let p = c as f64 / len;
            -p * p.log2()
        })
        .sum()
}

/// Check if a JSON key name suggests it contains a secret.
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    matches!(
        lower.as_str(),
        "password"
            | "passwd"
            | "secret"
            | "api_key"
            | "apikey"
            | "api-key"
            | "token"
            | "access_token"
            | "refresh_token"
            | "auth_token"
            | "private_key"
            | "secret_key"
            | "client_secret"
            | "connection_string"
            | "credentials"
    )
}

/// Build all built-in secret detection patterns (102 patterns).
fn build_patterns() -> Vec<CompiledSecretPattern> {
    let pattern_defs: Vec<(&str, &str)> = vec![
        // === Cloud Provider Keys ===
        ("aws_access_key", r"(?i)AKIA[0-9A-Z]{16}"),
        (
            "aws_secret_key",
            r#"(?i)(?:aws_secret_access_key|aws_secret)\s*[=:]\s*['"]?([A-Za-z0-9/+=]{40})['"]?"#,
        ),
        (
            "aws_session_token",
            r#"(?i)(?:aws_session_token)\s*[=:]\s*['"]?([A-Za-z0-9/+=]{100,})['"]?"#,
        ),
        (
            "gcp_service_account",
            r#"(?i)"type"\s*:\s*"service_account""#,
        ),
        ("gcp_api_key", r"AIza[0-9A-Za-z\-_]{35}"),
        (
            "azure_storage_key",
            r"(?i)(?:DefaultEndpointsProtocol|AccountKey)\s*=\s*[A-Za-z0-9+/=]{44,}",
        ),
        // === Version Control ===
        ("github_token", r"gh[pousr]_[A-Za-z0-9_]{36,}"),
        ("github_classic_token", r"ghp_[A-Za-z0-9]{36}"),
        ("github_oauth", r"gho_[A-Za-z0-9]{36}"),
        ("github_app_token", r"(?:ghu|ghs)_[A-Za-z0-9]{36}"),
        ("gitlab_token", r"glpat-[A-Za-z0-9\-_]{20,}"),
        (
            "bitbucket_token",
            r#"(?i)(?:bitbucket).{0,20}['"][A-Za-z0-9]{32,}['"]"#,
        ),
        // === Payment Providers ===
        ("stripe_secret_key", r"sk_live_[a-zA-Z0-9]{24,}"),
        ("stripe_publishable", r"pk_live_[a-zA-Z0-9]{24,}"),
        ("stripe_restricted", r"rk_live_[a-zA-Z0-9]{24,}"),
        ("square_access_token", r"sq0atp-[A-Za-z0-9\-_]{22,}"),
        ("square_oauth", r"sq0csp-[A-Za-z0-9\-_]{43,}"),
        // === Communication Services ===
        (
            "slack_token",
            r"xox[bpors]-[0-9]{10,}-[0-9]{10,}-[a-zA-Z0-9]{24,}",
        ),
        (
            "slack_webhook",
            r"https://hooks\.slack\.com/services/T[A-Z0-9]{8,}/B[A-Z0-9]{8,}/[A-Za-z0-9]{24,}",
        ),
        (
            "discord_token",
            r"(?:mfa\.)?[A-Za-z0-9_-]{24,}\.[A-Za-z0-9_-]{6}\.[A-Za-z0-9_-]{27,}",
        ),
        (
            "discord_webhook",
            r"https://discord(?:app)?\.com/api/webhooks/\d+/[A-Za-z0-9_-]+",
        ),
        ("twilio_api_key", r"SK[0-9a-fA-F]{32}"),
        (
            "sendgrid_api_key",
            r"SG\.[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43}",
        ),
        ("mailgun_api_key", r"key-[0-9a-zA-Z]{32}"),
        // === Database & Storage ===
        ("mongodb_uri", r"mongodb(?:\+srv)?://[^\s]+@[^\s]+"),
        ("postgres_uri", r"postgres(?:ql)?://[^\s]+:[^\s]+@[^\s]+"),
        ("mysql_uri", r"mysql://[^\s]+:[^\s]+@[^\s]+"),
        ("redis_uri", r"redis://[^\s]*:[^\s]+@[^\s]+"),
        // === Auth Tokens ===
        (
            "jwt_token",
            r"eyJ[A-Za-z0-9\-_]+\.eyJ[A-Za-z0-9\-_]+\.[A-Za-z0-9\-_.+/=]+",
        ),
        ("bearer_token", r"(?i)bearer\s+[A-Za-z0-9\-_.~+/]+=*"),
        // === Cryptographic Keys ===
        ("rsa_private_key", r"-----BEGIN RSA PRIVATE KEY-----"),
        ("ec_private_key", r"-----BEGIN EC PRIVATE KEY-----"),
        (
            "openssh_private_key",
            r"-----BEGIN OPENSSH PRIVATE KEY-----",
        ),
        ("pgp_private_key", r"-----BEGIN PGP PRIVATE KEY BLOCK-----"),
        ("generic_private_key", r"-----BEGIN PRIVATE KEY-----"),
        ("certificate", r"-----BEGIN CERTIFICATE-----"),
        // === Infrastructure ===
        (
            "heroku_api_key",
            r"(?i)(?:heroku).{0,20}[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}",
        ),
        ("digitalocean_token", r"dop_v1_[a-f0-9]{64}"),
        ("npm_token", r"(?i)//registry\.npmjs\.org/:_authToken=.+"),
        ("pypi_token", r"pypi-[A-Za-z0-9_-]{100,}"),
        ("nuget_api_key", r"oy2[A-Za-z0-9]{43}"),
        ("rubygems_api_key", r"rubygems_[a-f0-9]{48}"),
        // === SaaS Services ===
        ("datadog_api_key", r"(?i)(?:datadog|dd).{0,20}[0-9a-f]{32}"),
        ("new_relic_key", r"NRAK-[A-Z0-9]{27}"),
        (
            "sentry_dsn",
            r"https://[a-f0-9]{32}@[a-z0-9]+\.ingest\.sentry\.io/\d+",
        ),
        ("algolia_api_key", r"(?i)(?:algolia).{0,20}[a-zA-Z0-9]{32}"),
        ("firebase_key", r"(?i)(?:firebase).{0,20}[A-Za-z0-9_-]{39}"),
        ("shopify_token", r"shpat_[a-fA-F0-9]{32}"),
        ("shopify_secret", r"shpss_[a-fA-F0-9]{32}"),
        // === Auth Providers ===
        ("okta_token", r"(?i)(?:okta).{0,20}[0-9a-zA-Z]{42}"),
        ("auth0_token", r"(?i)(?:auth0).{0,20}[A-Za-z0-9_-]{32,}"),
        // === Generic Patterns ===
        (
            "generic_api_key",
            r#"(?i)(?:api[_-]?key|apikey)\s*[=:]\s*['"]?([A-Za-z0-9_\-]{16,})['"]?"#,
        ),
        (
            "generic_secret",
            r#"(?i)(?:secret|private[_-]?key)\s*[=:]\s*['"]?([A-Za-z0-9_\-/+=]{16,})['"]?"#,
        ),
        (
            "generic_password",
            r#"(?i)(?:password|passwd|pwd)\s*[=:]\s*['"]?([^\s'"]{8,})['"]?"#,
        ),
        (
            "generic_token",
            r#"(?i)(?:token|auth_token|access_token)\s*[=:]\s*['"]?([A-Za-z0-9_\-/+=.]{16,})['"]?"#,
        ),
        (
            "generic_connection_string",
            r#"(?i)(?:connection_string|conn_str)\s*[=:]\s*['"]?([^\s'"]{16,})['"]?"#,
        ),
        // === CI/CD ===
        ("travis_token", r"(?i)(?:travis).{0,20}[A-Za-z0-9]{22}"),
        ("circle_ci_token", r"(?i)(?:circle).{0,20}[a-f0-9]{40}"),
        ("jenkins_token", r"(?i)(?:jenkins).{0,20}[A-Za-z0-9]{32,}"),
        // === Encryption ===
        (
            "encryption_key_hex",
            r#"(?i)(?:encryption[_-]?key|aes[_-]?key|enc[_-]?key)\s*[=:]\s*['"]?([0-9a-fA-F]{32,})['"]?"#,
        ),
        // === Cloud Providers (Extended) ===
        (
            "aws_mws_key",
            r"amzn\.mws\.[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
        ),
        (
            "aws_cognito_pool",
            r"(?i)(?:us|eu|ap|sa|ca|me|af)-[a-z]+-\d+_[A-Za-z0-9]{9}",
        ),
        (
            "azure_client_secret",
            r#"(?i)(?:azure[_-]?client[_-]?secret|AZURE_CLIENT_SECRET)\s*[=:]\s*['"]?([A-Za-z0-9~._-]{34,})['"]?"#,
        ),
        (
            "azure_sas_token",
            r"(?i)(?:sv=\d{4}-\d{2}-\d{2}&s[a-z]=[a-z&=]+&sig=[A-Za-z0-9%+/=]+)",
        ),
        ("gcp_oauth_token", r"ya29\.[A-Za-z0-9_-]{50,}"),
        ("ibm_cloud_key", r"(?i)(?:ibm).{0,20}[a-zA-Z0-9_-]{44}"),
        ("alibaba_access_key", r"LTAI[A-Za-z0-9]{20}"),
        (
            "cloudflare_api_token",
            r"(?i)(?:cloudflare).{0,20}[A-Za-z0-9_-]{40}",
        ),
        // === Payment & Financial (Extended) ===
        (
            "paypal_braintree",
            r"access_token\$production\$[a-z0-9]{16}\$[a-f0-9]{32}",
        ),
        ("stripe_test_key", r"sk_test_[a-zA-Z0-9]{24,}"),
        ("plaid_client_id", r"(?i)(?:plaid).{0,20}[a-f0-9]{24}"),
        ("coinbase_key", r"(?i)(?:coinbase).{0,20}[A-Za-z0-9]{16,}"),
        // === Communication (Extended) ===
        ("telegram_bot_token", r"\d{8,10}:[A-Za-z0-9_-]{35}"),
        (
            "slack_signing_secret",
            r"(?i)(?:slack[_-]?signing[_-]?secret)\s*[=:]\s*[a-f0-9]{32}",
        ),
        (
            "intercom_token",
            r"(?i)(?:intercom).{0,20}[A-Za-z0-9_-]{40,}",
        ),
        ("zendesk_token", r"(?i)(?:zendesk).{0,20}[A-Za-z0-9]{40}"),
        // === Version Control (Extended) ===
        ("gitlab_runner_token", r"glrt-[A-Za-z0-9\-_]{20,}"),
        ("gitlab_pipeline_token", r"glptt-[A-Za-z0-9\-_]{20,}"),
        (
            "azure_devops_pat",
            r"(?i)(?:azure[_-]?devops|vsts).{0,20}[a-z2-7]{52}",
        ),
        // === Database (Extended) ===
        ("sqlite_uri", r"sqlite://[^\s]+"),
        (
            "mssql_uri",
            r"(?i)(?:Server|Data Source)\s*=\s*[^;]+;\s*(?:User ID|uid)\s*=\s*[^;]+;\s*(?:Password|pwd)\s*=\s*[^;]+",
        ),
        (
            "cassandra_uri",
            r"(?i)(?:cassandra|cql)://[^\s]+:[^\s]+@[^\s]+",
        ),
        // === DevOps & Infrastructure (Extended) ===
        (
            "docker_config_auth",
            r#""auth"\s*:\s*"[A-Za-z0-9+/=]{20,}""#,
        ),
        (
            "kubernetes_service_token",
            r"(?i)(?:kubernetes|k8s).{0,20}[A-Za-z0-9_-]{20,}",
        ),
        ("vault_token", r"(?:hvs|s)\.[A-Za-z0-9]{24,}"),
        (
            "consul_token",
            r"(?i)(?:consul).{0,20}[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
        ),
        (
            "terraform_cloud_token",
            r"(?i)(?:TFE_TOKEN|ATLAS_TOKEN|TF_TOKEN)\s*[=:]\s*[A-Za-z0-9.]{14,}",
        ),
        (
            "github_actions_secret",
            r"(?i)(?:GITHUB_TOKEN|GH_TOKEN)\s*[=:]\s*[A-Za-z0-9_-]{36,}",
        ),
        // === SaaS & Analytics (Extended) ===
        ("mixpanel_token", r"(?i)(?:mixpanel).{0,20}[a-f0-9]{32}"),
        (
            "segment_write_key",
            r"(?i)(?:segment).{0,20}[A-Za-z0-9]{32}",
        ),
        ("amplitude_key", r"(?i)(?:amplitude).{0,20}[a-f0-9]{32}"),
        (
            "launchdarkly_key",
            r"(?i)(?:launchdarkly|ld).{0,20}[a-z0-9-]{24,}",
        ),
        (
            "pagerduty_key",
            r"(?i)(?:pagerduty).{0,20}[A-Za-z0-9+/]{20,}",
        ),
        (
            "supabase_key",
            r"(?i)(?:supabase).{0,20}eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
        ),
        ("vercel_token", r"(?i)(?:vercel).{0,20}[A-Za-z0-9]{24}"),
        ("netlify_token", r"(?i)(?:netlify).{0,20}[A-Za-z0-9_-]{40,}"),
        // === Email & Messaging ===
        (
            "postmark_api_key",
            r"(?i)(?:postmark).{0,20}[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}",
        ),
        ("mailchimp_api_key", r"[a-f0-9]{32}-us\d{1,2}"),
        ("sparkpost_api_key", r"(?i)(?:sparkpost).{0,20}[a-f0-9]{40}"),
        // === Cryptographic Material ===
        ("dsa_private_key", r"-----BEGIN DSA PRIVATE KEY-----"),
        (
            "pkcs8_encrypted_key",
            r"-----BEGIN ENCRYPTED PRIVATE KEY-----",
        ),
        ("putty_ppk_key", r"PuTTY-User-Key-File-\d+:"),
    ];

    pattern_defs
        .into_iter()
        .filter_map(|(name, pattern)| match Regex::new(pattern) {
            Ok(regex) => Some(CompiledSecretPattern {
                name: name.to_string(),
                regex,
            }),
            Err(e) => {
                tracing::warn!("Failed to compile secret pattern '{}': {}", name, e);
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aws_key_redaction() {
        let redactor = SecretRedactor::new();
        let input = "My AWS key is AKIAIOSFODNN7EXAMPLE and more text";
        let result = redactor.redact(input);
        assert!(result.count >= 1);
        assert!(result.redacted.contains("[REDACTED:aws_access_key]"));
        assert!(!result.redacted.contains("AKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn test_github_token_redaction() {
        let redactor = SecretRedactor::new();
        let input = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefgh";
        let result = redactor.redact(input);
        assert!(result.count >= 1);
        assert!(!result.redacted.contains("ghp_"));
    }

    #[test]
    fn test_stripe_key_redaction() {
        let redactor = SecretRedactor::new();
        let prefix = "sk_live";
        let input = format!("STRIPE_KEY={prefix}_abcdefghijklmnopqrstuvwx");
        let result = redactor.redact(&input);
        assert!(result.count >= 1);
        assert!(!result.redacted.contains("sk_live_"));
    }

    #[test]
    fn test_jwt_redaction() {
        let redactor = SecretRedactor::new();
        let input = "Authorization: eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
        let result = redactor.redact(input);
        assert!(result.count >= 1);
        assert!(!result.redacted.contains("eyJhbGci"));
    }

    #[test]
    fn test_private_key_redaction() {
        let redactor = SecretRedactor::new();
        let input = "Found: -----BEGIN RSA PRIVATE KEY----- data here";
        let result = redactor.redact(input);
        assert!(result.count >= 1);
    }

    #[test]
    fn test_json_redaction() {
        let redactor = SecretRedactor::new();
        let mut json = serde_json::json!({
            "username": "admin",
            "password": "supersecret123",
            "api_key": "my-secret-api-key-value",
            "nested": {
                "token": "bearer-token-value-here"
            }
        });
        redactor.redact_json(&mut json);

        let password = json["password"].as_str().unwrap();
        assert!(password.starts_with("[REDACTED"));

        let api_key = json["api_key"].as_str().unwrap();
        assert!(api_key.starts_with("[REDACTED"));
    }

    #[test]
    fn test_shannon_entropy() {
        // Low entropy (repeated chars)
        assert!(shannon_entropy("aaaaaaaaaa") < 1.0);
        // High entropy (random-looking)
        assert!(shannon_entropy("aB3$cD5&eF7") > 3.0);
    }

    #[test]
    fn test_no_false_positives_on_normal_text() {
        let redactor = SecretRedactor::new();
        let input = "This is a normal English sentence about programming.";
        let result = redactor.redact(input);
        assert_eq!(result.count, 0);
        assert_eq!(result.redacted, input);
    }

    #[test]
    fn test_redaction_idempotency() {
        let redactor = SecretRedactor::new();
        let input = "key: AKIAIOSFODNN7EXAMPLE";
        let first = redactor.redact(input);
        let second = redactor.redact(&first.redacted);
        assert_eq!(first.redacted, second.redacted);
    }

    #[test]
    fn test_database_uri_redaction() {
        let redactor = SecretRedactor::new();
        let input = "DATABASE_URL=postgres://user:password@localhost:5432/mydb";
        let result = redactor.redact(input);
        assert!(result.count >= 1);
        assert!(!result.redacted.contains("password@"));
    }

    #[test]
    fn test_total_redactions_metric() {
        let redactor = SecretRedactor::new();
        assert_eq!(redactor.total_redactions(), 0);
        redactor.redact("key: AKIAIOSFODNN7EXAMPLE");
        assert!(redactor.total_redactions() > 0);
    }
}
