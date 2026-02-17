//! Integration tests for the Rustant agent.
//!
//! These tests exercise the full agent loop end-to-end using MockLlmProvider,
//! verifying the Think → Act → Observe cycle works correctly.

use rustant_core::Agent;
use rustant_core::agent::{RecordingCallback, RegisteredTool};
use rustant_core::brain::MockLlmProvider;
use rustant_core::config::AgentConfig;
use rustant_core::error::{AgentError, RustantError};
use rustant_core::memory::MemorySystem;
use rustant_core::types::{AgentStatus, RiskLevel, ToolDefinition, ToolOutput};
use std::path::Path;
use std::sync::Arc;

/// Helper to create a test agent with recording callback.
fn create_agent(provider: Arc<MockLlmProvider>) -> (Agent, Arc<RecordingCallback>) {
    let callback = Arc::new(RecordingCallback::new());
    let mut config = AgentConfig::default();
    // Use non-streaming for deterministic test behavior
    config.llm.use_streaming = false;
    let agent = Agent::new(provider, config, callback.clone());
    (agent, callback)
}

/// Helper to register a simple echo tool on an agent.
fn register_echo_tool(agent: &mut Agent) {
    agent.register_tool(RegisteredTool {
        definition: ToolDefinition {
            name: "echo".to_string(),
            description: "Echo input text back".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" }
                },
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
}

/// Helper to register a calculator tool.
fn register_calculator_tool(agent: &mut Agent) {
    agent.register_tool(RegisteredTool {
        definition: ToolDefinition {
            name: "calculator".to_string(),
            description: "Evaluate arithmetic expressions".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": { "type": "string" }
                },
                "required": ["expression"]
            }),
        },
        risk_level: RiskLevel::ReadOnly,
        executor: Box::new(|args: serde_json::Value| {
            Box::pin(async move {
                let expr = args["expression"].as_str().unwrap_or("0");
                // Simple eval for testing
                let result = match expr {
                    "2 + 2" => "4",
                    "10 * 5" => "50",
                    _ => "unknown",
                };
                Ok(ToolOutput::text(result.to_string()))
            })
        }),
    });
}

// --- Integration Tests ---

#[tokio::test]
async fn test_full_task_text_response() {
    let provider = Arc::new(MockLlmProvider::new());
    provider.queue_response(MockLlmProvider::text_response(
        "The answer to your question is 42.",
    ));

    let (mut agent, callback) = create_agent(provider);
    let result = agent
        .process_task("What is the meaning of life?")
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.response, "The answer to your question is 42.");
    assert_eq!(result.iterations, 1);

    let messages = callback.messages().await;
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0], "The answer to your question is 42.");

    let statuses = callback.status_changes().await;
    assert!(statuses.contains(&AgentStatus::Thinking));
    assert!(statuses.contains(&AgentStatus::Complete));
}

#[tokio::test]
async fn test_full_task_tool_then_text() {
    let provider = Arc::new(MockLlmProvider::new());

    // First: LLM requests a tool call
    provider.queue_response(MockLlmProvider::tool_call_response(
        "echo",
        serde_json::json!({"text": "hello world"}),
    ));
    // Second: After seeing the tool result, LLM responds with text
    provider.queue_response(MockLlmProvider::text_response(
        "The echo tool returned: hello world",
    ));

    let (mut agent, callback) = create_agent(provider);
    register_echo_tool(&mut agent);

    let result = agent.process_task("Echo hello world").await.unwrap();

    assert!(result.success);
    assert_eq!(result.iterations, 2);
    assert!(result.response.contains("hello world"));

    let tool_calls = callback.tool_calls().await;
    assert_eq!(tool_calls.len(), 1);
    assert_eq!(tool_calls[0], "echo");
}

#[tokio::test]
async fn test_full_task_multiple_tools() {
    let provider = Arc::new(MockLlmProvider::new());

    // First: echo tool
    provider.queue_response(MockLlmProvider::tool_call_response(
        "echo",
        serde_json::json!({"text": "step 1"}),
    ));
    // Second: calculator tool
    provider.queue_response(MockLlmProvider::tool_call_response(
        "calculator",
        serde_json::json!({"expression": "2 + 2"}),
    ));
    // Third: final text response
    provider.queue_response(MockLlmProvider::text_response(
        "I echoed 'step 1' and calculated 2+2=4.",
    ));

    let (mut agent, callback) = create_agent(provider);
    register_echo_tool(&mut agent);
    register_calculator_tool(&mut agent);

    let result = agent
        .process_task("Echo step 1, then calculate 2+2")
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.iterations, 3);

    let tool_calls = callback.tool_calls().await;
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0], "echo");
    assert_eq!(tool_calls[1], "calculator");
}

