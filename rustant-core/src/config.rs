//! Configuration system for Rustant.
//!
//! Uses `figment` for layered configuration: defaults -> config file -> environment -> CLI args.
//! Configuration is loaded from `~/.config/rustant/config.toml` and/or `.rustant/config.toml`
//! in the workspace directory.

use figment::{
    providers::{Env, Format, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::channels::discord::DiscordConfig;
use crate::channels::email::EmailConfig;
use crate::channels::imessage::IMessageConfig;
use crate::channels::irc::IrcConfig;
use crate::channels::matrix::MatrixConfig;
use crate::channels::signal::SignalConfig;
use crate::channels::slack::SlackConfig;
use crate::channels::sms::SmsConfig;
use crate::channels::teams::TeamsConfig;
use crate::channels::telegram::TelegramConfig;
use crate::channels::webchat::WebChatConfig;
use crate::channels::webhook::WebhookConfig;
use crate::channels::whatsapp::WhatsAppConfig;
use crate::gateway::GatewayConfig;
use crate::memory::FlushConfig;
use crate::search::SearchConfig;

/// Top-level configuration for the Rustant agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm: LlmConfig,
    pub safety: SafetyConfig,
    pub memory: MemoryConfig,
    pub ui: UiConfig,
    pub tools: ToolsConfig,
    /// Optional WebSocket gateway configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway: Option<GatewayConfig>,
    /// Optional hybrid search configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchConfig>,
    /// Optional memory flush configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flush: Option<FlushConfig>,
    /// Optional channels configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<ChannelsConfig>,
    /// Optional multi-agent configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_agent: Option<MultiAgentConfig>,
    /// Optional workflow engine configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<WorkflowConfig>,
    /// Optional browser automation configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser: Option<BrowserConfig>,
    /// Optional scheduler configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduler: Option<SchedulerConfig>,
    /// Optional voice and audio configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceConfig>,
    /// Optional token budget configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetConfig>,
    /// Optional cross-session knowledge distillation configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge: Option<KnowledgeConfig>,
    /// Optional channel intelligence configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intelligence: Option<IntelligenceConfig>,
}

/// Configuration for the workflow engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowConfig {
    /// Whether the workflow engine is enabled.
    pub enabled: bool,
    /// Directory containing custom workflow definitions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_dir: Option<PathBuf>,
    /// Maximum concurrent workflow runs.
    pub max_concurrent_runs: usize,
    /// Default timeout per step in seconds.
    pub default_step_timeout_secs: u64,
    /// Path for persisting workflow state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_persistence_path: Option<PathBuf>,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            workflow_dir: None,
            max_concurrent_runs: 4,
            default_step_timeout_secs: 300,
            state_persistence_path: None,
        }
    }
}

/// Configuration for the browser automation system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    /// Whether browser automation is enabled.
    pub enabled: bool,
    /// Path to the Chrome/Chromium binary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chrome_path: Option<String>,
    /// Whether to run headless (no visible window).
    pub headless: bool,
    /// Default viewport width in pixels.
    pub default_viewport_width: u32,
    /// Default viewport height in pixels.
    pub default_viewport_height: u32,
    /// Default timeout per operation in seconds.
    pub default_timeout_secs: u64,
    /// If non-empty, only these domains are allowed.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// These domains are always blocked.
    #[serde(default)]
    pub blocked_domains: Vec<String>,
    /// Whether to use an isolated browser profile.
    pub isolate_profile: bool,
    /// Maximum number of open pages/tabs.
    pub max_pages: usize,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chrome_path: None,
            headless: true,
            default_viewport_width: 1280,
            default_viewport_height: 720,
            default_timeout_secs: 30,
            allowed_domains: Vec::new(),
            blocked_domains: Vec::new(),
            isolate_profile: true,
            max_pages: 5,
        }
    }
}

/// Configuration for the scheduler system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Whether the scheduler is enabled.
    pub enabled: bool,
    /// Cron job definitions.
    #[serde(default)]
    pub cron_jobs: Vec<crate::scheduler::CronJobConfig>,
    /// Optional heartbeat configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat: Option<crate::scheduler::HeartbeatConfig>,
    /// Optional port for webhook listener.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_port: Option<u16>,
    /// Maximum number of concurrent background jobs.
    pub max_background_jobs: usize,
    /// Path for persisting scheduler state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_path: Option<PathBuf>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            cron_jobs: Vec::new(),
            heartbeat: None,
            webhook_port: None,
            max_background_jobs: 10,
            state_path: None,
        }
    }
}

/// Configuration for the voice and audio system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceConfig {
    /// Whether voice features are enabled.
    pub enabled: bool,
    /// STT provider: "openai", "whisper-local", "mock".
    pub stt_provider: String,
    /// Whisper model size (for local): "tiny", "base", "small", "medium", "large".
    pub stt_model: String,
    /// Language code for STT (e.g., "en").
    pub stt_language: String,
    /// TTS provider: "openai", "mock".
    pub tts_provider: String,
    /// TTS voice name.
    pub tts_voice: String,
    /// TTS speech speed multiplier.
    pub tts_speed: f32,
    /// Whether VAD (voice activity detection) is enabled.
    pub vad_enabled: bool,
    /// VAD energy threshold (0.0-1.0).
    pub vad_threshold: f32,
    /// Wake word phrases (e.g., ["hey rustant"]).
    #[serde(default)]
    pub wake_words: Vec<String>,
    /// Wake word sensitivity (0.0-1.0).
    pub wake_sensitivity: f32,
    /// Whether to auto-speak responses.
    pub auto_speak: bool,
    /// Maximum listening duration in seconds.
    pub max_listen_secs: u64,
    /// Audio input device name (None = system default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device: Option<String>,
    /// Audio output device name (None = system default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_device: Option<String>,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            stt_provider: "openai".to_string(),
            stt_model: "base".to_string(),
            stt_language: "en".to_string(),
            tts_provider: "openai".to_string(),
            tts_voice: "alloy".to_string(),
            tts_speed: 1.0,
            vad_enabled: true,
            vad_threshold: 0.01,
            wake_words: vec!["hey rustant".to_string()],
            wake_sensitivity: 0.5,
            auto_speak: false,
            max_listen_secs: 30,
            input_device: None,
            output_device: None,
        }
    }
}

