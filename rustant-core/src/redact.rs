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

/// Shared redactor type used throughout the agent.
pub type SharedRedactor = Arc<dyn OutputRedactor>;

/// Create a basic redactor wrapped in Arc for shared use.
pub fn create_basic_redactor() -> SharedRedactor {
    Arc::new(BasicRedactor::new())
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
        let text = format!("stripe key: {}_abcdefghijklmnopqrstuvwx", prefix);
        let result = redactor.redact(&text);
        assert!(result.contains("[REDACTED:STRIPE_SECRET]"));
    }
}
