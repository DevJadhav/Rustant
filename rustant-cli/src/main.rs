//! Rustant CLI — Terminal interface for the Rustant autonomous agent.
//!
//! Provides both single-task and interactive REPL modes.

mod commands;
mod repl;
mod tui;

use clap::Parser;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

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
}

#[derive(clap::Subcommand, Debug)]
enum ConfigAction {
    /// Create default configuration file
    Init,
    /// Show current configuration
    Show,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Set up tracing
    let filter = match cli.verbose {
        0 if cli.quiet => "error",
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
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
