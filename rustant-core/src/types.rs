//! Core type definitions for the Rustant agent.
//!
//! Defines the fundamental data structures used throughout the system:
//! messages, tool calls, content types, and agent state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use uuid::Uuid;

/// Represents a participant role in a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

/// Content within a message — text, tool call, or tool result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        call_id: String,
        output: String,
        is_error: bool,
    },
    MultiPart {
        parts: Vec<Content>,
    },
}

impl Content {
    /// Create a simple text content.
    pub fn text(text: impl Into<String>) -> Self {
        Content::Text { text: text.into() }
    }

    /// Create a tool call content.
    pub fn tool_call(id: impl Into<String>, name: impl Into<String>, arguments: serde_json::Value) -> Self {
        Content::ToolCall {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    /// Create a tool result content.
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>, is_error: bool) -> Self {
        Content::ToolResult {
            call_id: call_id.into(),
            output: output.into(),
            is_error,
        }
    }

    /// Returns the text representation of this content.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Content::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// A single message in the conversation history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub role: Role,
    pub content: Content,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Message {
    /// Create a new message with auto-generated ID and current timestamp.
    pub fn new(role: Role, content: Content) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Create a system message.
    pub fn system(text: impl Into<String>) -> Self {
        Self::new(Role::System, Content::text(text))
    }

    /// Create a user message.
    pub fn user(text: impl Into<String>) -> Self {
        Self::new(Role::User, Content::text(text))
    }

    /// Create an assistant message.
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::new(Role::Assistant, Content::text(text))
    }

    /// Create a tool result message.
    pub fn tool_result(call_id: impl Into<String>, output: impl Into<String>, is_error: bool) -> Self {
        Self::new(Role::Tool, Content::tool_result(call_id, output, is_error))
    }

    /// Add metadata to this message.
    pub fn with_metadata(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// A definition describing a tool for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// The risk level of a tool operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Read-only operations (level 0).
    ReadOnly = 0,
    /// Reversible write operations (level 1).
    Write = 1,
    /// Shell command execution (level 2).
    Execute = 2,
    /// Network operations (level 3).
    Network = 3,
    /// Destructive / irreversible operations (level 4).
    Destructive = 4,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::ReadOnly => write!(f, "read-only"),
            RiskLevel::Write => write!(f, "write"),
            RiskLevel::Execute => write!(f, "execute"),
            RiskLevel::Network => write!(f, "network"),
            RiskLevel::Destructive => write!(f, "destructive"),
        }
    }
}

/// Output produced by a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ToolOutput {
    /// Create a simple text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            artifacts: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// Create an error output.
    pub fn error(message: impl Into<String>) -> Self {
        let mut output = Self::text(message);
        output
            .metadata
            .insert("is_error".into(), serde_json::Value::Bool(true));
        output
    }

    /// Add an artifact to this output.
    pub fn with_artifact(mut self, artifact: Artifact) -> Self {
        self.artifacts.push(artifact);
        self
    }
}

/// An artifact produced by a tool (file created, data generated, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Artifact {
    FileCreated { path: PathBuf },
    FileModified { path: PathBuf, diff: String },
    FileDeleted { path: PathBuf },
    Data { mime_type: String, data: String },
}

/// The current state of the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Thinking,
    Deciding,
    Executing,
    WaitingForApproval,
    Complete,
    Error,
}

impl std::fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentStatus::Idle => write!(f, "idle"),
            AgentStatus::Thinking => write!(f, "thinking"),
            AgentStatus::Deciding => write!(f, "deciding"),
            AgentStatus::Executing => write!(f, "executing"),
            AgentStatus::WaitingForApproval => write!(f, "waiting for approval"),
            AgentStatus::Complete => write!(f, "complete"),
            AgentStatus::Error => write!(f, "error"),
        }
    }
}

/// Tracks the full state of the agent during task execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    pub task_id: Option<Uuid>,
    pub status: AgentStatus,
    pub current_goal: Option<String>,
    pub iteration: usize,
    pub max_iterations: usize,
    pub checkpoints: Vec<String>,
}

