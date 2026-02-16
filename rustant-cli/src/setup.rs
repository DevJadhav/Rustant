//! Interactive provider setup wizard.
//!
//! Walks the user through configuring an LLM provider:
//! 1. Select a provider (OpenAI, Anthropic, Custom)
//! 2. Enter API key (masked input)
//! 3. Optionally enter a custom base URL
//! 4. Validate the key by fetching available models
//! 5. Select a model from the fetched list
//! 6. Store the key in the OS credential store
//! 7. Update the workspace configuration file

use dialoguer::{Input, Password, Select};
use rustant_core::credentials::{CredentialStore, KeyringCredentialStore};
use rustant_core::providers::models::{list_models, ModelInfo};
use std::path::Path;

/// A provider option presented during the setup wizard.
#[derive(Debug, Clone)]
pub struct ProviderChoice {
    /// Internal provider name (e.g., "openai", "anthropic", "custom").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Default env var name for the API key.
    pub api_key_env: String,
}

/// Return the list of supported provider choices.
pub fn available_providers() -> Vec<ProviderChoice> {
    vec![
        ProviderChoice {
            name: "openai".to_string(),
            display_name: "OpenAI".to_string(),

            api_key_env: "OPENAI_API_KEY".to_string(),
        },
        ProviderChoice {
            name: "anthropic".to_string(),
            display_name: "Anthropic (Claude)".to_string(),

            api_key_env: "ANTHROPIC_API_KEY".to_string(),
        },
        ProviderChoice {
            name: "gemini".to_string(),
            display_name: "Google Gemini".to_string(),

            api_key_env: "GEMINI_API_KEY".to_string(),
        },
        ProviderChoice {
            name: "custom".to_string(),
            display_name: "Custom OpenAI-compatible endpoint".to_string(),

            api_key_env: "CUSTOM_API_KEY".to_string(),
        },
    ]
}

/// Validate the format of an API key for the given provider.
///
/// Returns `Ok(())` if the key looks valid, or an error message describing the issue.
/// This is a basic format check, not a full validation (that happens via the API).
pub fn validate_api_key_format(provider: &str, key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    match provider {
        "openai" => {
            if !key.starts_with("sk-") {
                return Err("OpenAI API keys typically start with 'sk-'".to_string());
            }
        }
        "anthropic" => {
            if !key.starts_with("sk-ant-") {
                return Err("Anthropic API keys typically start with 'sk-ant-'".to_string());
            }
        }
        _ => {} // No format validation for custom providers
    }
    Ok(())
}

/// Update the workspace configuration file with the selected provider and model.
///
/// Creates the `.rustant/config.toml` file if it doesn't exist, or updates
/// the LLM section of an existing config.
pub fn update_config(
    workspace: &Path,
    provider: &ProviderChoice,
    model: &ModelInfo,
    base_url: Option<&str>,
    auth_method: &str,
) -> anyhow::Result<()> {
    let config_dir = workspace.join(".rustant");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");

    // Load existing config or use default
    let mut config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        toml::from_str::<rustant_core::AgentConfig>(&content).unwrap_or_default()
    } else {
        rustant_core::AgentConfig::default()
    };

    // Update LLM config
    config.llm.provider = if provider.name == "custom" {
        "openai".to_string() // Custom endpoints use OpenAI-compatible API
    } else {
        provider.name.clone()
    };
    config.llm.model = model.id.clone();
    config.llm.api_key_env = provider.api_key_env.clone();
    config.llm.base_url = base_url.map(|s| s.to_string());
    config.llm.credential_store_key = Some(provider.name.clone());
    config.llm.auth_method = auth_method.to_string();
    if let Some(ctx) = model.context_window {
        config.llm.context_window = ctx;
    }

    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(&config_path, &toml_str)?;
    Ok(())
}

