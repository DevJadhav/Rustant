//! LLM provider implementations.
//!
//! Provides concrete implementations of the `LlmProvider` trait for:
//! - OpenAI-compatible APIs (OpenAI, Azure, Ollama, vLLM, LM Studio)
//! - Anthropic Messages API (Claude models)
//!
//! Use `create_provider()` to instantiate the appropriate provider based on config.

pub mod anthropic;
pub mod openai_compat;

use crate::brain::LlmProvider;
use crate::config::LlmConfig;
use crate::error::LlmError;
use std::sync::Arc;

pub use anthropic::AnthropicProvider;
pub use openai_compat::OpenAiCompatibleProvider;

/// Create an LLM provider based on the configuration.
///
/// Routes to the appropriate provider implementation:
/// - `"anthropic"` → `AnthropicProvider` (native Anthropic Messages API)
/// - Everything else → `OpenAiCompatibleProvider` (OpenAI, Azure, Ollama, local, etc.)
///
/// Returns an error if the provider cannot be initialized (e.g., missing API key).
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, LlmError> {
    match config.provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(config)?)),
        _ => Ok(Arc::new(OpenAiCompatibleProvider::new(config)?)),
    }
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
}