/// Configuration for the multi-agent system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiAgentConfig {
    /// Whether multi-agent mode is enabled.
    pub enabled: bool,
    /// Maximum number of concurrent agents.
    pub max_agents: usize,
    /// Maximum messages per agent mailbox.
    pub max_mailbox_size: usize,
    /// Default resource limits applied to new agents.
    #[serde(default)]
    pub default_resource_limits: crate::multi::ResourceLimits,
    /// Default base directory for agent workspaces.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_workspace_base: Option<String>,
}

impl Default for MultiAgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_agents: 8,
            max_mailbox_size: 1000,
            default_resource_limits: crate::multi::ResourceLimits::default(),
            default_workspace_base: None,
        }
    }
}

// Channel Intelligence Configuration

/// Auto-reply mode for channel intelligence.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AutoReplyMode {
    /// Never auto-reply to channel messages.
    Disabled,
    /// Generate draft replies but do not send, store for user review.
    DraftOnly,
    /// Auto-reply for routine messages, queue high-priority for approval.
    AutoWithApproval,
    /// Send all replies automatically.
    #[default]
    FullAuto,
}

/// Frequency for generating channel digest summaries.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DigestFrequency {
    /// No digests generated.
    #[default]
    Off,
    /// Generate digest every hour.
    Hourly,
    /// Generate digest once per day.
    Daily,
    /// Generate digest once per week.
    Weekly,
}

/// Priority level for classifying channel messages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessagePriority {
    /// Low priority, informational, no action needed.
    Low = 0,
    /// Normal priority, standard messages.
    #[default]
    Normal = 1,
    /// High priority, needs timely attention.
    High = 2,
    /// Urgent, needs immediate attention.
    Urgent = 3,
}

/// Per-channel intelligence settings.
///
/// These can be overridden per-channel in the `[intelligence.channels.<name>]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelIntelligenceConfig {
    /// Auto-reply mode for this channel.
    #[serde(default)]
    pub auto_reply: AutoReplyMode,
    /// Digest generation frequency.
    #[serde(default)]
    pub digest: DigestFrequency,
    /// Whether to auto-schedule follow-ups for urgent messages.
    #[serde(default = "default_true")]
    pub smart_scheduling: bool,
    /// Priority threshold for escalation, messages at or above this level get escalated.
    #[serde(default)]
    pub escalation_threshold: MessagePriority,
    /// Default follow-up reminder delay in minutes (default: 60).
    #[serde(default = "default_followup_minutes")]
    pub default_followup_minutes: u32,
}

impl Default for ChannelIntelligenceConfig {
    fn default() -> Self {
        Self {
            auto_reply: AutoReplyMode::default(),
            digest: DigestFrequency::default(),
            smart_scheduling: true,
            escalation_threshold: MessagePriority::High,
            default_followup_minutes: default_followup_minutes(),
        }
    }
}

/// Top-level channel intelligence configuration.
///
/// Controls autonomous message handling across all channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntelligenceConfig {
    /// Whether channel intelligence is enabled globally.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Default settings for all channels (overridden per-channel).
    #[serde(default)]
    pub defaults: ChannelIntelligenceConfig,
    /// Per-channel overrides keyed by channel name (e.g., "email", "slack").
    #[serde(default)]
    pub channels: HashMap<String, ChannelIntelligenceConfig>,
    /// Quiet hours: suppress auto-actions during these times.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<crate::scheduler::QuietHours>,
    /// Directory for digest file export (default: ".rustant/digests").
    #[serde(default = "default_digest_dir")]
    pub digest_dir: PathBuf,
    /// Directory for ICS calendar/reminder export (default: ".rustant/reminders").
    #[serde(default = "default_reminders_dir")]
    pub reminders_dir: PathBuf,
    /// Maximum tokens per auto-reply LLM call (cost control).
    #[serde(default = "default_max_reply_tokens")]
    pub max_reply_tokens: usize,
}

fn default_true() -> bool {
    true
}

fn default_followup_minutes() -> u32 {
    60
}

fn default_digest_dir() -> PathBuf {
    PathBuf::from(".rustant/digests")
}

fn default_reminders_dir() -> PathBuf {
    PathBuf::from(".rustant/reminders")
}

fn default_max_reply_tokens() -> usize {
    500
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            defaults: ChannelIntelligenceConfig::default(),
            channels: HashMap::new(),
            quiet_hours: None,
            digest_dir: default_digest_dir(),
            reminders_dir: default_reminders_dir(),
            max_reply_tokens: 500,
        }
    }
}