/// Detect API keys present in environment variables.
///
/// Returns a list of `(provider_name, display_name)` for providers whose
/// API key environment variable is set and non-empty.
pub fn detect_env_api_keys() -> Vec<(String, String)> {
    let providers = available_providers();
    let mut found = Vec::new();
    for p in &providers {
        if let Ok(key) = std::env::var(&p.api_key_env) {
            if !key.trim().is_empty() {
                found.push((p.name.clone(), p.display_name.clone()));
            }
        }
    }
    found
}

/// Run the interactive provider setup wizard.
///
/// Guides the user through provider selection, auth method choice, model selection,
/// and configuration saving. Supports both API key and OAuth browser-based login.
pub async fn run_setup(workspace: &Path) -> anyhow::Result<()> {
    println!("\n  Rustant Provider Setup\n");

    // Step 1: Provider selection
    let providers = available_providers();
    let provider_names: Vec<&str> = providers.iter().map(|p| p.display_name.as_str()).collect();
    let selection = Select::new()
        .with_prompt("Select your LLM provider")
        .items(&provider_names)
        .default(0)
        .interact()?;
    let chosen_provider = &providers[selection];

    // Step 2: Auth method selection (OAuth or API key)
    let supports_oauth = rustant_core::oauth::provider_supports_oauth(&chosen_provider.name);
    let use_oauth = if supports_oauth {
        let auth_options = vec!["Login with browser (OAuth)", "Enter API key manually"];
        let auth_selection = Select::new()
            .with_prompt("Select authentication method")
            .items(&auth_options)
            .default(0)
            .interact()?;
        auth_selection == 0
    } else {
        if chosen_provider.name == "anthropic" {
            println!("  Note: OAuth login is not currently available for Anthropic.");
            println!("  Anthropic does not support third-party OAuth. Using API key.\n");
        }
        false
    };

    let cred_store = KeyringCredentialStore::new();
    let auth_method: &str;
    let api_key: String;

    // Check for existing API key in the credential store
    if !use_oauth {
        if let Ok(existing_key) = cred_store.get_key(&chosen_provider.name) {
            if !existing_key.trim().is_empty() {
                println!(
                    "  An API key for {} is already stored in the OS credential store.",
                    chosen_provider.display_name
                );
                let reuse_options = vec!["Use existing key", "Enter a new key"];
                let reuse_selection = Select::new()
                    .with_prompt("What would you like to do?")
                    .items(&reuse_options)
                    .default(0)
                    .interact()?;
                if reuse_selection == 0 {
                    // Validate existing key by fetching models
                    println!("\n  Validating existing credentials...");
                    let base_url: Option<String> = if chosen_provider.name == "custom" {
                        let url: String = Input::new()
                            .with_prompt(
                                "Enter the base URL (e.g., http://localhost:11434/v1)",
                            )
                            .interact_text()?;
                        Some(url)
                    } else {
                        None
                    };
                    match list_models(
                        &chosen_provider.name,
                        &existing_key,
                        base_url.as_deref(),
                    )
                    .await
                    {
                        Ok(models) if !models.is_empty() => {
                            println!("  Credentials valid! Found {} model(s).\n", models.len());

                            let model_names: Vec<String> = models
                                .iter()
                                .map(|m| {
                                    if let Some(ctx) = m.context_window {
                                        format!("{} ({}k context)", m.id, ctx / 1000)
                                    } else {
                                        m.id.clone()
                                    }
                                })
                                .collect();
                            let model_refs: Vec<&str> =
                                model_names.iter().map(|s| s.as_str()).collect();
                            let model_selection = Select::new()
                                .with_prompt("Select a model")
                                .items(&model_refs)
                                .default(0)
                                .interact()?;
                            let chosen_model = &models[model_selection];

                            update_config(
                                workspace,
                                chosen_provider,
                                chosen_model,
                                base_url.as_deref(),
                                "api_key",
                            )?;
                            println!(
                                "  Configuration saved to {}",
                                workspace.join(".rustant").join("config.toml").display()
                            );
                            println!(
                                "\n  Setup complete! Using {} with model {}.\n",
                                chosen_provider.display_name, chosen_model.id
                            );
                            return Ok(());
                        }
                        _ => {
                            println!(
                                "  Existing key validation failed. Please enter a new key.\n"
                            );
                            // Fall through to normal flow
                        }
                    }
                }
                // "Enter a new key" selected or validation failed — fall through
            }
        }
    }

    if use_oauth {
        // OAuth browser flow
        auth_method = "oauth";
        let oauth_config = rustant_core::oauth::oauth_config_for_provider(&chosen_provider.name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "OAuth not available for provider '{}'",
                    chosen_provider.name
                )
            })?;

        println!(
            "\n  Opening browser for {} authentication...",
            chosen_provider.display_name
        );

        let token = match rustant_core::oauth::authorize_browser_flow(&oauth_config, None).await {
            Ok(token) => token,
            Err(e) => {
                println!("  OAuth login failed: {}", e);
                println!("  Falling back to API key entry.\n");
                return run_setup_api_key(workspace, chosen_provider).await;
            }
        };

        println!("  Authentication successful!");

        // Store the OAuth token
        rustant_core::oauth::store_oauth_token(&cred_store, &chosen_provider.name, &token)
            .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {}", e))?;
        println!("  OAuth token stored securely in OS credential store.");

        api_key = token.access_token;
    } else {
        // Traditional API key flow
        auth_method = "api_key";
        api_key = Password::new()
            .with_prompt(format!(
                "Enter your {} API key",
                chosen_provider.display_name
            ))
            .interact()?;

        // Basic format validation
        if let Err(warning) = validate_api_key_format(&chosen_provider.name, &api_key) {
            println!("  Warning: {}", warning);
            println!("  Continuing anyway — the key will be validated against the API.\n");
        }

        // Store API key
        cred_store
            .store_key(&chosen_provider.name, &api_key)
            .map_err(|e| anyhow::anyhow!("Failed to store API key: {}", e))?;
        println!("  API key stored securely in OS credential store.");
    }

    // Step 3: Optional base URL (for custom providers)
    let base_url: Option<String> = if chosen_provider.name == "custom" {
        let url: String = Input::new()
            .with_prompt("Enter the base URL (e.g., http://localhost:11434/v1)")
            .interact_text()?;
        Some(url)
    } else {
        None
    };

    // Step 4: Validate key by fetching models
    println!("\n  Validating credentials and fetching available models...");
    let models = list_models(&chosen_provider.name, &api_key, base_url.as_deref())
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to validate credentials: {}. Please check and try again.",
                e
            )
        })?;

    if models.is_empty() {
        anyhow::bail!("No models found. Please check your credentials and endpoint.");
    }

    println!("  Found {} available model(s).\n", models.len());

    // Step 5: Model selection
    let model_names: Vec<String> = models
        .iter()
        .map(|m| {
            if let Some(ctx) = m.context_window {
                format!("{} ({}k context)", m.id, ctx / 1000)
            } else {
                m.id.clone()
            }
        })
        .collect();
    let model_refs: Vec<&str> = model_names.iter().map(|s| s.as_str()).collect();
    let model_selection = Select::new()
        .with_prompt("Select a model")
        .items(&model_refs)
        .default(0)
        .interact()?;
    let chosen_model = &models[model_selection];

    // Step 6: Update config
    update_config(
        workspace,
        chosen_provider,
        chosen_model,
        base_url.as_deref(),
        auth_method,
    )?;
    println!(
        "  Configuration saved to {}",
        workspace.join(".rustant").join("config.toml").display()
    );
    println!(
        "\n  Setup complete! Using {} with model {} ({}).\n",
        chosen_provider.display_name, chosen_model.id, auth_method
    );

    Ok(())
}

