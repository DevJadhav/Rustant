//! Rustant CLI — Terminal interface for the Rustant autonomous agent.
//!
//! Provides both single-task and interactive REPL modes.

pub(crate) mod channel_setup;
mod commands;
mod repl;
pub(crate) mod setup;
mod tui;

use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

/// Rustant: Your Rust-Powered Autonomous Assistant
#[derive(Parser, Debug)]
#[command(name = "rustant", version, about, long_about = None)]
struct Cli {
    /// Task to execute (starts interactive mode if omitted)
    task: Option<String>,

    /// LLM model to use
    #[arg(short, long)]
    model: Option<String>,

    /// Workspace directory
    #[arg(short, long, default_value = ".")]
    workspace: PathBuf,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Disable TUI, use simple REPL
    #[arg(long)]
    no_tui: bool,

    /// Approval mode: safe, cautious, paranoid, yolo
    #[arg(long)]
    approval: Option<String>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long)]
    quiet: bool,

    /// Subcommand
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Interactive provider setup wizard
    Setup,
    /// Manage messaging channels
    Channel {
        #[command(subcommand)]
        action: ChannelAction,
    },
    /// Manage OAuth authentication for LLM providers and channels
    Auth {
        #[command(subcommand)]
        action: AuthAction,
    },
    /// Manage workflows
    Workflow {
        #[command(subcommand)]
        action: WorkflowAction,
    },
    /// Manage scheduled jobs
    Cron {
        #[command(subcommand)]
        action: CronAction,
    },
    /// Voice operations (TTS/STT via OpenAI)
    Voice {
        #[command(subcommand)]
        action: VoiceAction,
    },
    /// Browser automation operations
    Browser {
        #[command(subcommand)]
        action: BrowserAction,
    },
    /// Launch the Tauri dashboard UI
    Ui {
        /// Gateway port to connect to
        #[arg(short, long, default_value = "18790")]
        port: u16,
    },
    /// Canvas operations (push, clear, snapshot)
    Canvas {
        #[command(subcommand)]
        action: CanvasAction,
    },
    /// Manage skills (SKILL.md files)
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },
    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
    /// Check for updates or install the latest version
    Update {
        #[command(subcommand)]
        action: UpdateAction,
    },
}

#[derive(clap::Subcommand, Debug)]
enum CanvasAction {
    /// Push content to the canvas
    Push {
        /// Content type (html, markdown, code, chart, table, form, image, diagram)
        content_type: String,
        /// Content string (raw text, JSON for chart/table/form/diagram)
        content: String,
    },
    /// Clear the canvas
    Clear,
    /// Get a snapshot of the canvas state
    Snapshot,
}