impl ChannelIntelligenceConfig {
    /// Validate this channel intelligence config and return any warnings.
    ///
    /// Returns an empty Vec if the config is valid. Returns human-readable
    /// warning messages for problematic values (backward compatible — does not error).
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // S13: Warn on zero followup minutes (likely a mistake)
        if self.default_followup_minutes == 0 {
            warnings.push(
                "default_followup_minutes is 0 — follow-ups will trigger immediately".to_string(),
            );
        }

        // S13: Warn on extremely large followup minutes (> 30 days)
        if self.default_followup_minutes > 43_200 {
            warnings.push(format!(
                "default_followup_minutes is {} (>{} days) — this is unusually large",
                self.default_followup_minutes,
                self.default_followup_minutes / 1440
            ));
        }

        // S13: Warn on Low escalation threshold (everything escalates)
        if self.escalation_threshold == MessagePriority::Low {
            warnings
                .push("escalation_threshold is Low — all messages will be escalated".to_string());
        }

        warnings
    }
}

impl IntelligenceConfig {
    /// Get the intelligence config for a specific channel, falling back to defaults.
    pub fn for_channel(&self, channel_name: &str) -> &ChannelIntelligenceConfig {
        self.channels.get(channel_name).unwrap_or(&self.defaults)
    }

    /// Check if the current time is within quiet hours.
    pub fn is_quiet_hours_now(&self) -> bool {
        if let Some(ref quiet) = self.quiet_hours {
            quiet.is_active(&chrono::Utc::now())
        } else {
            false
        }
    }

    /// Validate the entire intelligence config and return any warnings.
    ///
    /// Checks the default config and all per-channel overrides, plus quiet hours format.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Validate defaults
        for w in self.defaults.validate() {
            warnings.push(format!("[defaults] {}", w));
        }

        // Validate per-channel overrides
        for (name, cfg) in &self.channels {
            for w in cfg.validate() {
                warnings.push(format!("[channel:{}] {}", name, w));
            }
        }

        // S13: Validate quiet hours time format
        if let Some(ref quiet) = self.quiet_hours {
            if !is_valid_time_format(&quiet.start) {
                warnings.push(format!(
                    "quiet_hours.start '{}' is not in HH:MM format",
                    quiet.start
                ));
            }
            if !is_valid_time_format(&quiet.end) {
                warnings.push(format!(
                    "quiet_hours.end '{}' is not in HH:MM format",
                    quiet.end
                ));
            }
        }

        // S13: Warn on zero max_reply_tokens
        if self.max_reply_tokens == 0 {
            warnings.push("max_reply_tokens is 0 — auto-replies will be empty".to_string());
        }

        warnings
    }
}

/// Check if a string is a valid HH:MM time format.
fn is_valid_time_format(s: &str) -> bool {
    if s.len() != 5 {
        return false;
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    match (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
        (Ok(h), Ok(m)) => h < 24 && m < 60,
        _ => false,
    }
}

/// Configuration for messaging channels.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discord: Option<DiscordConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slack: Option<SlackConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webchat: Option<WebChatConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matrix: Option<MatrixConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<SignalConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub whatsapp: Option<WhatsAppConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<EmailConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imessage: Option<IMessageConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub teams: Option<TeamsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sms: Option<SmsConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub irc: Option<IrcConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook: Option<WebhookConfig>,
}

/// LLM provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider name: "openai", "anthropic", "local".
    pub provider: String,
    /// Model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514").
    pub model: String,
    /// Environment variable name containing the API key.
    pub api_key_env: String,
    /// Optional base URL override for the API endpoint.
    pub base_url: Option<String>,
    /// Maximum tokens to generate in a response.
    pub max_tokens: usize,
    /// Default temperature for generation.
    pub temperature: f32,
    /// Context window size for the model.
    pub context_window: usize,
    /// Cost per 1M input tokens (USD).
    pub input_cost_per_million: f64,
    /// Cost per 1M output tokens (USD).
    pub output_cost_per_million: f64,
    /// Whether to use streaming for LLM responses (enables token-by-token output).
    pub use_streaming: bool,
    /// Optional fallback providers tried in order if the primary fails.
    #[serde(default)]
    pub fallback_providers: Vec<FallbackProviderConfig>,
    /// Optional credential store key (provider name in the OS credential store).
    /// If set, the API key is loaded from the credential store instead of the env var.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_store_key: Option<String>,
    /// Authentication method: "api_key" (default) or "oauth".
    /// When set to "oauth", the provider will use an OAuth token from the credential
    /// store instead of a traditional API key.
    #[serde(default)]
    pub auth_method: String,
}

/// Configuration for a fallback LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackProviderConfig {
    /// Provider name: "openai", "anthropic", etc.
    pub provider: String,
    /// Model identifier.
    pub model: String,
    /// Environment variable name containing the API key.
    pub api_key_env: String,
    /// Optional base URL override.
    #[serde(default)]
    pub base_url: Option<String>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key_env: "OPENAI_API_KEY".to_string(),
            base_url: None,
            max_tokens: 4096,
            temperature: 0.7,
            context_window: 128_000,
            input_cost_per_million: 2.50,
            output_cost_per_million: 10.00,
            use_streaming: false,
            fallback_providers: Vec::new(),
            credential_store_key: None,
            auth_method: String::new(),
        }
    }
}

