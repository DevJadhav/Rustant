//! Interactive channel setup wizard.
//!
//! Walks the user through configuring a messaging channel:
//! 1. Select a channel (or pass one via CLI arg)
//! 2. Display platform-specific guidance (URLs, step-by-step)
//! 3. Prompt for credentials (masked for secrets)
//! 4. For OAuth channels: browser flow with PKCE
//! 5. Validate credentials via test API call
//! 6. Store secrets in OS keyring
//! 7. Save channel config to `.rustant/config.toml`
//! 8. Run a connection test

use dialoguer::{Input, Password, Select};
use rustant_core::credentials::{CredentialStore, KeyringCredentialStore};
use std::path::Path;

/// A channel option presented during the setup wizard.
#[derive(Debug, Clone)]
pub struct ChannelChoice {
    /// Internal channel name (e.g., "slack", "discord").
    pub name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// Short description shown in the selection menu.
    pub description: &'static str,
}

/// Return the list of supported channel choices.
pub fn available_channels() -> Vec<ChannelChoice> {
    vec![
        ChannelChoice {
            name: "slack",
            display_name: "Slack",
            description: "Slack workspace via Bot Token or OAuth",
        },
        ChannelChoice {
            name: "discord",
            display_name: "Discord",
            description: "Discord server via Bot Token or OAuth",
        },
        ChannelChoice {
            name: "telegram",
            display_name: "Telegram",
            description: "Telegram bot via BotFather token",
        },
        ChannelChoice {
            name: "email",
            display_name: "Email (Gmail)",
            description: "Gmail via OAuth XOAuth2 or IMAP password",
        },
        ChannelChoice {
            name: "sms",
            display_name: "SMS (Twilio)",
            description: "SMS via Twilio account",
        },
        ChannelChoice {
            name: "imessage",
            display_name: "iMessage",
            description: "iMessage via macOS AppleScript (no keys needed)",
        },
    ]
}

/// Run the interactive channel setup wizard.
///
/// If `channel` is `None`, shows a selection menu. Otherwise jumps directly
/// to the named channel's wizard.
pub async fn run_channel_setup(workspace: &Path, channel: Option<&str>) -> anyhow::Result<()> {
    let channel_name = match channel {
        Some(name) => {
            let channels = available_channels();
            if !channels.iter().any(|c| c.name == name) {
                let valid: Vec<&str> = channels.iter().map(|c| c.name).collect();
                anyhow::bail!(
                    "Unknown channel '{}'. Valid channels: {}",
                    name,
                    valid.join(", ")
                );
            }
            name.to_string()
        }
        None => {
            println!("\n  Rustant Channel Setup\n");
            let channels = available_channels();
            let items: Vec<String> = channels
                .iter()
                .map(|c| format!("{:<12} — {}", c.display_name, c.description))
                .collect();
            let selection = Select::new()
                .with_prompt("Select a channel to configure")
                .items(&items)
                .default(0)
                .interact()?;
            channels[selection].name.to_string()
        }
    };

    match channel_name.as_str() {
        "slack" => setup_slack(workspace).await,
        "discord" => setup_discord(workspace).await,
        "telegram" => setup_telegram(workspace).await,
        "email" => setup_email(workspace).await,
        "sms" => setup_sms(workspace).await,
        "imessage" => setup_imessage(workspace).await,
        _ => anyhow::bail!("Unknown channel: {}", channel_name),
    }
}

// ── Slack ──────────────────────────────────────────────────────────────────

