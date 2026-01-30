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

/// Token counter using tiktoken-rs for accurate BPE tokenization.
pub struct TokenCounter {
    bpe: tiktoken_rs::CoreBPE,
}

impl TokenCounter {
    /// Create a token counter for the given model.
    /// Falls back to cl100k_base if the model isn't recognized.
    pub fn for_model(model: &str) -> Self {
        let bpe = tiktoken_rs::get_bpe_from_model(model).unwrap_or_else(|_| {
            tiktoken_rs::cl100k_base().expect("cl100k_base should be available")
        });
        Self { bpe }
    }

    /// Count the number of tokens in a string.
    pub fn count(&self, text: &str) -> usize {
        self.bpe.encode_with_special_tokens(text).len()
    }

    /// Estimate the token count for a set of messages.
    /// Adds overhead for message structure (role, separators).
    pub fn count_messages(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for msg in messages {
            // Each message has overhead: role token + separators (~4 tokens)
            total += 4;
            match &msg.content {
                Content::Text { text } => total += self.count(text),
                Content::ToolCall {
                    name, arguments, ..
                } => {
                    total += self.count(name);
                    total += self.count(&arguments.to_string());
                }
                Content::ToolResult { output, .. } => {
                    total += self.count(output);
                }
                Content::MultiPart { parts } => {
                    for part in parts {
                        match part {
                            Content::Text { text } => total += self.count(text),
                            Content::ToolCall {
                                name, arguments, ..
                            } => {
                                total += self.count(name);
                                total += self.count(&arguments.to_string());
                            }
                            Content::ToolResult { output, .. } => {
                                total += self.count(output);
                            }
                            _ => total += 10,
                        }
                    }
                }
            }
        }
        total + 3 // reply priming overhead
    }
}

/// The Brain wraps an LLM provider and adds higher-level logic:
/// prompt construction, cost tracking, and model selection.
pub struct Brain {
    provider: Arc<dyn LlmProvider>,
    system_prompt: String,
    total_usage: TokenUsage,
    total_cost: CostEstimate,
    token_counter: TokenCounter,
}

impl Brain {
    pub fn new(provider: Arc<dyn LlmProvider>, system_prompt: impl Into<String>) -> Self {
        let model_name = provider.model_name().to_string();
        Self {
            provider,
            system_prompt: system_prompt.into(),
            total_usage: TokenUsage::default(),
            total_cost: CostEstimate::default(),
            token_counter: TokenCounter::for_model(&model_name),
        }
    }

