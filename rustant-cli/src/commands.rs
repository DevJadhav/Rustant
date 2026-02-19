//! CLI subcommand handlers.

use crate::AlertsAction;
use crate::AuditAction;
use crate::AuthAction;
use crate::BrowserAction;
use crate::CanvasAction;
use crate::ChannelAction;
use crate::Commands;
use crate::ComplianceAction;
use crate::ConfigAction;
use crate::CronAction;
use crate::LicenseAction;
use crate::PluginAction;
use crate::PolicyAction;
use crate::QualityAction;
use crate::ReviewAction;
use crate::RiskAction;
use crate::SbomAction;
use crate::ScanAction;
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
        Commands::Init => handle_init(workspace).await,
        Commands::Resume { session } => handle_resume(session.as_deref(), workspace).await,
        Commands::Sessions { limit } => handle_sessions(limit, workspace),
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
        Commands::Scan { action } => handle_scan(action, workspace).await,
        Commands::Review { action } => handle_review(action, workspace).await,
        Commands::Quality { action } => handle_quality(action, workspace).await,
        Commands::License { action } => handle_license(action, workspace).await,
        Commands::Sbom { action } => handle_sbom(action, workspace).await,
        Commands::Compliance { action } => handle_compliance(action, workspace).await,
        Commands::Audit { action } => handle_audit(action, workspace).await,
        Commands::Risk { action } => handle_risk(action, workspace).await,
        Commands::Policy { action } => handle_policy(action, workspace).await,
        Commands::Alerts { action } => handle_alerts(action, workspace).await,
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
                .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{toml_str}");
            Ok(())
        }
    }
}

async fn handle_init(workspace: &Path) -> anyhow::Result<()> {
    use rustant_core::project_detect::{
        detect_project, example_tasks, recommended_allowed_commands,
    };

    println!("\n  \x1b[1mRustant Smart Init\x1b[0m\n");

    // Step 1: Detect project type
    println!("  Scanning workspace...");
    let info = detect_project(workspace);

    println!("  Project type: \x1b[36m{}\x1b[0m", info.project_type);
    if let Some(ref fw) = info.framework {
        println!("  Framework:    \x1b[36m{fw}\x1b[0m");
    }
    if let Some(ref pm) = info.package_manager {
        println!("  Package mgr:  \x1b[36m{pm}\x1b[0m");
    }
    if info.has_git {
        let status = if info.git_clean { "clean" } else { "dirty" };
        println!("  Git:          \x1b[36m{status}\x1b[0m");
    }
    if info.has_ci {
        println!("  CI:           \x1b[36mdetected\x1b[0m");
    }
    println!();

    // Step 2: Check for existing config
    let config_dir = workspace.join(".rustant");
    let config_path = config_dir.join("config.toml");

    if config_path.exists() {
        println!("  Config already exists at: {}\n", config_path.display());
        let overwrite = dialoguer::Confirm::new()
            .with_prompt("  Overwrite with project-optimized config?")
            .default(false)
            .interact()?;
        if !overwrite {
            println!("  Keeping existing config. Done.\n");
            return Ok(());
        }
    }

    // Step 3: Detect existing API keys or run setup wizard
    let detected_keys = crate::setup::detect_env_api_keys();
    let mut config = rustant_core::AgentConfig::default();

    if !detected_keys.is_empty() {
        println!(
            "  Detected API key(s): {}",
            detected_keys
                .iter()
                .map(|(_, name)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        // Use the first detected provider
        let (provider_name, display_name) = &detected_keys[0];
        println!("  Using: \x1b[36m{display_name}\x1b[0m");
        config.llm.provider = provider_name.clone();
        config.llm.api_key_env = match provider_name.as_str() {
            "openai" => "OPENAI_API_KEY".to_string(),
            "anthropic" => "ANTHROPIC_API_KEY".to_string(),
            "gemini" => "GEMINI_API_KEY".to_string(),
            _ => format!("{}_API_KEY", provider_name.to_uppercase()),
        };
        config.llm.model = match provider_name.as_str() {
            "anthropic" => "claude-sonnet-4-20250514".to_string(),
            "gemini" => "gemini-2.0-flash".to_string(),
            _ => "gpt-4o".to_string(),
        };
        println!();
    } else {
        // Check for Ollama
        let ollama_running = reqwest::Client::new()
            .get("http://localhost:11434/api/tags")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
            .is_ok();

        if ollama_running {
            println!("  Detected \x1b[36mOllama\x1b[0m running locally (no API key needed)");
            config.llm.provider = "openai".to_string();
            config.llm.base_url = Some("http://localhost:11434/v1".to_string());
            config.llm.model = "llama3.2".to_string();
            config.llm.api_key_env = "OLLAMA_API_KEY".to_string();
            println!();
        } else {
            println!("  No API keys detected. Starting provider setup...\n");
            if let Err(e) = crate::setup::run_setup(workspace).await {
                eprintln!("  Setup failed: {e}. Generating config with defaults.\n");
            } else {
                // Reload config after setup wizard
                if config_path.exists()
                    && let Ok(content) = std::fs::read_to_string(&config_path)
                    && let Ok(loaded) = toml::from_str::<rustant_core::AgentConfig>(&content)
                {
                    config = loaded;
                }
            }
        }
    }

    // Step 4: Apply project-specific safety settings
    let allowed_cmds = recommended_allowed_commands(&info);
    config.safety.allowed_commands = allowed_cmds;

    // Set approval mode based on git status
    config.safety.approval_mode = if info.has_git && info.git_clean {
        rustant_core::ApprovalMode::Safe
    } else {
        rustant_core::ApprovalMode::Cautious
    };

    // Add source dirs to allowed paths
    if !info.source_dirs.is_empty() {
        config.safety.allowed_paths = info.source_dirs.iter().map(|d| format!("{d}/**")).collect();
        config
            .safety
            .allowed_paths
            .extend(["tests/**".to_string(), "docs/**".to_string()]);
    }

    // Step 5: Write config
    std::fs::create_dir_all(&config_dir)?;
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(&config_path, &toml_str)?;
    println!(
        "  Config saved to: \x1b[36m{}\x1b[0m\n",
        config_path.display()
    );

    // Step 6: Print quick-start guide
    println!("  \x1b[1m--- Quick Start ---\x1b[0m\n");
    println!("  \x1b[33mInteractive mode:\x1b[0m  rustant");
    println!("  \x1b[33mSingle task:\x1b[0m       rustant \"your task here\"");
    println!("  \x1b[33mTUI mode:\x1b[0m          rustant --tui");
    println!();
    println!("  \x1b[33mSlash commands:\x1b[0m");
    println!("    /help     Show available commands");
    println!("    /tools    List available tools");
    println!("    /safety   Show current safety settings");
    println!("    /cost     Show token usage and cost");
    println!("    /session  Save or load sessions");
    println!();

    // Build/test commands
    if !info.build_commands.is_empty() || !info.test_commands.is_empty() {
        println!("  \x1b[33mDetected project commands:\x1b[0m");
        for cmd in &info.build_commands {
            println!("    Build: {cmd}");
        }
        for cmd in &info.test_commands {
            println!("    Test:  {cmd}");
        }
        println!();
    }

    // Example tasks
    let examples = example_tasks(&info);
    if !examples.is_empty() {
        println!("  \x1b[33mTry these tasks:\x1b[0m");
        for task in &examples {
            println!("    rustant {task}");
        }
        println!();
    }

    println!(
        "  Approval mode: \x1b[36m{}\x1b[0m (reads auto-approved, writes ask first)",
        config.safety.approval_mode
    );
    println!(
        "  Provider: \x1b[36m{}\x1b[0m | Model: \x1b[36m{}\x1b[0m\n",
        config.llm.provider, config.llm.model
    );

    Ok(())
}

async fn handle_resume(session: Option<&str>, workspace: &Path) -> anyhow::Result<()> {
    let mut mgr = rustant_core::SessionManager::new(workspace)
        .map_err(|e| anyhow::anyhow!("Failed to initialize session manager: {e}"))?;

    let (memory, continuation) = if let Some(query) = session {
        mgr.resume_session(query)
    } else {
        mgr.resume_latest()
    }
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    let goal = memory.working.current_goal.clone().unwrap_or_default();
    let msg_count = memory.short_term.len();

    println!("\x1b[1;32mSession resumed!\x1b[0m");
    if !goal.is_empty() {
        println!("  Last goal: \x1b[36m{goal}\x1b[0m");
    }
    println!("  Messages restored: \x1b[36m{msg_count}\x1b[0m");
    println!(
        "  Facts in memory: \x1b[36m{}\x1b[0m",
        memory.long_term.facts.len()
    );
    println!();

    // Load config and start interactive session with resumed memory
    let config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;

    let provider = match rustant_core::create_provider(&config.llm) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("LLM provider init failed: {}. Using mock.", e);
            std::sync::Arc::new(rustant_core::MockLlmProvider::new())
        }
    };

    let callback = std::sync::Arc::new(crate::repl::CliCallback::new(false));
    let mut agent = rustant_core::Agent::new(provider, config, callback);
    agent.set_output_redactor(rustant_core::create_basic_redactor());
    *agent.memory_mut() = memory;

    // Inject the continuation context
    agent
        .memory_mut()
        .add_message(rustant_core::types::Message::system(continuation));

    // Run in interactive REPL mode
    println!("  Type your next instruction to continue, or /quit to exit.\n");

    let stdin = std::io::stdin();
    loop {
        print!("\x1b[1;34m> \x1b[0m");
        std::io::Write::flush(&mut std::io::stdout())?;

        let mut input = String::new();
        if std::io::BufRead::read_line(&mut stdin.lock(), &mut input).is_err() || input.is_empty() {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }
        if input == "/quit" || input == "/exit" || input == "/q" {
            println!("Goodbye!");
            break;
        }

        match agent.process_task(input).await {
            Ok(result) => {
                println!(
                    "\x1b[90m  [{} iterations, {} tokens, ${:.4}]\x1b[0m",
                    result.iterations,
                    result.total_usage.total(),
                    result.total_cost.total()
                );
            }
            Err(e) => {
                println!("\x1b[31mError: {e}\x1b[0m");
            }
        }
    }

    Ok(())
}

