//! # Rustant Core
//!
//! Core library for the Rustant autonomous agent.
//! Provides the agent orchestrator, LLM interface (brain), memory system,
//! safety guardian, configuration, and fundamental types.

pub mod agent;
pub mod brain;
pub mod config;
pub mod error;
pub mod memory;
pub mod safety;
pub mod types;

// Re-export commonly used types at the crate root.
pub use agent::{Agent, AgentCallback, AgentMessage, NoOpCallback, RegisteredTool, TaskResult};
pub use brain::{Brain, LlmProvider, MockLlmProvider, TokenCounter};
pub use config::{AgentConfig, ApprovalMode};
pub use error::{Result, RustantError};
pub use memory::{MemorySystem, Session, SessionMetadata};
pub use safety::SafetyGuardian;
pub use types::{
    AgentState, AgentStatus, Artifact, CompletionRequest, CompletionResponse, Content,
    CostEstimate, Message, RiskLevel, Role, StreamEvent, TokenUsage, ToolDefinition, ToolOutput,
};
