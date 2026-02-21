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
pub mod rate_limiter;
pub mod warmup;

use crate::brain::LlmProvider;
use crate::config::LlmConfig;
use crate::credentials::CredentialStore;
use crate::error::LlmError;
use std::future::Future;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

// =============================================================================
// Shared HTTP Client
// =============================================================================

/// Shared reqwest::Client for all providers.
///
/// Creating a new `reqwest::Client` is expensive (~5-20ms): it spawns a connection
/// pool, compiles TLS settings, and resolves system proxy. By sharing one client
/// across all providers, we:
/// 1. Save 50-150ms TTFT on the first request (no redundant pool creation)
/// 2. Enable HTTP/2 connection reuse across providers hitting the same host
/// 3. Pool TCP connections (keep-alive) for subsequent requests
///
/// The client is lazily initialized on first use with generous timeouts suitable
/// for LLM APIs (120s response timeout, 10s connect timeout).
static SHARED_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// Get the shared HTTP client, creating it on first call.
///
/// All providers should use this instead of `Client::new()` or `Client::builder().build()`.
/// The client is configured with:
/// - 120s response timeout (LLM APIs can be slow)
/// - 10s connect timeout
/// - Connection pooling (keep-alive) enabled by default
/// - System proxy settings respected
pub fn shared_http_client() -> reqwest::Client {
    SHARED_HTTP_CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .connect_timeout(Duration::from_secs(10))
                .pool_max_idle_per_host(4)
                .build()
                .expect("Failed to build shared HTTP client")
        })
        .clone()
}

pub use crate::config::RetryConfig;
pub use anthropic::AnthropicProvider;
pub use failover::{AuthProfile, CircuitBreaker, CircuitState, FailoverProvider};
pub use gemini::GeminiProvider;
pub use models::{ModelInfo, PricingCache, PricingEntry};
pub use openai_compat::OpenAiCompatibleProvider;

/// Execute an async operation with exponential backoff retry on transient errors.
///
/// Retries on `LlmError::RateLimited` (respects `retry_after_secs`), `LlmError::Streaming`,
/// `LlmError::Connection`, and `LlmError::Timeout`. Permanent errors (auth, parse) return immediately.
pub async fn with_retry<F, Fut, T>(config: &RetryConfig, operation: F) -> Result<T, LlmError>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, LlmError>>,
{
    let mut last_err = None;
    for attempt in 0..=config.max_retries {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if !is_retryable(&e) || attempt == config.max_retries {
                    return Err(e);
                }

                let backoff_ms = compute_backoff(config, attempt, &e);
                tracing::warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    backoff_ms = backoff_ms,
                    error = %e,
                    "Retrying after transient error"
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| LlmError::Connection {
        message: "All retry attempts exhausted".to_string(),
    }))
}

/// Check if an error is retryable (transient).
fn is_retryable(err: &LlmError) -> bool {
    matches!(
        err,
        LlmError::RateLimited { .. }
            | LlmError::Streaming { .. }
            | LlmError::Connection { .. }
            | LlmError::Timeout { .. }
    )
}

/// Compute backoff delay, respecting rate limit retry-after headers.
fn compute_backoff(config: &RetryConfig, attempt: u32, err: &LlmError) -> u64 {
    // For rate limiting, respect the server's retry-after if present
    if let LlmError::RateLimited { retry_after_secs } = err {
        let server_ms = retry_after_secs * 1000;
        let computed = compute_exponential_backoff(config, attempt);
        return server_ms.max(computed);
    }
    compute_exponential_backoff(config, attempt)
}

/// Pure exponential backoff with optional jitter.
fn compute_exponential_backoff(config: &RetryConfig, attempt: u32) -> u64 {
    let base = config.initial_backoff_ms as f64 * config.backoff_multiplier.powi(attempt as i32);
    let capped = base.min(config.max_backoff_ms as f64) as u64;
    if config.jitter {
        // Add up to 25% jitter
        let jitter = (capped as f64 * 0.25 * rand_simple()) as u64;
        capped + jitter
    } else {
        capped
    }
}

