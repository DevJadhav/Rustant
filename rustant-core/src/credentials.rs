//! Credential storage for LLM provider API keys.
//!
//! Provides a trait-based abstraction over credential storage with two implementations:
//! - `KeyringCredentialStore`: Uses the OS-native credential store (macOS Keychain,
//!   Windows Credential Manager, Linux Secret Service).
//! - `InMemoryCredentialStore`: In-memory store for testing.

use std::collections::HashMap;
use std::sync::Mutex;

/// Errors from credential storage operations.
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("Credential not found for {service}:{account}")]
    NotFound { service: String, account: String },

    #[error("Failed to store credential: {message}")]
    StoreFailed { message: String },

    #[error("Failed to delete credential: {message}")]
    DeleteFailed { message: String },

    #[error("Keyring backend not available: {message}")]
    BackendUnavailable { message: String },
}

/// Trait for credential storage backends.
pub trait CredentialStore: Send + Sync {
    /// Store an API key for the given provider.
    fn store_key(&self, provider: &str, api_key: &str) -> Result<(), CredentialError>;

    /// Retrieve the API key for the given provider.
    fn get_key(&self, provider: &str) -> Result<String, CredentialError>;

    /// Delete the API key for the given provider.
    fn delete_key(&self, provider: &str) -> Result<(), CredentialError>;

    /// Check whether a key exists for the given provider.
    fn has_key(&self, provider: &str) -> bool;
}

/// OS-native credential store using the `keyring` crate.
///
/// Stores credentials under service `"rustant"` with account names
/// formatted as `"provider:{name}"`.
pub struct KeyringCredentialStore {
    service: String,
}

impl KeyringCredentialStore {
    /// Create a new keyring-backed credential store.
    pub fn new() -> Self {
        Self {
            service: "rustant".to_string(),
        }
    }

    /// Format the account name for a given provider.
    pub fn account_name(provider: &str) -> String {
        format!("provider:{provider}")
    }
}

impl Default for KeyringCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for KeyringCredentialStore {
    fn store_key(&self, provider: &str, api_key: &str) -> Result<(), CredentialError> {
        let account = Self::account_name(provider);
        let entry = keyring::Entry::new(&self.service, &account).map_err(|e| {
            CredentialError::BackendUnavailable {
                message: e.to_string(),
            }
        })?;
        entry
            .set_password(api_key)
            .map_err(|e| CredentialError::StoreFailed {
                message: e.to_string(),
            })
    }

    fn get_key(&self, provider: &str) -> Result<String, CredentialError> {
        let account = Self::account_name(provider);
        let entry = keyring::Entry::new(&self.service, &account).map_err(|e| {
            CredentialError::BackendUnavailable {
                message: e.to_string(),
            }
        })?;
        entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => CredentialError::NotFound {
                service: self.service.clone(),
                account,
            },
            other => CredentialError::StoreFailed {
                message: other.to_string(),
            },
        })
    }

    fn delete_key(&self, provider: &str) -> Result<(), CredentialError> {
        let account = Self::account_name(provider);
        let entry = keyring::Entry::new(&self.service, &account).map_err(|e| {
            CredentialError::BackendUnavailable {
                message: e.to_string(),
            }
        })?;
        entry
            .delete_credential()
            .map_err(|e| CredentialError::DeleteFailed {
                message: e.to_string(),
            })
    }

    fn has_key(&self, provider: &str) -> bool {
        self.get_key(provider).is_ok()
    }
}

/// In-memory credential store for testing.
///
/// Thread-safe via `Mutex<HashMap>`. Does not persist across process restarts.
pub struct InMemoryCredentialStore {
    store: Mutex<HashMap<String, String>>,
}

impl InMemoryCredentialStore {
    /// Create an empty in-memory credential store.
    pub fn new() -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryCredentialStore {
    fn default() -> Self {
        Self::new()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn store_key(&self, provider: &str, api_key: &str) -> Result<(), CredentialError> {
        let account = KeyringCredentialStore::account_name(provider);
        self.store
            .lock()
            .unwrap()
            .insert(account, api_key.to_string());
        Ok(())
    }

    fn get_key(&self, provider: &str) -> Result<String, CredentialError> {
        let account = KeyringCredentialStore::account_name(provider);
        self.store
            .lock()
            .unwrap()
            .get(&account)
            .cloned()
            .ok_or_else(|| CredentialError::NotFound {
                service: "rustant".to_string(),
                account,
            })
    }

    fn delete_key(&self, provider: &str) -> Result<(), CredentialError> {
        let account = KeyringCredentialStore::account_name(provider);
        self.store.lock().unwrap().remove(&account);
        Ok(())
    }

    fn has_key(&self, provider: &str) -> bool {
        let account = KeyringCredentialStore::account_name(provider);
        self.store.lock().unwrap().contains_key(&account)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> InMemoryCredentialStore {
        InMemoryCredentialStore::new()
    }

    #[test]
    fn test_store_and_retrieve_key() {
        let store = make_store();
        store.store_key("openai", "sk-test-123").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), "sk-test-123");
    }

    #[test]
    fn test_get_nonexistent_key() {
        let store = make_store();
        let result = store.get_key("nonexistent");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CredentialError::NotFound { .. }
        ));
    }

    #[test]
    fn test_delete_key() {
        let store = make_store();
        store.store_key("anthropic", "sk-ant-test").unwrap();
        store.delete_key("anthropic").unwrap();
        assert!(!store.has_key("anthropic"));
    }

    #[test]
    fn test_has_key() {
        let store = make_store();
        assert!(!store.has_key("openai"));
        store.store_key("openai", "sk-test").unwrap();
        assert!(store.has_key("openai"));
    }

    #[test]
    fn test_overwrite_key() {
        let store = make_store();
        store.store_key("openai", "sk-old").unwrap();
        store.store_key("openai", "sk-new").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), "sk-new");
    }

    #[test]
    fn test_account_name_format() {
        assert_eq!(
            KeyringCredentialStore::account_name("openai"),
            "provider:openai"
        );
        assert_eq!(
            KeyringCredentialStore::account_name("anthropic"),
            "provider:anthropic"
        );
    }
}