impl LlmConfig {
    /// Validate this LLM config and return any warnings.
    ///
    /// Returns an empty Vec if the config is valid. Returns human-readable
    /// warning messages for problematic values (backward compatible — does not error).
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();
        if self.max_tokens >= self.context_window {
            warnings.push(format!(
                "max_tokens ({}) >= context_window ({}); responses may be truncated or fail",
                self.max_tokens, self.context_window
            ));
        }
        if self.temperature < 0.0 || self.temperature > 2.0 {
            warnings.push(format!(
                "temperature ({}) is outside the typical range 0.0–2.0",
                self.temperature
            ));
        }
        warnings
    }
}

/// Approval mode controlling how much autonomy the agent has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalMode {
    /// Only read operations are auto-approved; all writes require approval.
    #[default]
    Safe,
    /// All reversible operations are auto-approved; destructive requires approval.
    Cautious,
    /// Every single action requires explicit approval.
    Paranoid,
    /// All operations are auto-approved (use at own risk).
    Yolo,
}

impl std::fmt::Display for ApprovalMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalMode::Safe => write!(f, "safe"),
            ApprovalMode::Cautious => write!(f, "cautious"),
            ApprovalMode::Paranoid => write!(f, "paranoid"),
            ApprovalMode::Yolo => write!(f, "yolo"),
        }
    }
}

/// Safety and permission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    pub approval_mode: ApprovalMode,
    /// Glob patterns for allowed file paths (relative to workspace).
    pub allowed_paths: Vec<String>,
    /// Glob patterns for denied file paths.
    pub denied_paths: Vec<String>,
    /// Allowed shell command prefixes.
    pub allowed_commands: Vec<String>,
    /// Commands that always require approval.
    pub ask_commands: Vec<String>,
    /// Commands that are never allowed.
    pub denied_commands: Vec<String>,
    /// Allowed network hosts.
    pub allowed_hosts: Vec<String>,
    /// Maximum iterations before the agent pauses.
    pub max_iterations: usize,
    /// Prompt injection detection settings.
    #[serde(default)]
    pub injection_detection: InjectionDetectionConfig,
    /// Optional adaptive trust configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adaptive_trust: Option<AdaptiveTrustConfig>,
}

/// Configuration for the prompt injection detection system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionDetectionConfig {
    /// Whether injection detection is enabled.
    pub enabled: bool,
    /// Risk score threshold (0.0 - 1.0) above which content is considered suspicious.
    pub threshold: f32,
    /// Whether to scan tool outputs for indirect injection attempts.
    pub scan_tool_outputs: bool,
}

impl Default for InjectionDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.5,
            scan_tool_outputs: true,
        }
    }
}

/// Configuration for the adaptive trust gradient system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveTrustConfig {
    /// Whether adaptive trust is enabled.
    pub enabled: bool,
    /// Number of consecutive approvals required before a tool is auto-promoted.
    pub trust_escalation_threshold: usize,
    /// Anomaly score [0, 1] above which trust is de-escalated.
    pub anomaly_threshold: f64,
}

impl Default for AdaptiveTrustConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trust_escalation_threshold: 5,
            anomaly_threshold: 0.7,
        }
    }
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            approval_mode: ApprovalMode::Safe,
            allowed_paths: vec![
                "src/**".to_string(),
                "tests/**".to_string(),
                "docs/**".to_string(),
            ],
            denied_paths: vec![
                ".env*".to_string(),
                "**/*.key".to_string(),
                "**/secrets/**".to_string(),
                "**/*.pem".to_string(),
                "**/credentials*".to_string(),
                ".ssh/**".to_string(),
                ".aws/**".to_string(),
                ".docker/config.json".to_string(),
                "**/*id_rsa*".to_string(),
                "**/*id_ed25519*".to_string(),
            ],
            allowed_commands: vec![
                "cargo".to_string(),
                "git".to_string(),
                "npm".to_string(),
                "pnpm".to_string(),
                "yarn".to_string(),
                "python -m pytest".to_string(),
            ],
            ask_commands: vec![
                "rm".to_string(),
                "mv".to_string(),
                "cp".to_string(),
                "chmod".to_string(),
            ],
            denied_commands: vec![
                "sudo".to_string(),
                "curl | sh".to_string(),
                "wget | bash".to_string(),
            ],
            allowed_hosts: vec![
                "api.github.com".to_string(),
                "crates.io".to_string(),
                "registry.npmjs.org".to_string(),
            ],
            max_iterations: 25,
            injection_detection: InjectionDetectionConfig::default(),
            adaptive_trust: None,
        }
    }
}

/// Memory system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Number of recent messages to keep verbatim in short-term memory.
    pub window_size: usize,
    /// Fraction of context window at which to trigger compression (0.0 - 1.0).
    pub compression_threshold: f32,
    /// Path for persistent long-term memory storage.
    pub persist_path: Option<PathBuf>,
    /// Whether to enable long-term memory persistence.
    pub enable_persistence: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            window_size: 12,
            compression_threshold: 0.7,
            persist_path: None,
            enable_persistence: false,
        }
    }
}

/// UI configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Color theme name.
    pub theme: String,
    /// Whether to enable vim keybindings.
    pub vim_mode: bool,
    /// Whether to show cost information in the UI.
    pub show_cost: bool,
    /// Whether to use the TUI (false = simple REPL).
    pub use_tui: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            vim_mode: false,
            show_cost: true,
            use_tui: false, // Start with REPL in Phase 0
        }
    }
}

