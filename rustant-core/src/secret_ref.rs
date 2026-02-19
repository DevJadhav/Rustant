//! Secret reference types for secure credential resolution.
//!
//! `SecretRef` provides a unified way to reference secrets that may be stored in:
//! - OS keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service)
//! - Environment variables
//! - Inline plaintext (deprecated, with migration warnings)
//!
//! Format: `"keychain:<account>"`, `"env:<VAR>"`, or bare string (inline plaintext).

use crate::credentials::{CredentialError, CredentialStore};
use serde::{Deserialize, Serialize};

/// A reference to a secret value that can be resolved at runtime.
///
/// Stored as a plain string with prefix-based dispatch:
/// - `"keychain:<account>"` — resolve from OS keychain
/// - `"env:<VAR_NAME>"` — resolve from environment variable
/// - Any other string — treated as inline plaintext (deprecated)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// Create a new keychain-backed secret reference.
    pub fn keychain(account: &str) -> Self {
        Self(format!("keychain:{account}"))
    }

    /// Create a new environment variable-backed secret reference.
    pub fn env(var_name: &str) -> Self {
        Self(format!("env:{var_name}"))
    }

    /// Create an inline (plaintext) secret reference. This is deprecated.
    pub fn inline(value: &str) -> Self {
        Self(value.to_string())
    }

    /// Check if this reference is empty (no secret configured).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check if this is a keychain reference.
    pub fn is_keychain(&self) -> bool {
        self.0.starts_with("keychain:")
    }

    /// Check if this is an environment variable reference.
    pub fn is_env(&self) -> bool {
        self.0.starts_with("env:")
    }

    /// Check if this is an inline (plaintext) value — deprecated usage.
    pub fn is_inline(&self) -> bool {
        !self.0.is_empty() && !self.is_keychain() && !self.is_env()
    }

    /// Get the raw reference string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SecretRef {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SecretRef {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Resolves `SecretRef` values to actual secret strings.
pub struct SecretResolver;

impl SecretResolver {
    /// Resolve a `SecretRef` to its actual secret value.
    ///
    /// Resolution order by prefix:
    /// 1. `"keychain:<account>"` — looks up in the OS credential store
    /// 2. `"env:<VAR>"` — reads the environment variable
    /// 3. Bare string — returns as-is with a deprecation warning
    pub fn resolve(
        secret_ref: &SecretRef,
        store: &dyn CredentialStore,
    ) -> Result<String, SecretResolveError> {
        let raw = &secret_ref.0;
        if raw.is_empty() {
            return Err(SecretResolveError::Empty);
        }

        if let Some(account) = raw.strip_prefix("keychain:") {
            store
                .get_key(account)
                .map_err(|e| SecretResolveError::KeychainError {
                    account: account.to_string(),
                    source: e,
                })
        } else if let Some(var) = raw.strip_prefix("env:") {
            std::env::var(var).map_err(|_| SecretResolveError::EnvVarMissing {
                var: var.to_string(),
            })
        } else {
            // Inline plaintext — deprecated
            tracing::warn!(
                "Inline plaintext secret detected. Migrate to keychain with: rustant setup migrate-secrets"
            );
            Ok(raw.clone())
        }
    }
}

/// Errors from secret resolution.
#[derive(Debug, thiserror::Error)]
pub enum SecretResolveError {
    #[error("Secret reference is empty")]
    Empty,

    #[error("Keychain lookup failed for '{account}': {source}")]
    KeychainError {
        account: String,
        source: CredentialError,
    },

    #[error("Environment variable '{var}' not set")]
    EnvVarMissing { var: String },
}

/// Result of migrating secrets from plaintext to keychain.
#[derive(Debug, Default)]
pub struct MigrationResult {
    /// Number of secrets successfully migrated to keychain.
    pub migrated: usize,
    /// Number of secrets already using keychain or env refs.
    pub already_secure: usize,
    /// Errors encountered during migration.
    pub errors: Vec<String>,
}

impl std::fmt::Display for MigrationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Migration complete: {} migrated, {} already secure, {} errors",
            self.migrated,
            self.already_secure,
            self.errors.len()
        )
    }
}

