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
}