fn handle_sessions(limit: usize, workspace: &Path) -> anyhow::Result<()> {
    let mgr = rustant_core::SessionManager::new(workspace)
        .map_err(|e| anyhow::anyhow!("Failed to initialize session manager: {e}"))?;

    let sessions = mgr.list_sessions(limit);
    if sessions.is_empty() {
        println!("No saved sessions found.");
        println!("Sessions are saved automatically when using the agent.");
        return Ok(());
    }

    println!("\x1b[1mSaved Sessions\x1b[0m (most recent first):\n");
    for entry in &sessions {
        let status = if entry.completed {
            "\x1b[32mdone\x1b[0m"
        } else {
            "\x1b[33min progress\x1b[0m"
        };
        let goal = entry.last_goal.as_deref().unwrap_or("(no goal recorded)");

        println!("  \x1b[1;36m{}\x1b[0m  [{}]", entry.name, status);
        println!(
            "    Goal: {}",
            if goal.len() > 60 {
                format!("{}...", &goal[..60])
            } else {
                goal.to_string()
            }
        );
        println!(
            "    Messages: {} | Tokens: {} | Updated: {}",
            entry.message_count,
            entry.total_tokens,
            entry.updated_at.format("%Y-%m-%d %H:%M UTC")
        );
        println!();
    }

    println!("Resume with: \x1b[36mrustant resume [name]\x1b[0m");
    Ok(())
}

pub async fn handle_channel(action: ChannelAction, workspace: &Path) -> anyhow::Result<()> {
    let config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;

    let channels_config = config.channels.unwrap_or_default();

    match action {
        ChannelAction::Setup { channel } => {
            return crate::channel_setup::run_channel_setup(workspace, channel.as_deref()).await;
        }
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
                        .map(|s| format!("{s:?}"))
                        .unwrap_or_else(|| "unknown".to_string());
                    println!("  {name} ({status})");
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
                    "Channel '{name}' not found in configuration. Available: {available}"
                );
            }

            println!("Testing channel '{name}'...");

            // Connect all (which includes our target)
            let results = mgr.connect_all().await;
            for (ch_name, result) in &results {
                if ch_name == &name {
                    match result {
                        Ok(()) => println!("  Connected successfully!"),
                        Err(e) => {
                            println!("  Connection failed: {e}");
                            anyhow::bail!("Channel test failed for '{name}'");
                        }
                    }
                }
            }

            // Disconnect
            mgr.disconnect_all().await;
            println!("  Disconnected. Channel '{name}' is working.");
            Ok(())
        }
    }
}