/// Migrate plaintext secrets in channel configs to keychain storage.
///
/// Scans all channel configs for non-empty plaintext token fields,
/// stores each in the keychain under `"channel:{type}:{field}"`,
/// and returns `SecretRef::keychain(...)` values to replace them.
pub fn migrate_channel_secrets(
    slack_token: Option<&str>,
    discord_token: Option<&str>,
    telegram_token: Option<&str>,
    email_password: Option<&str>,
    matrix_token: Option<&str>,
    whatsapp_token: Option<&str>,
    store: &dyn CredentialStore,
) -> MigrationResult {
    let mut result = MigrationResult::default();

    let secrets = [
        ("channel:slack:bot_token", slack_token),
        ("channel:discord:bot_token", discord_token),
        ("channel:telegram:bot_token", telegram_token),
        ("channel:email:password", email_password),
        ("channel:matrix:access_token", matrix_token),
        ("channel:whatsapp:access_token", whatsapp_token),
    ];

    for (account, value) in &secrets {
        match value {
            Some(v) if !v.is_empty() && !v.starts_with("keychain:") && !v.starts_with("env:") => {
                match store.store_key(account, v) {
                    Ok(()) => {
                        tracing::info!(account = account, "Migrated secret to keychain");
                        result.migrated += 1;
                    }
                    Err(e) => {
                        result
                            .errors
                            .push(format!("Failed to store {account}: {e}"));
                    }
                }
            }
            Some(v) if v.starts_with("keychain:") || v.starts_with("env:") => {
                result.already_secure += 1;
            }
            _ => {
                // Empty or None — nothing to migrate
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::InMemoryCredentialStore;

    #[test]
    fn test_secret_ref_keychain() {
        let sr = SecretRef::keychain("channel:slack:bot_token");
        assert!(sr.is_keychain());
        assert!(!sr.is_env());
        assert!(!sr.is_inline());
        assert!(!sr.is_empty());
        assert_eq!(sr.as_str(), "keychain:channel:slack:bot_token");
    }

    #[test]
    fn test_secret_ref_env() {
        let sr = SecretRef::env("SLACK_BOT_TOKEN");
        assert!(sr.is_env());
        assert!(!sr.is_keychain());
        assert!(!sr.is_inline());
        assert_eq!(sr.as_str(), "env:SLACK_BOT_TOKEN");
    }

    #[test]
    fn test_secret_ref_inline() {
        let sr = SecretRef::inline("xoxb-123-456");
        assert!(sr.is_inline());
        assert!(!sr.is_keychain());
        assert!(!sr.is_env());
    }

    #[test]
    fn test_secret_ref_empty() {
        let sr = SecretRef::default();
        assert!(sr.is_empty());
        assert!(!sr.is_inline());
    }

    #[test]
    fn test_resolve_keychain() {
        let store = InMemoryCredentialStore::new();
        store
            .store_key("channel:slack:bot_token", "xoxb-secret")
            .unwrap();
        let sr = SecretRef::keychain("channel:slack:bot_token");
        let resolved = SecretResolver::resolve(&sr, &store).unwrap();
        assert_eq!(resolved, "xoxb-secret");
    }

    #[test]
    fn test_resolve_env() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_SECRET_REF", "env-secret-value") };
        let sr = SecretRef::env("RUSTANT_TEST_SECRET_REF");
        let store = InMemoryCredentialStore::new();
        let resolved = SecretResolver::resolve(&sr, &store).unwrap();
        assert_eq!(resolved, "env-secret-value");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_SECRET_REF") };
    }

    #[test]
    fn test_resolve_inline() {
        let sr = SecretRef::inline("plaintext-token");
        let store = InMemoryCredentialStore::new();
        let resolved = SecretResolver::resolve(&sr, &store).unwrap();
        assert_eq!(resolved, "plaintext-token");
    }

    #[test]
    fn test_resolve_empty_errors() {
        let sr = SecretRef::default();
        let store = InMemoryCredentialStore::new();
        assert!(SecretResolver::resolve(&sr, &store).is_err());
    }

    #[test]
    fn test_resolve_keychain_not_found() {
        let sr = SecretRef::keychain("nonexistent");
        let store = InMemoryCredentialStore::new();
        let result = SecretResolver::resolve(&sr, &store);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SecretResolveError::KeychainError { .. }
        ));
    }

    #[test]
    fn test_resolve_env_missing() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_ABSOLUTELY_MISSING_VAR") };
        let sr = SecretRef::env("RUSTANT_ABSOLUTELY_MISSING_VAR");
        let store = InMemoryCredentialStore::new();
        let result = SecretResolver::resolve(&sr, &store);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SecretResolveError::EnvVarMissing { .. }
        ));
    }

    #[test]
    fn test_serde_transparent() {
        let sr = SecretRef::keychain("test:account");
        let json = serde_json::to_string(&sr).unwrap();
        assert_eq!(json, "\"keychain:test:account\"");
        let parsed: SecretRef = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.as_str(), "keychain:test:account");
    }

    #[test]
    fn test_migrate_channel_secrets() {
        let store = InMemoryCredentialStore::new();
        let result = migrate_channel_secrets(
            Some("xoxb-plaintext-token"),
            None,
            Some("env:TELEGRAM_TOKEN"), // already secure
            Some("my-email-password"),
            None,
            None,
            &store,
        );
        assert_eq!(result.migrated, 2); // slack + email
        assert_eq!(result.already_secure, 1); // telegram
        assert!(result.errors.is_empty());

        // Verify stored in keychain
        assert_eq!(
            store.get_key("channel:slack:bot_token").unwrap(),
            "xoxb-plaintext-token"
        );
        assert_eq!(
            store.get_key("channel:email:password").unwrap(),
            "my-email-password"
        );
    }
}
