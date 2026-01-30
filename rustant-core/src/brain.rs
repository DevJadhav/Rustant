//! Brain module — LLM provider abstraction and interaction.
//!
//! Defines the `LlmProvider` trait for model-agnostic LLM interactions,
//! and provides an OpenAI-compatible implementation with streaming support.

use crate::error::LlmError;
use crate::types::{
    CompletionRequest, CompletionResponse, Content, CostEstimate, Message, Role, StreamEvent,
    TokenUsage, ToolDefinition,
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Trait for LLM providers, supporting both full and streaming completions.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Perform a full completion and return the response.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError>;

    /// Perform a streaming completion, sending events to the channel.
    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError>;

    /// Estimate the token count for a set of messages.
    fn estimate_tokens(&self, messages: &[Message]) -> usize;

    /// Return the context window size for this provider/model.
    fn context_window(&self) -> usize;

    /// Return whether this provider supports tool/function calling.
    fn supports_tools(&self) -> bool;

    /// Return the cost per token (input, output) in USD.
    fn cost_per_token(&self) -> (f64, f64);

    /// Return the model name.
    fn model_name(&self) -> &str;
}

/// The Brain wraps an LLM provider and adds higher-level logic:
/// prompt construction, cost tracking, and model selection.
pub struct Brain {
    provider: Arc<dyn LlmProvider>,
    system_prompt: String,
    total_usage: TokenUsage,
    total_cost: CostEstimate,
}

impl Brain {
    pub fn new(provider: Arc<dyn LlmProvider>, system_prompt: impl Into<String>) -> Self {
        Self {
            provider,
            system_prompt: system_prompt.into(),
            total_usage: TokenUsage::default(),
            total_cost: CostEstimate::default(),
        }
    }

    /// Construct messages for the LLM with system prompt prepended.
    pub fn build_messages(
        &self,
        conversation: &[Message],
    ) -> Vec<Message> {
        let mut messages = Vec::with_capacity(conversation.len() + 1);
        messages.push(Message::system(&self.system_prompt));
        messages.extend_from_slice(conversation);
        messages
    }

    /// Send a completion request and return the response, tracking usage.
    pub async fn think(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResponse, LlmError> {
        let messages = self.build_messages(conversation);
        let token_estimate = self.provider.estimate_tokens(&messages);
        let context_limit = self.provider.context_window();

        if token_estimate > context_limit {
            return Err(LlmError::ContextOverflow {
                used: token_estimate,
                limit: context_limit,
            });
        }

        debug!(
            model = self.provider.model_name(),
            estimated_tokens = token_estimate,
            "Sending completion request"
        );

        let request = CompletionRequest {
            messages,
            tools,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: Vec::new(),
            model: None,
        };

        let response = self.provider.complete(request).await?;

        // Track usage
        self.total_usage.accumulate(&response.usage);
        let (input_rate, output_rate) = self.provider.cost_per_token();
        let cost = CostEstimate {
            input_cost: response.usage.input_tokens as f64 * input_rate,
            output_cost: response.usage.output_tokens as f64 * output_rate,
        };
        self.total_cost.accumulate(&cost);

        info!(
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            cost = format!("${:.4}", cost.total()),
            "Completion received"
        );

        Ok(response)
    }

    /// Send a streaming completion request, returning events via channel.
    pub async fn think_streaming(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let messages = self.build_messages(conversation);

        let request = CompletionRequest {
            messages,
            tools,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: Vec::new(),
            model: None,
        };

        self.provider.complete_streaming(request, tx).await
    }

    /// Get total token usage across all calls.
    pub fn total_usage(&self) -> &TokenUsage {
        &self.total_usage
    }

    /// Get total cost across all calls.
    pub fn total_cost(&self) -> &CostEstimate {
        &self.total_cost
    }

    /// Get the model name.
    pub fn model_name(&self) -> &str {
        self.provider.model_name()
    }

    /// Get the context window size.
    pub fn context_window(&self) -> usize {
        self.provider.context_window()
    }

    /// Get the current token usage as a fraction of the context window.
    pub fn context_usage_ratio(&self, conversation: &[Message]) -> f32 {
        let messages = self.build_messages(conversation);
        let tokens = self.provider.estimate_tokens(&messages);
        tokens as f32 / self.provider.context_window() as f32
    }
}

/// A mock LLM provider for testing and development.
pub struct MockLlmProvider {
    model: String,
    context_window: usize,
    responses: std::sync::Mutex<Vec<CompletionResponse>>,
}

impl MockLlmProvider {
    pub fn new() -> Self {
        Self {
            model: "mock-model".to_string(),
            context_window: 128_000,
            responses: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Queue a response to be returned by the next `complete` call.
    pub fn queue_response(&self, response: CompletionResponse) {
        self.responses.lock().unwrap().push(response);
    }

    /// Create a simple text response for testing.
    pub fn text_response(text: &str) -> CompletionResponse {
        CompletionResponse {
            message: Message::assistant(text),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
            },
            model: "mock-model".to_string(),
            finish_reason: Some("stop".to_string()),
        }
    }

    /// Create a tool call response for testing.
    pub fn tool_call_response(
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> CompletionResponse {
        let call_id = format!("call_{}", uuid::Uuid::new_v4());
        CompletionResponse {
            message: Message::new(
                Role::Assistant,
                Content::tool_call(&call_id, tool_name, arguments),
            ),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 30,
            },
            model: "mock-model".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        }
    }
}

impl Default for MockLlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmProvider for MockLlmProvider {
    async fn complete(&self, _request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut responses = self.responses.lock().unwrap();
        if responses.is_empty() {
            Ok(MockLlmProvider::text_response(
                "I'm a mock LLM. No queued responses available.",
            ))
        } else {
            Ok(responses.remove(0))
        }
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let response = self.complete(request).await?;
        if let Some(text) = response.message.content.as_text() {
            for word in text.split_whitespace() {
                let _ = tx.send(StreamEvent::Token(format!("{} ", word))).await;
            }
        }
        let _ = tx.send(StreamEvent::Done { usage: response.usage }).await;
        Ok(())
    }

    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        // Rough estimate: ~4 chars per token
        messages
            .iter()
            .map(|m| match &m.content {
                Content::Text { text } => text.len() / 4,
                Content::ToolCall { arguments, .. } => arguments.to_string().len() / 4,
                Content::ToolResult { output, .. } => output.len() / 4,
                Content::MultiPart { parts } => parts
                    .iter()
                    .map(|p| match p {
                        Content::Text { text } => text.len() / 4,
                        _ => 50,
                    })
                    .sum(),
            })
            .sum::<usize>()
            + 100 // overhead for message structure
    }

