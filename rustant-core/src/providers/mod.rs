//! LLM provider implementations.
//!
//! Provides concrete implementations of the `LlmProvider` trait for:
//! - OpenAI-compatible APIs (OpenAI, Azure, Ollama, vLLM, LM Studio)
//! - Anthropic Messages API (Claude models)
//! - Google Gemini API (Gemini models)
//!
//! Use `create_provider()` to instantiate the appropriate provider based on config.

pub mod anthropic;
pub mod failover;
pub mod gemini;
pub mod models;
pub mod openai_compat;

use crate::brain::LlmProvider;
use crate::config::LlmConfig;
use crate::credentials::CredentialStore;
use crate::error::LlmError;
use std::sync::Arc;
use std::time::Duration;

pub use anthropic::AnthropicProvider;
pub use failover::{AuthProfile, CircuitBreaker, CircuitState, FailoverProvider};
pub use gemini::GeminiProvider;
pub use models::ModelInfo;
pub use openai_compat::OpenAiCompatibleProvider;

/// Resolve the API key for a provider, checking the credential store first,
/// then falling back to the environment variable.
///
/// Returns the API key string, or an `LlmError::AuthFailed` if neither source has a key.
pub fn resolve_api_key(
    config: &LlmConfig,
    cred_store: &dyn CredentialStore,
) -> Result<String, LlmError> {
    // 1. Try credential store first
    if let Some(ref cs_key) = config.credential_store_key {
        if let Ok(key) = cred_store.get_key(cs_key) {
            return Ok(key);
        }
    }
    // 2. Fall back to env var
    std::env::var(&config.api_key_env).map_err(|_| LlmError::AuthFailed {
        provider: format!(
            "env var '{}' not set and no credential store key found",
            config.api_key_env
        ),
    })
}

/// Resolve authentication for a provider, supporting both API keys and OAuth tokens.
///
/// If `config.auth_method` is `"oauth"`, loads and (if needed) refreshes the OAuth
/// token from the credential store. Otherwise falls back to the standard API key
/// resolution via [`resolve_api_key`].
///
/// Returns the resolved API key or access token string.
pub async fn resolve_auth(
    config: &LlmConfig,
    cred_store: &dyn CredentialStore,
) -> Result<String, LlmError> {
    if config.auth_method == "oauth" {
        let token = crate::oauth::load_oauth_token(cred_store, &config.provider)?;

        if crate::oauth::is_token_expired(&token) {
            if let Some(ref rt) = token.refresh_token {
                let oauth_cfg = crate::oauth::oauth_config_for_provider(&config.provider)
                    .ok_or_else(|| LlmError::OAuthFailed {
                        message: format!(
                            "OAuth not supported for provider '{}' — cannot refresh token",
                            config.provider
                        ),
                    })?;
                let new_token = crate::oauth::refresh_token(&oauth_cfg, rt).await?;
                crate::oauth::store_oauth_token(cred_store, &config.provider, &new_token)?;
                return Ok(new_token.access_token);
            }
            return Err(LlmError::OAuthFailed {
                message: format!(
                    "OAuth token for '{}' has expired and no refresh token is available. \
                     Please re-authenticate with: rustant auth login {}",
                    config.provider, config.provider
                ),
            });
        }

        Ok(token.access_token)
    } else {
        resolve_api_key(config, cred_store)
    }
}

/// Create a single LLM provider based on the configuration.
fn create_single_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    match config.provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(config)?)),
        "gemini" => Ok(Arc::new(GeminiProvider::new(config)?)),
        _ => Ok(Arc::new(OpenAiCompatibleProvider::new(config)?)),
    }
}

/// Create a single LLM provider using a pre-resolved API key or token.
fn create_single_provider_with_key(
    config: &LlmConfig,
    api_key: String,
) -> Result<Arc<dyn LlmProvider>, LlmError> {
    match config.provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new_with_key(config, api_key)?)),
        "gemini" => Ok(Arc::new(GeminiProvider::new_with_key(config, api_key)?)),
        _ => Ok(Arc::new(OpenAiCompatibleProvider::new_with_key(
            config, api_key,
        )?)),
    }
}

/// Create an LLM provider based on the configuration.
///
/// Routes to the appropriate provider implementation:
/// - `"anthropic"` → `AnthropicProvider` (native Anthropic Messages API)
/// - Everything else → `OpenAiCompatibleProvider` (OpenAI, Azure, Ollama, local, etc.)
///
/// If `fallback_providers` are configured, wraps in a `FailoverProvider` that
/// tries providers in priority order with circuit breaker protection.
///
/// Returns an error if the primary provider cannot be initialized.
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    let primary = create_single_provider(config)?;

    if config.fallback_providers.is_empty() {
        return Ok(primary);
    }

    // Build fallback providers, logging warnings for any that fail to initialize
    let mut providers: Vec<Arc<dyn LlmProvider>> = vec![primary];
    for fallback_config in &config.fallback_providers {
        let fb_llm_config = LlmConfig {
            provider: fallback_config.provider.clone(),
            model: fallback_config.model.clone(),
            api_key_env: fallback_config.api_key_env.clone(),
            base_url: fallback_config.base_url.clone(),
            ..config.clone()
        };
        match create_single_provider(&fb_llm_config) {
            Ok(p) => providers.push(p),
            Err(e) => {
                tracing::warn!(
                    provider = %fallback_config.provider,
                    model = %fallback_config.model,
                    error = %e,
                    "Skipping fallback provider that failed to initialize"
                );
            }
        }
    }

    if providers.len() == 1 {
        // All fallbacks failed, just return primary
        return Ok(providers.remove(0));
    }

    Ok(Arc::new(FailoverProvider::new(
        providers,
        5,                       // open circuit after 5 consecutive failures
        Duration::from_secs(60), // recovery timeout
    )))
}