impl AgentState {
    pub fn new(max_iterations: usize) -> Self {
        Self {
            task_id: None,
            status: AgentStatus::Idle,
            current_goal: None,
            iteration: 0,
            max_iterations,
            checkpoints: Vec::new(),
        }
    }

    pub fn start_task(&mut self, goal: impl Into<String>) {
        self.task_id = Some(Uuid::new_v4());
        self.status = AgentStatus::Thinking;
        self.current_goal = Some(goal.into());
        self.iteration = 0;
        self.checkpoints.clear();
    }

    pub fn increment_iteration(&mut self) -> bool {
        self.iteration += 1;
        self.iteration <= self.max_iterations
    }

    pub fn complete(&mut self) {
        self.status = AgentStatus::Complete;
    }

    pub fn set_error(&mut self) {
        self.status = AgentStatus::Error;
    }

    pub fn reset(&mut self) {
        self.task_id = None;
        self.status = AgentStatus::Idle;
        self.current_goal = None;
        self.iteration = 0;
        self.checkpoints.clear();
    }
}

/// Token usage statistics from an LLM call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
}

impl TokenUsage {
    pub fn total(&self) -> usize {
        self.input_tokens + self.output_tokens
    }

    pub fn accumulate(&mut self, other: &TokenUsage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

/// Cost tracking for LLM usage.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CostEstimate {
    pub input_cost: f64,
    pub output_cost: f64,
}

impl CostEstimate {
    pub fn total(&self) -> f64 {
        self.input_cost + self.output_cost
    }

    pub fn accumulate(&mut self, other: &CostEstimate) {
        self.input_cost += other.input_cost;
        self.output_cost += other.output_cost;
    }
}

/// A stream event received during LLM response streaming.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments_delta: String },
    ToolCallEnd { id: String },
    Done { usage: TokenUsage },
    Error(String),
}

/// The result of an LLM completion request.
#[derive(Debug, Clone)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: TokenUsage,
    pub model: String,
    pub finish_reason: Option<String>,
}

/// A request to the LLM for completion.
#[derive(Debug, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<ToolDefinition>>,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
    pub stop_sequences: Vec<String>,
    pub model: Option<String>,
}