pub async fn handle_auth(action: AuthAction, workspace: &Path) -> anyhow::Result<()> {
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
                    println!("    {provider}: not configured");
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
                                methods.push(format!("OAuth (expires in {secs}s)"));
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
                                println!("    {provider}: OAuth (expired)");
                            } else if let Some(expires_at) = token.expires_at {
                                let remaining = expires_at - chrono::Utc::now();
                                let secs = remaining.num_seconds().max(0);
                                println!("    {provider}: OAuth (expires in {secs}s)");
                            } else {
                                println!("    {provider}: OAuth (active)");
                            }
                        }
                        Err(_) => {
                            println!("    {provider}: OAuth (error reading token)");
                        }
                    }
                } else {
                    println!("    {provider}: not configured");
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
                        "OAuth for '{provider}' requires environment variables: {env_hint}\n\
                         Set these from your app's developer console and try again."
                    )
                } else {
                    anyhow::anyhow!(
                        "Unknown or unsupported provider '{provider}'. Supported: openai, gemini, slack, discord, teams, whatsapp, gmail"
                    )
                }
            })?;

            println!("Starting OAuth login for {provider}...");
            let effective_redirect = match &redirect_uri {
                Some(uri) => uri.clone(),
                None => format!(
                    "https://localhost:{}/auth/callback",
                    rustant_core::oauth::OAUTH_CALLBACK_PORT
                ),
            };
            println!("Redirect URI: {effective_redirect}");
            println!("(Make sure this URI is registered in your {provider} app settings)");
            println!();
            println!("Opening your browser for authentication...");

            let token = oauth::authorize_browser_flow(&oauth_cfg, redirect_uri.as_deref())
                .await
                .map_err(|e| anyhow::anyhow!("OAuth login failed: {e}"))?;

            oauth::store_oauth_token(&cred_store, &provider, &token)
                .map_err(|e| anyhow::anyhow!("Failed to store OAuth token: {e}"))?;

            println!("Successfully authenticated with {provider}.");

            if let Some(expires_at) = token.expires_at {
                let remaining = expires_at - chrono::Utc::now();
                println!("Token expires in {}s.", remaining.num_seconds().max(0));
            }

            if is_channel {
                println!(
                    "Tip: Add {provider} to your channel config with auth_method = \"oauth\" to use this token."
                );
            } else {
                // Update config to use OAuth auth method
                let config = rustant_core::config::load_config(Some(workspace), None)
                    .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;
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
                    .map_err(|e| anyhow::anyhow!("Failed to delete OAuth token: {e}"))?;
                println!("OAuth token removed for {provider}.");
            } else {
                println!("No OAuth token found for {provider}.");
            }

            Ok(())
        }

        AuthAction::Refresh { provider } => {
            let provider = provider.to_lowercase();

            let token = oauth::load_oauth_token(&cred_store, &provider)
                .map_err(|e| anyhow::anyhow!("No OAuth token found for {provider}: {e}"))?;

            let refresh_token_str = token.refresh_token.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "No refresh token available for {provider}. Re-login with `rustant auth login {provider}`."
                )
            })?;

            let oauth_cfg = oauth::oauth_config_for_provider(&provider).ok_or_else(|| {
                anyhow::anyhow!("No OAuth configuration available for '{provider}'")
            })?;

            println!("Refreshing token for {provider}...");

            let new_token = oauth::refresh_token(&oauth_cfg, refresh_token_str)
                .await
                .map_err(|e| anyhow::anyhow!("Token refresh failed: {e}"))?;

            oauth::store_oauth_token(&cred_store, &provider, &new_token)
                .map_err(|e| anyhow::anyhow!("Failed to store refreshed token: {e}"))?;

            println!("Token refreshed successfully for {provider}.");

            if let Some(expires_at) = new_token.expires_at {
                let remaining = expires_at - chrono::Utc::now();
                println!("New token expires in {}s.", remaining.num_seconds().max(0));
            }

            Ok(())
        }
    }
}

pub async fn handle_workflow(action: WorkflowAction, _workspace: &Path) -> anyhow::Result<()> {
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
                eprintln!("Workflow '{name}' not found");
                Ok(())
            }
        },
        WorkflowAction::Run { name, input } => {
            let _wf = rustant_core::get_builtin(&name)
                .ok_or_else(|| anyhow::anyhow!("Workflow '{name}' not found"))?;

            let mut inputs = std::collections::HashMap::new();
            for kv in &input {
                if let Some((key, value)) = kv.split_once('=') {
                    inputs.insert(
                        key.to_string(),
                        serde_json::Value::String(value.to_string()),
                    );
                } else {
                    return Err(anyhow::anyhow!(
                        "Invalid input format '{kv}', expected key=value"
                    ));
                }
            }

            println!("Starting workflow '{name}'...");
            println!("  (Workflow execution requires an active agent session)");
            println!("  Inputs: {inputs:?}");
            Ok(())
        }
        WorkflowAction::Runs => {
            println!("No active workflow runs.");
            Ok(())
        }
        WorkflowAction::Resume { run_id } => {
            println!("Resuming workflow run: {run_id}");
            Ok(())
        }
        WorkflowAction::Cancel { run_id } => {
            println!("Cancelling workflow run: {run_id}");
            Ok(())
        }
        WorkflowAction::Status { run_id } => {
            println!("Checking status of workflow run: {run_id}");
            Ok(())
        }
    }
}

