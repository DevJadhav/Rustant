//! Agent orchestrator implementing the Think → Act → Observe event loop.
//!
//! The `Agent` struct ties together the Brain, ToolRegistry, Memory, and Safety
//! Guardian to autonomously execute tasks through LLM-powered reasoning.

use crate::brain::{Brain, LlmProvider};
use crate::config::AgentConfig;
use crate::error::{AgentError, RustantError, ToolError};
use crate::memory::MemorySystem;
use crate::safety::{
    ActionDetails, ActionRequest, PermissionResult, SafetyGuardian,
};
use crate::types::{
    AgentState, AgentStatus, Content, CostEstimate, Message,
    RiskLevel, TokenUsage, ToolDefinition, ToolOutput,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use uuid::Uuid;

/// Messages sent to the agent loop via the handle.
pub enum AgentMessage {
    ProcessTask {
        task: String,
        reply: oneshot::Sender<TaskResult>,
    },
    Cancel {
        task_id: Uuid,
    },
    GetStatus {
        reply: oneshot::Sender<AgentStatus>,
    },
    Shutdown,
}

/// The result of a completed task.
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: Uuid,
    pub success: bool,
    pub response: String,
    pub iterations: usize,
    pub total_usage: TokenUsage,
    pub total_cost: CostEstimate,
}

/// Callback trait for user interaction (approval, display).
#[async_trait::async_trait]
pub trait AgentCallback: Send + Sync {
    /// Display a message from the assistant to the user.
    async fn on_assistant_message(&self, message: &str);

    /// Display a streaming token from the assistant.
    async fn on_token(&self, token: &str);

    /// Request approval for an action. Returns true if approved.
    async fn request_approval(&self, action: &ActionRequest) -> bool;

    /// Notify about a tool execution.
    async fn on_tool_start(&self, tool_name: &str, args: &serde_json::Value);

    /// Notify about a tool result.
    async fn on_tool_result(&self, tool_name: &str, output: &ToolOutput, duration_ms: u64);

    /// Notify about agent status changes.
    async fn on_status_change(&self, status: AgentStatus);
}