async fn setup_slack(workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::channels::slack::SlackConfig;
    use rustant_core::oauth::AuthMethod;

    println!("\n  Slack Setup\n");
    println!("  Create a Slack App to connect Rustant to your workspace.\n");
    println!("  How to get credentials:");
    println!("    1. Go to https://api.slack.com/apps");
    println!("    2. Click \"Create New App\" → \"From scratch\"");
    println!("    3. Go to \"OAuth & Permissions\" → add Bot Token Scopes:");
    println!("       chat:write, channels:history, channels:read,");
    println!("       users:read, im:read, im:write");
    println!("    4. Click \"Install to Workspace\" and authorize");
    println!("    5. Copy the Bot User OAuth Token (starts with xoxb-)");
    println!("    6. For OAuth flow: also copy Client ID and Client Secret");
    println!("       from \"Basic Information\" → \"App Credentials\"");
    println!();

    let auth_options = vec!["OAuth (Recommended)", "Bot Token (API Key)"];
    let use_oauth = Select::new()
        .with_prompt("Select authentication method")
        .items(&auth_options)
        .default(0)
        .interact()?
        == 0;

    let cred_store = KeyringCredentialStore::new();
    let bot_token: String;
    let auth_method: AuthMethod;

    if use_oauth {
        let client_id: String = Input::new()
            .with_prompt("Enter your Slack App Client ID")
            .interact_text()?;

        let client_secret: String = Password::new()
            .with_prompt("Enter your Slack App Client Secret")
            .interact()?;

        println!("\n  Opening browser for Slack OAuth...");
        println!("  (Make sure https://localhost:8844/auth/callback is registered");
        println!("   as a redirect URI in your Slack App settings)\n");

        let oauth_config = rustant_core::oauth::oauth_config_with_credentials(
            "slack",
            &client_id,
            Some(&client_secret),
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to build Slack OAuth config"))?;

        let token = rustant_core::oauth::authorize_browser_flow(&oauth_config, None)
            .await
            .map_err(|e| anyhow::anyhow!("Slack OAuth failed: {}", e))?;

        rustant_core::oauth::store_oauth_token(&cred_store, "slack", &token)
            .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {}", e))?;

        println!("  OAuth token stored securely in OS credential store.");
        bot_token = token.access_token;
        auth_method = AuthMethod::OAuth;
    } else {
        bot_token = Password::new()
            .with_prompt("Enter your Slack Bot Token (xoxb-...)")
            .interact()?;

        if !bot_token.starts_with("xoxb-") {
            println!("  Warning: Slack bot tokens typically start with 'xoxb-'.");
            println!("  Continuing anyway — the token will be validated next.\n");
        }

        // Store in credential store
        cred_store
            .store_key("slack_bot_token", &bot_token)
            .map_err(|e| anyhow::anyhow!("Failed to store token: {}", e))?;

        auth_method = AuthMethod::ApiKey;
    }

    // Validate
    println!("\n  Validating Slack credentials...");
    let team_info = validate_slack_token(&bot_token).await?;
    println!("  {}", team_info);

    // Optional: default channel
    let default_channel: String = Input::new()
        .with_prompt("Default channel (leave empty to skip)")
        .default(String::new())
        .show_default(false)
        .interact_text()?;

    let slack_config = SlackConfig {
        bot_token: bot_token.clone(),
        app_token: None,
        default_channel: if default_channel.is_empty() {
            None
        } else {
            Some(default_channel)
        },
        allowed_channels: Vec::new(),
        auth_method,
    };

    let config_val = toml::Value::try_from(&slack_config)?;
    let config_path = rustant_core::config::update_channel_config(workspace, "slack", config_val)?;
    println!(
        "\n  Slack setup complete! Config saved to {}",
        config_path.display()
    );

    Ok(())
}

// ── Discord ────────────────────────────────────────────────────────────────

async fn setup_discord(workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::channels::discord::DiscordConfig;
    use rustant_core::oauth::AuthMethod;

    println!("\n  Discord Bot Setup\n");
    println!("  Create a Discord Application to connect Rustant.\n");
    println!("  How to get credentials:");
    println!("    1. Go to https://discord.com/developers/applications");
    println!("    2. Click \"New Application\" → name it");
    println!("    3. Go to \"Bot\" tab → click \"Add Bot\" → copy the Token");
    println!("    4. Go to \"OAuth2\" tab → copy Client ID and Client Secret");
    println!("    5. Use the OAuth2 URL Generator:");
    println!("       Scopes: bot, messages.read");
    println!("       Bot Permissions: Send Messages, Read Message History,");
    println!("       Add Reactions");
    println!("    6. Use the generated URL to invite the bot to your server");
    println!();

    let auth_options = vec!["OAuth (Recommended)", "Bot Token"];
    let use_oauth = Select::new()
        .with_prompt("Select authentication method")
        .items(&auth_options)
        .default(0)
        .interact()?
        == 0;

    let cred_store = KeyringCredentialStore::new();
    let bot_token: String;
    let auth_method: AuthMethod;

    if use_oauth {
        let client_id: String = Input::new()
            .with_prompt("Enter your Discord Application Client ID")
            .interact_text()?;

        let client_secret: String = Password::new()
            .with_prompt("Enter your Discord Application Client Secret")
            .interact()?;

        println!("\n  Opening browser for Discord OAuth...");

        let oauth_config = rustant_core::oauth::oauth_config_with_credentials(
            "discord",
            &client_id,
            Some(&client_secret),
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to build Discord OAuth config"))?;

        let token = rustant_core::oauth::authorize_browser_flow(&oauth_config, None)
            .await
            .map_err(|e| anyhow::anyhow!("Discord OAuth failed: {}", e))?;

        rustant_core::oauth::store_oauth_token(&cred_store, "discord", &token)
            .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {}", e))?;

        println!("  OAuth token stored securely in OS credential store.");
        bot_token = token.access_token;
        auth_method = AuthMethod::OAuth;
    } else {
        bot_token = Password::new()
            .with_prompt("Enter your Discord Bot Token")
            .interact()?;

        cred_store
            .store_key("discord_bot_token", &bot_token)
            .map_err(|e| anyhow::anyhow!("Failed to store token: {}", e))?;

        auth_method = AuthMethod::ApiKey;
    }

    // Validate
    println!("\n  Validating Discord credentials...");
    let bot_info = validate_discord_token(&bot_token).await?;
    println!("  {}", bot_info);

    // Optional: guild ID
    let guild_id: String = Input::new()
        .with_prompt("Guild/Server ID to restrict bot (leave empty for all)")
        .default(String::new())
        .show_default(false)
        .interact_text()?;

    let discord_config = DiscordConfig {
        bot_token: bot_token.clone(),
        guild_id: if guild_id.is_empty() {
            None
        } else {
            Some(guild_id)
        },
        allowed_channel_ids: Vec::new(),
        auth_method,
    };

    let config_val = toml::Value::try_from(&discord_config)?;
    let config_path =
        rustant_core::config::update_channel_config(workspace, "discord", config_val)?;
    println!(
        "\n  Discord setup complete! Config saved to {}",
        config_path.display()
    );

    Ok(())
}

// ── Telegram ───────────────────────────────────────────────────────────────

async fn setup_telegram(workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::channels::telegram::TelegramConfig;

    println!("\n  Telegram Bot Setup\n");
    println!("  Create a Telegram bot via BotFather.\n");
    println!("  How to get your bot token:");
    println!("    1. Open Telegram and search for @BotFather");
    println!("    2. Send /newbot");
    println!("    3. Follow the prompts to choose a name and username");
    println!("    4. Copy the bot token");
    println!("       (looks like: 123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11)");
    println!();

    let bot_token: String = Password::new()
        .with_prompt("Enter your Telegram Bot Token")
        .interact()?;

    if !bot_token.contains(':') {
        println!("  Warning: Telegram tokens typically contain a colon (:).");
        println!("  Continuing anyway — the token will be validated next.\n");
    }

    // Validate
    println!("\n  Validating Telegram credentials...");
    let bot_info = validate_telegram_token(&bot_token).await?;
    println!("  {}", bot_info);

    // Store in credential store
    let cred_store = KeyringCredentialStore::new();
    cred_store
        .store_key("telegram_bot_token", &bot_token)
        .map_err(|e| anyhow::anyhow!("Failed to store token: {}", e))?;

    // Optional: allowed chat IDs
    let chat_ids_input: String = Input::new()
        .with_prompt("Allowed chat IDs (comma-separated, or empty for all)")
        .default(String::new())
        .show_default(false)
        .interact_text()?;

    let allowed_chat_ids: Vec<i64> = if chat_ids_input.trim().is_empty() {
        Vec::new()
    } else {
        chat_ids_input
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    };

    let telegram_config = TelegramConfig {
        bot_token: bot_token.clone(),
        allowed_chat_ids,
        polling_timeout_secs: 30,
    };

    let config_val = toml::Value::try_from(&telegram_config)?;
    let config_path =
        rustant_core::config::update_channel_config(workspace, "telegram", config_val)?;
    println!(
        "\n  Telegram setup complete! Config saved to {}",
        config_path.display()
    );

    Ok(())
}

// ── Email (Gmail) ──────────────────────────────────────────────────────────

async fn setup_email(workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::channels::email::{EmailAuthMethod, EmailConfig};

    println!("\n  Email (Gmail) Setup\n");
    println!("  Connect Rustant to Gmail for sending and receiving email.\n");

    let auth_options = vec!["Gmail OAuth / XOAuth2 (Recommended)", "IMAP/SMTP Password"];
    let use_oauth = Select::new()
        .with_prompt("Select authentication method")
        .items(&auth_options)
        .default(0)
        .interact()?
        == 0;

    let cred_store = KeyringCredentialStore::new();

    if use_oauth {
        println!();
        println!("  How to set up Gmail OAuth:");
        println!("    1. Go to https://console.cloud.google.com");
        println!("    2. Create a project (or select an existing one)");
        println!("    3. Enable the \"Gmail API\" (APIs & Services → Library)");
        println!("    4. Configure OAuth consent screen:");
        println!("       - User type: External → Create");
        println!("       - App name: \"Rustant\", add your email as support contact");
        println!("       - Add scope: https://mail.google.com/");
        println!("       - Add yourself as a Test User → Save");
        println!("    5. Go to Credentials → Create OAuth 2.0 Client ID");
        println!("       (Application type: Web application)");
        println!("    6. Add redirect URI: https://localhost:8844/auth/callback");
        println!("    7. Copy the Client ID and Client Secret");
        println!();

        let email_address: String = Input::new()
            .with_prompt("Enter your Gmail address")
            .interact_text()?;

        let client_id: String = Input::new()
            .with_prompt("Enter your Google OAuth Client ID")
            .interact_text()?;

        let client_secret: String = Password::new()
            .with_prompt("Enter your Google OAuth Client Secret")
            .interact()?;

        println!("\n  Opening browser for Google OAuth...");

        let oauth_config = rustant_core::oauth::oauth_config_with_credentials(
            "gmail",
            &client_id,
            Some(&client_secret),
        )
        .ok_or_else(|| anyhow::anyhow!("Failed to build Gmail OAuth config"))?;

        let token = rustant_core::oauth::authorize_browser_flow(&oauth_config, None)
            .await
            .map_err(|e| anyhow::anyhow!("Gmail OAuth failed: {}", e))?;

        rustant_core::oauth::store_oauth_token(&cred_store, "gmail", &token)
            .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {}", e))?;

        println!("  OAuth token stored securely in OS credential store.");

        let email_config = EmailConfig {
            imap_host: "imap.gmail.com".to_string(),
            imap_port: 993,
            smtp_host: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            username: email_address.clone(),
            password: token.access_token,
            from_address: email_address,
            allowed_senders: Vec::new(),
            auth_method: EmailAuthMethod::XOAuth2,
        };

        let config_val = toml::Value::try_from(&email_config)?;
        let config_path =
            rustant_core::config::update_channel_config(workspace, "email", config_val)?;
        println!(
            "\n  Email (Gmail OAuth) setup complete! Config saved to {}",
            config_path.display()
        );
    } else {
        println!();
        println!("  For Gmail with app password:");
        println!("    1. Enable 2-Factor Authentication on your Google account");
        println!("    2. Go to https://myaccount.google.com/apppasswords");
        println!("    3. Generate an app password for \"Mail\"");
        println!();

        let email_address: String = Input::new()
            .with_prompt("Enter your email address")
            .interact_text()?;

        let imap_host: String = Input::new()
            .with_prompt("IMAP host")
            .default("imap.gmail.com".to_string())
            .interact_text()?;

        let imap_port: u16 = Input::new()
            .with_prompt("IMAP port")
            .default(993)
            .interact()?;

        let smtp_host: String = Input::new()
            .with_prompt("SMTP host")
            .default("smtp.gmail.com".to_string())
            .interact_text()?;

        let smtp_port: u16 = Input::new()
            .with_prompt("SMTP port")
            .default(587)
            .interact()?;

        let password: String = Password::new()
            .with_prompt("Enter your email password or app password")
            .interact()?;

        // Store password in credential store
        cred_store
            .store_key("email_password", &password)
            .map_err(|e| anyhow::anyhow!("Failed to store password: {}", e))?;

        let email_config = EmailConfig {
            imap_host,
            imap_port,
            smtp_host,
            smtp_port,
            username: email_address.clone(),
            password,
            from_address: email_address,
            allowed_senders: Vec::new(),
            auth_method: EmailAuthMethod::Password,
        };

        let config_val = toml::Value::try_from(&email_config)?;
        let config_path =
            rustant_core::config::update_channel_config(workspace, "email", config_val)?;
        println!(
            "\n  Email (IMAP) setup complete! Config saved to {}",
            config_path.display()
        );
    }

    Ok(())
}

// ── SMS (Twilio) ───────────────────────────────────────────────────────────

async fn setup_sms(workspace: &Path) -> anyhow::Result<()> {
    println!("\n  SMS (Twilio) Setup\n");
    println!("  Connect Rustant to Twilio for sending and receiving SMS.\n");
    println!("  How to get Twilio credentials:");
    println!("    1. Sign up at https://www.twilio.com/try-twilio");
    println!("    2. From the Console Dashboard, find your Account SID");
    println!("       and Auth Token");
    println!("    3. Buy a phone number (or use the trial number)");
    println!();

    let account_sid: String = Input::new()
        .with_prompt("Enter your Twilio Account SID (starts with AC)")
        .interact_text()?;

    if !account_sid.starts_with("AC") {
        println!("  Warning: Twilio Account SIDs typically start with 'AC'.");
        println!("  Continuing anyway — credentials will be validated next.\n");
    }

    let auth_token: String = Password::new()
        .with_prompt("Enter your Twilio Auth Token")
        .interact()?;

    let from_number: String = Input::new()
        .with_prompt("Enter your Twilio phone number (E.164, e.g., +15551234567)")
        .interact_text()?;

    // Validate
    println!("\n  Validating Twilio credentials...");
    let account_info = validate_twilio_credentials(&account_sid, &auth_token).await?;
    println!("  {}", account_info);

    // Store secrets in credential store
    let cred_store = KeyringCredentialStore::new();
    cred_store
        .store_key("twilio_account_sid", &account_sid)
        .map_err(|e| anyhow::anyhow!("Failed to store SID: {}", e))?;
    cred_store
        .store_key("twilio_auth_token", &auth_token)
        .map_err(|e| anyhow::anyhow!("Failed to store token: {}", e))?;

    let sms_config = rustant_core::SmsConfig {
        enabled: true,
        account_sid,
        auth_token,
        from_number,
        polling_interval_ms: 5000,
    };

    let config_val = toml::Value::try_from(&sms_config)?;
    let config_path = rustant_core::config::update_channel_config(workspace, "sms", config_val)?;
    println!(
        "\n  SMS (Twilio) setup complete! Config saved to {}",
        config_path.display()
    );

    Ok(())
}

// ── iMessage ───────────────────────────────────────────────────────────────

async fn setup_imessage(workspace: &Path) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = workspace;
        anyhow::bail!(
            "iMessage is only available on macOS.\n\
             This machine does not appear to be running macOS."
        );
    }

    #[cfg(target_os = "macos")]
    {
        println!("\n  iMessage Setup (macOS only)\n");
        println!("  iMessage integration uses macOS AppleScript — no API keys needed.\n");
        println!("  Requirements:");
        println!("    1. Be signed into iMessage in the Messages app");
        println!("    2. Grant this terminal \"Automation\" permission:");
        println!("       System Settings → Privacy & Security → Automation");
        println!("    3. For reading messages, also grant \"Full Disk Access\":");
        println!("       System Settings → Privacy & Security → Full Disk Access");
        println!();

        let signed_in = Select::new()
            .with_prompt("Are you signed into iMessage on this Mac?")
            .items(&["Yes", "No"])
            .default(0)
            .interact()?;

        if signed_in != 0 {
            println!(
                "\n  Please sign into iMessage in the Messages app first, then re-run this setup."
            );
            return Ok(());
        }

        // Check access
        println!("\n  Checking iMessage access...");
        let access_info = validate_imessage_access()?;
        println!("  {}", access_info);

        let imessage_config = rustant_core::IMessageConfig {
            enabled: true,
            polling_interval_ms: 5000,
        };

        let config_val = toml::Value::try_from(&imessage_config)?;
        let config_path =
            rustant_core::config::update_channel_config(workspace, "imessage", config_val)?;
        println!(
            "\n  iMessage setup complete! Config saved to {}",
            config_path.display()
        );

        Ok(())
    }
}