async fn handle_cron(action: CronAction, workspace: &Path) -> anyhow::Result<()> {
    let config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Failed to load config: {e}"))?;
    let scheduler_config = config.scheduler.unwrap_or_default();

    // State file for persisting cron jobs across CLI invocations
    let state_dir = workspace.join(".rustant").join("cron");
    let state_file = state_dir.join("state.json");

    // Load scheduler from state file first, falling back to config
    let load_scheduler = || -> rustant_core::CronScheduler {
        if state_file.exists()
            && let Ok(json) = std::fs::read_to_string(&state_file)
            && let Ok(scheduler) = rustant_core::CronScheduler::from_json(&json)
        {
            return scheduler;
        }
        // Fall back to config-defined jobs
        let mut scheduler = rustant_core::CronScheduler::new();
        for job_config in &scheduler_config.cron_jobs {
            let _ = scheduler.add_job(job_config.clone());
        }
        scheduler
    };

    // Save scheduler state to disk
    let save_scheduler = |scheduler: &rustant_core::CronScheduler| -> anyhow::Result<()> {
        std::fs::create_dir_all(&state_dir)?;
        let json = scheduler.to_json()?;
        let tmp = state_file.with_extension("tmp");
        std::fs::write(&tmp, &json)?;
        std::fs::rename(&tmp, &state_file)?;
        Ok(())
    };

    match action {
        CronAction::List => {
            let scheduler = load_scheduler();
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
            let mut scheduler = load_scheduler();
            let job_config = rustant_core::CronJobConfig::new(&name, &schedule, &task);
            // Validate and add to scheduler
            scheduler.add_job(job_config)?;
            let job = scheduler.get_job(&name).unwrap();
            let next = job
                .next_run
                .map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "N/A".to_string());
            save_scheduler(&scheduler)?;
            println!("Cron job '{name}' added.");
            println!("  Schedule: {schedule}");
            println!("  Task: {task}");
            println!("  Next run: {next}");
            Ok(())
        }
        CronAction::Run { name } => {
            let scheduler = load_scheduler();
            match scheduler.get_job(&name) {
                Some(job) => {
                    println!("Manually triggering job '{name}'...");
                    println!("  Task: {}", job.config.task);
                    println!("  (Task execution requires an active agent session)");
                    Ok(())
                }
                None => {
                    anyhow::bail!("Cron job '{name}' not found");
                }
            }
        }
        CronAction::Disable { name } => {
            let mut scheduler = load_scheduler();
            scheduler.disable_job(&name)?;
            save_scheduler(&scheduler)?;
            println!("Cron job '{name}' disabled.");
            Ok(())
        }
        CronAction::Enable { name } => {
            let mut scheduler = load_scheduler();
            scheduler.enable_job(&name)?;
            save_scheduler(&scheduler)?;
            println!("Cron job '{name}' enabled.");
            Ok(())
        }
        CronAction::Remove { name } => {
            let mut scheduler = load_scheduler();
            scheduler.remove_job(&name)?;
            save_scheduler(&scheduler)?;
            println!("Cron job '{name}' removed.");
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
                .map_err(|e| anyhow::anyhow!("Invalid job ID '{job_id}': {e}"))?;
            let mut manager = rustant_core::JobManager::new(scheduler_config.max_background_jobs);
            manager.cancel_job(&id)?;
            println!("Job {job_id} cancelled.");
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
        anyhow::anyhow!("No Slack OAuth token found. Run `rustant auth login slack` first.\n{e}")
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
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Message sent (ts: {ts})");
        }

        SlackCommand::History { channel, limit } => {
            let messages = http
                .conversations_history(&channel, limit)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            if messages.is_empty() {
                println!("No messages found.");
            } else {
                for msg in messages.iter().rev() {
                    let thread = msg
                        .thread_ts
                        .as_deref()
                        .map(|t| format!(" [thread:{t}]"))
                        .unwrap_or_default();
                    println!("[{}] {}: {}{}", &msg.ts, msg.user, msg.text, thread);
                }
            }
        }

        SlackCommand::Channels => {
            let channels = http
                .conversations_list("public_channel,private_channel", 200)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Reaction :{emoji}:  added.");
        }

        SlackCommand::Files { channel } => {
            let files = http
                .files_list(channel.as_deref(), 100)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
            let team = http.team_info().await.map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Workspace: {}", team.name);
            println!("ID:        {}", team.id);
            println!("Domain:    {}.slack.com", team.domain);
            if let Some(icon) = &team.icon_url {
                println!("Icon:      {icon}");
            }
        }

        SlackCommand::Groups => {
            let groups = http
                .usergroups_list()
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
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
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            let ts = http
                .post_message(&dm_channel, &message)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("DM sent to {user} (channel: {dm_channel}, ts: {ts})");
        }

        SlackCommand::Thread {
            channel,
            timestamp,
            message,
        } => {
            let ts = http
                .post_thread_reply(&channel, &timestamp, &message)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Thread reply sent (ts: {ts})");
        }

        SlackCommand::Join { channel } => {
            http.conversations_join(&channel)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            println!("Joined channel {channel}");
        }
    }

    Ok(())
}

pub async fn handle_voice(action: VoiceAction) -> anyhow::Result<()> {
    let api_key =
        rustant_core::resolve_api_key_by_env("OPENAI_API_KEY").map_err(|e| anyhow::anyhow!(e))?;

    match action {
        VoiceAction::Speak { text, voice } => {
            use rustant_core::voice::{OpenAiTtsProvider, SynthesisRequest, TtsProvider};

            println!("Synthesizing: \"{text}\" (voice: {voice})");
            let tts = OpenAiTtsProvider::new(&api_key);
            let request = SynthesisRequest::new(&text).with_voice(&voice);
            let result = tts
                .synthesize(&request)
                .await
                .map_err(|e| anyhow::anyhow!("TTS synthesis failed: {e}"))?;

            println!("  Duration:    {:.2}s", result.duration_secs);
            println!("  Sample rate: {} Hz", result.audio.sample_rate);
            println!("  Channels:    {}", result.audio.channels);
            println!("  Samples:     {}", result.audio.samples.len());
            println!("  Characters:  {}", result.characters_used);

            // Play audio directly through speakers
            println!("  Playing audio...");
            rustant_core::voice::audio_io::play_audio(&result.audio)
                .await
                .map_err(|e| anyhow::anyhow!("Audio playback failed: {e}"))?;
            println!("  Playback complete.");

            Ok(())
        }
        VoiceAction::Roundtrip { text } => {
            use rustant_core::voice::{
                OpenAiSttProvider, OpenAiTtsProvider, SttProvider, SynthesisRequest, TtsProvider,
            };

            println!("Original: \"{text}\"");

            // TTS: text -> audio
            println!("  [1/2] Synthesizing speech...");
            let tts = OpenAiTtsProvider::new(&api_key);
            let request = SynthesisRequest::new(&text);
            let tts_result = tts
                .synthesize(&request)
                .await
                .map_err(|e| anyhow::anyhow!("TTS synthesis failed: {e}"))?;
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
                .map_err(|e| anyhow::anyhow!("STT transcription failed: {e}"))?;
            println!("Transcribed: \"{}\"", transcription.text);
            if let Some(lang) = &transcription.language {
                println!("  Language:   {lang}");
            }
            println!("  Duration:   {:.2}s", transcription.duration_secs);
            println!("  Confidence: {:.2}", transcription.confidence);

            Ok(())
        }
    }
}