/// A tool executor function type. The agent holds tool executors and their definitions.
pub type ToolExecutor = Box<
    dyn Fn(serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ToolOutput, ToolError>> + Send>>
        + Send
        + Sync,
>;

/// A registered tool with its definition and executor.
pub struct RegisteredTool {
    pub definition: ToolDefinition,
    pub risk_level: RiskLevel,
    pub executor: ToolExecutor,
}

/// The Agent orchestrator running the Think → Act → Observe loop.
pub struct Agent {
    brain: Brain,
    memory: MemorySystem,
    safety: SafetyGuardian,
    tools: HashMap<String, RegisteredTool>,
    state: AgentState,
    #[allow(dead_code)]
    config: AgentConfig,
    cancellation: CancellationToken,
    callback: Arc<dyn AgentCallback>,
}

impl Agent {
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        config: AgentConfig,
        callback: Arc<dyn AgentCallback>,
    ) -> Self {
        let brain = Brain::new(
            provider,
            crate::brain::DEFAULT_SYSTEM_PROMPT,
        );
        let memory = MemorySystem::new(config.memory.window_size);
        let safety = SafetyGuardian::new(config.safety.clone());
        let max_iter = config.safety.max_iterations;

        Self {
            brain,
            memory,
            safety,
            tools: HashMap::new(),
            state: AgentState::new(max_iter),
            config,
            cancellation: CancellationToken::new(),
            callback,
        }
    }

    /// Register a tool with the agent.
    pub fn register_tool(&mut self, tool: RegisteredTool) {
        self.tools.insert(tool.definition.name.clone(), tool);
    }

    /// Get tool definitions for the LLM.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition.clone()).collect()
    }

    /// Process a user task through the agent loop.
    pub async fn process_task(&mut self, task: &str) -> Result<TaskResult, RustantError> {
        let task_id = Uuid::new_v4();
        info!(task_id = %task_id, task = task, "Starting task processing");

        self.state.start_task(task);
        self.state.task_id = Some(task_id);
        self.memory.start_new_task(task);
        self.memory.add_message(Message::user(task));
        self.callback.on_status_change(AgentStatus::Thinking).await;

        let mut final_response = String::new();

        loop {
            // Check cancellation
            if self.cancellation.is_cancelled() {
                self.state.set_error();
                return Err(RustantError::Agent(AgentError::Cancelled));
            }

            // Check iteration limit
            if !self.state.increment_iteration() {
                warn!(
                    task_id = %task_id,
                    iterations = self.state.iteration,
                    "Maximum iterations reached"
                );
                self.state.set_error();
                return Err(RustantError::Agent(AgentError::MaxIterationsReached {
                    max: self.state.max_iterations,
                }));
            }

            debug!(
                task_id = %task_id,
                iteration = self.state.iteration,
                "Agent loop iteration"
            );

            // --- THINK ---
            self.state.status = AgentStatus::Thinking;
            self.callback.on_status_change(AgentStatus::Thinking).await;

            let conversation = self.memory.context_messages();
            let tools = Some(self.tool_definitions());
            let response = self.brain.think(&conversation, tools).await?;

            // --- DECIDE ---
            self.state.status = AgentStatus::Deciding;
            match &response.message.content {
                Content::Text { text } => {
                    // LLM produced a text response — task may be complete
                    info!(task_id = %task_id, "Agent produced text response");
                    self.callback.on_assistant_message(text).await;
                    self.memory.add_message(response.message.clone());
                    final_response = text.clone();
                    // Text response means the agent is done thinking
                    break;
                }
                Content::ToolCall { id, name, arguments } => {
                    // LLM wants to call a tool
                    info!(
                        task_id = %task_id,
                        tool = name,
                        "Agent requesting tool execution"
                    );
                    self.memory.add_message(response.message.clone());

                    // --- ACT ---
                    let result = self
                        .execute_tool(id, name, arguments)
                        .await;

                    // --- OBSERVE ---
                    match result {
                        Ok(output) => {
                            let result_msg = Message::tool_result(id, &output.content, false);
                            self.memory.add_message(result_msg);
                        }
                        Err(e) => {
                            let error_msg = format!("Tool error: {}", e);
                            let result_msg = Message::tool_result(id, &error_msg, true);
                            self.memory.add_message(result_msg);
                        }
                    }

                    // Check context compression
                    if self.memory.short_term.needs_compression() {
                        debug!("Triggering context compression");
                        // For now, simple truncation-based compression
                        let msgs = self.memory.short_term.messages_to_summarize();
                        let summary = msgs
                            .iter()
                            .filter_map(|m| m.content.as_text())
                            .collect::<Vec<_>>()
                            .join("\n");
                        let compressed_summary = if summary.len() > 500 {
                            format!("{}...", &summary[..500])
                        } else {
                            summary
                        };
                        self.memory.short_term.compress(compressed_summary);
                    }

                    // Continue loop — agent needs to observe and think again
                }
                Content::MultiPart { parts } => {
                    // Handle multi-part responses (text + tool calls)
                    self.memory.add_message(response.message.clone());

                    let mut has_tool_call = false;
                    for part in parts {
                        match part {
                            Content::Text { text } => {
                                self.callback.on_assistant_message(text).await;
                                final_response = text.clone();
                            }
                            Content::ToolCall { id, name, arguments } => {
                                has_tool_call = true;
                                let result = self.execute_tool(id, name, arguments).await;
                                match result {
                                    Ok(output) => {
                                        let msg = Message::tool_result(id, &output.content, false);
                                        self.memory.add_message(msg);
                                    }
                                    Err(e) => {
                                        let msg = Message::tool_result(
                                            id,
                                            &format!("Tool error: {}", e),
                                            true,
                                        );
                                        self.memory.add_message(msg);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    if !has_tool_call {
                        break; // Only text, we're done
                    }
                    // If there were tool calls, continue the loop
                }
                Content::ToolResult { .. } => {
                    // Shouldn't happen from LLM directly, but handle gracefully
                    warn!("Received unexpected ToolResult from LLM");
                    break;
                }
            }
        }

        self.state.complete();
        self.callback.on_status_change(AgentStatus::Complete).await;

        info!(
            task_id = %task_id,
            iterations = self.state.iteration,
            total_tokens = self.brain.total_usage().total(),
            total_cost = format!("${:.4}", self.brain.total_cost().total()),
            "Task completed"
        );

        Ok(TaskResult {
            task_id,
            success: true,
            response: final_response,
            iterations: self.state.iteration,
            total_usage: *self.brain.total_usage(),
            total_cost: *self.brain.total_cost(),
        })
    }

    /// Execute a tool with safety checks.
    async fn execute_tool(
        &mut self,
        _call_id: &str,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        // Look up the tool
        let tool = self.tools.get(tool_name).ok_or_else(|| ToolError::NotFound {
            name: tool_name.to_string(),
        })?;

        // Build action request for safety check
        let action = SafetyGuardian::create_action_request(
            tool_name,
            tool.risk_level,
            format!("Execute tool: {}", tool_name),
            ActionDetails::Other {
                info: arguments.to_string(),
            },
        );

        // Check permissions
        let perm = self.safety.check_permission(&action);
        match perm {
            PermissionResult::Allowed => {
                // Proceed
            }
            PermissionResult::Denied { reason } => {
                return Err(ToolError::PermissionDenied {
                    name: tool_name.to_string(),
                    reason,
                });
            }
            PermissionResult::RequiresApproval { context: _ } => {
                self.state.status = AgentStatus::WaitingForApproval;
                self.callback
                    .on_status_change(AgentStatus::WaitingForApproval)
                    .await;

                let approved = self.callback.request_approval(&action).await;
                self.safety.log_approval_decision(tool_name, approved);

                if !approved {
                    return Err(ToolError::PermissionDenied {
                        name: tool_name.to_string(),
                        reason: "User rejected the action".to_string(),
                    });
                }
            }
        }

        // Execute the tool
        self.state.status = AgentStatus::Executing;
        self.callback.on_status_change(AgentStatus::Executing).await;
        self.callback.on_tool_start(tool_name, arguments).await;

        let start = Instant::now();

        // We need to get the executor without borrowing self
        let executor = &self.tools.get(tool_name).unwrap().executor;
        let result = (executor)(arguments.clone()).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        match &result {
            Ok(output) => {
                self.safety.log_execution(tool_name, true, duration_ms);
                self.callback
                    .on_tool_result(tool_name, output, duration_ms)
                    .await;
            }
            Err(e) => {
                self.safety.log_execution(tool_name, false, duration_ms);
                let error_output = ToolOutput::error(e.to_string());
                self.callback
                    .on_tool_result(tool_name, &error_output, duration_ms)
                    .await;
            }
        }

        result
    }

    /// Get the current agent state.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Get a cancellation token for this agent.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    /// Cancel the current task.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// Get the brain reference (for usage stats).
    pub fn brain(&self) -> &Brain {
        &self.brain
    }

    /// Get the safety guardian reference (for audit log).
    pub fn safety(&self) -> &SafetyGuardian {
        &self.safety
    }
}

/// A no-op callback for testing.
pub struct NoOpCallback;

#[async_trait::async_trait]
impl AgentCallback for NoOpCallback {
    async fn on_assistant_message(&self, _message: &str) {}
    async fn on_token(&self, _token: &str) {}
    async fn request_approval(&self, _action: &ActionRequest) -> bool {
        true // auto-approve in tests
    }
    async fn on_tool_start(&self, _tool_name: &str, _args: &serde_json::Value) {}
    async fn on_tool_result(&self, _tool_name: &str, _output: &ToolOutput, _duration_ms: u64) {}
    async fn on_status_change(&self, _status: AgentStatus) {}
}

/// A callback that records all events for test assertions.
pub struct RecordingCallback {
    messages: tokio::sync::Mutex<Vec<String>>,
    tool_calls: tokio::sync::Mutex<Vec<String>>,
    status_changes: tokio::sync::Mutex<Vec<AgentStatus>>,
}

impl RecordingCallback {
    pub fn new() -> Self {
        Self {
            messages: tokio::sync::Mutex::new(Vec::new()),
            tool_calls: tokio::sync::Mutex::new(Vec::new()),
            status_changes: tokio::sync::Mutex::new(Vec::new()),
        }
    }

    pub async fn messages(&self) -> Vec<String> {
        self.messages.lock().await.clone()
    }

    pub async fn tool_calls(&self) -> Vec<String> {
        self.tool_calls.lock().await.clone()
    }

    pub async fn status_changes(&self) -> Vec<AgentStatus> {
        self.status_changes.lock().await.clone()
    }
}

impl Default for RecordingCallback {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AgentCallback for RecordingCallback {
    async fn on_assistant_message(&self, message: &str) {
        self.messages.lock().await.push(message.to_string());
    }
    async fn on_token(&self, _token: &str) {}
    async fn request_approval(&self, _action: &ActionRequest) -> bool {
        true
    }
    async fn on_tool_start(&self, tool_name: &str, _args: &serde_json::Value) {
        self.tool_calls.lock().await.push(tool_name.to_string());
    }
    async fn on_tool_result(&self, _tool_name: &str, _output: &ToolOutput, _duration_ms: u64) {}
    async fn on_status_change(&self, status: AgentStatus) {
        self.status_changes.lock().await.push(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MockLlmProvider;

    fn create_test_agent(provider: Arc<MockLlmProvider>) -> (Agent, Arc<RecordingCallback>) {
        let callback = Arc::new(RecordingCallback::new());
        let config = AgentConfig::default();
        let agent = Agent::new(provider, config, callback.clone());
        (agent, callback)
    }

    #[tokio::test]
    async fn test_agent_simple_text_response() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("Hello! I can help you."));

        let (mut agent, callback) = create_test_agent(provider);
        let result = agent.process_task("Say hello").await.unwrap();

        assert!(result.success);
        assert_eq!(result.response, "Hello! I can help you.");
        assert_eq!(result.iterations, 1);

        let messages = callback.messages().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0], "Hello! I can help you.");
    }

    #[tokio::test]
    async fn test_agent_tool_call_then_response() {
        let provider = Arc::new(MockLlmProvider::new());

        // First response: tool call
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "test"}),
        ));
        // Second response after tool result: text
        provider.queue_response(MockLlmProvider::text_response(
            "I executed the echo tool successfully.",
        ));

        let (mut agent, callback) = create_test_agent(provider);

        // Register a simple echo tool
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo input text".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|args: serde_json::Value| {
                Box::pin(async move {
                    let text = args["text"].as_str().unwrap_or("no text");
                    Ok(ToolOutput::text(format!("Echo: {}", text)))
                })
            }),
        });

        let result = agent.process_task("Test echo tool").await.unwrap();

        assert!(result.success);
        assert_eq!(result.iterations, 2);

        let tool_calls = callback.tool_calls().await;
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0], "echo");
    }

    #[tokio::test]
    async fn test_agent_tool_not_found() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::tool_call_response(
            "nonexistent_tool",
            serde_json::json!({}),
        ));
        // After tool error, agent should respond with text
        provider.queue_response(MockLlmProvider::text_response("Sorry, that tool doesn't exist."));

        let (mut agent, _callback) = create_test_agent(provider);
        let result = agent.process_task("Use nonexistent tool").await.unwrap();

        // Agent should still complete (with the tool error in context)
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_agent_state_tracking() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("Done"));

        let (mut agent, callback) = create_test_agent(provider);

        assert_eq!(agent.state().status, AgentStatus::Idle);

        agent.process_task("Simple task").await.unwrap();

        assert_eq!(agent.state().status, AgentStatus::Complete);

        let statuses = callback.status_changes().await;
        assert!(statuses.contains(&AgentStatus::Thinking));
        assert!(statuses.contains(&AgentStatus::Complete));
    }

    #[tokio::test]
    async fn test_agent_max_iterations() {
        let provider = Arc::new(MockLlmProvider::new());
        // Queue many tool calls to exhaust iterations
        for _ in 0..30 {
            provider.queue_response(MockLlmProvider::tool_call_response(
                "echo",
                serde_json::json!({"text": "loop"}),
            ));
        }

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        let result = agent.process_task("Infinite loop test").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RustantError::Agent(AgentError::MaxIterationsReached { max }) => {
                assert_eq!(max, 25);
            }
            e => panic!("Expected MaxIterationsReached, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_agent_cancellation() {
        let provider = Arc::new(MockLlmProvider::new());
        // Queue a tool call response so the agent enters the loop
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "test"}),
        ));

        let (mut agent, _callback) = create_test_agent(provider);
        agent.register_tool(RegisteredTool {
            definition: ToolDefinition {
                name: "echo".to_string(),
                description: "Echo".to_string(),
                parameters: serde_json::json!({}),
            },
            risk_level: RiskLevel::ReadOnly,
            executor: Box::new(|_| Box::pin(async { Ok(ToolOutput::text("echoed")) })),
        });

        // Cancel before processing
        agent.cancel();
        let result = agent.process_task("Cancelled task").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            RustantError::Agent(AgentError::Cancelled) => {}
            e => panic!("Expected Cancelled, got: {:?}", e),
        }
    }

    #[test]
    fn test_no_op_callback() {
        // Just ensure it compiles and doesn't panic
        let _callback = NoOpCallback;
    }

    #[tokio::test]
    async fn test_recording_callback() {
        let callback = RecordingCallback::new();
        callback.on_assistant_message("hello").await;
        callback.on_tool_start("file_read", &serde_json::json!({})).await;
        callback.on_status_change(AgentStatus::Thinking).await;

        assert_eq!(callback.messages().await, vec!["hello"]);
        assert_eq!(callback.tool_calls().await, vec!["file_read"]);
        assert_eq!(callback.status_changes().await, vec![AgentStatus::Thinking]);
    }
}