/// Tools configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    /// Whether to enable built-in tools.
    pub enable_builtins: bool,
    /// Timeout for tool execution in seconds.
    pub default_timeout_secs: u64,
    /// Maximum output size from a tool in bytes.
    pub max_output_bytes: usize,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            enable_builtins: true,
            default_timeout_secs: 30,
            max_output_bytes: 1_048_576, // 1MB
        }
    }
}

/// Token budget configuration for cost control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetConfig {
    /// Maximum cost in USD per session (0.0 = unlimited).
    pub session_limit_usd: f64,
    /// Maximum cost in USD per task (0.0 = unlimited).
    pub task_limit_usd: f64,
    /// Maximum total tokens per session (0 = unlimited).
    pub session_token_limit: usize,
    /// Whether to warn (false) or halt (true) when budget is exceeded.
    pub halt_on_exceed: bool,
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self {
            session_limit_usd: 0.0,
            task_limit_usd: 0.0,
            session_token_limit: 0,
            halt_on_exceed: false,
        }
    }
}

/// Configuration for cross-session knowledge distillation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeConfig {
    /// Whether knowledge distillation is enabled.
    pub enabled: bool,
    /// Maximum number of distilled rules to inject into the system prompt.
    pub max_rules: usize,
    /// Minimum number of corrections/facts before distillation is triggered.
    pub min_entries_for_distillation: usize,
    /// Path to the local knowledge store file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_path: Option<PathBuf>,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_rules: 20,
            min_entries_for_distillation: 3,
            knowledge_path: None,
        }
    }
}

/// Load configuration from layered sources.
///
/// Priority (highest to lowest):
/// 1. Explicit overrides (passed as argument)
/// 2. Environment variables (prefixed with `RUSTANT_`)
/// 3. Workspace-local config (`.rustant/config.toml`)
/// 4. User config (`~/.config/rustant/config.toml`)
/// 5. Built-in defaults
pub fn load_config(
    workspace: Option<&Path>,
    overrides: Option<&AgentConfig>,
) -> Result<AgentConfig, Box<figment::Error>> {
    let mut figment = Figment::from(Serialized::defaults(AgentConfig::default()));

    // User-level config
    if let Some(config_dir) = directories::ProjectDirs::from("dev", "rustant", "rustant") {
        let user_config = config_dir.config_dir().join("config.toml");
        if user_config.exists() {
            figment = figment.merge(Toml::file(&user_config));
        }
    }

    // Workspace-level config
    if let Some(ws) = workspace {
        let ws_config = ws.join(".rustant").join("config.toml");
        if ws_config.exists() {
            figment = figment.merge(Toml::file(&ws_config));
        }
    }

    // Environment variables (RUSTANT_LLM__MODEL, RUSTANT_SAFETY__APPROVAL_MODE, etc.)
    figment = figment.merge(Env::prefixed("RUSTANT_").split("__"));

    // Explicit overrides
    if let Some(overrides) = overrides {
        figment = figment.merge(Serialized::defaults(overrides));
    }

    figment.extract().map_err(Box::new)
}

/// Check whether any Rustant configuration file exists (user-level or workspace-level).
///
/// Returns `true` if a config file is found at either:
/// - `~/.config/rustant/config.toml` (user-level, via `directories` crate)
/// - `<workspace>/.rustant/config.toml` (workspace-level)
pub fn config_exists(workspace: Option<&Path>) -> bool {
    // Check user-level config
    if let Some(config_dir) = directories::ProjectDirs::from("dev", "rustant", "rustant") {
        if config_dir.config_dir().join("config.toml").exists() {
            return true;
        }
    }

    // Check workspace-level config
    if let Some(ws) = workspace {
        if ws.join(".rustant").join("config.toml").exists() {
            return true;
        }
    }

    false
}