#[allow(unused_variables)]
pub async fn handle_browser(action: BrowserAction, workspace: &Path) -> anyhow::Result<()> {
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

        BrowserAction::Connect { port } => {
            #[cfg(feature = "browser")]
            {
                use rustant_core::browser::{
                    BrowserConnectionInfo, BrowserSessionStore, CdpClient, ChromiumCdpClient,
                };

                let url = format!("http://127.0.0.1:{}", port);
                println!("Connecting to Chrome at {}...", url);

                let client = ChromiumCdpClient::connect(&url, port)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to connect: {}", e))?;

                let tabs = client
                    .list_tabs()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to list tabs: {}", e))?;

                println!("Connected! {} tab(s) open:", tabs.len());
                for tab in &tabs {
                    let marker = if tab.active { " *" } else { "" };
                    println!("  [{}]{} {} — {}", tab.id, marker, tab.title, tab.url);
                }

                // Save session for REPL reconnection
                let info = BrowserConnectionInfo {
                    debug_port: port,
                    ws_url: None,
                    user_data_dir: None,
                    tabs,
                    active_tab_id: client.active_tab_id().await.ok(),
                    saved_at: chrono::Utc::now(),
                };
                if let Err(e) = BrowserSessionStore::save(workspace, &info) {
                    tracing::warn!("Failed to save browser session: {}", e);
                } else {
                    println!("Session saved. Rustant REPL will auto-reconnect.");
                }

                Ok(())
            }

            #[cfg(not(feature = "browser"))]
            {
                let _ = port;
                eprintln!(
                    "Browser feature not enabled.\n\
                     Recompile with: cargo build --features browser"
                );
                Ok(())
            }
        }

        BrowserAction::Launch { port, headless } => {
            #[cfg(feature = "browser")]
            {
                use rustant_core::browser::{
                    BrowserConnectionInfo, BrowserSessionStore, CdpClient, ChromiumCdpClient,
                };
                use rustant_core::config::BrowserConfig;

                let config = BrowserConfig {
                    enabled: true,
                    headless,
                    debug_port: port,
                    ..Default::default()
                };

                println!(
                    "Launching Chrome with remote debugging on port {}{}...",
                    port,
                    if headless { " (headless)" } else { "" }
                );

                let client = ChromiumCdpClient::launch_with_debugging(&config)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to launch Chrome: {}", e))?;

                let tabs = client
                    .list_tabs()
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to list tabs: {}", e))?;

                println!("Chrome launched. {} tab(s) open.", tabs.len());

                // Save session for REPL reconnection
                let info = BrowserConnectionInfo {
                    debug_port: port,
                    ws_url: None,
                    user_data_dir: None,
                    tabs,
                    active_tab_id: client.active_tab_id().await.ok(),
                    saved_at: chrono::Utc::now(),
                };
                if let Err(e) = BrowserSessionStore::save(workspace, &info) {
                    tracing::warn!("Failed to save browser session: {}", e);
                } else {
                    println!("Session saved. Rustant REPL will auto-reconnect.");
                }

                // Keep Chrome alive until user presses Enter
                println!("\nPress Enter to close Chrome...");
                let _ = std::io::stdin().read_line(&mut String::new());
                let _ = client.close().await;
                BrowserSessionStore::clear(workspace).ok();
                println!("Chrome closed.");

                Ok(())
            }

            #[cfg(not(feature = "browser"))]
            {
                let _ = (port, headless);
                eprintln!(
                    "Browser feature not enabled.\n\
                     Recompile with: cargo build --features browser"
                );
                Ok(())
            }
        }

        BrowserAction::Status => {
            #[cfg(feature = "browser")]
            {
                use rustant_core::browser::{BrowserSessionStore, CdpClient, ChromiumCdpClient};

                // Check for saved session
                match BrowserSessionStore::load(workspace) {
                    Ok(Some(info)) => {
                        println!("Saved browser session found:");
                        println!("  Port: {}", info.debug_port);
                        if let Some(ref ws) = info.ws_url {
                            println!("  WebSocket: {}", ws);
                        }
                        println!(
                            "  Saved at: {}",
                            info.saved_at.format("%Y-%m-%d %H:%M:%S UTC")
                        );
                        println!("  Tabs: {}", info.tabs.len());
                        for tab in &info.tabs {
                            let marker = if tab.active { " *" } else { "" };
                            println!("    [{}]{} {} — {}", tab.id, marker, tab.title, tab.url);
                        }

                        // Try to actually connect to verify it's still alive
                        let url = format!("http://127.0.0.1:{}", info.debug_port);
                        match ChromiumCdpClient::connect(&url, info.debug_port).await {
                            Ok(client) => {
                                let live_tabs = client.list_tabs().await.unwrap_or_default();
                                println!(
                                    "\n  Status: \x1b[32mConnected\x1b[0m ({} live tabs)",
                                    live_tabs.len()
                                );
                            }
                            Err(_) => {
                                println!(
                                    "\n  Status: \x1b[31mDisconnected\x1b[0m (Chrome not reachable)"
                                );
                                println!("  Clearing stale session...");
                                BrowserSessionStore::clear(workspace).ok();
                            }
                        }
                    }
                    Ok(None) => {
                        println!("No saved browser session.");
                        println!("\nTo start one:");
                        println!(
                            "  rustant browser launch            # Launch Chrome with debugging"
                        );
                        println!(
                            "  rustant browser connect -p 9222   # Connect to existing Chrome"
                        );
                    }
                    Err(e) => {
                        eprintln!("Error reading session: {}", e);
                    }
                }

                Ok(())
            }

            #[cfg(not(feature = "browser"))]
            {
                eprintln!(
                    "Browser feature not enabled.\n\
                     Recompile with: cargo build --features browser"
                );
                Ok(())
            }
        }
    }
}

/// Resolve the path to the `frontend/` directory containing static assets.
///
/// Checks several locations in order:
/// 1. `RUSTANT_FRONTEND_DIR` environment variable
/// 2. `./frontend` relative to the current executable
/// 3. `./rustant-ui/frontend` relative to the working directory (development)
fn resolve_frontend_dir() -> Option<std::path::PathBuf> {
    // 1. Explicit env var
    if let Ok(dir) = std::env::var("RUSTANT_FRONTEND_DIR") {
        let p = std::path::PathBuf::from(dir);
        if p.join("index.html").exists() {
            return Some(p);
        }
    }

    // 2. Next to the executable
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let p = exe_dir.join("frontend");
        if p.join("index.html").exists() {
            return Some(p);
        }
    }

    // 3. Workspace development layout
    let workspace_candidates = ["rustant-ui/frontend", "frontend"];
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in &workspace_candidates {
            let p = cwd.join(candidate);
            if p.join("index.html").exists() {
                return Some(p);
            }
        }
    }

    None
}

async fn handle_ui(port: u16) -> anyhow::Result<()> {
    use tower_http::services::{ServeDir, ServeFile};

    println!("Starting Rustant Dashboard...");
    println!();

    // Resolve frontend static assets
    let frontend_dir = resolve_frontend_dir();
    if let Some(ref dir) = frontend_dir {
        println!("Frontend directory: {}", dir.display());
    } else {
        println!("Warning: Frontend directory not found.");
        println!("  Set RUSTANT_FRONTEND_DIR or run from the workspace root.");
        println!("  The API will still be available but the dashboard UI won't load.");
        println!();
    }

    // Start the gateway server in the background
    let config = rustant_core::gateway::GatewayConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port,
        auth_tokens: Vec::new(),
        max_connections: 50,
        session_timeout_secs: 3600,
        broadcast_capacity: 256,
    };

    let gw: rustant_core::gateway::SharedGateway = std::sync::Arc::new(tokio::sync::Mutex::new(
        rustant_core::gateway::GatewayServer::new(config.clone()),
    ));

    let gw_for_server = gw.clone();

    // Build the API router
    let api_router = rustant_core::gateway::gateway_router(gw_for_server);

    // Merge with static file serving if frontend is available
    let addr = format!("127.0.0.1:{port}");

    tokio::spawn(async move {
        let listener = match tokio::net::TcpListener::bind(&addr).await {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Failed to bind to {addr}: {e}");
                return;
            }
        };

        if let Some(dir) = frontend_dir {
            // Serve frontend files as fallback — "/" returns index.html
            let index_file = dir.join("index.html");
            let static_service = ServeDir::new(&dir).not_found_service(ServeFile::new(&index_file));
            let app = api_router.fallback_service(static_service);

            if let Err(e) = axum::serve(listener, app).await {
                eprintln!("Gateway error: {e}");
            }
        } else {
            // API-only mode (no frontend)
            if let Err(e) = axum::serve(listener, api_router).await {
                eprintln!("Gateway error: {e}");
            }
        }
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    println!();
    println!("Rustant Dashboard running at:");
    println!("  http://127.0.0.1:{port}");
    println!();
    println!("API endpoints:");
    println!("  http://127.0.0.1:{port}/api/status");
    println!("  http://127.0.0.1:{port}/api/sessions");
    println!("  http://127.0.0.1:{port}/api/config");
    println!("  http://127.0.0.1:{port}/api/metrics");
    println!("  http://127.0.0.1:{port}/health");
    println!("  ws://127.0.0.1:{port}/ws");
    println!();
    println!("Press Ctrl+C to stop.");

    // Wait forever (or until Ctrl+C)
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
    }
}