#[tokio::test]
async fn test_safety_denies_unknown_tool() {
    let provider = Arc::new(MockLlmProvider::new());

    // LLM requests a tool that doesn't exist
    provider.queue_response(MockLlmProvider::tool_call_response(
        "dangerous_tool",
        serde_json::json!({}),
    ));
    // After seeing the error, LLM responds with text
    provider.queue_response(MockLlmProvider::text_response(
        "Sorry, that tool is not available.",
    ));

    let (mut agent, _callback) = create_agent(provider);
    // Don't register any tools

    let result = agent.process_task("Use dangerous_tool").await.unwrap();

    // Agent should complete with the text response after tool error
    assert!(result.success);
    assert_eq!(result.iterations, 2);
}

#[tokio::test]
async fn test_context_compression_during_task() {
    let provider = Arc::new(MockLlmProvider::new());

    // Create enough tool call iterations to trigger compression.
    // Default window_size is 20, compression at 2x = 40 messages.
    // Each iteration adds ~2 messages (assistant tool_call + tool_result).
    // After 20 tool iterations: 1 (user) + 40 (20*2) = 41 >= 40, triggers compression.
    // The LLM-based ContextSummarizer consumes one provider response when
    // compression triggers, so we insert a text response at position 20.
    //
    // Queue order: 20 tool calls, 1 summarizer text, 2 tool calls, 1 final text
    // = 24 total responses, 23 agent iterations (22 tool + 1 text).
    for _ in 0..20 {
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "iteration"}),
        ));
    }
    // Response consumed by ContextSummarizer when compression triggers
    provider.queue_response(MockLlmProvider::text_response(
        "Summary of previous context.",
    ));
    for _ in 0..2 {
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "iteration"}),
        ));
    }
    provider.queue_response(MockLlmProvider::text_response(
        "Done after many iterations.",
    ));

    let (mut agent, _callback) = create_agent(provider);
    register_echo_tool(&mut agent);

    let result = agent
        .process_task("Run echo 22 times then finish")
        .await
        .unwrap();

    assert!(result.success);
    assert_eq!(result.iterations, 23);
}

#[tokio::test]
async fn test_session_save_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let session_path = dir.path().join("test_session.json");

    // Build memory with state
    let mut mem = MemorySystem::new(10);
    mem.start_new_task("integration test task");
    mem.add_message(rustant_core::Message::user("hello"));
    mem.add_message(rustant_core::Message::assistant("hi there"));
    mem.long_term.add_fact(rustant_core::memory::Fact::new(
        "Project uses Rust",
        "integration test",
    ));

    // Save
    mem.save_session(&session_path).unwrap();
    assert!(session_path.exists());

    // Load
    let loaded = MemorySystem::load_session(&session_path).unwrap();
    assert_eq!(
        loaded.working.current_goal.as_deref(),
        Some("integration test task")
    );
    assert_eq!(loaded.short_term.len(), 2);
    assert_eq!(loaded.long_term.facts.len(), 1);
    assert_eq!(loaded.long_term.facts[0].content, "Project uses Rust");
}

#[tokio::test]
async fn test_agent_with_streaming_config() {
    let provider = Arc::new(MockLlmProvider::new());
    provider.queue_response(MockLlmProvider::text_response("Streamed response."));

    let callback = Arc::new(RecordingCallback::new());
    let mut config = AgentConfig::default();
    config.llm.use_streaming = true;

    let mut agent = Agent::new(provider, config, callback.clone());

    let result = agent.process_task("Test streaming mode").await.unwrap();
    assert!(result.success);
    assert!(result.response.contains("Streamed"));
}

#[tokio::test]
async fn test_max_iterations_in_integration() {
    let provider = Arc::new(MockLlmProvider::new());

    // Queue more tool calls than max_iterations allows (default is 50)
    for _ in 0..55 {
        provider.queue_response(MockLlmProvider::tool_call_response(
            "echo",
            serde_json::json!({"text": "loop"}),
        ));
    }

    let (mut agent, _callback) = create_agent(provider);
    register_echo_tool(&mut agent);

    let result = agent.process_task("Infinite loop test").await;
    assert!(result.is_err());
    match result.unwrap_err() {
        RustantError::Agent(AgentError::MaxIterationsReached { max }) => {
            assert_eq!(max, 50); // default config
        }
        e => panic!("Expected MaxIterationsReached, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_load_nonexistent_session() {
    let result = MemorySystem::load_session(Path::new("/nonexistent/path.json"));
    assert!(result.is_err());
}
