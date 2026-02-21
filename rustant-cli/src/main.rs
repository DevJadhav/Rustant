//! Rustant CLI — Terminal interface for the Rustant autonomous agent.
//!
//! Provides both single-task and interactive REPL modes.

pub(crate) mod channel_setup;
pub mod commands;
pub(crate) mod markdown;
mod repl;
mod repl_input;
pub(crate) mod setup;
pub(crate) mod slash;

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

    /// Approval mode: safe, cautious, paranoid, yolo
    #[arg(short = 'a', long)]
    approval: Option<String>,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress non-essential output
    #[arg(short, long)]
    quiet: bool,

    /// Enable voice input mode (requires microphone access)
    #[arg(long)]
    voice: bool,

    /// Subcommand
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(clap::Subcommand, Debug)]
pub enum Commands {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Interactive provider setup wizard
    Setup,
    /// Smart project initialization: detects project type, generates optimal config
    Init,
    /// Resume a previous session (most recent, or by name)
    Resume {
        /// Session name or ID to resume (omit for most recent)
        session: Option<String>,
    },
    /// List saved sessions
    Sessions {
        /// Maximum number of sessions to show
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
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
    /// Security scanning (SAST, SCA, secrets, IaC, container, supply chain)
    Scan {
        #[command(subcommand)]
        action: ScanAction,
    },
    /// AI-powered code review
    Review {
        #[command(subcommand)]
        action: ReviewAction,
    },
    /// Code quality analysis
    Quality {
        #[command(subcommand)]
        action: QualityAction,
    },
    /// License compliance checking
    License {
        #[command(subcommand)]
        action: LicenseAction,
    },
    /// Software Bill of Materials (SBOM) generation
    Sbom {
        #[command(subcommand)]
        action: SbomAction,
    },
    /// Compliance reporting and auditing
    Compliance {
        #[command(subcommand)]
        action: ComplianceAction,
    },
    /// Audit trail export and verification
    Audit {
        #[command(subcommand)]
        action: AuditAction,
    },
    /// Risk scoring and assessment
    Risk {
        #[command(subcommand)]
        action: RiskAction,
    },
    /// Security policy management
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
    /// Alert management and triage
    Alerts {
        #[command(subcommand)]
        action: AlertsAction,
    },
    /// Manage the Rustant background daemon
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Siri voice integration (macOS)
    #[cfg(target_os = "macos")]
    Siri {
        #[command(subcommand)]
        action: SiriAction,
    },
    /// Deep research mode
    Research {
        #[command(subcommand)]
        action: ResearchAction,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ScanAction {
    /// Run all enabled scanners
    All {
        /// Path to scan (default: current directory)
        #[arg(short, long, default_value = ".")]
        path: String,
        /// Output format (sarif, markdown, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },
    /// Run SAST (Static Application Security Testing) only
    Sast {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
        /// Languages to scan (comma-separated)
        #[arg(short, long)]
        languages: Option<String>,
    },
    /// Run SCA (Software Composition Analysis) only
    Sca {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    /// Scan for hardcoded secrets
    Secrets {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
        /// Scan git history
        #[arg(long)]
        history: bool,
    },
    /// Scan infrastructure-as-code files
    Iac {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    /// Scan container images or Dockerfiles
    Container {
        /// Image name or Dockerfile path
        target: String,
    },
    /// Check supply chain security
    SupplyChain {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ReviewAction {
    /// Review code changes (diff-based)
    Diff {
        /// Git diff base (e.g., HEAD~1, main)
        #[arg(default_value = "HEAD~1")]
        base: String,
    },
    /// Review a specific file or directory
    Path {
        /// Path to review
        path: String,
    },
    /// Generate fix suggestions for findings
    Fix {
        /// Auto-apply high-confidence fixes
        #[arg(long)]
        auto: bool,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum QualityAction {
    /// Calculate code quality score (A-F)
    Score {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
    /// Analyze cyclomatic complexity
    Complexity {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
    /// Detect dead/unreachable code
    DeadCode {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
    /// Find code duplication
    Duplicates {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
    /// Generate technical debt report
    Debt {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum LicenseAction {
    /// Check license compliance against policy
    Check {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    /// Show license summary for all dependencies
    Summary {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum SbomAction {
    /// Generate SBOM (Software Bill of Materials)
    Generate {
        /// Path to scan
        #[arg(short, long, default_value = ".")]
        path: String,
        /// Output format (cyclonedx, spdx, csv)
        #[arg(short, long, default_value = "cyclonedx")]
        format: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Compare two SBOM versions
    Diff {
        /// First SBOM file
        old: String,
        /// Second SBOM file
        new: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ComplianceAction {
    /// Generate compliance report for a framework
    Report {
        /// Compliance framework (soc2, iso27001, nist, pci-dss, owasp)
        framework: String,
        /// Output format (markdown, html, pdf, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },
    /// Show compliance status summary
    Status,
}

#[derive(clap::Subcommand, Debug)]
pub enum AuditAction {
    /// Export audit trail
    Export {
        /// Start date (ISO 8601)
        #[arg(short, long)]
        start: Option<String>,
        /// End date (ISO 8601)
        #[arg(short, long)]
        end: Option<String>,
        /// Output format (sarif, ocsf, json, csv)
        #[arg(short, long, default_value = "json")]
        format: String,
    },
    /// Verify audit trail integrity (Merkle chain)
    Verify,
}

#[derive(clap::Subcommand, Debug)]
pub enum RiskAction {
    /// Calculate risk score
    Score {
        /// Path to analyze
        #[arg(default_value = ".")]
        path: String,
    },
    /// Show risk trend over time
    Trend,
}

#[derive(clap::Subcommand, Debug)]
pub enum PolicyAction {
    /// List active policies
    List,
    /// Check policies against current project
    Check {
        /// Path to check
        #[arg(short, long, default_value = ".")]
        path: String,
    },
    /// Validate a policy file
    Validate {
        /// Path to policy file
        path: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum AlertsAction {
    /// List active alerts
    List {
        /// Filter by severity (critical, high, medium, low)
        #[arg(short, long)]
        severity: Option<String>,
    },
    /// Run AI-powered alert triage
    Triage,
    /// Acknowledge an alert
    Acknowledge {
        /// Alert ID
        id: String,
    },
    /// Resolve an alert
    Resolve {
        /// Alert ID
        id: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum CanvasAction {
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
pub enum SkillAction {
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
pub enum PluginAction {
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
pub enum UpdateAction {
    /// Check for available updates
    Check,
    /// Download and install the latest version
    Install,
}

#[derive(clap::Subcommand, Debug)]
pub enum WorkflowAction {
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
pub enum CronAction {
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
pub enum VoiceAction {
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
pub enum BrowserAction {
    /// Test browser automation by navigating to a URL
    Test {
        /// URL to navigate to
        #[arg(default_value = "https://example.com")]
        url: String,
    },
    /// Connect to an existing Chrome instance with remote debugging enabled
    Connect {
        /// Remote debugging port (default: 9222)
        #[arg(short, long, default_value = "9222")]
        port: u16,
    },
    /// Launch a new Chrome instance with remote debugging enabled
    Launch {
        /// Remote debugging port (default: 9222)
        #[arg(short, long, default_value = "9222")]
        port: u16,
        /// Run Chrome headless (no visible window)
        #[arg(long)]
        headless: bool,
    },
    /// Show browser connection status and open tabs
    Status,
}

#[derive(clap::Subcommand, Debug)]
pub enum ConfigAction {
    /// Create default configuration file
    Init,
    /// Show current configuration
    Show,
}

#[derive(clap::Subcommand, Debug)]
pub enum ChannelAction {
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
pub enum SlackCommand {
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
pub enum AuthAction {
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

#[derive(clap::Subcommand, Debug)]
pub enum DaemonAction {
    /// Start the background daemon
    Start {
        /// Enable Siri routing mode
        #[arg(long)]
        siri_mode: bool,
    },
    /// Stop the background daemon
    Stop {
        /// Also deactivate Siri mode
        #[arg(long)]
        siri_mode: bool,
    },
    /// Show daemon status
    Status,
    /// Install daemon for auto-start on login (launchd on macOS, systemd on Linux)
    Install,
    /// Remove auto-start configuration
    Uninstall,
}

#[cfg(target_os = "macos")]
#[derive(clap::Subcommand, Debug)]
pub enum SiriAction {
    /// Interactive Siri shortcut setup wizard
    Setup,
    /// Send a command to Rustant via Siri (used by Shortcuts)
    Send {
        /// The command text from Siri
        command: String,
    },
    /// List available Siri shortcuts
    Shortcuts,
    /// Show Siri integration status
    Status,
    /// Confirm or deny a pending approval
    Confirm {
        /// Session ID
        session_id: String,
        /// yes or no
        answer: String,
    },
}

#[derive(clap::Subcommand, Debug)]
pub enum ResearchAction {
    /// Start a new deep research session
    Start {
        /// The research question
        question: String,
        /// Research depth: quick, detailed, comprehensive
        #[arg(short, long, default_value = "detailed")]
        depth: String,
        /// Output format: summary, detailed, bibliography, roadmap
        #[arg(short, long, default_value = "detailed")]
        format: String,
        /// Use multi-model council for synthesis
        #[arg(long)]
        council: bool,
    },
    /// Show status of the active research session
    Status,
    /// Resume a paused research session
    Resume {
        /// Session ID to resume
        id: String,
    },
    /// List all research sessions
    Sessions,
    /// Export the research report
    Report {
        /// Output format: summary, detailed, bibliography, roadmap
        #[arg(short, long, default_value = "detailed")]
        format: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env file if present
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    // Set up tracing: human-readable stderr + JSON file logging
    // Default to "warn" for clean output (hides INFO tool execution noise).
    // Use -v for debug, -vv for trace, -q for errors only.
    let filter = match cli.verbose {
        0 if cli.quiet => "error",
        0 => "warn",
        1 => "info",
        2 => "debug",
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
        .map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;

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
            eprintln!("  Setup failed: {e}. Using defaults.\n");
        } else {
            // Reload configuration after setup
            config = rustant_core::config::load_config(Some(&workspace), None)
                .map_err(|e| anyhow::anyhow!("Configuration error: {e}"))?;
        }
    }

    // Track the approval mode source for diagnostics
    let approval_before_cli = config.safety.approval_mode;
    let mut approval_source = if std::env::var("RUSTANT_SAFETY__APPROVAL_MODE").is_ok() {
        "env(RUSTANT_SAFETY__APPROVAL_MODE)"
    } else if approval_before_cli != rustant_core::ApprovalMode::Safe {
        "config file"
    } else {
        "default"
    };

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
                eprintln!("Unknown approval mode: '{approval}'. Using 'safe'.");
                rustant_core::ApprovalMode::Safe
            }
        };
        approval_source = "CLI --approval flag";
    }

    // Always show approval mode when non-default (helps diagnose env var issues)
    if config.safety.approval_mode != rustant_core::ApprovalMode::Safe {
        eprintln!(
            "  \x1b[33m⚠ Approval mode: {} (source: {})\x1b[0m",
            config.safety.approval_mode, approval_source
        );
    }

    tracing::debug!(
        approval_mode = %config.safety.approval_mode,
        source = approval_source,
        "Config loaded — approval mode"
    );

    // Start voice mode, REPL, or execute single task
    if cli.voice {
        #[cfg(feature = "voice")]
        {
            commands::run_voice_mode(config, workspace).await
        }
        #[cfg(not(feature = "voice"))]
        {
            anyhow::bail!(
                "Voice mode requires the 'voice' feature. Recompile with: cargo build --features voice"
            );
        }
    } else if let Some(task) = cli.task {
        repl::run_single_task(&task, config, workspace).await
    } else {
        repl::run_interactive(config, workspace).await
    }
}