pub async fn handle_canvas(action: CanvasAction) -> anyhow::Result<()> {
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
                    "Unknown content type '{content_type}'. Valid types: html, markdown, code, chart, table, form, image, diagram"
                )
            })?;

            // For structured types, try to render them
            match ct {
                ContentType::Chart => {
                    let spec: rustant_core::canvas::ChartSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid chart JSON: {e}"))?;
                    let config = rustant_core::canvas::render_chart_config(&spec);
                    println!("Chart.js config:\n{config}");
                }
                ContentType::Table => {
                    let spec: rustant_core::canvas::TableSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid table JSON: {e}"))?;
                    let html = rustant_core::canvas::render_table_html(&spec);
                    println!("Table HTML:\n{html}");
                }
                ContentType::Form => {
                    let spec: rustant_core::canvas::FormSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid form JSON: {e}"))?;
                    let html = rustant_core::canvas::render_form_html(&spec);
                    println!("Form HTML:\n{html}");
                }
                ContentType::Diagram => {
                    let spec: rustant_core::canvas::DiagramSpec = serde_json::from_str(&content)
                        .map_err(|e| anyhow::anyhow!("Invalid diagram JSON: {e}"))?;
                    let mermaid = rustant_core::canvas::render_diagram_mermaid(&spec);
                    println!("Mermaid:\n{mermaid}");
                }
                _ => {
                    println!("Content ({content_type}):\n{content}");
                }
            }

            let id = canvas
                .push(&target, ct, content)
                .map_err(|e| anyhow::anyhow!("Canvas push failed: {e}"))?;
            println!("\nPushed to canvas (id: {id})");
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

pub async fn handle_skill(action: SkillAction) -> anyhow::Result<()> {
    use rustant_core::skills::{SkillLoader, parse_skill_md, validate_skill};

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
                println!("No skill files found in: {skills_dir}");
                println!("Create SKILL.md files in that directory to define skills.");
            } else {
                println!("Skills in {skills_dir}:");
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
                .map_err(|e| anyhow::anyhow!("Failed to read '{path}': {e}"))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{path}': {e}"))?;

            println!("Skill: {}", skill.name);
            println!("Version: {}", skill.version);
            println!("Description: {}", skill.description);
            if let Some(author) = &skill.author {
                println!("Author: {author}");
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
                        println!("    Body: {body_preview}");
                    }
                }
            }
            Ok(())
        }
        SkillAction::Validate { path } => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read '{path}': {e}"))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{path}': {e}"))?;

            // Validate with empty available tools/secrets (strict check)
            let result = validate_skill(&skill, &[], &[]);

            println!("Validation result for '{}':", skill.name);
            println!("  Valid: {}", result.is_valid);
            println!("  Risk Level: {:?}", result.risk_level);

            if !result.warnings.is_empty() {
                println!("\n  Warnings:");
                for warning in &result.warnings {
                    println!("    - {warning}");
                }
            }

            if !result.errors.is_empty() {
                println!("\n  Errors:");
                for error in &result.errors {
                    println!("    - {error}");
                }
            }

            if result.is_valid && result.warnings.is_empty() {
                println!("\n  Skill passed all validation checks.");
            }
            Ok(())
        }
        SkillAction::Load { path } => {
            let content = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read '{path}': {e}"))?;
            let skill = parse_skill_md(&content)
                .map_err(|e| anyhow::anyhow!("Failed to parse '{path}': {e}"))?;

            println!("Loaded skill: {}", skill.name);
            let json = serde_json::to_string_pretty(&skill)?;
            println!("{json}");
            Ok(())
        }
    }
}

pub async fn handle_plugin(action: PluginAction) -> anyhow::Result<()> {
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
                println!("No plugins found in: {plugins_dir}");
                println!("Place .so/.dll/.dylib plugin files in that directory.");
            } else {
                println!("Plugin files in {plugins_dir}:");
                for path in &found {
                    println!("  {}", path.display());
                }
                println!("\nTotal: {} plugin files", found.len());
            }
            Ok(())
        }
        PluginAction::Info { name } => {
            println!("Plugin: {name}");
            println!("  Status: not loaded");
            println!("  (Plugin loading requires an active agent session)");
            Ok(())
        }
    }
}

pub async fn handle_update(action: UpdateAction) -> anyhow::Result<()> {
    use rustant_core::updater::{CURRENT_VERSION, UpdateChecker, UpdateConfig, Updater};

    match action {
        UpdateAction::Check => {
            println!("Current version: {CURRENT_VERSION}");
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
                            println!("Release: {url}");
                        }
                        if let Some(notes) = &result.release_notes
                            && !notes.is_empty()
                        {
                            let preview = if notes.len() > 200 {
                                format!("{}...", &notes[..197])
                            } else {
                                notes.clone()
                            };
                            println!("\nRelease notes:\n{preview}");
                        }
                        println!("\nRun `rustant update install` to update.");
                    } else {
                        println!("You are running the latest version.");
                    }
                }
                Err(e) => {
                    println!("Failed to check for updates: {e}");
                    println!("You can check manually at:");
                    println!("  https://github.com/DevJadhav/Rustant/releases");
                }
            }
            Ok(())
        }
        UpdateAction::Install => {
            println!("Current version: {CURRENT_VERSION}");
            println!("Downloading and installing latest version...");

            match Updater::update() {
                Ok(()) => {
                    println!("Update installed successfully!");
                    println!("Restart rustant to use the new version.");
                }
                Err(e) => {
                    println!("Update failed: {e}");
                    println!("You can update manually by downloading from:");
                    println!("  https://github.com/DevJadhav/Rustant/releases");
                }
            }
            Ok(())
        }
    }
}

