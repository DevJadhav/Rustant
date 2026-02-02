//! CLI subcommand handlers.

use crate::AuthAction;
use crate::ChannelAction;
use crate::Commands;
use crate::ConfigAction;
use std::path::Path;

/// Handle a CLI subcommand.
pub async fn handle_command(command: Commands, workspace: &Path) -> anyhow::Result<()> {
    match command {
        Commands::Config { action } => handle_config(action, workspace).await,
        Commands::Setup => crate::setup::run_setup(workspace).await,
        Commands::Channel { action } => handle_channel(action, workspace).await,
        Commands::Auth { action } => handle_auth(action, workspace).await,
    }
}

async fn handle_config(action: ConfigAction, workspace: &Path) -> anyhow::Result<()> {
    match action {
        ConfigAction::Init => {
            let config_dir = workspace.join(".rustant");
            std::fs::create_dir_all(&config_dir)?;

            let config_path = config_dir.join("config.toml");
            if config_path.exists() {
                println!(
                    "Configuration file already exists at: {}",
                    config_path.display()
                );
                return Ok(());
            }

            let default_config = rustant_core::AgentConfig::default();
            let toml_str = toml::to_string_pretty(&default_config)?;
            std::fs::write(&config_path, &toml_str)?;
            println!(
                "Created default configuration at: {}",
                config_path.display()
            );
            Ok(())
        }
        ConfigAction::Show => {
            let config = rustant_core::config::load_config(Some(workspace), None)
                .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{}", toml_str);
            Ok(())
        }
    }
}

async fn handle_channel(action: ChannelAction, workspace: &Path) -> anyhow::Result<()> {
    let config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;

    let channels_config = config.channels.unwrap_or_default();

    match action {
        ChannelAction::List => {
            let mgr = rustant_core::channels::build_channel_manager(&channels_config);
            let names = mgr.channel_names();
            if names.is_empty() {
                println!("No channels configured. Add channel configs to your config file.");
            } else {
                println!("Configured channels ({}):", names.len());
                for name in &names {
                    let status = mgr
                        .channel_status(name)
                        .map(|s| format!("{:?}", s))
                        .unwrap_or_else(|| "unknown".to_string());
                    println!("  {} ({})", name, status);
                }
            }
            Ok(())
        }
        ChannelAction::Test { name } => {
            let mut mgr = rustant_core::channels::build_channel_manager(&channels_config);
            let names = mgr.channel_names();
            if !names.contains(&name.as_str()) {
                let available = if names.is_empty() {
                    "none".to_string()
                } else {
                    names.join(", ")
                };
                anyhow::bail!(
                    "Channel '{}' not found in configuration. Available: {}",
                    name,
                    available
                );
            }

            println!("Testing channel '{}'...", name);

            // Connect all (which includes our target)
            let results = mgr.connect_all().await;
            for (ch_name, result) in &results {
                if ch_name == &name {
                    match result {
                        Ok(()) => println!("  Connected successfully!"),
                        Err(e) => {
                            println!("  Connection failed: {}", e);
                            anyhow::bail!("Channel test failed for '{}'", name);
                        }
                    }
                }
            }

            // Disconnect
            mgr.disconnect_all().await;
            println!("  Disconnected. Channel '{}' is working.", name);
            Ok(())
        }
    }
}

