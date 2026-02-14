//! Error types for the Rustant agent core.
//!
//! Uses `thiserror` for public API error types with structured error variants
//! covering LLM, tool execution, memory, configuration, and safety domains.

use std::path::PathBuf;
use uuid::Uuid;

/// Top-level error type for the Rustant core library.
#[derive(Debug, thiserror::Error)]
pub enum RustantError {
    #[error("LLM error: {0}")]
    Llm(#[from] LlmError),

    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),

    #[error("Memory error: {0}")]
    Memory(#[from] MemoryError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("Safety error: {0}")]
    Safety(#[from] SafetyError),

    #[error("Agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("Channel error: {0}")]
    Channel(#[from] ChannelError),

    #[error("Node error: {0}")]
    Node(#[from] NodeError),

    #[error("Workflow error: {0}")]
    Workflow(#[from] WorkflowError),

    #[error("Browser error: {0}")]
    Browser(#[from] BrowserError),

    #[error("Scheduler error: {0}")]
    Scheduler(#[from] SchedulerError),

    #[error("Voice error: {0}")]
    Voice(#[from] VoiceError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Errors from LLM provider interactions.
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API request failed: {message}")]
    ApiRequest { message: String },

    #[error("API response parse error: {message}")]
    ResponseParse { message: String },

    #[error("Streaming error: {message}")]
    Streaming { message: String },

    #[error("Context window exceeded: used {used} of {limit} tokens")]
    ContextOverflow { used: usize, limit: usize },

    #[error("Model not supported: {model}")]
    UnsupportedModel { model: String },

    #[error("Authentication failed for provider {provider}")]
    AuthFailed { provider: String },

    #[error("Rate limited by provider, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Provider connection failed: {message}")]
    Connection { message: String },

    #[error("OAuth flow failed: {message}")]
    OAuthFailed { message: String },
}

/// Errors from tool registration and execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool not found: {name}")]
    NotFound { name: String },

    #[error("Tool already registered: {name}")]
    AlreadyRegistered { name: String },

    #[error("Invalid arguments for tool '{name}': {reason}")]
    InvalidArguments { name: String, reason: String },

    #[error("Tool '{name}' execution failed: {message}")]
    ExecutionFailed { name: String, message: String },

    #[error("Tool '{name}' timed out after {timeout_secs}s")]
    Timeout { name: String, timeout_secs: u64 },

    #[error("Tool '{name}' was cancelled")]
    Cancelled { name: String },

    #[error("Permission denied for tool '{name}': {reason}")]
    PermissionDenied { name: String, reason: String },
}

/// Errors from the memory system.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("Context compression failed: {message}")]
    CompressionFailed { message: String },

    #[error("Memory persistence error: {message}")]
    PersistenceError { message: String },

    #[error("Memory capacity exceeded")]
    CapacityExceeded,

    #[error("Failed to load session: {message}")]
    SessionLoadFailed { message: String },
}

/// Errors from the configuration system.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Configuration file not found: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Invalid configuration: {message}")]
    Invalid { message: String },

    #[error("Missing required field: {field}")]
    MissingField { field: String },

    #[error("Environment variable not set: {var}")]
    EnvVarMissing { var: String },

    #[error("Configuration parse error: {message}")]
    ParseError { message: String },
}

/// Errors from the safety guardian.
#[derive(Debug, thiserror::Error)]
pub enum SafetyError {
    #[error("Action denied by safety policy: {reason}")]
    PolicyDenied { reason: String },

    #[error("Path access denied: {path}")]
    PathDenied { path: PathBuf },

    #[error("Command not allowed: {command}")]
    CommandDenied { command: String },

    #[error("Network access denied for host: {host}")]
    NetworkDenied { host: String },

    #[error("Sandbox creation failed: {message}")]
    SandboxFailed { message: String },

    #[error("Approval was rejected by user")]
    ApprovalRejected,
}

/// Errors from the agent orchestrator.
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("Maximum iterations ({max}) reached without completing task")]
    MaxIterationsReached { max: usize },

    #[error("Agent is already processing a task")]
    AlreadyBusy,

    #[error("Agent has been shut down")]
    ShutDown,

    #[error("Task was cancelled")]
    Cancelled,

    #[error("Invalid state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    #[error("Budget exceeded: {message}")]
    BudgetExceeded { message: String },
}

/// Errors from the channel system.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("Channel '{name}' connection failed: {message}")]
    ConnectionFailed { name: String, message: String },

    #[error("Channel '{name}' send failed: {message}")]
    SendFailed { name: String, message: String },

    #[error("Channel '{name}' is not connected")]
    NotConnected { name: String },

    #[error("Channel '{name}' authentication failed")]
    AuthFailed { name: String },

    #[error("Channel '{name}' rate limited")]
    RateLimited { name: String },
}

/// Errors from the node system.
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("No capable node for capability: {capability}")]
    NoCapableNode { capability: String },