    fn context_window(&self) -> usize {
        self.context_window
    }

    fn supports_tools(&self) -> bool {
        true
    }

    fn cost_per_token(&self) -> (f64, f64) {
        (0.0, 0.0) // free for mock
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

/// The system prompt used by default for the Rustant agent.
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Rustant, a high-performance autonomous coding assistant built in Rust. You help developers with software engineering tasks by reading, writing, and modifying code, searching files, and executing commands.

Key behaviors:
- Always read a file before modifying it
- Explain your reasoning before taking actions
- Use the most specific tool available for each task
- Respect file boundaries and permissions
- Ask for clarification when the task is ambiguous
- Prefer small, focused changes over large rewrites

You have access to tools for file operations, search, and shell execution. Use them judiciously."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_provider_default_response() {
        let provider = MockLlmProvider::new();
        let request = CompletionRequest::default();
        let response = provider.complete(request).await.unwrap();
        assert!(response.message.content.as_text().is_some());
    }

    #[tokio::test]
    async fn test_mock_provider_queued_responses() {
        let provider = MockLlmProvider::new();
        provider.queue_response(MockLlmProvider::text_response("first"));
        provider.queue_response(MockLlmProvider::text_response("second"));

        let r1 = provider.complete(CompletionRequest::default()).await.unwrap();
        assert_eq!(r1.message.content.as_text(), Some("first"));

        let r2 = provider.complete(CompletionRequest::default()).await.unwrap();
        assert_eq!(r2.message.content.as_text(), Some("second"));
    }

    #[tokio::test]
    async fn test_mock_provider_streaming() {
        let provider = MockLlmProvider::new();
        provider.queue_response(MockLlmProvider::text_response("hello world"));

        let (tx, mut rx) = mpsc::channel(32);
        provider
            .complete_streaming(CompletionRequest::default(), tx)
            .await
            .unwrap();

        let mut tokens = Vec::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(t) => tokens.push(t),
                StreamEvent::Done { .. } => break,
                _ => {}
            }
        }
        assert_eq!(tokens.len(), 2); // "hello " and "world "
    }

    #[test]
    fn test_mock_provider_token_estimation() {
        let provider = MockLlmProvider::new();
        let messages = vec![Message::user("Hello, this is a test message.")];
        let tokens = provider.estimate_tokens(&messages);
        assert!(tokens > 0);
    }

    #[test]
    fn test_mock_provider_properties() {
        let provider = MockLlmProvider::new();
        assert_eq!(provider.context_window(), 128_000);
        assert!(provider.supports_tools());
        assert_eq!(provider.cost_per_token(), (0.0, 0.0));
        assert_eq!(provider.model_name(), "mock-model");
    }

    #[tokio::test]
    async fn test_brain_think() {
        let provider = Arc::new(MockLlmProvider::new());
        provider.queue_response(MockLlmProvider::text_response("I can help with that."));

        let mut brain = Brain::new(provider, "You are a helpful assistant.");
        let conversation = vec![Message::user("Help me refactor")];

        let response = brain.think(&conversation, None).await.unwrap();
        assert_eq!(
            response.message.content.as_text(),
            Some("I can help with that.")
        );
        assert!(brain.total_usage().total() > 0);
    }

    #[tokio::test]
    async fn test_brain_builds_messages_with_system_prompt() {
        let provider = Arc::new(MockLlmProvider::new());
        let brain = Brain::new(provider, "system prompt");
        let conversation = vec![Message::user("hello")];

        let messages = brain.build_messages(&conversation);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[0].content.as_text(), Some("system prompt"));
        assert_eq!(messages[1].role, Role::User);
    }

    #[test]
    fn test_brain_context_usage_ratio() {
        let provider = Arc::new(MockLlmProvider::new());
        let brain = Brain::new(provider, "system");
        let conversation = vec![Message::user("short message")];

        let ratio = brain.context_usage_ratio(&conversation);
        assert!(ratio > 0.0);
        assert!(ratio < 1.0);
    }

    #[test]
    fn test_mock_tool_call_response() {
        let response = MockLlmProvider::tool_call_response(
            "file_read",
            serde_json::json!({"path": "/tmp/test.rs"}),
        );
        match &response.message.content {
            Content::ToolCall { name, arguments, .. } => {
                assert_eq!(name, "file_read");
                assert_eq!(arguments["path"], "/tmp/test.rs");
            }
            _ => panic!("Expected ToolCall content"),
        }
    }

    #[test]
    fn test_default_system_prompt() {
        assert!(DEFAULT_SYSTEM_PROMPT.contains("Rustant"));
        assert!(DEFAULT_SYSTEM_PROMPT.contains("autonomous"));
    }
}
