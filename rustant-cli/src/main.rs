//! Rustant CLI — Terminal interface for the Rustant autonomous agent.
//!
//! Provides both single-task and interactive REPL modes.

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
    /// Test a channel's connection (connect + disconnect)
    Test {
        /// Channel name (e.g., slack, telegram, discord)
        name: String,
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