impl Default for CompletionRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: None,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: Vec::new(),
            model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = Message::user("Hello, world!");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.as_text(), Some("Hello, world!"));
        assert!(msg.metadata.is_empty());
    }

    #[test]
    fn test_message_with_metadata() {
        let msg = Message::assistant("Response")
            .with_metadata("model", serde_json::json!("gpt-4"));
        assert_eq!(msg.metadata.get("model"), Some(&serde_json::json!("gpt-4")));
    }

    #[test]
    fn test_system_message() {
        let msg = Message::system("You are a helpful assistant.");
        assert_eq!(msg.role, Role::System);
        assert_eq!(msg.content.as_text(), Some("You are a helpful assistant."));
    }

    #[test]
    fn test_tool_result_message() {
        let msg = Message::tool_result("call-1", "file contents here", false);
        match &msg.content {
            Content::ToolResult { call_id, output, is_error } => {
                assert_eq!(call_id, "call-1");
                assert_eq!(output, "file contents here");
                assert!(!is_error);
            }
            _ => panic!("Expected ToolResult content"),
        }
    }

    #[test]
    fn test_content_variants() {
        let text = Content::text("hello");
        assert_eq!(text.as_text(), Some("hello"));

        let tool_call = Content::tool_call("id1", "file_read", serde_json::json!({"path": "/tmp"}));
        assert_eq!(tool_call.as_text(), None);

        let tool_result = Content::tool_result("id1", "contents", false);
        assert_eq!(tool_result.as_text(), None);
    }

    #[test]
    fn test_role_display() {
        assert_eq!(Role::System.to_string(), "system");
        assert_eq!(Role::User.to_string(), "user");
        assert_eq!(Role::Assistant.to_string(), "assistant");
        assert_eq!(Role::Tool.to_string(), "tool");
    }

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::Write);
        assert!(RiskLevel::Write < RiskLevel::Execute);
        assert!(RiskLevel::Execute < RiskLevel::Network);
        assert!(RiskLevel::Network < RiskLevel::Destructive);
    }

    #[test]
    fn test_tool_output() {
        let output = ToolOutput::text("hello");
        assert_eq!(output.content, "hello");
        assert!(output.artifacts.is_empty());

        let output = ToolOutput::error("something went wrong");
        assert_eq!(output.content, "something went wrong");
        assert_eq!(
            output.metadata.get("is_error"),
            Some(&serde_json::Value::Bool(true))
        );
    }

    #[test]
    fn test_tool_output_with_artifact() {
        let output = ToolOutput::text("created file")
            .with_artifact(Artifact::FileCreated {
                path: "/tmp/test.rs".into(),
            });
        assert_eq!(output.artifacts.len(), 1);
    }

    #[test]
    fn test_agent_state_lifecycle() {
        let mut state = AgentState::new(25);
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.task_id.is_none());

        state.start_task("refactor auth module");
        assert_eq!(state.status, AgentStatus::Thinking);
        assert!(state.task_id.is_some());
        assert_eq!(state.current_goal.as_deref(), Some("refactor auth module"));
        assert_eq!(state.iteration, 0);

        assert!(state.increment_iteration());
        assert_eq!(state.iteration, 1);

        state.complete();
        assert_eq!(state.status, AgentStatus::Complete);

        state.reset();
        assert_eq!(state.status, AgentStatus::Idle);
        assert!(state.task_id.is_none());
    }

    #[test]
    fn test_agent_state_max_iterations() {
        let mut state = AgentState::new(2);
        state.start_task("test");

        assert!(state.increment_iteration()); // 1 <= 2
        assert!(state.increment_iteration()); // 2 <= 2
        assert!(!state.increment_iteration()); // 3 > 2
    }

    #[test]
    fn test_token_usage() {
        let mut usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        assert_eq!(usage.total(), 150);

        let other = TokenUsage {
            input_tokens: 200,
            output_tokens: 100,
        };
        usage.accumulate(&other);
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 150);
        assert_eq!(usage.total(), 450);
    }

    #[test]
    fn test_cost_estimate() {
        let mut cost = CostEstimate {
            input_cost: 0.01,
            output_cost: 0.03,
        };
        assert!((cost.total() - 0.04).abs() < f64::EPSILON);

        let other = CostEstimate {
            input_cost: 0.02,
            output_cost: 0.06,
        };
        cost.accumulate(&other);
        assert!((cost.input_cost - 0.03).abs() < f64::EPSILON);
        assert!((cost.output_cost - 0.09).abs() < f64::EPSILON);
    }

    #[test]
    fn test_message_serialization_roundtrip() {
        let msg = Message::user("test message");
        let json = serde_json::to_string(&msg).unwrap();
        let deserialized: Message = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.role, Role::User);
        assert_eq!(deserialized.content.as_text(), Some("test message"));
    }

    #[test]
    fn test_tool_definition_serialization() {
        let def = ToolDefinition {
            name: "file_read".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };
        let json = serde_json::to_string(&def).unwrap();
        let deserialized: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "file_read");
    }

    #[test]
    fn test_completion_request_default() {
        let req = CompletionRequest::default();
        assert!(req.messages.is_empty());
        assert!(req.tools.is_none());
        assert!((req.temperature - 0.7).abs() < f32::EPSILON);
        assert!(req.max_tokens.is_none());
        assert!(req.stop_sequences.is_empty());
        assert!(req.model.is_none());
    }

    #[test]
    fn test_agent_status_display() {
        assert_eq!(AgentStatus::Idle.to_string(), "idle");
        assert_eq!(AgentStatus::Thinking.to_string(), "thinking");
        assert_eq!(AgentStatus::WaitingForApproval.to_string(), "waiting for approval");
    }
}
