//! CLI subcommand handlers.

use crate::AuthAction;
use crate::BrowserAction;
use crate::CanvasAction;
use crate::ChannelAction;
use crate::Commands;
use crate::ConfigAction;
use crate::CronAction;
use crate::PluginAction;
use crate::SkillAction;
use crate::SlackCommand;
use crate::UpdateAction;
use crate::VoiceAction;
use crate::WorkflowAction;
use std::path::Path;

/// Handle a CLI subcommand.
pub async fn handle_command(command: Commands, workspace: &Path) -> anyhow::Result<()> {
    match command {
        Commands::Config { action } => handle_config(action, workspace).await,
        Commands::Setup => crate::setup::run_setup(workspace).await,
        Commands::Channel { action } => handle_channel(action, workspace).await,
        Commands::Auth { action } => handle_auth(action, workspace).await,
        Commands::Workflow { action } => handle_workflow(action, workspace).await,
        Commands::Cron { action } => handle_cron(action, workspace).await,
        Commands::Voice { action } => handle_voice(action).await,
        Commands::Browser { action } => handle_browser(action, workspace).await,
        Commands::Ui { port } => handle_ui(port).await,
        Commands::Canvas { action } => handle_canvas(action).await,
        Commands::Skill { action } => handle_skill(action).await,
        Commands::Plugin { action } => handle_plugin(action).await,
        Commands::Update { action } => handle_update(action).await,
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
        ChannelAction::Slack { action } => handle_slack(action).await,
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

        AuthAction::Login {
            provider,
            redirect_uri,
        } => {
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
            println!(
                "(Make sure this URI is registered in your {} app settings)",
                provider
            );
            println!();
            println!("Opening your browser for authentication...");

            let token = oauth::authorize_browser_flow(&oauth_cfg, redirect_uri.as_deref())
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

async fn handle_workflow(action: WorkflowAction, _workspace: &Path) -> anyhow::Result<()> {
    match action {
        WorkflowAction::List => {
            let names = rustant_core::list_builtin_names();
            println!("Available workflows:");
            for name in names {
                if let Some(wf) = rustant_core::get_builtin(name) {
                    println!("  {} - {}", name, wf.description);
                }
            }
            Ok(())
        }
        WorkflowAction::Show { name } => match rustant_core::get_builtin(&name) {
            Some(wf) => {
                println!("Workflow: {}", wf.name);
                println!("Description: {}", wf.description);
                println!("Version: {}", wf.version);
                if !wf.inputs.is_empty() {
                    println!("\nInputs:");
                    for input in &wf.inputs {
                        let required = if input.optional {
                            "(optional)"
                        } else {
                            "(required)"
                        };
                        println!(
                            "  {} [{}] {} - {}",
                            input.name, input.input_type, required, input.description
                        );
                    }
                }
                println!("\nSteps:");
                for (i, step) in wf.steps.iter().enumerate() {
                    let gate_str = if step.gate.is_some() { " [gated]" } else { "" };
                    println!("  {}. {} (tool: {}){}", i + 1, step.id, step.tool, gate_str);
                }
                if !wf.outputs.is_empty() {
                    println!("\nOutputs:");
                    for output in &wf.outputs {
                        println!("  {}", output.name);
                    }
                }
                Ok(())
            }
            None => {
                eprintln!("Workflow '{}' not found", name);
                Ok(())
            }
        },
        WorkflowAction::Run { name, input } => {
            let _wf = rustant_core::get_builtin(&name)
                .ok_or_else(|| anyhow::anyhow!("Workflow '{}' not found", name))?;

            let mut inputs = std::collections::HashMap::new();
            for kv in &input {
                if let Some((key, value)) = kv.split_once('=') {
                    inputs.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                } else {
                    return Err(anyhow::anyhow!(
                        "Invalid input format '{}', expected key=value",
                        kv
                    ));
                }
            }

            println!("Starting workflow '{}'...", name);
            println!("  (Workflow execution requires an active agent session)");
            println!("  Inputs: {:?}", inputs);
            Ok(())
        }
        WorkflowAction::Runs => {
            println!("No active workflow runs.");
            Ok(())
        }
        WorkflowAction::Resume { run_id } => {
            println!("Resuming workflow run: {}", run_id);
            Ok(())
        }
        WorkflowAction::Cancel { run_id } => {
            println!("Cancelling workflow run: {}", run_id);
            Ok(())
        }
        WorkflowAction::Status { run_id } => {
            println!("Checking status of workflow run: {}", run_id);
            Ok(())
        }
    }
}

async fn handle_cron(action: CronAction, workspace: &Path) -> anyhow::Result<()> {
    let config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
    let scheduler_config = config.scheduler.unwrap_or_default();

    match action {
        CronAction::List => {
            let mut scheduler = rustant_core::CronScheduler::new();
            for job_config in &scheduler_config.cron_jobs {
                // Silently skip invalid expressions
                let _ = scheduler.add_job(job_config.clone());
            }
            let jobs = scheduler.list_jobs();
            if jobs.is_empty() {
                println!("No cron jobs configured.");
                println!("Add jobs via config or: rustant cron add <name> <schedule> <task>");
            } else {
                println!("Cron jobs ({}):", jobs.len());
                for job in &jobs {
                    let enabled = if job.config.enabled {
                        "enabled"
                    } else {
                        "disabled"
                    };
                    let next = job
                        .next_run
                        .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        .unwrap_or_else(|| "N/A".to_string());
                    println!(
                        "  {} [{}] schedule=\"{}\" task=\"{}\" next={}",
                        job.config.name, enabled, job.config.schedule, job.config.task, next
                    );
                }
            }
            Ok(())
        }
        CronAction::Add {
            name,
            schedule,
            task,
        } => {
            let job_config = rustant_core::CronJobConfig::new(&name, &schedule, &task);
            // Validate the cron expression
            let job = rustant_core::CronJob::new(job_config)?;
            let next = job
                .next_run
                .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            println!("Cron job '{}' added.", name);
            println!("  Schedule: {}", schedule);
            println!("  Task: {}", task);
            println!("  Next run: {}", next);
            println!("\nNote: Add this job to your config file to persist it across restarts.");
            Ok(())
        }
        CronAction::Run { name } => {
            let mut scheduler = rustant_core::CronScheduler::new();
            for job_config in &scheduler_config.cron_jobs {
                let _ = scheduler.add_job(job_config.clone());
            }
            match scheduler.get_job(&name) {
                Some(job) => {
                    println!("Manually triggering job '{}'...", name);
                    println!("  Task: {}", job.config.task);
                    println!("  (Task execution requires an active agent session)");
                    Ok(())
                }
                None => {
                    anyhow::bail!("Cron job '{}' not found", name);
                }
            }
        }
        CronAction::Disable { name } => {
            let mut scheduler = rustant_core::CronScheduler::new();
            for job_config in &scheduler_config.cron_jobs {
                let _ = scheduler.add_job(job_config.clone());
            }
            scheduler.disable_job(&name)?;
            println!("Cron job '{}' disabled.", name);
            Ok(())
        }
        CronAction::Enable { name } => {
            let mut scheduler = rustant_core::CronScheduler::new();
            for job_config in &scheduler_config.cron_jobs {
                let _ = scheduler.add_job(job_config.clone());
            }
            scheduler.enable_job(&name)?;
            println!("Cron job '{}' enabled.", name);
            Ok(())
        }
        CronAction::Remove { name } => {
            let mut scheduler = rustant_core::CronScheduler::new();
            for job_config in &scheduler_config.cron_jobs {
                let _ = scheduler.add_job(job_config.clone());
            }
            scheduler.remove_job(&name)?;
            println!("Cron job '{}' removed.", name);
            println!("Note: Also remove it from your config file to persist the change.");
            Ok(())
        }
        CronAction::Jobs => {
            let manager = rustant_core::JobManager::new(scheduler_config.max_background_jobs);
            let jobs = manager.list();
            if jobs.is_empty() {
                println!("No background jobs running.");
            } else {
                println!("Background jobs ({}):", jobs.len());
                for job in &jobs {
                    println!(
                        "  {} [{}] {} (started: {})",
                        job.id,
                        job.status,
                        job.name,
                        job.started_at.format("%Y-%m-%d %H:%M:%S UTC")
                    );
                }
            }
            Ok(())
        }
        CronAction::CancelJob { job_id } => {
            let id: uuid::Uuid = job_id
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid job ID '{}': {}", job_id, e))?;
            let mut manager = rustant_core::JobManager::new(scheduler_config.max_background_jobs);
            manager.cancel_job(&id)?;
            println!("Job {} cancelled.", job_id);
            Ok(())
        }
    }
}

/// Load the Slack OAuth token from the keyring and create a RealSlackHttp client.
fn load_slack_client() -> anyhow::Result<rustant_core::channels::slack::RealSlackHttp> {
    use rustant_core::credentials::KeyringCredentialStore;
    use rustant_core::oauth;

    let store = KeyringCredentialStore::new();
    let token = oauth::load_oauth_token(&store, "slack").map_err(|e| {
        anyhow::anyhow!(
            "No Slack OAuth token found. Run `rustant auth login slack` first.\n{}",
            e
        )
    })?;
    Ok(rustant_core::channels::slack::RealSlackHttp::new(
        token.access_token,
    ))
}

async fn handle_slack(action: SlackCommand) -> anyhow::Result<()> {
    use rustant_core::channels::slack::SlackHttpClient;

    let http = load_slack_client()?;

    match action {
        SlackCommand::Send { channel, message } => {
            let ts = http
                .post_message(&channel, &message)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Message sent (ts: {})", ts);
        }

        SlackCommand::History { channel, limit } => {
            let messages = http
                .conversations_history(&channel, limit)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if messages.is_empty() {
                println!("No messages found.");
            } else {
                for msg in messages.iter().rev() {
                    let thread = msg
                        .thread_ts
                        .as_deref()
                        .map(|t| format!(" [thread:{}]", t))
                        .unwrap_or_default();
                    println!("[{}] {}: {}{}", &msg.ts, msg.user, msg.text, thread);
                }
            }
        }

        SlackCommand::Channels => {
            let channels = http
                .conversations_list("public_channel,private_channel", 200)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if channels.is_empty() {
                println!("No channels found.");
            } else {
                println!(
                    "{:<14} {:<25} {:>5}  {:<6}  Topic",
                    "ID", "Name", "Users", "Member"
                );
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
            let users = http
                .users_list(200)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if users.is_empty() {
                println!("No users found.");
            } else {
                println!(
                    "{:<14} {:<20} {:<25} {:<6} Status",
                    "ID", "Username", "Real Name", "Admin"
                );
                println!("{}", "-".repeat(80));
                for u in &users {
                    let kind = if u.is_bot { " [bot]" } else { "" };
                    let admin = if u.is_admin { "yes" } else { "" };
                    let status = if !u.status_emoji.is_empty() || !u.status_text.is_empty() {
                        format!("{} {}", u.status_emoji, u.status_text)
                            .trim()
                            .to_string()
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
            let info = http
                .conversations_info(&channel)
                .await
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

        SlackCommand::React {
            channel,
            timestamp,
            emoji,
        } => {
            http.reactions_add(&channel, &timestamp, &emoji)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Reaction :{}:  added.", emoji);
        }

        SlackCommand::Files { channel } => {
            let files = http
                .files_list(channel.as_deref(), 100)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if files.is_empty() {
                println!("No files found.");
            } else {
                println!(
                    "{:<14} {:<30} {:<8} {:>10} User",
                    "ID", "Name", "Type", "Size"
                );
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
                    println!(
                        "{:<14} {:<30} {:<8} {:>10} {}",
                        f.id, name, f.filetype, size, f.user
                    );
                }
                println!("\nTotal: {} files", files.len());
            }
        }

        SlackCommand::Team => {
            let team = http
                .team_info()
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Workspace: {}", team.name);
            println!("ID:        {}", team.id);
            println!("Domain:    {}.slack.com", team.domain);
            if let Some(icon) = &team.icon_url {
                println!("Icon:      {}", icon);
            }
        }

        SlackCommand::Groups => {
            let groups = http
                .usergroups_list()
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            if groups.is_empty() {
                println!("No user groups found.");
            } else {
                println!(
                    "{:<14} {:<20} {:<15} {:>6}  Description",
                    "ID", "Name", "@Handle", "Users"
                );
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
            let dm_channel = http
                .conversations_open(&[user.as_str()])
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let ts = http
                .post_message(&dm_channel, &message)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("DM sent to {} (channel: {}, ts: {})", user, dm_channel, ts);
        }

        SlackCommand::Thread {
            channel,
            timestamp,
            message,
        } => {
            let ts = http
                .post_thread_reply(&channel, &timestamp, &message)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Thread reply sent (ts: {})", ts);
        }

        SlackCommand::Join { channel } => {
            http.conversations_join(&channel)
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            println!("Joined channel {}", channel);
        }
    }

    Ok(())
}

async fn handle_voice(action: VoiceAction) -> anyhow::Result<()> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
        anyhow::anyhow!(
            "OPENAI_API_KEY environment variable not set.\n\
             Set it with: export OPENAI_API_KEY=sk-..."
        )
    })?;

    match action {
        VoiceAction::Speak { text, voice } => {
            use rustant_core::voice::{OpenAiTtsProvider, SynthesisRequest, TtsProvider};

            println!("Synthesizing: \"{}\" (voice: {})", text, voice);
            let tts = OpenAiTtsProvider::new(&api_key);
            let request = SynthesisRequest::new(&text).with_voice(&voice);
            let result = tts
                .synthesize(&request)
                .await
                .map_err(|e| anyhow::anyhow!("TTS synthesis failed: {}", e))?;

            println!("  Duration:    {:.2}s", result.duration_secs);
            println!("  Sample rate: {} Hz", result.audio.sample_rate);
            println!("  Channels:    {}", result.audio.channels);
            println!("  Samples:     {}", result.audio.samples.len());
            println!("  Characters:  {}", result.characters_used);

            // Save WAV to temp file for playback
            let wav_bytes = rustant_core::voice::audio_convert::encode_wav(&result.audio)
                .map_err(|e| anyhow::anyhow!("WAV encoding failed: {}", e))?;
            let out_path = std::env::temp_dir().join("rustant_tts_output.wav");
            std::fs::write(&out_path, &wav_bytes)?;
            println!("  WAV saved:   {}", out_path.display());

            Ok(())
        }
        VoiceAction::Roundtrip { text } => {
            use rustant_core::voice::{
                OpenAiSttProvider, OpenAiTtsProvider, SttProvider, SynthesisRequest, TtsProvider,
            };

            println!("Original: \"{}\"", text);

            // TTS: text -> audio
            println!("  [1/2] Synthesizing speech...");
            let tts = OpenAiTtsProvider::new(&api_key);
            let request = SynthesisRequest::new(&text);
            let tts_result = tts
                .synthesize(&request)
                .await
                .map_err(|e| anyhow::anyhow!("TTS synthesis failed: {}", e))?;
            println!(
                "  Audio: {:.2}s, {} samples @ {} Hz",
                tts_result.duration_secs,
                tts_result.audio.samples.len(),
                tts_result.audio.sample_rate
            );

            // STT: audio -> text
            println!("  [2/2] Transcribing audio...");
            let stt = OpenAiSttProvider::new(&api_key);
            let transcription = stt
                .transcribe(&tts_result.audio)
                .await
                .map_err(|e| anyhow::anyhow!("STT transcription failed: {}", e))?;
            println!("Transcribed: \"{}\"", transcription.text);
            if let Some(lang) = &transcription.language {
                println!("  Language:   {}", lang);
            }
            println!("  Duration:   {:.2}s", transcription.duration_secs);
            println!("  Confidence: {:.2}", transcription.confidence);

            Ok(())
        }
    }
}

async fn handle_browser(action: BrowserAction, _workspace: &Path) -> anyhow::Result<()> {
    match action {
        BrowserAction::Test { url } => {
            #[cfg(feature = "browser")]
            {
                use rustant_core::browser::{CdpClient, ChromiumCdpClient};
                use rustant_core::config::BrowserConfig;

                println!("Launching headless Chrome...");
                let config = BrowserConfig {
                    enabled: true,
                    headless: true,
                    ..Default::default()
                };
                let client = ChromiumCdpClient::launch(&config)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to launch Chrome: {}", e))?;

                println!("Navigating to: {}", url);
                client
                    .navigate(&url)
                    .await
                    .map_err(|e| anyhow::anyhow!("Navigation failed: {}", e))?;

                let title = client
                    .get_title()
                    .await
                    .unwrap_or_else(|_| "(unknown)".into());
                let page_url = client
                    .get_url()
                    .await
                    .unwrap_or_else(|_| "(unknown)".into());
                let text = client.get_text().await.unwrap_or_default();
                let text_preview = if text.len() > 200 {
                    format!("{}...", &text[..200])
                } else {
                    text
                };

                println!("  Title: {}", title);
                println!("  URL:   {}", page_url);
                println!(
                    "  Text preview:\n    {}",
                    text_preview.replace('\n', "\n    ")
                );

                // Take screenshot
                let screenshot = client.screenshot().await;
                match screenshot {
                    Ok(bytes) => {
                        let out_path = std::env::temp_dir().join("rustant_browser_screenshot.png");
                        std::fs::write(&out_path, &bytes)?;
                        println!(
                            "  Screenshot: {} ({} bytes)",
                            out_path.display(),
                            bytes.len()
                        );
                    }
                    Err(e) => println!("  Screenshot failed: {}", e),
                }

                client
                    .close()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to close browser: {}", e))?;
                println!("Browser closed.");
                Ok(())
            }

            #[cfg(not(feature = "browser"))]
            {
                let _ = url;
                eprintln!(
                    "Browser feature not enabled.\n\
                     Recompile with: cargo build --features browser"
                );
                Ok(())
            }
        }
    }
}

async fn handle_ui(port: u16) -> anyhow::Result<()> {
    println!("Starting Rustant Dashboard...");
    println!("Gateway will listen on: http://127.0.0.1:{}", port);
    println!();

    // Start the gateway server in the background
    let config = rustant_core::gateway::GatewayConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        auth_tokens: Vec::new(),
        max_connections: 50,
        session_timeout_secs: 3600,
    };

    let gw: rustant_core::gateway::SharedGateway = std::sync::Arc::new(tokio::sync::Mutex::new(
        rustant_core::gateway::GatewayServer::new(config.clone()),
    ));

    let gw_for_server = gw.clone();

    tokio::spawn(async move {
        if let Err(e) = rustant_core::gateway::run_gateway(gw_for_server).await {
            eprintln!("Gateway error: {}", e);
        }
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    println!("Gateway running. Open your browser to:");
    println!("  http://127.0.0.1:{}/api/status", port);
    println!();
    println!("Dashboard UI requires the rustant-ui binary (Tauri).");
    println!("  Build it with: cargo build -p rustant-ui");
    println!("  Or run the gateway-only mode and open the API in a browser.");
    println!();
    println!("Press Ctrl+C to stop.");

    // Wait forever (or until Ctrl+C)
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

async fn handle_canvas(action: CanvasAction) -> anyhow::Result<()> {
    use rustant_core::canvas::{CanvasManager, CanvasTarget, ContentType};

    // Create a local canvas manager for CLI operations
    let mut canvas = CanvasManager::new();
    let target = CanvasTarget::Broadcast;

    match action {
        CanvasAction::Push {
            content_type,
            content,
        } => {
            let ct = ContentType::from_str_loose(&content_type).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown content type '{}'. Valid types: html, markdown, code, chart, table, form, image, diagram",
                    content_type
                )
            })?;

            // For structured types, try to render them
            match ct {
                ContentType::Chart => {
                    let spec: rustant_core::canvas::ChartSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid chart JSON: {}", e))?;
                    let config = rustant_core::canvas::render_chart_config(&spec);
                    println!("Chart.js config:\n{}", config);
                }
                ContentType::Table => {
                    let spec: rustant_core::canvas::TableSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid table JSON: {}", e))?;
                    let html = rustant_core::canvas::render_table_html(&spec);
                    println!("Table HTML:\n{}", html);
                }
                ContentType::Form => {
                    let spec: rustant_core::canvas::FormSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid form JSON: {}", e))?;
                    let html = rustant_core::canvas::render_form_html(&spec);
                    println!("Form HTML:\n{}", html);
                }
                ContentType::Diagram => {
                    let spec: rustant_core::canvas::DiagramSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid diagram JSON: {}", e))?;
                    let mermaid = rustant_core::canvas::render_diagram_mermaid(&spec);
                    println!("Mermaid:\n{}", mermaid);
                }
                _ => {
                    println!("Content ({}):\n{}", content_type, content);
                }
            }

            let id = canvas
                .push(&target, ct, content)
                .map_err(|e| anyhow::anyhow!("Canvas push failed: {}", e))?;
            println!("\nPushed to canvas (id: {})", id);
            Ok(())
        }
        CanvasAction::Clear => {
            canvas.clear(&target);
            println!("Canvas cleared.");
            Ok(())
        }
        CanvasAction::Snapshot => {
            let items = canvas.snapshot(&target);
            if items.is_empty() {
                println!("Canvas is empty.");
            } else {
                println!("Canvas snapshot ({} items):", items.len());
                for item in items {
                    println!(
                        "  [{}] {:?}: {}",
                        &item.id.to_string()[..8],
                        item.content_type,
                        if item.content.len() > 80 {
                            format!("{}...", &item.content[..77])
                        } else {
                            item.content.clone()
                        }
                    );
                }
            }
            Ok(())
        }
    }
}

async fn handle_skill(action: SkillAction) -> anyhow::Result<()> {
    use rustant_core::skills::{parse_skill_md, validate_skill, SkillLoader};

    match action {
        SkillAction::List { dir } => {
            let skills_dir = dir.unwrap_or_else(|| {
                directories::ProjectDirs::from("dev", "rustant", "rustant")
                    .map(|d| d.data_dir().join("skills").to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".rustant/skills".into())
            });

            let loader = SkillLoader::new(&skills_dir);
            let results = loader.scan();

            if results.is_empty() {
                println!("No skill files found in: {}", skills_dir);
                println!("Create SKILL.md files in that directory to define skills.");
            } else {
                println!("Skills in {}:", skills_dir);
                for result in &results {
                    match result {
                        Ok(skill) => {
                            println!(
                                "  {} v{} - {} ({} tools)",
                                skill.name,
                                skill.version,
                                skill.description,
                                skill.tools.len()
                            );
                        }
                        Err((path, err)) => {
                            println!("  {} (error: {})", path.display(), err);
                        }
                    }
                }
                println!("\nTotal: {} skill files", results.len());
            }
            Ok(())
        }
        SkillAction::Info { path } => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{}': {}", path, e))?;

            println!("Skill: {}", skill.name);
            println!("Version: {}", skill.version);
            println!("Description: {}", skill.description);
            if let Some(author) = &skill.author {
                println!("Author: {}", author);
            }
            println!("Risk Level: {:?}", skill.risk_level);

            if !skill.requires.is_empty() {
                println!("\nRequirements:");
                for req in &skill.requires {
                    println!("  {} ({})", req.name, req.req_type);
                }
            }

            if !skill.tools.is_empty() {
                println!("\nTools ({}):", skill.tools.len());
                for tool in &skill.tools {
                    println!("  {} - {}", tool.name, tool.description);
                    if !tool.body.is_empty() {
                        let body_preview = if tool.body.len() > 60 {
                            format!("{}...", &tool.body[..57])
                        } else {
                            tool.body.clone()
                        };
                        println!("    Body: {}", body_preview);
                    }
                }
            }
            Ok(())
        }
        SkillAction::Validate { path } => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{}': {}", path, e))?;

            // Validate with empty available tools/secrets (strict check)
            let result = validate_skill(&skill, &[], &[]);

            println!("Validation result for '{}':", skill.name);
            println!("  Valid: {}", result.is_valid);
            println!("  Risk Level: {:?}", result.risk_level);

            if !result.warnings.is_empty() {
                println!("\n  Warnings:");
                for warning in &result.warnings {
                    println!("    - {}", warning);
                }
            }

            if !result.errors.is_empty() {
                println!("\n  Errors:");
                for error in &result.errors {
                    println!("    - {}", error);
                }
            }

            if result.is_valid && result.warnings.is_empty() {
                println!("\n  Skill passed all validation checks.");
            }
            Ok(())
        }
        SkillAction::Load { path } => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", path, e))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{}': {}", path, e))?;

            println!("Loaded skill: {}", skill.name);
            let json = serde_json::to_string_pretty(&skill)?;
            println!("{}", json);
            Ok(())
        }
    }
}