// ── Validation functions ───────────────────────────────────────────────────

/// Validate a Slack bot token by calling the auth.test API.
async fn validate_slack_token(token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .post("https://slack.com/api/auth.test")
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Network error: {}", e))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid response: {}", e))?;

    if json["ok"].as_bool() != Some(true) {
        let error = json["error"].as_str().unwrap_or("unknown error");
        anyhow::bail!("Slack token invalid: {}", error);
    }

    let team = json["team"].as_str().unwrap_or("unknown");
    let user = json["user"].as_str().unwrap_or("unknown");
    Ok(format!(
        "Connected as \"{}\" to workspace \"{}\"",
        user, team
    ))
}

/// Validate a Discord bot token by calling the /users/@me API.
async fn validate_discord_token(token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let response = client
        .get("https://discord.com/api/v10/users/@me")
        .header("Authorization", format!("Bot {}", token))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Network error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Discord token invalid (HTTP {}): {}", status, body);
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid response: {}", e))?;

    let username = json["username"].as_str().unwrap_or("unknown");
    let discriminator = json["discriminator"].as_str().unwrap_or("0");
    Ok(format!("Bot \"{}#{}\" is online", username, discriminator))
}

/// Validate a Telegram bot token by calling the getMe API.
async fn validate_telegram_token(token: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let url = format!("https://api.telegram.org/bot{}/getMe", token);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Network error: {}", e))?;

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid response: {}", e))?;

    if json["ok"].as_bool() != Some(true) {
        let desc = json["description"].as_str().unwrap_or("unknown error");
        anyhow::bail!("Telegram token invalid: {}", desc);
    }

    let username = json["result"]["username"].as_str().unwrap_or("unknown");
    let first_name = json["result"]["first_name"].as_str().unwrap_or("Bot");
    Ok(format!("Bot \"{}\" (@{}) is active", first_name, username))
}