/// Simple deterministic pseudo-random for jitter (avoids pulling in rand crate).
fn rand_simple() -> f64 {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 1000) as f64 / 1000.0
}

/// Resolve the API key for a provider, checking the credential store first,
/// then falling back to the environment variable.
///
/// Returns the API key string, or an `LlmError::AuthFailed` if neither source has a key.
pub fn resolve_api_key(
    config: &LlmConfig,
    cred_store: &dyn CredentialStore,
) -> Result<String, LlmError> {
    // 1. Try credential store first
    if let Some(ref cs_key) = config.credential_store_key
        && let Ok(key) = cred_store.get_key(cs_key)
    {
        return Ok(key);
    }
    // 2. Fall back to env var
    std::env::var(&config.api_key_env).map_err(|_| LlmError::AuthFailed {
        provider: format!(
            "env var '{}' not set and no credential store key found",
            config.api_key_env
        ),
    })
}

/// Resolve an API key by environment variable name.
///
/// This is a simplified helper for modules that need an API key but don't have
/// access to a full `LlmConfig` (e.g., voice, embeddings, meeting tools).
/// Checks the OS keychain first (via `keyring`), then the environment variable.
///
/// # Arguments
/// * `env_var` - Environment variable name (e.g., "OPENAI_API_KEY")
///
/// # Returns
/// The API key string, or an error message if not found.
pub fn resolve_api_key_by_env(env_var: &str) -> Result<String, String> {
    // 1. Try OS keychain (uses same key naming as credential store)
    let keychain_key = format!("rustant_{}", env_var.to_lowercase());
    if let Ok(entry) = keyring::Entry::new("rustant", &keychain_key)
        && let Ok(key) = entry.get_password()
        && !key.is_empty()
    {
        return Ok(key);
    }

    // 2. Fall back to environment variable
    std::env::var(env_var).map_err(|_| {
        format!(
            "{env_var} not set. Set the environment variable or store it via `rustant auth login`."
        )
    })
}

/// Resolve API key from OS keychain ONLY. No env var fallback.
///
/// More secure: keys never leak to child processes, logs, or crash reports.
/// Users store keys via `rustant auth login --service <name>`.
///
/// # Arguments
/// * `account` - Keychain account name (e.g., "semantic_scholar_api_key")
pub fn resolve_api_key_keychain(account: &str) -> Result<String, String> {
    let keychain_key = format!("rustant_{}", account.to_lowercase());
    match keyring::Entry::new("rustant", &keychain_key) {
        Ok(entry) => match entry.get_password() {
            Ok(key) if !key.is_empty() => Ok(key),
            Ok(_) => Err(format!(
                "API key '{account}' is empty in keychain. Store it via: rustant auth login --service {account}"
            )),
            Err(_) => Err(format!(
                "API key '{account}' not found in keychain. Store it via: rustant auth login --service {account}"
            )),
        },
        Err(e) => Err(format!("Keychain access failed for '{account}': {e}")),
    }
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
///
/// Resolves API key from credential store (keychain) first, then falls back to
/// env var. This ensures both primary and fallback providers can use keychain keys.
fn create_single_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    // Try to resolve API key from keychain via credential_store_key
    if config.api_key.is_none() {
        if let Some(ref cs_key) = config.credential_store_key {
            let cred_store = crate::credentials::KeyringCredentialStore::new();
            match cred_store.get_key(cs_key) {
                Ok(key) => {
                    tracing::debug!(provider = %config.provider, "Resolved API key from credential store");
                    return create_single_provider_with_key(config, key);
                }
                Err(e) => {
                    tracing::warn!(provider = %config.provider, "Credential store lookup failed for '{cs_key}': {e}");
                }
            }
        }
    }

    match config.provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(config)?)),
        "gemini" => Ok(Arc::new(GeminiProvider::new(config)?)),
        "ollama" => {
            // Ensure Ollama has the right defaults
            let mut ollama_config = config.clone();
            if ollama_config.base_url.is_none() {
                ollama_config.base_url = Some("http://localhost:11434/v1".to_string());
            }
            if ollama_config.api_key.is_none() {
                ollama_config.api_key = Some("ollama".to_string());
            }
            Ok(Arc::new(OpenAiCompatibleProvider::new(&ollama_config)?))
        }
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
        "ollama" => {
            let mut ollama_config = config.clone();
            if ollama_config.base_url.is_none() {
                ollama_config.base_url = Some("http://localhost:11434/v1".to_string());
            }
            Ok(Arc::new(OpenAiCompatibleProvider::new_with_key(
                &ollama_config,
                api_key,
            )?))
        }
        _ => Ok(Arc::new(OpenAiCompatibleProvider::new_with_key(
            config, api_key,
        )?)),
    }
}