    #[error("Node '{node_id}' execution failed: {message}")]
    ExecutionFailed { node_id: String, message: String },

    #[error("Node '{node_id}' is unreachable")]
    Unreachable { node_id: String },

    #[error("Consent denied for capability: {capability}")]
    ConsentDenied { capability: String },

    #[error("Node discovery failed: {message}")]
    DiscoveryFailed { message: String },
}

/// Errors from the workflow engine.
#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("Workflow parse error: {message}")]
    ParseError { message: String },

    #[error("Workflow validation failed: {message}")]
    ValidationFailed { message: String },

    #[error("Workflow step '{step}' failed: {message}")]
    StepFailed { step: String, message: String },

    #[error("Workflow '{name}' not found")]
    NotFound { name: String },

    #[error("Workflow run '{run_id}' not found")]
    RunNotFound { run_id: Uuid },

    #[error("Workflow approval timed out for step '{step}'")]
    ApprovalTimeout { step: String },

    #[error("Workflow cancelled")]
    Cancelled,

    #[error("Template render error: {message}")]
    TemplateError { message: String },
}

/// Errors from the browser automation system.
#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("Navigation failed: {message}")]
    NavigationFailed { message: String },

    #[error("Element not found: {selector}")]
    ElementNotFound { selector: String },

    #[error("JavaScript evaluation failed: {message}")]
    JsEvalFailed { message: String },

    #[error("Screenshot failed: {message}")]
    ScreenshotFailed { message: String },

    #[error("Browser timeout after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("URL blocked by security policy: {url}")]
    UrlBlocked { url: String },

    #[error("Browser session error: {message}")]
    SessionError { message: String },

    #[error("CDP protocol error: {message}")]
    CdpError { message: String },

    #[error("Page limit exceeded: maximum {max} pages")]
    PageLimitExceeded { max: usize },

    #[error("Tab not found: {tab_id}")]
    TabNotFound { tab_id: String },

    #[error("Browser not connected")]
    NotConnected,
}

/// Errors from the scheduler system.
#[derive(Debug, thiserror::Error)]
pub enum SchedulerError {
    #[error("Invalid cron expression '{expression}': {message}")]
    InvalidCronExpression { expression: String, message: String },

    #[error("Job '{name}' not found")]
    JobNotFound { name: String },

    #[error("Job '{name}' already exists")]
    JobAlreadyExists { name: String },

    #[error("Job '{name}' is disabled")]
    JobDisabled { name: String },

    #[error("Maximum background jobs ({max}) exceeded")]
    MaxJobsExceeded { max: usize },

    #[error("Background job '{id}' not found")]
    BackgroundJobNotFound { id: Uuid },

    #[error("Webhook verification failed: {message}")]
    WebhookVerificationFailed { message: String },

    #[error("Scheduler state persistence error: {message}")]
    PersistenceError { message: String },
}

/// Errors from the voice and audio system.
#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("Audio device error: {message}")]
    AudioDevice { message: String },

    #[error("STT transcription failed: {message}")]
    TranscriptionFailed { message: String },

    #[error("TTS synthesis failed: {message}")]
    SynthesisFailed { message: String },

    #[error("Wake word detection error: {message}")]
    WakeWordError { message: String },

    #[error("Voice pipeline error: {message}")]
    PipelineError { message: String },

    #[error("Unsupported audio format: {format}")]
    UnsupportedFormat { format: String },

    #[error("Voice model not found: {model}")]
    ModelNotFound { model: String },

    #[error("Voice feature not enabled (compile with --features voice)")]
    FeatureNotEnabled,

    #[error("Voice session timeout after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Voice provider authentication failed: {provider}")]
    AuthFailed { provider: String },

    #[error("Audio I/O error: {message}")]
    AudioError { message: String },
}

