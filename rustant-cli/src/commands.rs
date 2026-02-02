//! CLI subcommand handlers.

use crate::AuthAction;
use crate::ChannelAction;
use crate::Commands;
use crate::ConfigAction;
use crate::SlackCommand;
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
        ChannelAction::Slack { action } => {
            return handle_slack(action).await;
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

/// Load the Slack OAuth token from the keyring and create a RealSlackHttp client.
fn load_slack_client() -> anyhow::Result<rustant_core::channels::slack::RealSlackHttp> {
    use rustant_core::credentials::KeyringCredentialStore;
    use rustant_core::oauth;

    let store = KeyringCredentialStore::new();
    let token = oauth::load_oauth_token(&store, "slack")
        .map_err(|e| anyhow::anyhow!("No Slack OAuth token found. Run `rustant auth login slack` first.\n{}", e))?;
    Ok(rustant_core::channels::slack::RealSlackHttp::new(
        token.access_token,
    ))
}

async fn handle_slack(action: SlackCommand) -> anyhow::Result<()> {
    use rustant_core::channels::slack::SlackHttpClient;

    let http = load_slack_client()?;

    match action {
        SlackCommand::Send { channel, message } => {
            let ts = http.post_message(&channel, &message).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Message sent (ts: {})", ts);
        }

        SlackCommand::History { channel, limit } => {
            let messages = http.conversations_history(&channel, limit).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if messages.is_empty() {
                println!("No messages found.");
            } else {
                for msg in messages.iter().rev() {
                    let thread = msg.thread_ts.as_deref().map(|t| format!(" [thread:{}]", t)).unwrap_or_default();
                    println!("[{}] {}: {}{}", &msg.ts, msg.user, msg.text, thread);
                }
            }
        }

        SlackCommand::Channels => {
            let channels = http.conversations_list("public_channel,private_channel", 200).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if channels.is_empty() {
                println!("No channels found.");
            } else {
                println!("{:<14} {:<25} {:>5}  {:<6}  {}", "ID", "Name", "Users", "Member", "Topic");
                println!("{}", "-".repeat(75));
                for ch in &channels {
                    let private = if ch.is_private { "priv" } else { "pub" };
                    let member = if ch.is_member { "yes" } else { "no" };
                    let topic = if ch.topic.len() > 30 {
                        format!("{}...", &ch.topic[..27])
                    } else {
                        ch.topic.clone()
                    };
                    println!(
                        "{:<14} #{:<24} {:>5}  {:<6}  {} {}",
                        ch.id, ch.name, ch.num_members, member, private, topic
                    );
                }
                println!("\nTotal: {} channels", channels.len());
            }
        }

        SlackCommand::Users => {
            let users = http.users_list(200).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if users.is_empty() {
                println!("No users found.");
            } else {
                println!("{:<14} {:<20} {:<25} {:<6} {}", "ID", "Username", "Real Name", "Admin", "Status");
                println!("{}", "-".repeat(80));
                for u in &users {
                    let kind = if u.is_bot { " [bot]" } else { "" };
                    let admin = if u.is_admin { "yes" } else { "" };
                    let status = if !u.status_emoji.is_empty() || !u.status_text.is_empty() {
                        format!("{} {}", u.status_emoji, u.status_text).trim().to_string()
                    } else {
                        String::new()
                    };
                    println!(
                        "{:<14} {:<20} {:<25} {:<6} {}{}",
                        u.id, u.name, u.real_name, admin, status, kind
                    );
                }
                println!("\nTotal: {} users", users.len());
            }
        }

        SlackCommand::Info { channel } => {
            let info = http.conversations_info(&channel).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Channel: #{}", info.name);
            println!("ID:      {}", info.id);
            println!("Private: {}", info.is_private);
            println!("Member:  {}", info.is_member);
            println!("Members: {}", info.num_members);
            if !info.topic.is_empty() {
                println!("Topic:   {}", info.topic);
            }
            if !info.purpose.is_empty() {
                println!("Purpose: {}", info.purpose);
            }
        }

        SlackCommand::React { channel, timestamp, emoji } => {
            http.reactions_add(&channel, &timestamp, &emoji).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Reaction :{}:  added.", emoji);
        }

        SlackCommand::Files { channel } => {
            let files = http.files_list(channel.as_deref(), 100).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if files.is_empty() {
                println!("No files found.");
            } else {
                println!("{:<14} {:<30} {:<8} {:>10} {}", "ID", "Name", "Type", "Size", "User");
                println!("{}", "-".repeat(75));
                for f in &files {
                    let size = if f.size >= 1_048_576 {
                        format!("{:.1} MB", f.size as f64 / 1_048_576.0)
                    } else if f.size >= 1024 {
                        format!("{:.1} KB", f.size as f64 / 1024.0)
                    } else {
                        format!("{} B", f.size)
                    };
                    let name = if f.name.len() > 28 {
                        format!("{}...", &f.name[..25])
                    } else {
                        f.name.clone()
                    };
                    println!("{:<14} {:<30} {:<8} {:>10} {}", f.id, name, f.filetype, size, f.user);
                }
                println!("\nTotal: {} files", files.len());
            }
        }

        SlackCommand::Team => {
            let team = http.team_info().await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Workspace: {}", team.name);
            println!("ID:        {}", team.id);
            println!("Domain:    {}.slack.com", team.domain);
            if let Some(icon) = &team.icon_url {
                println!("Icon:      {}", icon);
            }
        }

        SlackCommand::Groups => {
            let groups = http.usergroups_list().await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if groups.is_empty() {
                println!("No user groups found.");
            } else {
                println!("{:<14} {:<20} {:<15} {:>6}  {}", "ID", "Name", "@Handle", "Users", "Description");
                println!("{}", "-".repeat(75));
                for g in &groups {
                    let desc = if g.description.len() > 25 {
                        format!("{}...", &g.description[..22])
                    } else {
                        g.description.clone()
                    };
                    println!(
                        "{:<14} {:<20} @{:<14} {:>5}  {}",
                        g.id, g.name, g.handle, g.user_count, desc
                    );
                }
                println!("\nTotal: {} groups", groups.len());
            }
        }

        SlackCommand::Dm { user, message } => {
            // Open a DM conversation, then send the message
            let dm_channel = http.conversations_open(&[user.as_str()]).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let ts = http.post_message(&dm_channel, &message).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("DM sent to {} (channel: {}, ts: {})", user, dm_channel, ts);
        }

        SlackCommand::Thread { channel, timestamp, message } => {
            let ts = http.post_thread_reply(&channel, &timestamp, &message).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Thread reply sent (ts: {})", ts);
        }

        SlackCommand::Join { channel } => {
            http.conversations_join(&channel).await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Joined channel {}", channel);
        }
    }

    Ok(())
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