/// Connect to configured external MCP servers and log results.
///
/// For each server with `auto_connect: true`, spawns the process, performs
/// the MCP initialize handshake, and lists available tools. Logs warnings
/// for servers that fail to connect.
pub async fn connect_mcp_servers(
    configs: &[rustant_core::ExternalMcpServerConfig],
) -> Vec<(String, Vec<String>)> {
    let mut connected = Vec::new();

    for config in configs {
        if !config.auto_connect {
            continue;
        }

        tracing::info!(name = %config.name, command = %config.command, "Connecting to MCP server");

        match rustant_mcp::transport::ProcessTransport::spawn(
            &config.command,
            &config.args,
            &config.env,
        )
        .await
        {
            Ok((mut transport, _child)) => {
                // Send initialize request
                let init_req = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": {"name": "rustant", "version": "1.0"}
                    }
                });

                use rustant_mcp::transport::Transport;

                if let Err(e) = transport
                    .write_message(&serde_json::to_string(&init_req).unwrap())
                    .await
                {
                    tracing::warn!(name = %config.name, error = %e, "MCP init write failed");
                    continue;
                }

                match transport.read_message().await {
                    Ok(Some(response)) => {
                        tracing::info!(
                            name = %config.name,
                            "MCP server connected: {}",
                            &response[..response.len().min(200)]
                        );

                        // Send initialized notification
                        let notif = serde_json::json!({
                            "jsonrpc": "2.0",
                            "method": "notifications/initialized"
                        });
                        let _ = transport
                            .write_message(&serde_json::to_string(&notif).unwrap())
                            .await;

                        // List tools
                        let list_req = serde_json::json!({
                            "jsonrpc": "2.0",
                            "id": 2,
                            "method": "tools/list",
                            "params": {}
                        });
                        let _ = transport
                            .write_message(&serde_json::to_string(&list_req).unwrap())
                            .await;

                        if let Ok(Some(tools_resp)) = transport.read_message().await
                            && let Ok(parsed) =
                                serde_json::from_str::<serde_json::Value>(&tools_resp)
                        {
                            let tools = parsed["result"]["tools"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default();

                            tracing::info!(
                                name = %config.name,
                                tools_count = tools.len(),
                                "MCP server tools discovered"
                            );
                            connected.push((config.name.clone(), tools));
                        }
                    }
                    Ok(None) => {
                        tracing::warn!(name = %config.name, "MCP server closed before init response");
                    }
                    Err(e) => {
                        tracing::warn!(name = %config.name, error = %e, "MCP init read failed");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(name = %config.name, error = %e, "Failed to start MCP server");
            }
        }
    }

    connected
}

