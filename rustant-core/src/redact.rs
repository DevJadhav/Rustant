//! Output redaction trait — prevents secrets from leaking to memory, audit, and LLM context.
//!
//! This module defines the `OutputRedactor` trait that can be implemented by
//! external crates (e.g., `rustant-security::SecretRedactor`) to provide
//! comprehensive secret redaction. A basic built-in implementation covers
//! the most critical patterns (AWS keys, GitHub tokens, private keys).

use std::sync::Arc;

/// Trait for redacting secrets from text before storage.
///
/// Implementors should replace detected secrets with `[REDACTED:<type>]` markers.
/// The original secret values must NEVER be stored anywhere.
pub trait OutputRedactor: Send + Sync {
    /// Redact all detected secrets from the given text.
    /// Returns the redacted text.
    fn redact(&self, text: &str) -> String;
}

/// A no-op redactor that passes text through unchanged.
/// Used when no redactor is configured.
pub struct NoOpRedactor;

impl OutputRedactor for NoOpRedactor {
    fn redact(&self, text: &str) -> String {
        text.to_string()
    }
}

/// Basic built-in redactor covering the most critical secret patterns.
///
/// For comprehensive redaction (60+ patterns + entropy), use the
/// `SecretRedactor` from `rustant-security` crate instead.
pub struct BasicRedactor {
    patterns: Vec<(regex::Regex, &'static str)>,
}

impl BasicRedactor {
    pub fn new() -> Self {
        let patterns = vec![
            // AWS Access Key
            (
                regex::Regex::new(r"AKIA[0-9A-Z]{16}").unwrap(),
                "AWS_ACCESS_KEY",
            ),
            // GitHub tokens
            (
                regex::Regex::new(r"gh[pousr]_[A-Za-z0-9_]{36,}").unwrap(),
                "GITHUB_TOKEN",
            ),
            // Generic API key patterns
            (
                regex::Regex::new(r#"(?i)(api[_-]?key|apikey|api[_-]?secret)\s*[:=]\s*["']?[A-Za-z0-9\-_]{20,}["']?"#).unwrap(),
                "API_KEY",
            ),
            // Private keys
            (
                regex::Regex::new(r"-----BEGIN\s+(RSA|EC|OPENSSH|DSA|PGP)\s+PRIVATE\s+KEY-----").unwrap(),
                "PRIVATE_KEY",
            ),
            // Stripe secret keys
            (
                regex::Regex::new(r"sk_live_[a-zA-Z0-9]{24,}").unwrap(),
                "STRIPE_SECRET",
            ),
            // JWT tokens
            (
                regex::Regex::new(r"eyJ[A-Za-z0-9\-_]{10,}\.eyJ[A-Za-z0-9\-_]{10,}\.[A-Za-z0-9\-_.]{10,}").unwrap(),
                "JWT_TOKEN",
            ),
            // Slack tokens
            (
                regex::Regex::new(r"xox[bpors]-[0-9A-Za-z\-]{10,}").unwrap(),
                "SLACK_TOKEN",
            ),
            // Generic bearer tokens
            (
                regex::Regex::new(r#"(?i)(bearer|token)\s+[A-Za-z0-9\-_\.]{20,}"#).unwrap(),
                "BEARER_TOKEN",
            ),
            // Password in connection strings
            (
                regex::Regex::new(r#"(?i)(password|passwd|pwd)\s*[:=]\s*["']?[^\s"']{8,}["']?"#).unwrap(),
                "PASSWORD",
            ),
        ];
        Self { patterns }
    }
}

impl Default for BasicRedactor {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputRedactor for BasicRedactor {
    fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (pattern, name) in &self.patterns {
            result = pattern
                .replace_all(&result, format!("[REDACTED:{name}]"))
                .into_owned();
        }
        result
    }
}

/// Enhanced redactor with 30+ patterns covering the most critical secret categories.
///
/// Extends `BasicRedactor`'s 9 patterns with cloud providers, CI/CD tokens,
/// communication services, database connection strings, and private keys.
/// For comprehensive 102-pattern + entropy detection, inject `SecretRedactor`
/// from `rustant-security` crate via the `SharedRedactor` type.
pub struct EnhancedRedactor {
    patterns: Vec<(regex::Regex, &'static str)>,
}

impl EnhancedRedactor {
    pub fn new() -> Self {
        let mut patterns = BasicRedactor::new().patterns;
        // Cloud providers (beyond BasicRedactor's AWS access key)
        let extra: Vec<(&str, &str)> = vec![
            // AWS secret key
            (r#"(?i)(?:aws_secret_access_key|aws_secret)\s*[=:]\s*['"]?[A-Za-z0-9/+=]{40}['"]?"#, "AWS_SECRET_KEY"),
            // GCP service account
            (r#"(?i)"type"\s*:\s*"service_account""#, "GCP_SERVICE_ACCOUNT"),
            // GCP API key
            (r"AIza[0-9A-Za-z\-_]{35}", "GCP_API_KEY"),
            // Azure storage key
            (r"(?i)(?:DefaultEndpointsProtocol|AccountKey)\s*=\s*[A-Za-z0-9+/=]{44,}", "AZURE_STORAGE_KEY"),
            // GitLab token
            (r"glpat-[A-Za-z0-9\-_]{20,}", "GITLAB_TOKEN"),
            // Stripe publishable
            (r"pk_live_[a-zA-Z0-9]{24,}", "STRIPE_PUBLISHABLE"),
            // Stripe restricted
            (r"rk_live_[a-zA-Z0-9]{24,}", "STRIPE_RESTRICTED"),
            // Slack webhook
            (r"https://hooks\.slack\.com/services/T[A-Z0-9]{8,}/B[A-Z0-9]{8,}/[A-Za-z0-9]{24,}", "SLACK_WEBHOOK"),
            // Discord token
            (r"[MN][A-Za-z\d]{23,}\.[\w-]{6}\.[\w-]{27}", "DISCORD_TOKEN"),
            // Twilio
            (r"SK[0-9a-fA-F]{32}", "TWILIO_API_KEY"),
            // SendGrid
            (r"SG\.[A-Za-z0-9\-_]{22,}\.[A-Za-z0-9\-_]{43,}", "SENDGRID_API_KEY"),
            // NPM token
            (r"npm_[A-Za-z0-9]{36}", "NPM_TOKEN"),
            // PyPI token
            (r"pypi-[A-Za-z0-9\-_]{100,}", "PYPI_TOKEN"),
            // Heroku
            (r"[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}", "HEROKU_API_KEY"),
            // Database connection strings (postgres/mysql/mongodb)
            (r"(?i)(?:postgres|mysql|mongodb)://[^\s]{10,}", "DATABASE_URL"),
            // SSH private key
            (r"-----BEGIN OPENSSH PRIVATE KEY-----", "SSH_PRIVATE_KEY"),
            // PGP private key
            (r"-----BEGIN PGP PRIVATE KEY BLOCK-----", "PGP_PRIVATE_KEY"),
            // Generic secret in env-style files
            (r#"(?i)(?:secret|credential|auth)[_-]?(?:key|token|password)\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#, "GENERIC_SECRET"),
            // Anthropic API key
            (r"sk-ant-[A-Za-z0-9\-_]{40,}", "ANTHROPIC_API_KEY"),
            // OpenAI API key
            (r"sk-[A-Za-z0-9]{20}T3BlbkFJ[A-Za-z0-9]{20}", "OPENAI_API_KEY"),
            // Datadog
            (r#"(?i)(?:dd|datadog)[_-]?(?:api[_-]?key|app[_-]?key)\s*[:=]\s*['"]?[a-f0-9]{32,}['"]?"#, "DATADOG_KEY"),
        ];
        for (pattern_str, name) in extra {
            if let Ok(re) = regex::Regex::new(pattern_str) {
                patterns.push((re, name));
            }
        }
        Self { patterns }
    }
}

impl Default for EnhancedRedactor {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputRedactor for EnhancedRedactor {
    fn redact(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (pattern, name) in &self.patterns {
            result = pattern
                .replace_all(&result, format!("[REDACTED:{name}]"))
                .into_owned();
        }
        result
    }
}

/// Shared redactor type used throughout the agent.
pub type SharedRedactor = Arc<dyn OutputRedactor>;

/// Create a basic redactor wrapped in Arc for shared use.
pub fn create_basic_redactor() -> SharedRedactor {
    Arc::new(BasicRedactor::new())
}

/// Create an enhanced redactor (30+ patterns) wrapped in Arc for shared use.
///
/// Recommended over `create_basic_redactor()` for production deployments.
/// For full 102-pattern + entropy detection, inject `SecretRedactor` from
/// `rustant-security` crate instead.
pub fn create_enhanced_redactor() -> SharedRedactor {
    Arc::new(EnhancedRedactor::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_redactor() {
        let redactor = NoOpRedactor;
        let text = "AKIAIOSFODNN7EXAMPLE";
        assert_eq!(redactor.redact(text), text);
    }

    #[test]
    fn test_basic_redactor_aws_key() {
        let redactor = BasicRedactor::new();
        let text = "My key is AKIAIOSFODNN7EXAMPLE ok";
        let result = redactor.redact(text);
        assert!(!result.contains("AKIAIOSFODNN7EXAMPLE"));
        assert!(result.contains("[REDACTED:AWS_ACCESS_KEY]"));
    }

    #[test]
    fn test_basic_redactor_github_token() {
        let redactor = BasicRedactor::new();
        let text = "token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:GITHUB_TOKEN]"));
    }

    #[test]
    fn test_basic_redactor_private_key() {
        let redactor = BasicRedactor::new();
        let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIB";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:PRIVATE_KEY]"));
    }

    #[test]
    fn test_basic_redactor_password() {
        let redactor = BasicRedactor::new();
        let text = "password=mysecretpassword123";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:PASSWORD]"));
    }

    #[test]
    fn test_basic_redactor_no_secrets() {
        let redactor = BasicRedactor::new();
        let text = "This is just normal text with no secrets.";
        assert_eq!(redactor.redact(text), text);
    }

    #[test]
    fn test_basic_redactor_stripe() {
        let redactor = BasicRedactor::new();
        let prefix = "sk_live";
        let text = format!("stripe key: {prefix}_abcdefghijklmnopqrstuvwx");
        let result = redactor.redact(&text);
        assert!(result.contains("[REDACTED:STRIPE_SECRET]"));
    }

    #[test]
    fn test_enhanced_redactor_gitlab_token() {
        let redactor = EnhancedRedactor::new();
        let text = "token: glpat-abcdefghijklmnopqrstu";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:GITLAB_TOKEN]"));
    }

    #[test]
    fn test_enhanced_redactor_gcp_api_key() {
        let redactor = EnhancedRedactor::new();
        // AIza + exactly 35 alphanumeric/-/_ chars
        let text = "key: AIzaSyC_1234567890abcdefghijklmnopqrstuv";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:GCP_API_KEY]"), "Got: {result}");
    }

    #[test]
    fn test_enhanced_redactor_database_url() {
        let redactor = EnhancedRedactor::new();
        let text = "DATABASE_URL=postgres://user:pass@localhost/mydb";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:DATABASE_URL]"));
    }

    #[test]
    fn test_enhanced_redactor_sendgrid() {
        let redactor = EnhancedRedactor::new();
        let prefix = "SG";
        let text = format!(
            "SENDGRID_KEY={}.abcdefghijklmnopqrstuv.ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopq",
            prefix
        );
        let result = redactor.redact(&text);
        assert!(result.contains("[REDACTED:SENDGRID_API_KEY]"));
    }

    #[test]
    fn test_enhanced_redactor_no_secrets() {
        let redactor = EnhancedRedactor::new();
        let text = "Just a normal string with no secrets.";
        assert_eq!(redactor.redact(text), text);
    }

    #[test]
    fn test_enhanced_redactor_inherits_basic_patterns() {
        let redactor = EnhancedRedactor::new();
        // Should still catch basic patterns (AWS key from BasicRedactor)
        let text = "My key is AKIAIOSFODNN7EXAMPLE ok";
        let result = redactor.redact(text);
        assert!(result.contains("[REDACTED:AWS_ACCESS_KEY]"));
    }

    #[test]
    fn test_enhanced_redactor_pattern_count() {
        let redactor = EnhancedRedactor::new();
        // Basic (9) + Enhanced (21) = 30 patterns
        assert!(redactor.patterns.len() >= 25, "Expected >= 25 patterns, got {}", redactor.patterns.len());
    }
}
