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
use std::path::{Path, PathBuf};

/// Top-level configuration for the Rustant agent.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentConfig {
    pub llm: LlmConfig,
    pub safety: SafetyConfig,
    pub memory: MemoryConfig,
    pub ui: UiConfig,
    pub tools: ToolsConfig,
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
        }
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
}