async fn handle_plugin(action: PluginAction) -> anyhow::Result<()> {
    use rustant_plugins::NativePluginLoader;

    match action {
        PluginAction::List { dir } => {
            let plugins_dir = dir.unwrap_or_else(|| {
                directories::ProjectDirs::from("dev", "rustant", "rustant")
                    .map(|d| d.data_dir().join("plugins").to_string_lossy().into_owned())
                    .unwrap_or_else(|| ".rustant/plugins".into())
            });

            let mut loader = NativePluginLoader::new();
            loader.add_search_dir(&plugins_dir);
            let found = loader.discover();

            if found.is_empty() {
                println!("No plugins found in: {}", plugins_dir);
                println!("Place .so/.dll/.dylib plugin files in that directory.");
            } else {
                println!("Plugin files in {}:", plugins_dir);
                for path in &found {
                    println!("  {}", path.display());
                }
                println!("\nTotal: {} plugin files", found.len());
            }
            Ok(())
        }
        PluginAction::Info { name } => {
            println!("Plugin: {}", name);
            println!("  Status: not loaded");
            println!("  (Plugin loading requires an active agent session)");
            Ok(())
        }
    }
}

async fn handle_update(action: UpdateAction) -> anyhow::Result<()> {
    use rustant_core::updater::{UpdateChecker, UpdateConfig, Updater, CURRENT_VERSION};

    match action {
        UpdateAction::Check => {
            println!("Current version: {}", CURRENT_VERSION);
            println!("Checking for updates...");

            let config = UpdateConfig::default();
            let checker = UpdateChecker::new(config);

            match checker.check().await {
                Ok(result) => {
                    if result.update_available {
                        println!(
                            "Update available: {} -> {}",
                            result.current_version,
                            result.latest_version.as_deref().unwrap_or("unknown")
                        );
                        if let Some(url) = &result.release_url {
                            println!("Release: {}", url);
                        }
                        if let Some(notes) = &result.release_notes {
                            if !notes.is_empty() {
                                let preview = if notes.len() > 200 {
                                    format!("{}...", &notes[..197])
                                } else {
                                    notes.clone()
                                };
                                println!("\nRelease notes:\n{}", preview);
                            }
                        }
                        println!("\nRun `rustant update install` to update.");
                    } else {
                        println!("You are running the latest version.");
                    }
                }
                Err(e) => {
                    println!("Failed to check for updates: {}", e);
                    println!("You can check manually at:");
                    println!("  https://github.com/DevJadhav/Rustant/releases");
                }
            }
            Ok(())
        }
        UpdateAction::Install => {
            println!("Current version: {}", CURRENT_VERSION);
            println!("Downloading and installing latest version...");

            match Updater::update() {
                Ok(()) => {
                    println!("Update installed successfully!");
                    println!("Restart rustant to use the new version.");
                }
                Err(e) => {
                    println!("Update failed: {}", e);
                    println!("You can update manually by downloading from:");
                    println!("  https://github.com/DevJadhav/Rustant/releases");
                }
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