    /// Estimate token count for messages using tiktoken-rs.
    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        self.token_counter.count_messages(messages)
    }

    /// Construct messages for the LLM with system prompt prepended.
    pub fn build_messages(&self, conversation: &[Message]) -> Vec<Message> {
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

    /// Send a completion request with retry logic and exponential backoff.
    ///
    /// Retries on transient errors (RateLimited, Timeout, Connection) up to
    /// `max_retries` times with exponential backoff (1s, 2s, 4s, ..., capped at 32s).
    /// Non-transient errors are returned immediately.
    pub async fn think_with_retry(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
        max_retries: usize,
    ) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match self.think(conversation, tools.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) if Self::is_retryable(&e) => {
                    if attempt < max_retries {
                        let backoff_secs = std::cmp::min(1u64 << attempt, 32);
                        let wait = match &e {
                            LlmError::RateLimited { retry_after_secs } => {
                                std::cmp::max(*retry_after_secs, backoff_secs)
                            }
                            _ => backoff_secs,
                        };
                        info!(
                            attempt = attempt + 1,
                            max_retries,
                            backoff_secs = wait,
                            error = %e,
                            "Retrying after transient error"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                        last_error = Some(e);
                    } else {
                        return Err(e);
                    }
                }
                Err(e) => return Err(e),
            }
        }

        Err(last_error.unwrap_or(LlmError::Connection {
            message: "Max retries exceeded".to_string(),
        }))
    }

    /// Check if an LLM error is transient and should be retried.
    fn is_retryable(error: &LlmError) -> bool {
        matches!(
            error,
            LlmError::RateLimited { .. } | LlmError::Timeout { .. } | LlmError::Connection { .. }
        )
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

    /// Get a reference to the underlying LLM provider.
    pub fn provider(&self) -> &dyn LlmProvider {
        &*self.provider
    }

    /// Track usage and cost from an external completion (e.g., streaming).
    pub fn track_usage(&mut self, usage: &TokenUsage) {
        self.total_usage.accumulate(usage);
        let (input_rate, output_rate) = self.provider.cost_per_token();
        let cost = CostEstimate {
            input_cost: usage.input_tokens as f64 * input_rate,
            output_cost: usage.output_tokens as f64 * output_rate,
        };
        self.total_cost.accumulate(&cost);
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
    pub fn tool_call_response(tool_name: &str, arguments: serde_json::Value) -> CompletionResponse {
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
        let _ = tx
            .send(StreamEvent::Done {
                usage: response.usage,
            })
            .await;
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

        let r1 = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(r1.message.content.as_text(), Some("first"));

        let r2 = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
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
            Content::ToolCall {
                name, arguments, ..
            } => {
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

    #[test]
    fn test_is_retryable() {
        assert!(Brain::is_retryable(&LlmError::RateLimited {
            retry_after_secs: 5
        }));
        assert!(Brain::is_retryable(&LlmError::Timeout { timeout_secs: 30 }));
        assert!(Brain::is_retryable(&LlmError::Connection {
            message: "reset".into()
        }));
        assert!(!Brain::is_retryable(&LlmError::ContextOverflow {
            used: 200_000,
            limit: 128_000
        }));
        assert!(!Brain::is_retryable(&LlmError::AuthFailed {
            provider: "openai".into()
        }));
    }

    /// A mock provider that fails N times before succeeding.
    struct FailingProvider {
        failures_remaining: std::sync::Mutex<usize>,
        error_type: String,
        success_response: CompletionResponse,
    }

    impl FailingProvider {
        fn new(failures: usize, error_type: &str) -> Self {
            Self {
                failures_remaining: std::sync::Mutex::new(failures),
                error_type: error_type.to_string(),
                success_response: MockLlmProvider::text_response("Success after retry"),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for FailingProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            let mut remaining = self.failures_remaining.lock().unwrap();
            if *remaining > 0 {
                *remaining -= 1;
                match self.error_type.as_str() {
                    "rate_limited" => Err(LlmError::RateLimited {
                        retry_after_secs: 0,
                    }),
                    "timeout" => Err(LlmError::Timeout { timeout_secs: 5 }),
                    "connection" => Err(LlmError::Connection {
                        message: "connection reset".into(),
                    }),
                    _ => Err(LlmError::ApiRequest {
                        message: "non-retryable".into(),
                    }),
                }
            } else {
                Ok(self.success_response.clone())
            }
        }

        async fn complete_streaming(
            &self,
            _request: CompletionRequest,
            _tx: mpsc::Sender<StreamEvent>,
        ) -> Result<(), LlmError> {
            Ok(())
        }

        fn estimate_tokens(&self, _messages: &[Message]) -> usize {
            100
        }
        fn context_window(&self) -> usize {
            128_000
        }
        fn supports_tools(&self) -> bool {
            true
        }
        fn cost_per_token(&self) -> (f64, f64) {
            (0.0, 0.0)
        }
        fn model_name(&self) -> &str {
            "failing-mock"
        }
    }

    #[tokio::test]
    async fn test_think_with_retry_succeeds_after_failures() {
        let provider = Arc::new(FailingProvider::new(2, "connection"));
        let mut brain = Brain::new(provider, "system");
        let conversation = vec![Message::user("test")];

        let result = brain.think_with_retry(&conversation, None, 3).await;
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().message.content.as_text(),
            Some("Success after retry")
        );
    }

    #[tokio::test]
    async fn test_think_with_retry_exhausted() {
        let provider = Arc::new(FailingProvider::new(5, "timeout"));
        let mut brain = Brain::new(provider, "system");
        let conversation = vec![Message::user("test")];

        let result = brain.think_with_retry(&conversation, None, 2).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::Timeout { .. }));
    }

    #[tokio::test]
    async fn test_think_with_retry_non_retryable_fails_immediately() {
        let provider = Arc::new(FailingProvider::new(1, "non_retryable"));
        let mut brain = Brain::new(provider, "system");
        let conversation = vec![Message::user("test")];

        let result = brain.think_with_retry(&conversation, None, 3).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), LlmError::ApiRequest { .. }));
    }

    #[tokio::test]
    async fn test_think_with_retry_rate_limited() {
        let provider = Arc::new(FailingProvider::new(1, "rate_limited"));
        let mut brain = Brain::new(provider, "system");
        let conversation = vec![Message::user("test")];

        let result = brain.think_with_retry(&conversation, None, 2).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_track_usage() {
        let provider = Arc::new(MockLlmProvider::new());
        let mut brain = Brain::new(provider, "system");

        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        brain.track_usage(&usage);

        assert_eq!(brain.total_usage().input_tokens, 100);
        assert_eq!(brain.total_usage().output_tokens, 50);
    }

    #[test]
    fn test_token_counter_basic() {
        let counter = TokenCounter::for_model("gpt-4o");
        let count = counter.count("Hello, world!");
        assert!(count > 0);
        assert!(count < 20); // should be ~4 tokens
    }

    #[test]
    fn test_token_counter_messages() {
        let counter = TokenCounter::for_model("gpt-4o");
        let messages = vec![
            Message::system("You are a helpful assistant."),
            Message::user("What is 2 + 2?"),
        ];
        let count = counter.count_messages(&messages);
        assert!(count > 5);
        assert!(count < 100);
    }

    #[test]
    fn test_token_counter_unknown_model_falls_back() {
        let counter = TokenCounter::for_model("unknown-model-xyz");
        let count = counter.count("Hello");
        assert!(count > 0); // Should use cl100k_base fallback
    }

    #[test]
    fn test_brain_estimate_tokens() {
        let provider = Arc::new(MockLlmProvider::new());
        let brain = Brain::new(provider, "system");
        let messages = vec![Message::user("Hello, this is a test.")];
        let estimate = brain.estimate_tokens(&messages);
        assert!(estimate > 0);
    }
}