/// Run the voice command loop with "hey rustant" wake word detection.
///
/// Continuously listens for the wake word, transcribes the command, processes it
/// through the agent, and speaks the response back.
#[cfg(feature = "voice")]
pub async fn run_voice_mode(
    config: rustant_core::AgentConfig,
    workspace: std::path::PathBuf,
) -> anyhow::Result<()> {
    use rustant_core::voice::{OpenAiSttProvider, OpenAiTtsProvider, SttWakeDetector};
    use std::sync::Arc;

    let api_key =
        rustant_core::resolve_api_key_by_env("OPENAI_API_KEY").map_err(|e| anyhow::anyhow!(e))?;

    println!("Voice mode active. Say \"hey rustant\" to give a command.");
    println!("Press Ctrl+C to exit.\n");

    let voice_config = config.voice.clone().unwrap_or_default();
    let stt: Arc<dyn rustant_core::voice::SttProvider> = Arc::new(OpenAiSttProvider::new(&api_key));
    let tts: Arc<dyn rustant_core::voice::TtsProvider> = Arc::new(OpenAiTtsProvider::new(&api_key));
    let wake_detector: Box<dyn rustant_core::voice::WakeWordDetector> =
        Box::new(SttWakeDetector::new(
            Box::new(OpenAiSttProvider::new(&api_key)),
            voice_config.wake_words.clone(),
            voice_config.wake_sensitivity,
        ));

    let pipeline = rustant_core::voice::VoicePipeline::new(stt, tts, wake_detector, voice_config)?;

    // Set up agent with a simple print callback
    let provider = rustant_core::create_provider(&config.llm)?;
    let callback = Arc::new(crate::repl::CliCallback::new(false));
    let mut agent = rustant_core::Agent::new(provider, config.clone(), callback);
    agent.set_output_redactor(rustant_core::create_basic_redactor());

    // Register tools
    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.clone());
    rustant_ml::register_ml_tools(&mut registry, workspace.clone());
    let registry_arc = Arc::new(registry);
    for def in registry_arc.list_definitions() {
        let name = def.name.clone();
        let ws = workspace.clone();
        let reg = registry_arc.clone();
        agent.register_tool(def, rustant_core::types::RiskLevel::Read, move |args| {
            let n = name.clone();
            let w = ws.clone();
            let r = reg.clone();
            Box::pin(async move {
                r.invoke(&n, args, &w).await.map_err(|e| {
                    rustant_core::error::ToolError::ExecutionFailed {
                        tool: n,
                        message: e.to_string(),
                    }
                })
            })
        });
    }

    loop {
        match pipeline.listen_for_command().await {
            Ok(Some(command)) => {
                println!("  Heard: \"{}\"", command);
                match agent.process_task(&command).await {
                    Ok(result) => {
                        // Speak the response
                        if !result.response.is_empty() {
                            if let Err(e) = pipeline.speak(&result.response).await {
                                eprintln!("  TTS error: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  Agent error: {}", e);
                    }
                }
            }
            Ok(None) => {
                // No wake word detected, continue listening
            }
            Err(e) => {
                eprintln!("Voice error: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
    }
}

// --- Security & compliance CLI handlers ---

pub async fn handle_scan(action: ScanAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        ScanAction::All { path, format } => {
            println!("Running all security scanners on: {path}");
            let args = serde_json::json!({
                "path": path,
                "scanners": "all",
                "format": format,
            });
            run_security_tool(&registry, "security_scan", args).await
        }
        ScanAction::Sast { path, languages } => {
            println!("Running SAST scan on: {path}");
            let mut args = serde_json::json!({ "path": path });
            if let Some(langs) = languages {
                args["languages"] = serde_json::Value::String(langs);
            }
            run_security_tool(&registry, "sast_scan", args).await
        }
        ScanAction::Sca { path } => {
            println!("Running SCA scan on: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "sca_scan", args).await
        }
        ScanAction::Secrets { path, history } => {
            println!("Running secrets scan on: {path}");
            let args = serde_json::json!({ "path": path, "history": history });
            run_security_tool(&registry, "secrets_scan", args).await
        }
        ScanAction::Iac { path } => {
            println!("Running IaC scan on: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "iac_scan", args).await
        }
        ScanAction::Container { target } => {
            println!("Scanning container: {target}");
            let args = serde_json::json!({ "image": target });
            run_security_tool(&registry, "container_scan", args).await
        }
        ScanAction::SupplyChain { path } => {
            println!("Checking supply chain security: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "supply_chain_check", args).await
        }
    }
}

pub async fn handle_review(action: ReviewAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        ReviewAction::Diff { base } => {
            println!("Reviewing changes since: {base}");
            let args = serde_json::json!({ "diff": base });
            run_security_tool(&registry, "analyze_diff", args).await
        }
        ReviewAction::Path { path } => {
            println!("Reviewing: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "code_review", args).await
        }
        ReviewAction::Fix { auto } => {
            if auto {
                println!("Applying high-confidence fixes...");
                let args = serde_json::json!({ "mode": "auto" });
                run_security_tool(&registry, "apply_fix", args).await
            } else {
                println!("Generating fix suggestions...");
                let args = serde_json::json!({});
                run_security_tool(&registry, "suggest_fix", args).await
            }
        }
    }
}

pub async fn handle_quality(action: QualityAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        QualityAction::Score { path } => {
            println!("Calculating quality score for: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "quality_score", args).await
        }
        QualityAction::Complexity { path } => {
            println!("Analyzing complexity: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "complexity_check", args).await
        }
        QualityAction::DeadCode { path } => {
            println!("Detecting dead code: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "dead_code_detect", args).await
        }
        QualityAction::Duplicates { path } => {
            println!("Finding duplicates: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "duplicate_detect", args).await
        }
        QualityAction::Debt { path } => {
            println!("Generating debt report: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "tech_debt_report", args).await
        }
    }
}

pub async fn handle_license(action: LicenseAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        LicenseAction::Check { path } => {
            println!("Checking license compliance: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "license_check", args).await
        }
        LicenseAction::Summary { path } => {
            println!("License summary: {path}");
            let args = serde_json::json!({ "path": path, "action": "summary" });
            run_security_tool(&registry, "license_check", args).await
        }
    }
}

pub async fn handle_sbom(action: SbomAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        SbomAction::Generate {
            path,
            format,
            output,
        } => {
            println!("Generating SBOM ({format}): {path}");
            let mut args = serde_json::json!({ "path": path, "format": format });
            if let Some(out) = output {
                args["output"] = serde_json::Value::String(out);
            }
            run_security_tool(&registry, "sbom_generate", args).await
        }
        SbomAction::Diff { old, new } => {
            println!("Comparing SBOMs: {old} vs {new}");
            let args = serde_json::json!({ "old": old, "new": new });
            run_security_tool(&registry, "sbom_diff", args).await
        }
    }
}

pub async fn handle_compliance(action: ComplianceAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        ComplianceAction::Report { framework, format } => {
            println!("Generating {framework} compliance report ({format})");
            let args = serde_json::json!({
                "framework": framework,
                "format": format,
            });
            run_security_tool(&registry, "compliance_report", args).await
        }
        ComplianceAction::Status => {
            println!("Compliance status summary:");
            let args = serde_json::json!({ "action": "status" });
            run_security_tool(&registry, "compliance_report", args).await
        }
    }
}

pub async fn handle_audit(action: AuditAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        AuditAction::Export { start, end, format } => {
            println!("Exporting audit trail (format: {format})");
            let mut args = serde_json::json!({ "format": format });
            if let Some(s) = start {
                args["start"] = serde_json::Value::String(s);
            }
            if let Some(e) = end {
                args["end"] = serde_json::Value::String(e);
            }
            run_security_tool(&registry, "audit_export", args).await
        }
        AuditAction::Verify => {
            println!("Verifying audit trail integrity...");
            let args = serde_json::json!({ "action": "verify" });
            run_security_tool(&registry, "audit_export", args).await
        }
    }
}

pub async fn handle_risk(action: RiskAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        RiskAction::Score { path } => {
            println!("Calculating risk score: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "risk_score", args).await
        }
        RiskAction::Trend => {
            println!("Risk trend analysis:");
            let args = serde_json::json!({ "action": "trend" });
            run_security_tool(&registry, "risk_score", args).await
        }
    }
}

pub async fn handle_policy(action: PolicyAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        PolicyAction::List => {
            println!("Active policies:");
            let args = serde_json::json!({ "action": "list" });
            run_security_tool(&registry, "policy_check", args).await
        }
        PolicyAction::Check { path } => {
            println!("Checking policies: {path}");
            let args = serde_json::json!({ "path": path });
            run_security_tool(&registry, "policy_check", args).await
        }
        PolicyAction::Validate { path } => {
            println!("Validating policy file: {path}");
            let args = serde_json::json!({ "action": "validate", "path": path });
            run_security_tool(&registry, "policy_check", args).await
        }
    }
}

pub async fn handle_alerts(action: AlertsAction, workspace: &Path) -> anyhow::Result<()> {
    let _config = rustant_core::config::load_config(Some(workspace), None)
        .map_err(|e| anyhow::anyhow!("Config error: {e}"))?;

    let mut registry = rustant_tools::registry::ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, workspace.to_path_buf());
    rustant_security::register_security_tools(&mut registry);

    match action {
        AlertsAction::List { severity } => {
            println!("Active alerts:");
            let mut args = serde_json::json!({ "action": "list" });
            if let Some(sev) = severity {
                args["severity"] = serde_json::Value::String(sev);
            }
            run_security_tool(&registry, "alert_status", args).await
        }
        AlertsAction::Triage => {
            println!("Running AI-powered alert triage...");
            let args = serde_json::json!({});
            run_security_tool(&registry, "alert_triage", args).await
        }
        AlertsAction::Acknowledge { id } => {
            println!("Acknowledging alert: {id}");
            let args = serde_json::json!({ "action": "acknowledge", "alert_id": id });
            run_security_tool(&registry, "alert_status", args).await
        }
        AlertsAction::Resolve { id } => {
            println!("Resolving alert: {id}");
            let args = serde_json::json!({ "action": "resolve", "alert_id": id });
            run_security_tool(&registry, "alert_status", args).await
        }
    }
}

/// Helper: execute a security tool from the registry and print its output.
async fn run_security_tool(
    registry: &rustant_tools::registry::ToolRegistry,
    tool_name: &str,
    args: serde_json::Value,
) -> anyhow::Result<()> {
    match registry.execute(tool_name, args).await {
        Ok(output) => {
            println!("{}", output.content);
            Ok(())
        }
        Err(e) => {
            eprintln!("Tool error: {e}");
            Err(anyhow::anyhow!("{e}"))
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
