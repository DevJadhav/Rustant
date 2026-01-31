//! Error types for the Rustant agent core.
//!
//! Uses `thiserror` for public API error types with structured error variants
//! covering LLM, tool execution, memory, configuration, and safety domains.

use std::path::PathBuf;

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