/// Trait providing actionable recovery guidance for errors.
///
/// Each error variant maps to a human-friendly suggestion and next steps,
/// helping users recover without external documentation.
pub trait UserGuidance {
    /// A concise suggestion for the most likely recovery action.
    fn suggestion(&self) -> Option<String>;

    /// Ordered list of next steps the user can try.
    fn next_steps(&self) -> Vec<String>;
}

impl UserGuidance for RustantError {
    fn suggestion(&self) -> Option<String> {
        match self {
            RustantError::Llm(e) => e.suggestion(),
            RustantError::Tool(e) => e.suggestion(),
            RustantError::Memory(e) => e.suggestion(),
            RustantError::Config(e) => e.suggestion(),
            RustantError::Safety(e) => e.suggestion(),
            RustantError::Agent(e) => e.suggestion(),
            RustantError::Channel(e) => e.suggestion(),
            RustantError::Node(e) => e.suggestion(),
            RustantError::Workflow(e) => e.suggestion(),
            RustantError::Browser(e) => e.suggestion(),
            RustantError::Scheduler(e) => e.suggestion(),
            RustantError::Voice(e) => e.suggestion(),
            RustantError::Io(_) => Some("Check file permissions and disk space.".into()),
            RustantError::Serialization(_) => {
                Some("Data may be corrupted. Try /doctor to check.".into())
            }
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            RustantError::Llm(e) => e.next_steps(),
            RustantError::Tool(e) => e.next_steps(),
            RustantError::Agent(e) => e.next_steps(),
            RustantError::Node(e) => e.next_steps(),
            RustantError::Workflow(e) => e.next_steps(),
            RustantError::Browser(e) => e.next_steps(),
            RustantError::Scheduler(e) => e.next_steps(),
            RustantError::Voice(e) => e.next_steps(),
            RustantError::Memory(e) => e.next_steps(),
            RustantError::Config(e) => e.next_steps(),
            RustantError::Safety(e) => e.next_steps(),
            RustantError::Channel(e) => e.next_steps(),
            _ => vec![],
        }
    }
}