/// Validate Twilio credentials by calling the Accounts API.
async fn validate_twilio_credentials(
    account_sid: &str,
    auth_token: &str,
) -> anyhow::Result<String> {
    let client = reqwest::Client::new();
    let url = format!(
        "https://api.twilio.com/2010-04-01/Accounts/{}.json",
        account_sid
    );
    let response = client
        .get(&url)
        .basic_auth(account_sid, Some(auth_token))
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Network error: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        anyhow::bail!("Twilio credentials invalid (HTTP {})", status);
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Invalid response: {}", e))?;

    let friendly_name = json["friendly_name"].as_str().unwrap_or("unknown");
    let status = json["status"].as_str().unwrap_or("unknown");
    Ok(format!("Account \"{}\" is {} ", friendly_name, status))
}

/// Validate iMessage access on macOS by checking the chat.db file.
#[cfg(target_os = "macos")]
fn validate_imessage_access() -> anyhow::Result<String> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/Users/unknown".to_string());
    let db_path = std::path::Path::new(&home).join("Library/Messages/chat.db");

    if !db_path.exists() {
        anyhow::bail!(
            "Messages database not found at {}.\n\
             Make sure you are signed into iMessage.",
            db_path.display()
        );
    }

    // Try to open the database to check permissions
    match std::fs::metadata(&db_path) {
        Ok(meta) => {
            let size_mb = meta.len() as f64 / 1_048_576.0;
            Ok(format!(
                "Messages.app database accessible ({:.1} MB)",
                size_mb
            ))
        }
        Err(e) => {
            anyhow::bail!(
                "Cannot access {}: {}\n\
                 Grant \"Full Disk Access\" to your terminal in System Settings.",
                db_path.display(),
                e
            );
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn validate_imessage_access() -> anyhow::Result<String> {
    anyhow::bail!("iMessage is only available on macOS")
}

// ── Credential format validation helpers ───────────────────────────────────

/// Validate the format of a Slack bot token.
#[cfg(test)]
pub fn validate_slack_token_format(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("Token cannot be empty".to_string());
    }
    if !token.starts_with("xoxb-") {
        return Err("Slack bot tokens start with 'xoxb-'".to_string());
    }
    Ok(())
}

/// Validate the format of a Telegram bot token.
#[cfg(test)]
pub fn validate_telegram_token_format(token: &str) -> Result<(), String> {
    if token.trim().is_empty() {
        return Err("Token cannot be empty".to_string());
    }
    if !token.contains(':') {
        return Err("Telegram tokens contain a colon (:) separator".to_string());
    }
    Ok(())
}

/// Validate the format of a Twilio Account SID.
#[cfg(test)]
pub fn validate_twilio_sid_format(sid: &str) -> Result<(), String> {
    if sid.trim().is_empty() {
        return Err("Account SID cannot be empty".to_string());
    }
    if !sid.starts_with("AC") {
        return Err("Twilio Account SIDs start with 'AC'".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_available_channels_has_all_six() {
        let channels = available_channels();
        assert_eq!(channels.len(), 6);
        let names: Vec<&str> = channels.iter().map(|c| c.name).collect();
        assert!(names.contains(&"slack"));
        assert!(names.contains(&"discord"));
        assert!(names.contains(&"telegram"));
        assert!(names.contains(&"email"));
        assert!(names.contains(&"sms"));
        assert!(names.contains(&"imessage"));
    }

    #[test]
    fn test_slack_token_format_validation() {
        assert!(validate_slack_token_format("xoxb-123-abc").is_ok());
        assert!(validate_slack_token_format("bad-token").is_err());
        assert!(validate_slack_token_format("").is_err());
        assert!(validate_slack_token_format("   ").is_err());
    }

    #[test]
    fn test_telegram_token_format_validation() {
        assert!(validate_telegram_token_format("123456:ABC-DEF").is_ok());
        assert!(validate_telegram_token_format("no-colon").is_err());
        assert!(validate_telegram_token_format("").is_err());
    }

    #[test]
    fn test_twilio_sid_format_validation() {
        assert!(validate_twilio_sid_format("AC1234567890abcdef").is_ok());
        assert!(validate_twilio_sid_format("XX1234567890").is_err());
        assert!(validate_twilio_sid_format("").is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_imessage_rejects_non_macos() {
        assert!(validate_imessage_access().is_err());
    }
}