#[derive(clap::Subcommand, Debug)]
enum SkillAction {
    /// List loaded skills
    List {
        /// Directory to scan for skill files
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Show details of a skill
    Info {
        /// Path to a SKILL.md file
        path: String,
    },
    /// Validate a skill file for security issues
    Validate {
        /// Path to a SKILL.md file
        path: String,
    },
    /// Load a skill file and show parsed definition
    Load {
        /// Path to a SKILL.md file
        path: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum PluginAction {
    /// List loaded plugins
    List {
        /// Directory to scan for plugin files
        #[arg(short, long)]
        dir: Option<String>,
    },
    /// Show plugin info
    Info {
        /// Plugin name
        name: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum UpdateAction {
    /// Check for available updates
    Check,
    /// Download and install the latest version
    Install,
}

#[derive(clap::Subcommand, Debug)]
enum WorkflowAction {
    /// List available workflow definitions
    List,
    /// Show details of a workflow
    Show {
        /// Workflow name
        name: String,
    },
    /// Run a workflow
    Run {
        /// Workflow name
        name: String,
        /// Input parameters as key=value pairs
        #[arg(short, long)]
        input: Vec<String>,
    },
    /// List active workflow runs
    Runs,
    /// Resume a paused workflow run
    Resume {
        /// Run ID
        run_id: String,
    },
    /// Cancel a running workflow
    Cancel {
        /// Run ID
        run_id: String,
    },
    /// Show status of a workflow run
    Status {
        /// Run ID
        run_id: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum CronAction {
    /// List all scheduled cron jobs
    List,
    /// Add a new cron job
    Add {
        /// Job name
        name: String,
        /// Cron expression (e.g., "0 0 9 * * * *")
        schedule: String,
        /// Task to execute
        task: String,
    },
    /// Manually trigger a cron job
    Run {
        /// Job name
        name: String,
    },
    /// Disable a cron job
    Disable {
        /// Job name
        name: String,
    },
    /// Enable a cron job
    Enable {
        /// Job name
        name: String,
    },
    /// Remove a cron job
    Remove {
        /// Job name
        name: String,
    },
    /// List background jobs
    Jobs,
    /// Cancel a background job
    CancelJob {
        /// Job ID
        job_id: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum VoiceAction {
    /// Synthesize text to speech and display audio stats
    Speak {
        /// Text to synthesize
        text: String,
        /// Voice name (alloy, echo, fable, onyx, nova, shimmer)
        #[arg(short, long, default_value = "alloy")]
        voice: String,
    },
    /// TTS→STT roundtrip: synthesize text, then transcribe it back
    Roundtrip {
        /// Text to synthesize and roundtrip
        text: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum BrowserAction {
    /// Test browser automation by navigating to a URL
    Test {
        /// URL to navigate to
        #[arg(default_value = "https://example.com")]
        url: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum ConfigAction {
    /// Create default configuration file
    Init,
    /// Show current configuration
    Show,
}

#[derive(clap::Subcommand, Debug)]
enum ChannelAction {
    /// List all configured channels and their status
    List,
    /// Interactive channel setup wizard
    Setup {
        /// Channel to configure (slack, discord, telegram, email, sms, imessage). Omit for menu.
        channel: Option<String>,
    },
    /// Test a channel's connection (connect + disconnect)
    Test {
        /// Channel name (e.g., slack, telegram, discord)
        name: String,
    },
    /// Slack-specific operations (uses stored OAuth token)
    Slack {
        #[command(subcommand)]
        action: SlackCommand,
    },
}

#[derive(clap::Subcommand, Debug)]
enum SlackCommand {
    /// Send a message to a Slack channel
    Send {
        /// Channel name or ID (e.g., general, C04M40V9B61)
        channel: String,
        /// Message text
        message: String,
    },
    /// Read recent message history from a channel
    History {
        /// Channel name or ID
        channel: String,
        /// Number of messages to fetch (default: 10)
        #[arg(short = 'n', long, default_value = "10")]
        limit: usize,
    },
    /// List all channels in the workspace
    Channels,
    /// List all users in the workspace
    Users,
    /// Get info about a specific channel
    Info {
        /// Channel ID (e.g., C04M40V9B61)
        channel: String,
    },
    /// Add an emoji reaction to a message
    React {
        /// Channel ID
        channel: String,
        /// Message timestamp (e.g., 1770007692.977549)
        timestamp: String,
        /// Emoji name without colons (e.g., thumbsup, rocket)
        emoji: String,
    },
    /// List files shared in the workspace (optionally filtered by channel)
    Files {
        /// Optional channel ID to filter files
        channel: Option<String>,
    },
    /// Show workspace/team information
    Team,
    /// List user groups (e.g., @engineering)
    Groups,
    /// Send a direct message to a user
    Dm {
        /// User ID (e.g., U0AC521V7UK)
        user: String,
        /// Message text
        message: String,
    },
    /// Reply in a thread
    Thread {
        /// Channel ID
        channel: String,
        /// Parent message timestamp
        timestamp: String,
        /// Reply text
        message: String,
    },
    /// Join a channel
    Join {
        /// Channel ID
        channel: String,
    },
}

#[derive(clap::Subcommand, Debug)]
enum AuthAction {
    /// Show current authentication status for all providers
    Status,
    /// Login to an LLM provider or channel via OAuth browser flow
    Login {
        /// Provider name (e.g., openai, gemini, slack, discord, teams, whatsapp)
        provider: String,

        /// Override the redirect URI (e.g. an ngrok HTTPS tunnel URL).
        /// Required for providers like Slack that mandate HTTPS redirect URIs.
        /// Example: --redirect-uri https://abc123.ngrok-free.app/auth/callback
        #[arg(long)]
        redirect_uri: Option<String>,
    },
    /// Remove stored OAuth tokens for a provider or channel
    Logout {
        /// Provider name (e.g., openai, gemini, slack, discord, teams, whatsapp)
        provider: String,
    },
    /// Manually refresh an OAuth token for a provider or channel
    Refresh {
        /// Provider name (e.g., openai, gemini, slack, discord, teams, whatsapp)
        provider: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Set up tracing: human-readable stderr + JSON file logging
    let filter = match cli.verbose {
        0 if cli.quiet => "error",
        0 => "info",
        1 => "debug",
        _ => "trace",
    };

    // Human-readable layer for stderr (always active)
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_filter(EnvFilter::new(filter));

    // JSON file layer for structured logging
    let log_dir = directories::ProjectDirs::from("dev", "rustant", "rustant")
        .map(|d| d.data_dir().join("logs"))
        .unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_appender = tracing_appender::rolling::daily(&log_dir, "rustant.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_filter(EnvFilter::new("debug"));

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(json_layer)
        .init();

    // Resolve workspace
    let workspace = cli
        .workspace
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // Handle subcommands
    if let Some(command) = cli.command {
        return commands::handle_command(command, &workspace).await;
    }

    // Load configuration
    let mut config = rustant_core::config::load_config(Some(&workspace), None)
        .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

    // First-run detection: if no config file exists, prompt setup wizard
    if !rustant_core::config_exists(Some(&workspace)) {
        let detected = setup::detect_env_api_keys();
        if !detected.is_empty() {
            println!(
                "\n  Detected API key(s) in environment: {}",
                detected
                    .iter()
                    .map(|(_, name)| name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        println!("\n  No configuration found. Starting setup wizard...\n");
        if let Err(e) = setup::run_setup(&workspace).await {
            eprintln!("  Setup failed: {}. Using defaults.\n", e);
        } else {
            // Reload configuration after setup
            config = rustant_core::config::load_config(Some(&workspace), None)
                .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;
        }
    }

    // Apply CLI overrides
    if let Some(model) = &cli.model {
        config.llm.model = model.clone();
    }
    if let Some(approval) = &cli.approval {
        config.safety.approval_mode = match approval.as_str() {
            "safe" => rustant_core::ApprovalMode::Safe,
            "cautious" => rustant_core::ApprovalMode::Cautious,
            "paranoid" => rustant_core::ApprovalMode::Paranoid,
            "yolo" => rustant_core::ApprovalMode::Yolo,
            _ => {
                eprintln!("Unknown approval mode: '{}'. Using 'safe'.", approval);
                rustant_core::ApprovalMode::Safe
            }
        };
    }

    // Start TUI, REPL, or execute single task
    if let Some(task) = cli.task {
        repl::run_single_task(&task, config, workspace).await
    } else if cli.no_tui || !config.ui.use_tui {
        repl::run_interactive(config, workspace).await
    } else {
        tui::run(config, workspace).await
    }
}