/// Create an LLM provider with full authentication resolution.
///
/// Resolves the API key or OAuth access token via [`resolve_auth`], then creates
/// the provider using the resolved credential. Supports OAuth token refresh.
///
/// Use this instead of [`create_provider`] when OAuth authentication may be in use.
pub async fn create_provider_with_auth(
    config: &LlmConfig,
    cred_store: &dyn CredentialStore,
) -> Result<Arc<dyn LlmProvider>, LlmError> {
    let api_key = resolve_auth(config, cred_store).await?;
    let primary = create_single_provider_with_key(config, api_key)?;

    if config.fallback_providers.is_empty() {
        return Ok(primary);
    }

    let mut providers: Vec<Arc<dyn LlmProvider>> = vec![primary];
    for fallback_config in &config.fallback_providers {
        let fb_llm_config = LlmConfig {
            provider: fallback_config.provider.clone(),
            model: fallback_config.model.clone(),
            api_key_env: fallback_config.api_key_env.clone(),
            base_url: fallback_config.base_url.clone(),
            ..config.clone()
        };
        match create_single_provider(&fb_llm_config) {
            Ok(p) => providers.push(p),
            Err(e) => {
                tracing::warn!(
                    provider = %fallback_config.provider,
                    model = %fallback_config.model,
                    error = %e,
                    "Skipping fallback provider that failed to initialize"
                );
            }
        }
    }

    if providers.len() == 1 {
        return Ok(providers.remove(0));
    }

    Ok(Arc::new(FailoverProvider::new(
        providers,
        5,
        Duration::from_secs(60),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(provider: &str) -> LlmConfig {
        LlmConfig {
            provider: provider.to_string(),
            model: "test-model".to_string(),
            api_key_env: "RUSTANT_TEST_API_KEY".to_string(),
            base_url: None,
            max_tokens: 4096,
            temperature: 0.7,
            context_window: 128_000,
            input_cost_per_million: 1.0,
            output_cost_per_million: 2.0,
            use_streaming: false,
            fallback_providers: Vec::new(),
            credential_store_key: None,
            auth_method: String::new(),
            api_key: None,
        }
    }

    #[test]
    fn test_create_provider_openai() {
        // Set a fake key for the test
        std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-123");
        let config = test_config("openai");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        std::env::remove_var("RUSTANT_TEST_API_KEY");
    }

    #[test]
    fn test_create_provider_anthropic() {
        std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-456");
        let config = test_config("anthropic");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        std::env::remove_var("RUSTANT_TEST_API_KEY");
    }

    #[test]
    fn test_create_provider_gemini() {
        std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-gemini");
        let config = test_config("gemini");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        std::env::remove_var("RUSTANT_TEST_API_KEY");
    }

    #[test]
    fn test_create_provider_unknown_defaults_to_openai() {
        std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-789");
        let config = test_config("local");
        let result = create_provider(&config);
        assert!(result.is_ok());
        std::env::remove_var("RUSTANT_TEST_API_KEY");
    }

    #[test]
    fn test_create_provider_missing_key() {
        // Ensure the env var does not exist
        std::env::remove_var("RUSTANT_NONEXISTENT_KEY");
        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_NONEXISTENT_KEY".to_string();
        let result = create_provider(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            LlmError::AuthFailed { provider } => {
                assert!(provider.contains("RUSTANT_NONEXISTENT_KEY"));
            }
            other => panic!("Expected AuthFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_api_key_from_credential_store() {
        use crate::credentials::InMemoryCredentialStore;

        let store = InMemoryCredentialStore::new();
        store.store_key("openai", "sk-from-cred-store").unwrap();

        let mut config = test_config("openai");
        config.credential_store_key = Some("openai".to_string());

        let key = resolve_api_key(&config, &store).unwrap();
        assert_eq!(key, "sk-from-cred-store");
    }

    #[test]
    fn test_resolve_api_key_prefers_credential_store() {
        use crate::credentials::InMemoryCredentialStore;

        // Both credential store and env var are set
        std::env::set_var("RUSTANT_RESOLVE_TEST_KEY", "sk-from-env");
        let store = InMemoryCredentialStore::new();
        store.store_key("openai", "sk-from-cred-store").unwrap();

        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_RESOLVE_TEST_KEY".to_string();
        config.credential_store_key = Some("openai".to_string());

        let key = resolve_api_key(&config, &store).unwrap();
        // Credential store should win
        assert_eq!(key, "sk-from-cred-store");
        std::env::remove_var("RUSTANT_RESOLVE_TEST_KEY");
    }

    #[test]
    fn test_resolve_api_key_falls_back_to_env() {
        use crate::credentials::InMemoryCredentialStore;

        std::env::set_var("RUSTANT_RESOLVE_FALLBACK_KEY", "sk-from-env");
        let store = InMemoryCredentialStore::new();
        // No key in credential store

        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_RESOLVE_FALLBACK_KEY".to_string();
        config.credential_store_key = Some("openai".to_string());

        let key = resolve_api_key(&config, &store).unwrap();
        assert_eq!(key, "sk-from-env");
        std::env::remove_var("RUSTANT_RESOLVE_FALLBACK_KEY");
    }
}