/// Fallback: run the API key setup flow when OAuth fails.
async fn run_setup_api_key(workspace: &Path, provider: &ProviderChoice) -> anyhow::Result<()> {
    let cred_store = KeyringCredentialStore::new();

    // Check for existing key before prompting
    if let Ok(existing_key) = cred_store.get_key(&provider.name) {
        if !existing_key.trim().is_empty() {
            println!(
                "  An API key for {} is already stored.",
                provider.display_name
            );
            let reuse_options = vec!["Use existing key", "Enter a new key"];
            let reuse_selection = Select::new()
                .with_prompt("What would you like to do?")
                .items(&reuse_options)
                .default(0)
                .interact()?;
            if reuse_selection == 0 {
                println!("\n  Validating existing credentials...");
                match list_models(&provider.name, &existing_key, None).await {
                    Ok(models) if !models.is_empty() => {
                        println!("  Credentials valid! Found {} model(s).\n", models.len());
                        let model_names: Vec<String> = models
                            .iter()
                            .map(|m| {
                                if let Some(ctx) = m.context_window {
                                    format!("{} ({}k context)", m.id, ctx / 1000)
                                } else {
                                    m.id.clone()
                                }
                            })
                            .collect();
                        let model_refs: Vec<&str> =
                            model_names.iter().map(|s| s.as_str()).collect();
                        let model_selection = Select::new()
                            .with_prompt("Select a model")
                            .items(&model_refs)
                            .default(0)
                            .interact()?;
                        let chosen_model = &models[model_selection];
                        update_config(workspace, provider, chosen_model, None, "api_key")?;
                        println!(
                            "\n  Setup complete! Using {} with model {}.\n",
                            provider.display_name, chosen_model.id
                        );
                        return Ok(());
                    }
                    _ => {
                        println!("  Existing key validation failed. Please enter a new key.\n");
                    }
                }
            }
        }
    }

    let api_key: String = Password::new()
        .with_prompt(format!("Enter your {} API key", provider.display_name))
        .interact()?;

    if let Err(warning) = validate_api_key_format(&provider.name, &api_key) {
        println!("  Warning: {}", warning);
        println!("  Continuing anyway — the key will be validated against the API.\n");
    }
    cred_store
        .store_key(&provider.name, &api_key)
        .map_err(|e| anyhow::anyhow!("Failed to store API key: {}", e))?;
    println!("  API key stored securely in OS credential store.");

    println!("\n  Validating credentials and fetching available models...");
    let models = list_models(&provider.name, &api_key, None)
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Failed to validate credentials: {}. Please check and try again.",
                e
            )
        })?;

    if models.is_empty() {
        anyhow::bail!("No models found. Please check your API key.");
    }

    println!("  Found {} available model(s).\n", models.len());

    let model_names: Vec<String> = models
        .iter()
        .map(|m| {
            if let Some(ctx) = m.context_window {
                format!("{} ({}k context)", m.id, ctx / 1000)
            } else {
                m.id.clone()
            }
        })
        .collect();
    let model_refs: Vec<&str> = model_names.iter().map(|s| s.as_str()).collect();
    let model_selection = Select::new()
        .with_prompt("Select a model")
        .items(&model_refs)
        .default(0)
        .interact()?;
    let chosen_model = &models[model_selection];

    update_config(workspace, provider, chosen_model, None, "api_key")?;
    println!(
        "\n  Setup complete! Using {} with model {}.\n",
        provider.display_name, chosen_model.id
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_provider_choices_available() {
        let providers = available_providers();
        assert!(providers.len() >= 4);
        assert!(providers.iter().any(|p| p.name == "openai"));
        assert!(providers.iter().any(|p| p.name == "anthropic"));
        assert!(providers.iter().any(|p| p.name == "gemini"));
        assert!(providers.iter().any(|p| p.name == "custom"));
    }

    #[test]
    fn test_validate_api_key_format_openai() {
        assert!(validate_api_key_format("openai", "sk-test123").is_ok());
        assert!(validate_api_key_format("openai", "bad-key").is_err());
        assert!(validate_api_key_format("openai", "").is_err());
        assert!(validate_api_key_format("openai", "   ").is_err());
    }

    #[test]
    fn test_validate_api_key_format_anthropic() {
        assert!(validate_api_key_format("anthropic", "sk-ant-test123").is_ok());
        assert!(validate_api_key_format("anthropic", "sk-wrong").is_err());
        assert!(validate_api_key_format("anthropic", "").is_err());
    }

    #[test]
    fn test_validate_api_key_format_custom() {
        // Custom providers accept any non-empty key
        assert!(validate_api_key_format("custom", "any-key-here").is_ok());
        assert!(validate_api_key_format("custom", "").is_err());
    }

    #[test]
    fn test_config_updated_after_setup() {
        let dir = TempDir::new().unwrap();
        let model = ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            context_window: Some(128_000),
            is_chat_model: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
        };
        let provider = ProviderChoice {
            name: "openai".to_string(),
            display_name: "OpenAI".to_string(),

            api_key_env: "OPENAI_API_KEY".to_string(),
        };

        update_config(dir.path(), &provider, &model, None, "api_key").unwrap();

        let config_path = dir.path().join(".rustant").join("config.toml");
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        let config: rustant_core::AgentConfig = toml::from_str(&content).unwrap();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o");
        assert_eq!(config.llm.context_window, 128_000);
        assert_eq!(config.llm.credential_store_key, Some("openai".to_string()));
    }

    #[test]
    fn test_setup_writes_config_file() {
        let dir = TempDir::new().unwrap();
        let model = ModelInfo {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
            context_window: Some(200_000),
            is_chat_model: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
        };
        let provider = ProviderChoice {
            name: "anthropic".to_string(),
            display_name: "Anthropic (Claude)".to_string(),

            api_key_env: "ANTHROPIC_API_KEY".to_string(),
        };

        update_config(dir.path(), &provider, &model, None, "api_key").unwrap();

        let config_path = dir.path().join(".rustant").join("config.toml");
        assert!(config_path.exists());
        let content = std::fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("anthropic"));
        assert!(content.contains("claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_setup_end_to_end_mock() {
        use rustant_core::credentials::InMemoryCredentialStore;

        let dir = TempDir::new().unwrap();
        let store = InMemoryCredentialStore::new();

        // Simulate: user chose openai, model gpt-4o
        let provider = ProviderChoice {
            name: "openai".to_string(),
            display_name: "OpenAI".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
        };
        let model = ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            context_window: Some(128_000),
            is_chat_model: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
        };

        // Store credential
        store.store_key("openai", "sk-test-e2e").unwrap();

        // Write config
        update_config(dir.path(), &provider, &model, None, "api_key").unwrap();

        // Verify credential stored
        assert_eq!(store.get_key("openai").unwrap(), "sk-test-e2e");

        // Verify config
        let config_path = dir.path().join(".rustant").join("config.toml");
        let config: rustant_core::AgentConfig =
            toml::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o");
        assert_eq!(config.llm.credential_store_key, Some("openai".to_string()));
        assert_eq!(config.llm.context_window, 128_000);
    }

    #[test]
    fn test_update_config_with_custom_base_url() {
        let dir = TempDir::new().unwrap();
        let model = ModelInfo {
            id: "llama-3".to_string(),
            name: "Llama 3".to_string(),
            context_window: None,
            is_chat_model: true,
            input_cost_per_million: None,
            output_cost_per_million: None,
        };
        let provider = ProviderChoice {
            name: "custom".to_string(),
            display_name: "Custom".to_string(),
            api_key_env: "CUSTOM_API_KEY".to_string(),
        };

        update_config(
            dir.path(),
            &provider,
            &model,
            Some("http://localhost:11434/v1"),
            "api_key",
        )
        .unwrap();

        let config_path = dir.path().join(".rustant").join("config.toml");
        let config: rustant_core::AgentConfig =
            toml::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
        // Custom providers use "openai" as the internal provider name
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "llama-3");
        assert_eq!(
            config.llm.base_url,
            Some("http://localhost:11434/v1".to_string())
        );
    }
}