/// Update a specific channel's configuration in the workspace config file.
///
/// Loads the existing `.rustant/config.toml`, sets or replaces the named channel's
/// config, preserves all other channels and settings, and writes back.
/// Returns the path to the config file.
pub fn update_channel_config(
    workspace: &std::path::Path,
    channel_name: &str,
    channel_toml: toml::Value,
) -> anyhow::Result<std::path::PathBuf> {
    let config_dir = workspace.join(".rustant");
    std::fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");

    // Load existing config or start from defaults
    let mut config: AgentConfig = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        toml::from_str(&content).unwrap_or_default()
    } else {
        AgentConfig::default()
    };

    // Serialize to a TOML table so we can set the channel dynamically
    let mut table: toml::Value = toml::Value::try_from(&config)?;

    // Ensure [channels] table exists
    let channels_table = table
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("config is not a TOML table"))?
        .entry("channels")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));

    // Set channels.<channel_name> = channel_toml
    if let Some(ch_table) = channels_table.as_table_mut() {
        ch_table.insert(channel_name.to_string(), channel_toml);
    }

    // Deserialize back to verify it's valid, then write
    config = table.try_into()?;
    let toml_str = toml::to_string_pretty(&config)?;
    std::fs::write(&config_path, &toml_str)?;

    Ok(config_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.llm.model, "gpt-4o");
        assert_eq!(config.safety.approval_mode, ApprovalMode::Safe);
        assert_eq!(config.memory.window_size, 12);
        assert!(!config.ui.vim_mode);
        assert!(config.tools.enable_builtins);
    }

    #[test]
    fn test_approval_mode_display() {
        assert_eq!(ApprovalMode::Safe.to_string(), "safe");
        assert_eq!(ApprovalMode::Cautious.to_string(), "cautious");
        assert_eq!(ApprovalMode::Paranoid.to_string(), "paranoid");
        assert_eq!(ApprovalMode::Yolo.to_string(), "yolo");
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AgentConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: AgentConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(deserialized.llm.model, config.llm.model);
        assert_eq!(
            deserialized.safety.approval_mode,
            config.safety.approval_mode
        );
        assert_eq!(deserialized.memory.window_size, config.memory.window_size);
    }

    #[test]
    fn test_load_config_defaults() {
        let config = load_config(None, None).unwrap();
        assert_eq!(config.llm.provider, "openai");
        assert_eq!(config.safety.max_iterations, 25);
    }

    #[test]
    fn test_load_config_with_overrides() {
        let mut overrides = AgentConfig::default();
        overrides.llm.model = "claude-sonnet".to_string();
        overrides.safety.max_iterations = 50;

        let config = load_config(None, Some(&overrides)).unwrap();
        assert_eq!(config.llm.model, "claude-sonnet");
        assert_eq!(config.safety.max_iterations, 50);
    }

    #[test]
    fn test_load_config_from_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let rustant_dir = dir.path().join(".rustant");
        std::fs::create_dir_all(&rustant_dir).unwrap();
        std::fs::write(
            rustant_dir.join("config.toml"),
            r#"
[llm]
model = "gpt-4o-mini"
provider = "openai"
api_key_env = "OPENAI_API_KEY"
max_tokens = 4096
temperature = 0.7
context_window = 128000
input_cost_per_million = 2.5
output_cost_per_million = 10.0

[safety]
max_iterations = 100
approval_mode = "cautious"
allowed_paths = ["src/**"]
denied_paths = []
allowed_commands = ["cargo"]
ask_commands = []
denied_commands = []
allowed_hosts = []

[memory]
window_size = 12
compression_threshold = 0.7
enable_persistence = false

[ui]
theme = "dark"
vim_mode = false
show_cost = true
use_tui = false

[tools]
enable_builtins = true
default_timeout_secs = 30
max_output_bytes = 1048576
"#,
        )
        .unwrap();

        let config = load_config(Some(dir.path()), None).unwrap();
        assert_eq!(config.llm.model, "gpt-4o-mini");
        assert_eq!(config.safety.max_iterations, 100);
        assert_eq!(config.safety.approval_mode, ApprovalMode::Cautious);
    }

    #[test]
    fn test_safety_config_defaults() {
        let config = SafetyConfig::default();
        assert!(config.allowed_paths.contains(&"src/**".to_string()));
        assert!(config.denied_paths.contains(&".env*".to_string()));
        assert!(config.allowed_commands.contains(&"cargo".to_string()));
        assert!(config.denied_commands.contains(&"sudo".to_string()));
    }

    #[test]
    fn test_llm_config_defaults() {
        let config = LlmConfig::default();
        assert_eq!(config.context_window, 128_000);
        assert_eq!(config.max_tokens, 4096);
        assert!((config.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_llm_config_validate_defaults_clean() {
        let config = LlmConfig::default();
        let warnings = config.validate();
        assert!(
            warnings.is_empty(),
            "Default LlmConfig should have no warnings, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_llm_config_validate_max_tokens_exceeds_context() {
        let config = LlmConfig {
            max_tokens: 200_000,
            context_window: 128_000,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("max_tokens"));
        assert!(warnings[0].contains("context_window"));
    }

    #[test]
    fn test_llm_config_validate_bad_temperature() {
        let config = LlmConfig {
            temperature: 3.0,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("temperature"));
    }

    #[test]
    fn test_safety_denied_paths_include_sensitive_defaults() {
        let config = SafetyConfig::default();
        assert!(config.denied_paths.contains(&".ssh/**".to_string()));
        assert!(config.denied_paths.contains(&".aws/**".to_string()));
        assert!(config.denied_paths.contains(&"**/*.pem".to_string()));
        assert!(config.denied_paths.contains(&"**/*id_rsa*".to_string()));
        assert!(config.denied_paths.contains(&"**/*id_ed25519*".to_string()));
    }

    #[test]
    fn test_memory_config_defaults() {
        let config = MemoryConfig::default();
        assert_eq!(config.window_size, 12);
        assert!((config.compression_threshold - 0.7).abs() < f32::EPSILON);
        assert!(!config.enable_persistence);
    }

    #[test]
    fn test_approval_mode_serde() {
        let json = serde_json::to_string(&ApprovalMode::Paranoid).unwrap();
        assert_eq!(json, "\"paranoid\"");
        let mode: ApprovalMode = serde_json::from_str("\"yolo\"").unwrap();
        assert_eq!(mode, ApprovalMode::Yolo);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_agent_config_with_gateway() {
        let mut config = AgentConfig::default();
        config.gateway = Some(crate::gateway::GatewayConfig::default());
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.gateway.is_some());
        let gw = deserialized.gateway.unwrap();
        assert_eq!(gw.port, 8080);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_agent_config_with_search() {
        let mut config = AgentConfig::default();
        config.search = Some(crate::search::SearchConfig::default());
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.search.is_some());
        let sc = deserialized.search.unwrap();
        assert_eq!(sc.max_results, 10);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_agent_config_with_flush() {
        let mut config = AgentConfig::default();
        config.flush = Some(crate::memory::FlushConfig::default());
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.flush.is_some());
        let fc = deserialized.flush.unwrap();
        assert!(!fc.enabled);
        assert_eq!(fc.interval_secs, 300);
    }

    #[test]
    fn test_agent_config_backward_compat_no_optional_fields() {
        // Deserialize config without gateway/search/flush — all should be None
        let json = serde_json::json!({
            "llm": LlmConfig::default(),
            "safety": SafetyConfig::default(),
            "memory": MemoryConfig::default(),
            "ui": UiConfig::default(),
            "tools": ToolsConfig::default()
        });
        let config: AgentConfig = serde_json::from_value(json).unwrap();
        assert!(config.gateway.is_none());
        assert!(config.search.is_none());
        assert!(config.flush.is_none());
        assert!(config.multi_agent.is_none());
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_agent_config_with_multi_agent() {
        let mut config = AgentConfig::default();
        config.multi_agent = Some(MultiAgentConfig::default());
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.multi_agent.is_some());
        let ma = deserialized.multi_agent.unwrap();
        assert!(!ma.enabled);
        assert_eq!(ma.max_agents, 8);
        assert_eq!(ma.max_mailbox_size, 1000);
    }

    #[test]
    fn test_injection_detection_config_defaults() {
        let config = InjectionDetectionConfig::default();
        assert!(config.enabled);
        assert!((config.threshold - 0.5).abs() < f32::EPSILON);
        assert!(config.scan_tool_outputs);
    }

    #[test]
    fn test_safety_config_includes_injection_detection() {
        let config = SafetyConfig::default();
        assert!(config.injection_detection.enabled);
        // Serialization roundtrip
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SafetyConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.injection_detection.enabled);
        assert!(deserialized.injection_detection.scan_tool_outputs);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_multi_agent_config_with_resource_limits() {
        let mut config = MultiAgentConfig::default();
        config.default_resource_limits = crate::multi::ResourceLimits {
            max_memory_mb: Some(256),
            max_tokens_per_turn: Some(2048),
            max_tool_calls: Some(20),
            max_runtime_secs: Some(120),
        };
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: MultiAgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.default_resource_limits.max_memory_mb,
            Some(256)
        );
        assert_eq!(
            deserialized.default_resource_limits.max_tool_calls,
            Some(20)
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn test_multi_agent_config_with_workspace_base() {
        let mut config = MultiAgentConfig::default();
        config.default_workspace_base = Some("/tmp/rustant-workspaces".into());
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: MultiAgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.default_workspace_base.as_deref(),
            Some("/tmp/rustant-workspaces")
        );
    }

    #[test]
    fn test_multi_agent_config_backward_compat() {
        // Deserialize config without new fields — should use defaults
        let json = serde_json::json!({
            "enabled": true,
            "max_agents": 4,
            "max_mailbox_size": 500
        });
        let config: MultiAgentConfig = serde_json::from_value(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.max_agents, 4);
        assert!(config.default_resource_limits.max_memory_mb.is_none());
        assert!(config.default_workspace_base.is_none());
    }

    #[test]
    fn test_multi_agent_config_defaults() {
        let config = MultiAgentConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_agents, 8);
        assert_eq!(config.max_mailbox_size, 1000);
        assert!(config.default_resource_limits.max_memory_mb.is_none());
        assert!(config.default_workspace_base.is_none());
    }

    #[test]
    fn test_intelligence_config_defaults() {
        let config = IntelligenceConfig::default();
        assert!(config.enabled);
        assert_eq!(config.defaults.auto_reply, AutoReplyMode::FullAuto);
        assert_eq!(config.defaults.digest, DigestFrequency::Off);
        assert!(config.defaults.smart_scheduling);
        assert_eq!(config.defaults.escalation_threshold, MessagePriority::High);
        assert!(config.quiet_hours.is_none());
        assert_eq!(config.max_reply_tokens, 500);
        assert_eq!(config.digest_dir, PathBuf::from(".rustant/digests"));
        assert_eq!(config.reminders_dir, PathBuf::from(".rustant/reminders"));
    }

    #[test]
    fn test_intelligence_config_for_channel() {
        let mut config = IntelligenceConfig::default();
        config.channels.insert(
            "email".to_string(),
            ChannelIntelligenceConfig {
                auto_reply: AutoReplyMode::DraftOnly,
                digest: DigestFrequency::Daily,
                smart_scheduling: false,
                escalation_threshold: MessagePriority::Urgent,
                default_followup_minutes: 60,
            },
        );

        // email channel gets override
        let email = config.for_channel("email");
        assert_eq!(email.auto_reply, AutoReplyMode::DraftOnly);
        assert_eq!(email.digest, DigestFrequency::Daily);
        assert!(!email.smart_scheduling);

        // slack channel falls back to defaults
        let slack = config.for_channel("slack");
        assert_eq!(slack.auto_reply, AutoReplyMode::FullAuto);
        assert_eq!(slack.digest, DigestFrequency::Off);
    }

    #[test]
    fn test_intelligence_config_toml_deserialization() {
        let toml_str = r#"
            [llm]
            provider = "openai"
            model = "gpt-4o"
            api_key_env = "OPENAI_API_KEY"
            max_tokens = 4096
            temperature = 0.7
            context_window = 128000
            input_cost_per_million = 2.5
            output_cost_per_million = 10.0
            use_streaming = true

            [safety]
            approval_mode = "safe"
            allowed_paths = ["src/**"]
            denied_paths = []
            allowed_commands = ["cargo"]
            ask_commands = []
            denied_commands = []
            allowed_hosts = []
            max_iterations = 25

            [memory]
            window_size = 12
            compression_threshold = 0.7
            enable_persistence = false

            [ui]
            theme = "dark"
            vim_mode = false
            show_cost = true
            use_tui = false

            [tools]
            enable_builtins = true
            default_timeout_secs = 30
            max_output_bytes = 1048576

            [intelligence]
            enabled = true
            max_reply_tokens = 1000

            [intelligence.defaults]
            auto_reply = "auto_with_approval"
            digest = "daily"
            smart_scheduling = true
            escalation_threshold = "urgent"

            [intelligence.channels.email]
            auto_reply = "draft_only"
            digest = "weekly"

            [intelligence.quiet_hours]
            start = "22:00"
            end = "07:00"
        "#;

        let config: AgentConfig = toml::from_str(toml_str).unwrap();
        let intel = config.intelligence.unwrap();
        assert!(intel.enabled);
        assert_eq!(intel.max_reply_tokens, 1000);
        assert_eq!(intel.defaults.auto_reply, AutoReplyMode::AutoWithApproval);
        assert_eq!(intel.defaults.digest, DigestFrequency::Daily);
        assert_eq!(intel.defaults.escalation_threshold, MessagePriority::Urgent);

        let email = intel.for_channel("email");
        assert_eq!(email.auto_reply, AutoReplyMode::DraftOnly);
        assert_eq!(email.digest, DigestFrequency::Weekly);

        let quiet = intel.quiet_hours.unwrap();
        assert_eq!(quiet.start, "22:00");
        assert_eq!(quiet.end, "07:00");
    }

    #[test]
    fn test_auto_reply_mode_serde() {
        assert_eq!(
            serde_json::from_str::<AutoReplyMode>("\"full_auto\"").unwrap(),
            AutoReplyMode::FullAuto
        );
        assert_eq!(
            serde_json::from_str::<AutoReplyMode>("\"disabled\"").unwrap(),
            AutoReplyMode::Disabled
        );
        assert_eq!(
            serde_json::from_str::<AutoReplyMode>("\"draft_only\"").unwrap(),
            AutoReplyMode::DraftOnly
        );
    }

    #[test]
    fn test_message_priority_ordering() {
        assert!(MessagePriority::Low < MessagePriority::Normal);
        assert!(MessagePriority::Normal < MessagePriority::High);
        assert!(MessagePriority::High < MessagePriority::Urgent);
    }

    #[test]
    fn test_agent_config_with_intelligence_none() {
        // Verify backward compat: AgentConfig without intelligence field still works
        let config = AgentConfig::default();
        assert!(config.intelligence.is_none());
    }

    // --- S13: Config Validation Tests ---

    #[test]
    fn test_channel_config_validate_defaults_clean() {
        let config = ChannelIntelligenceConfig::default();
        let warnings = config.validate();
        assert!(
            warnings.is_empty(),
            "Default config should have no warnings, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_channel_config_validate_zero_followup() {
        let config = ChannelIntelligenceConfig {
            default_followup_minutes: 0,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("immediately"));
    }

    #[test]
    fn test_channel_config_validate_huge_followup() {
        let config = ChannelIntelligenceConfig {
            default_followup_minutes: u32::MAX,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unusually large"));
    }

    #[test]
    fn test_channel_config_validate_low_escalation() {
        let config = ChannelIntelligenceConfig {
            escalation_threshold: MessagePriority::Low,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("all messages will be escalated"));
    }

    #[test]
    fn test_intelligence_config_validate_clean() {
        let config = IntelligenceConfig::default();
        let warnings = config.validate();
        assert!(
            warnings.is_empty(),
            "Default config should have no warnings, got: {:?}",
            warnings
        );
    }

    #[test]
    fn test_intelligence_config_validate_bad_quiet_hours() {
        let config = IntelligenceConfig {
            quiet_hours: Some(crate::scheduler::QuietHours {
                start: "25:00".to_string(),
                end: "abc".to_string(),
            }),
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 2);
        assert!(warnings[0].contains("start"));
        assert!(warnings[1].contains("end"));
    }

    #[test]
    fn test_intelligence_config_validate_zero_reply_tokens() {
        let config = IntelligenceConfig {
            max_reply_tokens: 0,
            ..Default::default()
        };
        let warnings = config.validate();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("auto-replies will be empty"));
    }

    #[test]
    fn test_intelligence_config_validate_per_channel() {
        let mut config = IntelligenceConfig::default();
        config.channels.insert(
            "email".to_string(),
            ChannelIntelligenceConfig {
                escalation_threshold: MessagePriority::Low,
                default_followup_minutes: 0,
                ..Default::default()
            },
        );
        let warnings = config.validate();
        assert_eq!(warnings.len(), 2);
        assert!(warnings.iter().all(|w| w.starts_with("[channel:email]")));
    }

    #[test]
    fn test_is_valid_time_format() {
        assert!(super::is_valid_time_format("00:00"));
        assert!(super::is_valid_time_format("23:59"));
        assert!(super::is_valid_time_format("12:30"));
        assert!(!super::is_valid_time_format("24:00"));
        assert!(!super::is_valid_time_format("12:60"));
        assert!(!super::is_valid_time_format("abc"));
        assert!(!super::is_valid_time_format("1:30"));
        assert!(!super::is_valid_time_format(""));
    }
}