async fn handle_auth(action: AuthAction, workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::credentials::KeyringCredentialStore;
    use rustant_core::oauth;

    use rustant_core::credentials::CredentialStore;

    let cred_store = KeyringCredentialStore::new();

    const CHANNEL_PROVIDERS: &[&str] = &["slack", "discord", "teams", "whatsapp", "gmail"];

    match action {
        AuthAction::Status => {
            let llm_providers = ["openai", "gemini", "anthropic"];
            println!("Authentication status:");
            println!();

            println!("  LLM Providers:");
            for provider in &llm_providers {
                let has_api_key = cred_store.has_key(provider);
                let has_oauth = oauth::has_oauth_token(&cred_store, provider);

                if !has_api_key && !has_oauth {
                    println!("    {}: not configured", provider);
                    continue;
                }

                let mut methods = Vec::new();
                if has_api_key {
                    methods.push("API key".to_string());
                }
                if has_oauth {
                    match oauth::load_oauth_token(&cred_store, provider) {
                        Ok(token) => {
                            if oauth::is_token_expired(&token) {
                                methods.push("OAuth (expired)".to_string());
                            } else if let Some(expires_at) = token.expires_at {
                                let remaining = expires_at - chrono::Utc::now();
                                let secs = remaining.num_seconds().max(0);
                                methods.push(format!("OAuth (expires in {}s)", secs));
                            } else {
                                methods.push("OAuth (no expiry)".to_string());
                            }
                        }
                        Err(_) => {
                            methods.push("OAuth (error reading token)".to_string());
                        }
                    }
                }

                println!("    {}: {}", provider, methods.join(", "));
            }

            if !oauth::provider_supports_oauth("anthropic") {
                println!();
                println!("    Note: Anthropic does not support OAuth for third-party tools.");
            }

            println!();
            println!("  Channel Providers:");
            for provider in CHANNEL_PROVIDERS {
                let has_oauth = oauth::has_oauth_token(&cred_store, provider);
                if has_oauth {
                    match oauth::load_oauth_token(&cred_store, provider) {
                        Ok(token) => {
                            if oauth::is_token_expired(&token) {
                                println!("    {}: OAuth (expired)", provider);
                            } else if let Some(expires_at) = token.expires_at {
                                let remaining = expires_at - chrono::Utc::now();
                                let secs = remaining.num_seconds().max(0);
                                println!("    {}: OAuth (expires in {}s)", provider, secs);
                            } else {
                                println!("    {}: OAuth (active)", provider);
                            }
                        }
                        Err(_) => {
                            println!("    {}: OAuth (error reading token)", provider);
                        }
                    }
                } else {
                    println!("    {}: not configured", provider);
                }
            }

            Ok(())
        }

        AuthAction::Login { provider, redirect_uri } => {
            let provider = provider.to_lowercase();
            let is_channel = CHANNEL_PROVIDERS.contains(&provider.as_str());

            let oauth_cfg = oauth::oauth_config_for_provider(&provider).ok_or_else(|| {
                if provider == "anthropic" {
                    anyhow::anyhow!(
                        "Anthropic does not support OAuth for third-party tools. Use an API key instead."
                    )
                } else if is_channel {
                    let env_hint = match provider.as_str() {
                        "slack" => "SLACK_CLIENT_ID and SLACK_CLIENT_SECRET",
                        "discord" => "DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET",
                        "teams" => "TEAMS_CLIENT_ID and TEAMS_CLIENT_SECRET",
                        "whatsapp" => "WHATSAPP_APP_ID and WHATSAPP_APP_SECRET",
                        "gmail" => "GMAIL_OAUTH_CLIENT_ID and GMAIL_OAUTH_CLIENT_SECRET",
                        _ => "the required environment variables",
                    };
                    anyhow::anyhow!(
                        "OAuth for '{}' requires environment variables: {}\n\
                         Set these from your app's developer console and try again.",
                        provider, env_hint
                    )
                } else {
                    anyhow::anyhow!(
                        "Unknown or unsupported provider '{}'. Supported: openai, gemini, slack, discord, teams, whatsapp, gmail",
                        provider
                    )
                }
            })?;

            println!("Starting OAuth login for {}...", provider);
            let effective_redirect = match &redirect_uri {
                Some(uri) => uri.clone(),
                None => format!(
                    "https://localhost:{}/auth/callback",
                    rustant_core::oauth::OAUTH_CALLBACK_PORT
                ),
            };
            println!("Redirect URI: {}", effective_redirect);
            println!("(Make sure this URI is registered in your {} app settings)", provider);
            println!();
            println!("Opening your browser for authentication...");

            let token = oauth::authorize_browser_flow(
                &oauth_cfg,
                redirect_uri.as_deref(),
            )
                .await
                .map_err(|e| anyhow::anyhow!("OAuth login failed: {}", e))?;

            oauth::store_oauth_token(&cred_store, &provider, &token)
                .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {}", e))?;

            println!("Successfully authenticated with {}.", provider);

            if let Some(expires_at) = token.expires_at {
                let remaining = expires_at - chrono::Utc::now();
                println!("Token expires in {}s.", remaining.num_seconds().max(0));
            }

            if is_channel {
                println!(
                    "Tip: Add {} to your channel config with auth_method = \"oauth\" to use this token.",
                    provider
                );
            } else {
                // Update config to use OAuth auth method
                let config = rustant_core::config::load_config(Some(workspace), None)
                    .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
                if config.llm.provider == provider && config.llm.auth_method != "oauth" {
                    println!(
                        "Tip: Run `rustant setup` or set auth_method = \"oauth\" in your config to use OAuth."
                    );
                }
            }

            Ok(())
        }

        AuthAction::Logout { provider } => {
            let provider = provider.to_lowercase();

            if oauth::has_oauth_token(&cred_store, &provider) {
                oauth::delete_oauth_token(&cred_store, &provider)
                    .map_err(|e| anyhow::anyhow!("Failed to delete OAuth token: {}", e))?;
                println!("OAuth token removed for {}.", provider);
            } else {
                println!("No OAuth token found for {}.", provider);
            }

            Ok(())
        }

        AuthAction::Refresh { provider } => {
            let provider = provider.to_lowercase();

            let token = oauth::load_oauth_token(&cred_store, &provider)
                .map_err(|e| anyhow::anyhow!("No OAuth token found for {}: {}", provider, e))?;

            let refresh_token_str = token.refresh_token.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "No refresh token available for {}. Re-login with `rustant auth login {}`.",
                    provider,
                    provider
                )
            })?;

            let oauth_cfg = oauth::oauth_config_for_provider(&provider).ok_or_else(|| {
                anyhow::anyhow!("No OAuth configuration available for '{}'", provider)
            })?;

            println!("Refreshing token for {}...", provider);

            let new_token = oauth::refresh_token(&oauth_cfg, refresh_token_str)
                .await
                .map_err(|e| anyhow::anyhow!("Token refresh failed: {}", e))?;

            oauth::store_oauth_token(&cred_store, &provider, &new_token)
                .map_err(|e| anyhow::anyhow!("Failed to store refreshed token: {}", e))?;

            println!("Token refreshed successfully for {}.", provider);

            if let Some(expires_at) = new_token.expires_at {
                let remaining = expires_at - chrono::Utc::now();
                println!("New token expires in {}s.", remaining.num_seconds().max(0));
            }

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_init_creates_file() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let config_path = workspace.join(".rustant").join("config.toml");
        assert!(config_path.exists());

        // Verify it's valid TOML
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: rustant_core::AgentConfig = toml::from_str(&content).unwrap();
        assert_eq!(parsed.llm.model, "gpt-4o");
        assert_eq!(
            parsed.safety.approval_mode,
            rustant_core::ApprovalMode::Safe
        );
    }

    #[tokio::test]
    async fn test_config_init_idempotent() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // First init
        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let config_path = workspace.join(".rustant").join("config.toml");
        let content_first = std::fs::read_to_string(&config_path).unwrap();

        // Second init should not overwrite
        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let content_second = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content_first, content_second);
    }

    #[tokio::test]
    async fn test_config_show_defaults() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // Show should work even without a config file (uses defaults)
        let command = Commands::Config {
            action: ConfigAction::Show,
        };
        let result = handle_command(command, workspace).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_show_after_init() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // Init first
        let init_cmd = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(init_cmd, workspace).await.unwrap();

        // Show should work with the config file present
        let show_cmd = Commands::Config {
            action: ConfigAction::Show,
        };
        let result = handle_command(show_cmd, workspace).await;
        assert!(result.is_ok());
    }
}