/// List models available from an Ollama instance.
///
/// Queries the Ollama `/api/tags` endpoint and returns a list of model names.
/// Returns an empty vec if Ollama is unreachable or the response is invalid.
pub async fn list_ollama_models(base_url: Option<&str>) -> Vec<String> {
    let base = base_url.unwrap_or("http://localhost:11434");
    let url = format!("{base}/api/tags");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_default();
    match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(json) = resp.json::<serde_json::Value>().await
                && let Some(models) = json["models"].as_array()
            {
                return models
                    .iter()
                    .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                    .collect();
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// Check if Ollama is running and reachable.
pub async fn is_ollama_available(base_url: Option<&str>) -> bool {
    let base = base_url.unwrap_or("http://localhost:11434");
    let url = format!("{base}/api/tags");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap_or_default();
    matches!(client.get(&url).send().await, Ok(resp) if resp.status().is_success())
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
        // Use the fallback's own credential_store_key if explicitly set.
        // Do NOT auto-derive from provider name — that causes unnecessary
        // keychain lookups that fail and mask the real env var fallback.
        let fb_llm_config = LlmConfig {
            provider: fallback_config.provider.clone(),
            model: fallback_config.model.clone(),
            api_key_env: fallback_config.api_key_env.clone(),
            base_url: fallback_config.base_url.clone(),
            credential_store_key: fallback_config.credential_store_key.clone(),
            api_key: None, // Don't inherit primary provider's resolved key
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
            credential_store_key: fallback_config.credential_store_key.clone(),
            api_key: None, // Don't inherit primary provider's resolved key
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

/// Create LLM providers for council members.
///
/// Iterates over the council member configs, creates a provider for each,
/// and logs warnings for any that fail to initialize.
pub fn create_council_members(
    config: &crate::config::CouncilConfig,
) -> Vec<(Arc<dyn LlmProvider>, crate::config::CouncilMemberConfig)> {
    let mut members = Vec::new();

    for member_cfg in &config.members {
        let llm_config = crate::config::LlmConfig {
            provider: member_cfg.provider.clone(),
            model: member_cfg.model.clone(),
            api_key_env: member_cfg.api_key_env.clone(),
            base_url: member_cfg.base_url.clone(),
            ..Default::default()
        };

        match create_single_provider(&llm_config) {
            Ok(provider) => {
                members.push((provider, member_cfg.clone()));
            }
            Err(e) => {
                tracing::warn!(
                    provider = %member_cfg.provider,
                    model = %member_cfg.model,
                    error = %e,
                    "Skipping council member that failed to initialize"
                );
            }
        }
    }

    members
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
            retry: RetryConfig::default(),
            rate_limits: None,
        }
    }

    #[test]
    fn test_create_provider_openai() {
        // Set a fake key for the test
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-123") };
        let config = test_config("openai");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_API_KEY") };
    }

    #[test]
    fn test_create_provider_anthropic() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-456") };
        let config = test_config("anthropic");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_API_KEY") };
    }

    #[test]
    fn test_create_provider_gemini() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-gemini") };
        let config = test_config("gemini");
        let result = create_provider(&config);
        assert!(result.is_ok());
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "test-model");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_API_KEY") };
    }

    #[test]
    fn test_create_provider_unknown_defaults_to_openai() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_API_KEY", "test-key-789") };
        let config = test_config("local");
        let result = create_provider(&config);
        assert!(result.is_ok());
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_API_KEY") };
    }

    #[test]
    fn test_create_provider_missing_key() {
        // Ensure the env var does not exist
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_NONEXISTENT_KEY") };
        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_NONEXISTENT_KEY".to_string();
        let result = create_provider(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            LlmError::AuthFailed { provider } => {
                assert!(provider.contains("RUSTANT_NONEXISTENT_KEY"));
            }
            other => panic!("Expected AuthFailed, got {other:?}"),
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
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_RESOLVE_TEST_KEY", "sk-from-env") };
        let store = InMemoryCredentialStore::new();
        store.store_key("openai", "sk-from-cred-store").unwrap();

        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_RESOLVE_TEST_KEY".to_string();
        config.credential_store_key = Some("openai".to_string());

        let key = resolve_api_key(&config, &store).unwrap();
        // Credential store should win
        assert_eq!(key, "sk-from-cred-store");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_RESOLVE_TEST_KEY") };
    }

    #[test]
    fn test_resolve_api_key_falls_back_to_env() {
        use crate::credentials::InMemoryCredentialStore;

        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_RESOLVE_FALLBACK_KEY", "sk-from-env") };
        let store = InMemoryCredentialStore::new();
        // No key in credential store

        let mut config = test_config("openai");
        config.api_key_env = "RUSTANT_RESOLVE_FALLBACK_KEY".to_string();
        config.credential_store_key = Some("openai".to_string());

        let key = resolve_api_key(&config, &store).unwrap();
        assert_eq!(key, "sk-from-env");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_RESOLVE_FALLBACK_KEY") };
    }

    #[test]
    fn test_is_retryable() {
        assert!(super::is_retryable(&LlmError::RateLimited {
            retry_after_secs: 30
        }));
        assert!(super::is_retryable(&LlmError::Connection {
            message: "timeout".into()
        }));
        assert!(super::is_retryable(&LlmError::Timeout { timeout_secs: 30 }));
        assert!(super::is_retryable(&LlmError::Streaming {
            message: "eof".into()
        }));
        assert!(!super::is_retryable(&LlmError::AuthFailed {
            provider: "test".into()
        }));
        assert!(!super::is_retryable(&LlmError::ResponseParse {
            message: "bad json".into()
        }));
    }

    #[test]
    fn test_compute_backoff_exponential() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            backoff_multiplier: 2.0,
            jitter: false,
        };
        assert_eq!(super::compute_exponential_backoff(&config, 0), 1000);
        assert_eq!(super::compute_exponential_backoff(&config, 1), 2000);
        assert_eq!(super::compute_exponential_backoff(&config, 2), 4000);
    }

    #[test]
    fn test_compute_backoff_respects_cap() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 3000,
            backoff_multiplier: 2.0,
            jitter: false,
        };
        assert_eq!(super::compute_exponential_backoff(&config, 0), 1000);
        assert_eq!(super::compute_exponential_backoff(&config, 1), 2000);
        assert_eq!(super::compute_exponential_backoff(&config, 2), 3000); // capped
    }

    #[test]
    fn test_compute_backoff_rate_limit_uses_server_value() {
        let config = RetryConfig {
            max_retries: 3,
            initial_backoff_ms: 1000,
            max_backoff_ms: 60000,
            backoff_multiplier: 2.0,
            jitter: false,
        };
        let err = LlmError::RateLimited {
            retry_after_secs: 30,
        };
        let backoff = super::compute_backoff(&config, 0, &err);
        assert_eq!(backoff, 30000); // server says 30s, computed is 1s, use max
    }

    #[tokio::test]
    async fn test_with_retry_succeeds_first_try() {
        let config = RetryConfig::default();
        let result = with_retry(&config, || async { Ok::<_, LlmError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn test_with_retry_permanent_error_no_retry() {
        let config = RetryConfig {
            max_retries: 3,
            ..Default::default()
        };
        let call_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let result = with_retry(&config, || {
            let cc = cc.clone();
            async move {
                cc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err::<i32, _>(LlmError::AuthFailed {
                    provider: "test".into(),
                })
            }
        })
        .await;
        assert!(result.is_err());
        assert_eq!(call_count.load(std::sync::atomic::Ordering::SeqCst), 1); // no retries
    }
}