impl UserGuidance for LlmError {
    fn suggestion(&self) -> Option<String> {
        match self {
            LlmError::AuthFailed { provider } => Some(format!(
                "Authentication failed for {}. Check your API key.",
                provider
            )),
            LlmError::RateLimited { retry_after_secs } => Some(format!(
                "Rate limited. Rustant will retry in {}s.",
                retry_after_secs
            )),
            LlmError::Connection { .. } => {
                Some("Cannot reach the LLM provider. Check your network.".into())
            }
            LlmError::Timeout { timeout_secs } => {
                Some(format!("Request timed out after {}s.", timeout_secs))
            }
            LlmError::ContextOverflow { used, limit } => Some(format!(
                "Context full ({}/{} tokens). Use /compact to free space.",
                used, limit
            )),
            LlmError::UnsupportedModel { model } => Some(format!(
                "Model '{}' is not supported by this provider.",
                model
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            LlmError::AuthFailed { .. } => vec![
                "Run /doctor to verify API key status.".into(),
                "Run /setup to reconfigure your provider.".into(),
            ],
            LlmError::RateLimited { .. } => {
                vec!["Wait for the retry or switch models with /config model <name>.".into()]
            }
            LlmError::Connection { .. } => vec![
                "Check your internet connection.".into(),
                "Run /doctor to test LLM connectivity.".into(),
            ],
            LlmError::ContextOverflow { .. } => vec![
                "Use /compact to compress conversation history.".into(),
                "Use /pin to protect important messages before compression.".into(),
            ],
            _ => vec![],
        }
    }
}

impl UserGuidance for ToolError {
    fn suggestion(&self) -> Option<String> {
        match self {
            ToolError::NotFound { name } => Some(format!(
                "Tool '{}' is not registered. Use /tools to list available tools.",
                name
            )),
            ToolError::InvalidArguments { name, reason } => {
                Some(format!("Invalid arguments for '{}': {}", name, reason))
            }
            ToolError::ExecutionFailed { name, message } => {
                // Try to categorize the failure
                if message.contains("No such file") || message.contains("not found") {
                    Some("File not found. Use file_list to browse available files.".to_string())
                } else if message.contains("Permission denied") {
                    Some(format!(
                        "Permission denied for '{name}'. Check file permissions."
                    ))
                } else {
                    Some(format!(
                        "Tool '{name}' failed. The agent will try to recover."
                    ))
                }
            }
            ToolError::Timeout { name, timeout_secs } => Some(format!(
                "Tool '{}' timed out after {}s. Consider breaking the task into smaller steps.",
                name, timeout_secs
            )),
            ToolError::PermissionDenied { name, .. } => Some(format!(
                "Permission denied for '{}'. Adjust with /permissions.",
                name
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            ToolError::Timeout { .. } => {
                vec!["Try a more specific query or smaller file range.".into()]
            }
            ToolError::NotFound { .. } => vec!["Run /tools to see registered tools.".into()],
            _ => vec![],
        }
    }
}

impl UserGuidance for MemoryError {
    fn suggestion(&self) -> Option<String> {
        match self {
            MemoryError::CompressionFailed { .. } => {
                Some("Context compression failed. Use /compact to retry manually.".into())
            }
            MemoryError::CapacityExceeded => {
                Some("Memory capacity exceeded. Use /compact or start a new session.".into())
            }
            MemoryError::SessionLoadFailed { message } => Some(format!(
                "Session load failed: {}. Use /sessions to list available sessions.",
                message
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        vec![]
    }
}

impl UserGuidance for ConfigError {
    fn suggestion(&self) -> Option<String> {
        match self {
            ConfigError::MissingField { field } => Some(format!(
                "Missing config field '{}'. Run /setup to configure.",
                field
            )),
            ConfigError::EnvVarMissing { var } => {
                Some(format!("Set environment variable {} or run /setup.", var))
            }
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        vec![]
    }
}

impl UserGuidance for SafetyError {
    fn suggestion(&self) -> Option<String> {
        match self {
            SafetyError::ApprovalRejected => {
                Some("Action was denied. The agent will try an alternative approach.".into())
            }
            SafetyError::PathDenied { path } => Some(format!(
                "Path '{}' is blocked by safety policy.",
                path.display()
            )),
            SafetyError::CommandDenied { command } => Some(format!(
                "Command '{}' is not in the allowed list. Adjust in config.",
                command
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        vec![]
    }
}

impl UserGuidance for AgentError {
    fn suggestion(&self) -> Option<String> {
        match self {
            AgentError::MaxIterationsReached { max } => Some(format!(
                "Task exceeded {} iterations. Break it into smaller steps.",
                max
            )),
            AgentError::BudgetExceeded { .. } => {
                Some("Token budget exceeded. Start a new session or increase the budget.".into())
            }
            AgentError::Cancelled => Some("Task was cancelled.".into()),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            AgentError::MaxIterationsReached { .. } => vec![
                "Increase limit with /config max_iterations <n>.".into(),
                "Break your task into smaller, focused steps.".into(),
            ],
            _ => vec![],
        }
    }
}

impl UserGuidance for ChannelError {
    fn suggestion(&self) -> Option<String> {
        match self {
            ChannelError::ConnectionFailed { name, .. } => Some(format!(
                "Channel '{}' connection failed. Check credentials.",
                name
            )),
            ChannelError::AuthFailed { name } => Some(format!(
                "Channel '{}' auth failed. Re-run channel setup.",
                name
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        vec![]
    }
}

impl UserGuidance for NodeError {
    fn suggestion(&self) -> Option<String> {
        match self {
            NodeError::NoCapableNode { capability } => Some(format!(
                "No node has the '{}' capability. Check node configuration.",
                capability
            )),
            NodeError::ExecutionFailed { node_id, .. } => Some(format!(
                "Node '{}' failed. It may be overloaded or misconfigured.",
                node_id
            )),
            NodeError::Unreachable { node_id } => Some(format!(
                "Node '{}' is unreachable. Check network connectivity.",
                node_id
            )),
            NodeError::ConsentDenied { capability } => Some(format!(
                "Consent denied for '{}'. Grant permission in node settings.",
                capability
            )),
            NodeError::DiscoveryFailed { .. } => {
                Some("Node discovery failed. Check gateway configuration.".into())
            }
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            NodeError::Unreachable { .. } => vec![
                "Verify the node is running and accessible.".into(),
                "Check firewall and network settings.".into(),
            ],
            _ => vec![],
        }
    }
}

impl UserGuidance for WorkflowError {
    fn suggestion(&self) -> Option<String> {
        match self {
            WorkflowError::NotFound { name } => Some(format!(
                "Workflow '{}' not found. Use /workflows to list available workflows.",
                name
            )),
            WorkflowError::StepFailed { step, .. } => Some(format!(
                "Workflow step '{}' failed. Check inputs and retry.",
                step
            )),
            WorkflowError::ValidationFailed { message } => Some(format!(
                "Workflow validation failed: {}. Fix the definition and retry.",
                message
            )),
            WorkflowError::ApprovalTimeout { step } => Some(format!(
                "Approval timed out for step '{}'. Re-run the workflow.",
                step
            )),
            WorkflowError::Cancelled => Some("Workflow was cancelled.".into()),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            WorkflowError::NotFound { .. } => {
                vec!["Run /workflows to see available workflow templates.".into()]
            }
            _ => vec![],
        }
    }
}

impl UserGuidance for BrowserError {
    fn suggestion(&self) -> Option<String> {
        match self {
            BrowserError::NotConnected => {
                Some("Browser is not connected. Start a browser session first.".into())
            }
            BrowserError::Timeout { timeout_secs } => Some(format!(
                "Browser timed out after {}s. The page may be slow to load.",
                timeout_secs
            )),
            BrowserError::ElementNotFound { selector } => Some(format!(
                "Element '{}' not found. The page structure may have changed.",
                selector
            )),
            BrowserError::UrlBlocked { url } => {
                Some(format!("URL '{}' is blocked by security policy.", url))
            }
            BrowserError::NavigationFailed { .. } => {
                Some("Navigation failed. Check the URL and try again.".into())
            }
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            BrowserError::NotConnected => {
                vec!["Run 'rustant browser test' to verify browser connectivity.".into()]
            }
            _ => vec![],
        }
    }
}

impl UserGuidance for SchedulerError {
    fn suggestion(&self) -> Option<String> {
        match self {
            SchedulerError::InvalidCronExpression { expression, .. } => Some(format!(
                "Invalid cron expression '{}'. Use standard cron syntax (e.g., '0 9 * * *').",
                expression
            )),
            SchedulerError::JobNotFound { name } => Some(format!(
                "Job '{}' not found. Use 'rustant cron list' to see existing jobs.",
                name
            )),
            SchedulerError::JobAlreadyExists { name } => Some(format!(
                "Job '{}' already exists. Use a different name or remove the existing one.",
                name
            )),
            SchedulerError::MaxJobsExceeded { max } => Some(format!(
                "Maximum of {} jobs reached. Remove some before adding new ones.",
                max
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            SchedulerError::JobNotFound { .. } => {
                vec!["Run 'rustant cron list' to see existing jobs.".into()]
            }
            _ => vec![],
        }
    }
}

impl UserGuidance for VoiceError {
    fn suggestion(&self) -> Option<String> {
        match self {
            VoiceError::FeatureNotEnabled => Some(
                "Voice features require the 'voice' feature flag. Recompile with --features voice."
                    .into(),
            ),
            VoiceError::AudioDevice { .. } => {
                Some("Audio device error. Check that a microphone/speaker is connected.".into())
            }
            VoiceError::AuthFailed { provider } => Some(format!(
                "Voice provider '{}' auth failed. Check API key.",
                provider
            )),
            VoiceError::ModelNotFound { model } => Some(format!(
                "Voice model '{}' not found. Check available models.",
                model
            )),
            VoiceError::Timeout { timeout_secs } => Some(format!(
                "Voice operation timed out after {}s.",
                timeout_secs
            )),
            _ => None,
        }
    }

    fn next_steps(&self) -> Vec<String> {
        match self {
            VoiceError::FeatureNotEnabled => {
                vec!["Recompile: cargo build --features voice".into()]
            }
            VoiceError::AuthFailed { .. } => {
                vec!["Run /doctor to verify API key status.".into()]
            }
            _ => vec![],
        }
    }
}

/// A type alias for results using the top-level `RustantError`.
pub type Result<T> = std::result::Result<T, RustantError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_llm() {
        let err = RustantError::Llm(LlmError::ApiRequest {
            message: "connection refused".into(),
        });
        assert_eq!(
            err.to_string(),
            "LLM error: API request failed: connection refused"
        );
    }

    #[test]
    fn test_error_display_tool() {
        let err = RustantError::Tool(ToolError::NotFound {
            name: "nonexistent".into(),
        });
        assert_eq!(err.to_string(), "Tool error: Tool not found: nonexistent");
    }

    #[test]
    fn test_error_display_safety() {
        let err = RustantError::Safety(SafetyError::PathDenied {
            path: PathBuf::from("/etc/passwd"),
        });
        assert_eq!(
            err.to_string(),
            "Safety error: Path access denied: /etc/passwd"
        );
    }

    #[test]
    fn test_error_display_config() {
        let err = RustantError::Config(ConfigError::MissingField {
            field: "llm.api_key".into(),
        });
        assert_eq!(
            err.to_string(),
            "Configuration error: Missing required field: llm.api_key"
        );
    }

    #[test]
    fn test_error_display_agent() {
        let err = RustantError::Agent(AgentError::MaxIterationsReached { max: 25 });
        assert_eq!(
            err.to_string(),
            "Agent error: Maximum iterations (25) reached without completing task"
        );
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: RustantError = io_err.into();
        assert!(matches!(err, RustantError::Io(_)));
    }

    #[test]
    fn test_error_from_serde() {
        let serde_err = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let err: RustantError = serde_err.into();
        assert!(matches!(err, RustantError::Serialization(_)));
    }

    #[test]
    fn test_tool_error_variants() {
        let err = ToolError::InvalidArguments {
            name: "file_read".into(),
            reason: "path is required".into(),
        };
        assert_eq!(
            err.to_string(),
            "Invalid arguments for tool 'file_read': path is required"
        );

        let err = ToolError::Timeout {
            name: "shell_exec".into(),
            timeout_secs: 30,
        };
        assert_eq!(err.to_string(), "Tool 'shell_exec' timed out after 30s");
    }

    #[test]
    fn test_error_display_channel() {
        let err = RustantError::Channel(ChannelError::ConnectionFailed {
            name: "telegram".into(),
            message: "timeout".into(),
        });
        assert_eq!(
            err.to_string(),
            "Channel error: Channel 'telegram' connection failed: timeout"
        );

        let err = ChannelError::NotConnected {
            name: "slack".into(),
        };
        assert_eq!(err.to_string(), "Channel 'slack' is not connected");
    }

    #[test]
    fn test_error_display_node() {
        let err = RustantError::Node(NodeError::NoCapableNode {
            capability: "shell".into(),
        });
        assert_eq!(
            err.to_string(),
            "Node error: No capable node for capability: shell"
        );

        let err = NodeError::ConsentDenied {
            capability: "filesystem".into(),
        };
        assert_eq!(err.to_string(), "Consent denied for capability: filesystem");
    }

    #[test]
    fn test_error_display_voice() {
        let err = RustantError::Voice(VoiceError::TranscriptionFailed {
            message: "model not loaded".into(),
        });
        assert_eq!(
            err.to_string(),
            "Voice error: STT transcription failed: model not loaded"
        );

        let err = VoiceError::FeatureNotEnabled;
        assert_eq!(
            err.to_string(),
            "Voice feature not enabled (compile with --features voice)"
        );

        let err = VoiceError::AudioDevice {
            message: "no microphone found".into(),
        };
        assert_eq!(err.to_string(), "Audio device error: no microphone found");
    }

    #[test]
    fn test_llm_error_variants() {
        let err = LlmError::ContextOverflow {
            used: 150_000,
            limit: 128_000,
        };
        assert_eq!(
            err.to_string(),
            "Context window exceeded: used 150000 of 128000 tokens"
        );

        let err = LlmError::RateLimited {
            retry_after_secs: 60,
        };
        assert_eq!(err.to_string(), "Rate limited by provider, retry after 60s");
    }

    #[test]
    fn test_node_error_guidance() {
        let err = NodeError::Unreachable {
            node_id: "node-1".into(),
        };
        assert!(err.suggestion().is_some());
        assert!(err.suggestion().unwrap().contains("node-1"));
        assert!(!err.next_steps().is_empty());
    }

    #[test]
    fn test_workflow_error_guidance() {
        let err = WorkflowError::NotFound {
            name: "deploy".into(),
        };
        assert!(err.suggestion().unwrap().contains("deploy"));
        assert!(!err.next_steps().is_empty());
    }

    #[test]
    fn test_browser_error_guidance() {
        let err = BrowserError::NotConnected;
        assert!(err.suggestion().is_some());
        assert!(!err.next_steps().is_empty());
    }

    #[test]
    fn test_scheduler_error_guidance() {
        let err = SchedulerError::JobNotFound {
            name: "backup".into(),
        };
        assert!(err.suggestion().unwrap().contains("backup"));
        assert!(!err.next_steps().is_empty());
    }

    #[test]
    fn test_voice_error_guidance() {
        let err = VoiceError::FeatureNotEnabled;
        assert!(err.suggestion().unwrap().contains("voice"));
        assert!(!err.next_steps().is_empty());
    }

    #[test]
    fn test_rustant_error_dispatches_all_guidance() {
        // Verify node errors dispatch through RustantError
        let err = RustantError::Node(NodeError::Unreachable {
            node_id: "n".into(),
        });
        assert!(err.suggestion().is_some());
        assert!(!err.next_steps().is_empty());

        // Verify voice errors dispatch through RustantError
        let err = RustantError::Voice(VoiceError::FeatureNotEnabled);
        assert!(err.suggestion().is_some());
        assert!(!err.next_steps().is_empty());

        // Verify workflow errors dispatch through RustantError
        let err = RustantError::Workflow(WorkflowError::NotFound { name: "w".into() });
        assert!(err.suggestion().is_some());
    }

    #[test]
    fn test_memory_error_guidance() {
        let err = MemoryError::CompressionFailed {
            message: "out of memory".into(),
        };
        assert!(err.suggestion().is_some());

        let err = MemoryError::CapacityExceeded;
        assert!(err.suggestion().is_some());
    }

    #[test]
    fn test_config_error_guidance() {
        let err = ConfigError::MissingField {
            field: "api_key".into(),
        };
        assert!(err.suggestion().is_some());

        let err = ConfigError::EnvVarMissing {
            var: "OPENAI_API_KEY".into(),
        };
        assert!(err.suggestion().is_some());
    }

    #[test]
    fn test_safety_error_guidance() {
        let err = SafetyError::PathDenied {
            path: "/etc/passwd".into(),
        };
        assert!(err.suggestion().is_some());

        let err = SafetyError::ApprovalRejected;
        assert!(err.suggestion().is_some());
    }

    #[test]
    fn test_next_steps_delegation_memory_config_safety_channel() {
        // These error types should delegate next_steps through RustantError
        let err = RustantError::Memory(MemoryError::CompressionFailed {
            message: "test".into(),
        });
        // Should not panic; returns vec (may be empty but delegation works)
        let _ = err.next_steps();

        let err = RustantError::Config(ConfigError::MissingField {
            field: "test".into(),
        });
        let _ = err.next_steps();

        let err = RustantError::Safety(SafetyError::ApprovalRejected);
        let _ = err.next_steps();

        let err = RustantError::Channel(ChannelError::ConnectionFailed {
            name: "test".into(),
            message: "fail".into(),
        });
        let _ = err.next_steps();
    }
}
