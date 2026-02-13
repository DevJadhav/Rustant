//! Anthropic Messages API provider implementation.
//!
//! Implements the `LlmProvider` trait for the native Anthropic Messages API,
//! supporting Claude model families with both synchronous and streaming completions.
//!
//! Key differences from OpenAI-compatible APIs:
//! - Auth via `x-api-key` header (not `Authorization: Bearer`)
//! - Required `anthropic-version` header
//! - System message is a top-level `system` field, not in the messages array
//! - Tool calls use `tool_use` / `tool_result` content block conventions
//! - SSE streaming uses Anthropic-specific event types

use crate::brain::{LlmProvider, TokenCounter};
use crate::config::LlmConfig;
use crate::error::LlmError;
use crate::types::{
    CompletionRequest, CompletionResponse, Content, Message, Role, StreamEvent, TokenUsage,
    ToolDefinition,
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// The default Anthropic API base URL.
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com/v1";

/// The required Anthropic API version header value.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic Messages API provider.
///
/// Communicates with the Anthropic Messages API to perform completions using
/// Claude models. Supports both full and streaming responses, including tool use.
pub struct AnthropicProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    context_window: usize,
    cost_input: f64,
    cost_output: f64,
    token_counter: TokenCounter,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider from configuration.
    ///
    /// Reads the API key from the environment variable specified in `config.api_key_env`.
    /// Returns `LlmError::AuthFailed` if the environment variable is not set.
    pub fn new(config: &LlmConfig) -> Result<Self, LlmError> {
        let api_key = std::env::var(&config.api_key_env).map_err(|_| LlmError::AuthFailed {
            provider: format!("Anthropic (env var '{}' not set)", config.api_key_env),
        })?;
        Self::new_with_key(config, api_key)
    }

    /// Create a new Anthropic provider with an explicitly provided API key.
    ///
    /// Use this when the API key has been resolved externally (e.g., from a credential store).
    pub fn new_with_key(config: &LlmConfig, api_key: String) -> Result<Self, LlmError> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = Client::new();
        let token_counter = TokenCounter::for_model(&config.model);

        Ok(Self {
            client,
            base_url,
            api_key,
            model: config.model.clone(),
            context_window: config.context_window,
            cost_input: config.input_cost_per_million / 1_000_000.0,
            cost_output: config.output_cost_per_million / 1_000_000.0,
            token_counter,
        })
    }

    /// Build the JSON request body for the Anthropic Messages API.
    ///
    /// Extracts any system messages from the messages list and places them
    /// as the top-level `system` field. All other messages are converted to
    /// Anthropic's message format.
    fn build_request_body(&self, request: &CompletionRequest, stream: bool) -> Value {
        let model = request.model.as_deref().unwrap_or(&self.model);

        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Extract system message(s) from the messages list.
        let (system_text, non_system_messages) = Self::extract_system_message(&request.messages);

        // Convert messages to Anthropic format.
        let messages_json: Vec<Value> = non_system_messages
            .iter()
            .map(|msg| Self::message_to_anthropic_json(msg))
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": request.temperature,
            "messages": messages_json,
        });

        // Add system message as top-level field if present.
        if let Some(system) = &system_text {
            body["system"] = Value::String(system.clone());
        }

        // Add stop sequences if provided.
        if !request.stop_sequences.is_empty() {
            body["stop_sequences"] = serde_json::json!(request.stop_sequences);
        }

        // Add tools if provided.
        if let Some(tools) = &request.tools {
            let tools_json: Vec<Value> = tools.iter().map(Self::tool_definition_to_json).collect();
            body["tools"] = Value::Array(tools_json);
        }

        // Enable streaming if requested.
        if stream {
            body["stream"] = Value::Bool(true);
        }

        body
    }

    /// Extract system messages from the messages list.
    ///
    /// Returns a tuple of (optional concatenated system text, non-system messages).
    /// If multiple system messages exist, they are concatenated with newlines.
    fn extract_system_message(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
        let mut system_parts: Vec<&str> = Vec::new();
        let mut non_system: Vec<&Message> = Vec::new();

        for msg in messages {
            if msg.role == Role::System {
                if let Some(text) = msg.content.as_text() {
                    system_parts.push(text);
                }
            } else {
                non_system.push(msg);
            }
        }

        let system_text = if system_parts.is_empty() {
            None
        } else {
            Some(system_parts.join("\n\n"))
        };

        (system_text, non_system)
    }

    /// Convert a single `Message` to Anthropic JSON format.
    ///
    /// Maps our `Content` variants to Anthropic's content block format:
    /// - `Content::Text` -> `{"type": "text", "text": "..."}`
    /// - `Content::ToolCall` -> `{"type": "tool_use", "id": "...", "name": "...", "input": {...}}`
    /// - `Content::ToolResult` -> `{"type": "tool_result", "tool_use_id": "...", "content": "..."}`
    /// - `Content::MultiPart` -> array of the above blocks
    fn message_to_anthropic_json(msg: &Message) -> Value {
        let role = match msg.role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "assistant",
            Role::System => "user", // Should not reach here after extraction
        };

        let content = Self::content_to_anthropic_json(&msg.content);

        serde_json::json!({
            "role": role,
            "content": content,
        })
    }

    /// Convert a `Content` enum to Anthropic JSON content block(s).
    fn content_to_anthropic_json(content: &Content) -> Value {
        match content {
            Content::Text { text } => {
                serde_json::json!([{
                    "type": "text",
                    "text": text,
                }])
            }
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                serde_json::json!([{
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": arguments,
                }])
            }
            Content::ToolResult {
                call_id,
                output,
                is_error,
            } => {
                let mut block = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": call_id,
                    "content": output,
                });
                if *is_error {
                    block["is_error"] = Value::Bool(true);
                }
                serde_json::json!([block])
            }
            Content::MultiPart { parts } => {
                let blocks: Vec<Value> = parts
                    .iter()
                    .flat_map(|part| match Self::content_to_anthropic_json(part) {
                        Value::Array(arr) => arr,
                        other => vec![other],
                    })
                    .collect();
                Value::Array(blocks)
            }
        }
    }

    /// Convert a `ToolDefinition` to Anthropic tool JSON format.
    fn tool_definition_to_json(tool: &ToolDefinition) -> Value {
        serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.parameters,
        })
    }

    /// Parse an Anthropic API response JSON into a `CompletionResponse`.
    fn parse_response(body: &Value) -> Result<CompletionResponse, LlmError> {
        let model = body["model"].as_str().unwrap_or("unknown").to_string();

        let finish_reason = body["stop_reason"].as_str().map(|s| s.to_string());

        let usage = TokenUsage {
            input_tokens: body["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize,
            output_tokens: body["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize,
        };

        let content_blocks = body["content"]
            .as_array()
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'content' array in response".to_string(),
            })?;

        let content = Self::parse_content_blocks(content_blocks)?;

        let message = Message::new(Role::Assistant, content);

        Ok(CompletionResponse {
            message,
            usage,
            model,
            finish_reason,
        })
    }

    /// Parse an array of Anthropic content blocks into a `Content` value.
    ///
    /// If there is exactly one text block, returns `Content::Text`.
    /// If there is exactly one tool_use block, returns `Content::ToolCall`.
    /// If there are multiple blocks, returns `Content::MultiPart`.
    fn parse_content_blocks(blocks: &[Value]) -> Result<Content, LlmError> {
        let mut parts: Vec<Content> = Vec::new();

        for block in blocks {
            let block_type = block["type"].as_str().unwrap_or("text");
            match block_type {
                "text" => {
                    let text = block["text"].as_str().unwrap_or("").to_string();
                    parts.push(Content::Text { text });
                }
                "tool_use" => {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    let input = block["input"].clone();
                    parts.push(Content::ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                other => {
                    debug!(block_type = other, "Ignoring unknown content block type");
                }
            }
        }

        // Simplify: if there is exactly one part, return it directly.
        match parts.len() {
            0 => Ok(Content::text("")),
            1 => Ok(parts.into_iter().next().unwrap()),
            _ => Ok(Content::MultiPart { parts }),
        }
    }

    /// Map an HTTP status code to the appropriate `LlmError`.
    fn map_http_error(status: reqwest::StatusCode, body_text: &str) -> LlmError {
        match status.as_u16() {
            401 => LlmError::AuthFailed {
                provider: "Anthropic".to_string(),
            },
            429 => {
                // Try to parse retry-after from the response body.
                let retry_after = serde_json::from_str::<Value>(body_text)
                    .ok()
                    .and_then(|v| v["error"]["retry_after_secs"].as_u64())
                    .unwrap_or(30);
                LlmError::RateLimited {
                    retry_after_secs: retry_after,
                }
            }
            _ => LlmError::ApiRequest {
                message: format!("HTTP {} from Anthropic API: {}", status, body_text),
            },
        }
    }

    /// Parse a single SSE line to extract the event type and data.
    ///
    /// Returns `(event_type, data_json)` if both are present.
    fn parse_sse_line(event_type: &str, data: &str) -> Option<(String, Value)> {
        let json: Value = serde_json::from_str(data).ok()?;
        Some((event_type.to_string(), json))
    }

    /// Process a parsed SSE event and send the appropriate `StreamEvent` on the channel.
    ///
    /// Tracks the current content block index and type for correlating deltas
    /// with their respective tool calls.
    async fn process_sse_event(
        event_type: &str,
        data: &Value,
        tx: &mpsc::Sender<StreamEvent>,
        current_block_id: &mut Option<String>,
        current_block_type: &mut Option<String>,
    ) -> Result<Option<TokenUsage>, LlmError> {
        match event_type {
            "message_start" => {
                // Extract usage from message_start if available.
                debug!("Stream message_start received");
                Ok(None)
            }
            "content_block_start" => {
                let content_block = &data["content_block"];
                let block_type = content_block["type"].as_str().unwrap_or("").to_string();

                *current_block_type = Some(block_type.clone());

                if block_type == "tool_use" {
                    let id = content_block["id"].as_str().unwrap_or("").to_string();
                    let name = content_block["name"].as_str().unwrap_or("").to_string();

                    *current_block_id = Some(id.clone());

                    let _ = tx.send(StreamEvent::ToolCallStart { id, name }).await;
                }

                Ok(None)
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                let delta_type = delta["type"].as_str().unwrap_or("");

                match delta_type {
                    "text_delta" => {
                        let text = delta["text"].as_str().unwrap_or("").to_string();
                        if !text.is_empty() {
                            let _ = tx.send(StreamEvent::Token(text)).await;
                        }
                    }
                    "input_json_delta" => {
                        let partial_json = delta["partial_json"].as_str().unwrap_or("").to_string();
                        if let Some(id) = current_block_id.as_ref() {
                            let _ = tx
                                .send(StreamEvent::ToolCallDelta {
                                    id: id.clone(),
                                    arguments_delta: partial_json,
                                })
                                .await;
                        }
                    }
                    _ => {
                        debug!(delta_type, "Ignoring unknown delta type in stream");
                    }
                }

                Ok(None)
            }
            "content_block_stop" => {
                // If this was a tool_use block, emit ToolCallEnd.
                if current_block_type.as_deref() == Some("tool_use") {
                    if let Some(id) = current_block_id.take() {
                        let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
                    }
                }
                *current_block_type = None;

                Ok(None)
            }
            "message_delta" => {
                // The message_delta event contains final usage information.
                let usage = &data["usage"];
                let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as usize;

                // We return the partial usage; the caller accumulates it.
                Ok(Some(TokenUsage {
                    input_tokens: 0,
                    output_tokens,
                }))
            }
            "message_stop" => {
                debug!("Stream message_stop received");
                Ok(None)
            }
            "ping" => {
                // Keepalive, ignore.
                Ok(None)
            }
            "error" => {
                let error_msg = data["error"]["message"]
                    .as_str()
                    .unwrap_or("Unknown streaming error")
                    .to_string();
                let _ = tx.send(StreamEvent::Error(error_msg.clone())).await;
                Err(LlmError::Streaming { message: error_msg })
            }
            _ => {
                debug!(event_type, "Ignoring unknown SSE event type");
                Ok(None)
            }
        }
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    /// Perform a full (non-streaming) completion via the Anthropic Messages API.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let body = self.build_request_body(&request, false);
        let url = format!("{}/messages", self.base_url);

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            "Sending Anthropic completion request"
        );

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Request to Anthropic API failed: {}", e),
            })?;

        let status = response.status();
        let body_text = response.text().await.map_err(|e| LlmError::ResponseParse {
            message: format!("Failed to read response body: {}", e),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &body_text));
        }

        let response_json: Value =
            serde_json::from_str(&body_text).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON in response: {}", e),
            })?;

        Self::parse_response(&response_json)
    }

    /// Perform a streaming completion via the Anthropic Messages API.
    ///
    /// Sends SSE events to the provided channel as they arrive. The final
    /// `StreamEvent::Done` event includes aggregated token usage.
    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let body = self.build_request_body(&request, true);
        let url = format!("{}/messages", self.base_url);

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            "Sending Anthropic streaming request"
        );

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Streaming request to Anthropic API failed: {}", e),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body_text));
        }

        // Read the SSE stream line by line.
        let body_text = response.text().await.map_err(|e| LlmError::Streaming {
            message: format!("Failed to read streaming response: {}", e),
        })?;

        let mut current_block_id: Option<String> = None;
        let mut current_block_type: Option<String> = None;
        let mut total_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };

        // Parse SSE events from the response body.
        // SSE format: lines starting with "event:" and "data:", separated by blank lines.
        let mut current_event_type = String::new();

        for line in body_text.lines() {
            let line = line.trim();

            if line.is_empty() {
                continue;
            }

            if let Some(event_value) = line.strip_prefix("event: ") {
                current_event_type = event_value.trim().to_string();
            } else if let Some(data_value) = line.strip_prefix("data: ") {
                let data_str = data_value.trim();

                if let Some((event_type, data_json)) =
                    Self::parse_sse_line(&current_event_type, data_str)
                {
                    match Self::process_sse_event(
                        &event_type,
                        &data_json,
                        &tx,
                        &mut current_block_id,
                        &mut current_block_type,
                    )
                    .await
                    {
                        Ok(Some(partial_usage)) => {
                            total_usage.output_tokens += partial_usage.output_tokens;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            warn!(error = %e, "Error processing SSE event");
                            return Err(e);
                        }
                    }

                    // Extract input tokens from message_start event.
                    if event_type == "message_start" {
                        if let Some(input_tokens) =
                            data_json["message"]["usage"]["input_tokens"].as_u64()
                        {
                            total_usage.input_tokens = input_tokens as usize;
                        }
                    }
                }

                current_event_type.clear();
            }
        }

        // Send the Done event with accumulated usage.
        let _ = tx.send(StreamEvent::Done { usage: total_usage }).await;

        Ok(())
    }

    /// Estimate the token count for a set of messages using tiktoken.
    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        self.token_counter.count_messages(messages)
    }

    /// Return the context window size for this model.
    fn context_window(&self) -> usize {
        self.context_window
    }

    /// Anthropic Claude models support tool/function calling.
    fn supports_tools(&self) -> bool {
        true
    }

    /// Return the cost per token (input, output) in USD.
    fn cost_per_token(&self) -> (f64, f64) {
        (self.cost_input, self.cost_output)
    }

    /// Return the model name.
    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a test config with a given env var name.
    fn test_config(api_key_env: &str) -> LlmConfig {
        LlmConfig {
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-20250514".to_string(),
            api_key_env: api_key_env.to_string(),
            base_url: None,
            max_tokens: 4096,
            temperature: 0.7,
            context_window: 200_000,
            input_cost_per_million: 3.0,
            output_cost_per_million: 15.0,
            use_streaming: false,
            fallback_providers: Vec::new(),
            credential_store_key: None,
            auth_method: String::new(),
            api_key: None,
        }
    }

    /// Helper to create a provider with a fake API key in the environment.
    fn make_provider() -> AnthropicProvider {
        std::env::set_var("ANTHROPIC_TEST_KEY_UNIT", "sk-ant-test-key-12345");
        let config = test_config("ANTHROPIC_TEST_KEY_UNIT");
        AnthropicProvider::new(&config).expect("Provider creation should succeed")
    }

    #[test]
    fn test_new_reads_env() {
        let env_var = "ANTHROPIC_TEST_KEY_NEW_READS";
        std::env::set_var(env_var, "sk-ant-my-secret-key");
        let config = test_config(env_var);
        let provider = AnthropicProvider::new(&config).unwrap();
        assert_eq!(provider.api_key, "sk-ant-my-secret-key");
        assert_eq!(provider.model, "claude-sonnet-4-20250514");
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.context_window, 200_000);
        std::env::remove_var(env_var);
    }

    #[test]
    fn test_new_missing_env_returns_auth_failed() {
        std::env::remove_var("ANTHROPIC_MISSING_KEY_XYZ");
        let config = test_config("ANTHROPIC_MISSING_KEY_XYZ");
        let result = AnthropicProvider::new(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            LlmError::AuthFailed { provider } => {
                assert!(provider.contains("ANTHROPIC_MISSING_KEY_XYZ"));
            }
            other => panic!("Expected AuthFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_new_custom_base_url() {
        let env_var = "ANTHROPIC_TEST_KEY_CUSTOM_URL";
        std::env::set_var(env_var, "test-key");
        let mut config = test_config(env_var);
        config.base_url = Some("https://my-proxy.example.com/v1".to_string());
        let provider = AnthropicProvider::new(&config).unwrap();
        assert_eq!(provider.base_url, "https://my-proxy.example.com/v1");
        std::env::remove_var(env_var);
    }

    #[test]
    fn test_system_message_extraction() {
        let messages = vec![
            Message::system("You are a helpful coding assistant."),
            Message::user("Hello!"),
            Message::assistant("Hi there!"),
        ];

        let (system_text, non_system) = AnthropicProvider::extract_system_message(&messages);

        assert_eq!(
            system_text,
            Some("You are a helpful coding assistant.".to_string())
        );
        assert_eq!(non_system.len(), 2);
        assert_eq!(non_system[0].role, Role::User);
        assert_eq!(non_system[1].role, Role::Assistant);
    }

    #[test]
    fn test_system_message_extraction_multiple() {
        let messages = vec![
            Message::system("First system instruction."),
            Message::system("Second system instruction."),
            Message::user("Hello!"),
        ];

        let (system_text, non_system) = AnthropicProvider::extract_system_message(&messages);

        assert_eq!(
            system_text,
            Some("First system instruction.\n\nSecond system instruction.".to_string())
        );
        assert_eq!(non_system.len(), 1);
    }

    #[test]
    fn test_system_message_extraction_none() {
        let messages = vec![Message::user("Hello!"), Message::assistant("Hi!")];

        let (system_text, non_system) = AnthropicProvider::extract_system_message(&messages);

        assert!(system_text.is_none());
        assert_eq!(non_system.len(), 2);
    }

    #[test]
    fn test_message_to_anthropic_json() {
        let user_msg = Message::user("What is Rust?");
        let json = AnthropicProvider::message_to_anthropic_json(&user_msg);

        assert_eq!(json["role"], "user");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "What is Rust?");
    }

    #[test]
    fn test_message_to_anthropic_json_assistant() {
        let msg = Message::assistant("Rust is a systems programming language.");
        let json = AnthropicProvider::message_to_anthropic_json(&msg);

        assert_eq!(json["role"], "assistant");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(
            content[0]["text"],
            "Rust is a systems programming language."
        );
    }

    #[test]
    fn test_message_to_anthropic_json_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            Content::tool_call(
                "toolu_01abc",
                "file_read",
                serde_json::json!({"path": "/src/main.rs"}),
            ),
        );
        let json = AnthropicProvider::message_to_anthropic_json(&msg);

        assert_eq!(json["role"], "assistant");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_use");
        assert_eq!(content[0]["id"], "toolu_01abc");
        assert_eq!(content[0]["name"], "file_read");
        assert_eq!(content[0]["input"]["path"], "/src/main.rs");
    }

    #[test]
    fn test_message_to_anthropic_json_tool_result() {
        let msg = Message::tool_result("toolu_01abc", "fn main() { }", false);
        let json = AnthropicProvider::message_to_anthropic_json(&msg);

        // Tool results are sent as user role in Anthropic API.
        assert_eq!(json["role"], "user");
        let content = json["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "toolu_01abc");
        assert_eq!(content[0]["content"], "fn main() { }");
        // is_error should not be present when false.
        assert!(content[0].get("is_error").is_none());
    }

    #[test]
    fn test_message_to_anthropic_json_tool_result_error() {
        let msg = Message::new(
            Role::Tool,
            Content::tool_result("toolu_02xyz", "File not found", true),
        );
        let json = AnthropicProvider::message_to_anthropic_json(&msg);

        let content = json["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["is_error"], true);
    }

    #[test]
    fn test_parse_text_response() {
        let response_json = serde_json::json!({
            "id": "msg_01XFDUDYJgAACzvnptvVoYEL",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {
                    "type": "text",
                    "text": "Hello! How can I help you today?"
                }
            ],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 25,
                "output_tokens": 10
            }
        });

        let result = AnthropicProvider::parse_response(&response_json).unwrap();
        assert_eq!(
            result.message.content.as_text(),
            Some("Hello! How can I help you today?")
        );
        assert_eq!(result.model, "claude-sonnet-4-20250514");
        assert_eq!(result.usage.input_tokens, 25);
        assert_eq!(result.usage.output_tokens, 10);
        assert_eq!(result.finish_reason, Some("end_turn".to_string()));
        assert_eq!(result.message.role, Role::Assistant);
    }

    #[test]
    fn test_parse_tool_use_response() {
        let response_json = serde_json::json!({
            "id": "msg_02abc",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {
                    "type": "text",
                    "text": "I'll read that file for you."
                },
                {
                    "type": "tool_use",
                    "id": "toolu_01abc",
                    "name": "file_read",
                    "input": {
                        "path": "/src/main.rs"
                    }
                }
            ],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 50,
                "output_tokens": 30
            }
        });

        let result = AnthropicProvider::parse_response(&response_json).unwrap();
        assert_eq!(result.finish_reason, Some("tool_use".to_string()));

        // Should be a MultiPart content with text + tool_call.
        match &result.message.content {
            Content::MultiPart { parts } => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    Content::Text { text } => {
                        assert_eq!(text, "I'll read that file for you.");
                    }
                    _ => panic!("Expected Text part"),
                }
                match &parts[1] {
                    Content::ToolCall {
                        id,
                        name,
                        arguments,
                    } => {
                        assert_eq!(id, "toolu_01abc");
                        assert_eq!(name, "file_read");
                        assert_eq!(arguments["path"], "/src/main.rs");
                    }
                    _ => panic!("Expected ToolCall part"),
                }
            }
            _ => panic!(
                "Expected MultiPart content, got {:?}",
                result.message.content
            ),
        }
    }

    #[test]
    fn test_parse_single_tool_use_response() {
        let response_json = serde_json::json!({
            "id": "msg_03xyz",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_02xyz",
                    "name": "shell_exec",
                    "input": {
                        "command": "cargo test"
                    }
                }
            ],
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 40,
                "output_tokens": 20
            }
        });

        let result = AnthropicProvider::parse_response(&response_json).unwrap();
        match &result.message.content {
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "toolu_02xyz");
                assert_eq!(name, "shell_exec");
                assert_eq!(arguments["command"], "cargo test");
            }
            _ => panic!("Expected ToolCall content"),
        }
    }

    #[test]
    fn test_http_error_mapping() {
        // 401 -> AuthFailed
        let err = AnthropicProvider::map_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"Invalid API key"}}"#,
        );
        match err {
            LlmError::AuthFailed { provider } => {
                assert_eq!(provider, "Anthropic");
            }
            _ => panic!("Expected AuthFailed, got {:?}", err),
        }

        // 429 -> RateLimited
        let err = AnthropicProvider::map_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Rate limited","retry_after_secs":60}}"#,
        );
        match err {
            LlmError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 60);
            }
            _ => panic!("Expected RateLimited, got {:?}", err),
        }

        // 429 without retry_after_secs defaults to 30.
        let err = AnthropicProvider::map_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Rate limited"}}"#,
        );
        match err {
            LlmError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 30);
            }
            _ => panic!("Expected RateLimited, got {:?}", err),
        }

        // 500 -> ApiRequest
        let err = AnthropicProvider::map_http_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":{"message":"Internal server error"}}"#,
        );
        match err {
            LlmError::ApiRequest { message } => {
                assert!(message.contains("500"));
                assert!(message.contains("Internal server error"));
            }
            _ => panic!("Expected ApiRequest, got {:?}", err),
        }
    }

    #[test]
    fn test_parse_sse_events() {
        // Test message_start event parsing.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "message_start",
            r#"{"type":"message_start","message":{"id":"msg_01","model":"claude-sonnet-4-20250514","usage":{"input_tokens":25,"output_tokens":0}}}"#,
        ).unwrap();
        assert_eq!(event_type, "message_start");
        assert_eq!(data["message"]["usage"]["input_tokens"], 25);

        // Test content_block_start with text type.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "content_block_start",
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
        )
        .unwrap();
        assert_eq!(event_type, "content_block_start");
        assert_eq!(data["content_block"]["type"], "text");

        // Test content_block_start with tool_use type.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01abc","name":"file_read"}}"#,
        ).unwrap();
        assert_eq!(event_type, "content_block_start");
        assert_eq!(data["content_block"]["type"], "tool_use");
        assert_eq!(data["content_block"]["id"], "toolu_01abc");
        assert_eq!(data["content_block"]["name"], "file_read");

        // Test content_block_delta with text_delta.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        ).unwrap();
        assert_eq!(event_type, "content_block_delta");
        assert_eq!(data["delta"]["type"], "text_delta");
        assert_eq!(data["delta"]["text"], "Hello");

        // Test content_block_delta with input_json_delta.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#,
        ).unwrap();
        assert_eq!(event_type, "content_block_delta");
        assert_eq!(data["delta"]["type"], "input_json_delta");

        // Test message_delta with usage.
        let (event_type, data) = AnthropicProvider::parse_sse_line(
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#,
        ).unwrap();
        assert_eq!(event_type, "message_delta");
        assert_eq!(data["usage"]["output_tokens"], 42);

        // Test message_stop.
        let (event_type, _data) =
            AnthropicProvider::parse_sse_line("message_stop", r#"{"type":"message_stop"}"#)
                .unwrap();
        assert_eq!(event_type, "message_stop");

        // Test invalid JSON returns None.
        let result = AnthropicProvider::parse_sse_line("content_block_delta", "not valid json");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_process_sse_text_delta() {
        let (tx, mut rx) = mpsc::channel(32);
        let mut block_id: Option<String> = None;
        let mut block_type: Option<String> = Some("text".to_string());

        let data = serde_json::json!({
            "type": "content_block_delta",
            "delta": {
                "type": "text_delta",
                "text": "Hello world"
            }
        });

        let result = AnthropicProvider::process_sse_event(
            "content_block_delta",
            &data,
            &tx,
            &mut block_id,
            &mut block_type,
        )
        .await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::Token(text) => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Token event"),
        }
    }

    #[tokio::test]
    async fn test_process_sse_tool_use_flow() {
        let (tx, mut rx) = mpsc::channel(32);
        let mut block_id: Option<String> = None;
        let mut block_type: Option<String> = None;

        // 1. content_block_start with tool_use.
        let data = serde_json::json!({
            "type": "content_block_start",
            "content_block": {
                "type": "tool_use",
                "id": "toolu_01test",
                "name": "file_read"
            }
        });
        AnthropicProvider::process_sse_event(
            "content_block_start",
            &data,
            &tx,
            &mut block_id,
            &mut block_type,
        )
        .await
        .unwrap();

        assert_eq!(block_id, Some("toolu_01test".to_string()));
        assert_eq!(block_type, Some("tool_use".to_string()));

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "toolu_01test");
                assert_eq!(name, "file_read");
            }
            _ => panic!("Expected ToolCallStart event"),
        }

        // 2. content_block_delta with input_json_delta.
        let data = serde_json::json!({
            "type": "content_block_delta",
            "delta": {
                "type": "input_json_delta",
                "partial_json": "{\"path\": \"/src\"}"
            }
        });
        AnthropicProvider::process_sse_event(
            "content_block_delta",
            &data,
            &tx,
            &mut block_id,
            &mut block_type,
        )
        .await
        .unwrap();

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "toolu_01test");
                assert_eq!(arguments_delta, "{\"path\": \"/src\"}");
            }
            _ => panic!("Expected ToolCallDelta event"),
        }

        // 3. content_block_stop.
        let data = serde_json::json!({"type": "content_block_stop"});
        AnthropicProvider::process_sse_event(
            "content_block_stop",
            &data,
            &tx,
            &mut block_id,
            &mut block_type,
        )
        .await
        .unwrap();

        assert!(block_id.is_none());
        assert!(block_type.is_none());

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ToolCallEnd { id } => {
                assert_eq!(id, "toolu_01test");
            }
            _ => panic!("Expected ToolCallEnd event"),
        }
    }

    #[tokio::test]
    async fn test_process_sse_message_delta_usage() {
        let (tx, _rx) = mpsc::channel(32);
        let mut block_id: Option<String> = None;
        let mut block_type: Option<String> = None;

        let data = serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn"},
            "usage": {"output_tokens": 42}
        });

        let result = AnthropicProvider::process_sse_event(
            "message_delta",
            &data,
            &tx,
            &mut block_id,
            &mut block_type,
        )
        .await
        .unwrap();

        assert!(result.is_some());
        let usage = result.unwrap();
        assert_eq!(usage.output_tokens, 42);
    }

    #[test]
    fn test_build_request_body_basic() {
        let provider = make_provider();

        let request = CompletionRequest {
            messages: vec![
                Message::system("You are a helpful assistant."),
                Message::user("What is 2+2?"),
            ],
            tools: None,
            temperature: 0.5,
            max_tokens: Some(1024),
            stop_sequences: vec![],
            model: None,
        };

        let body = provider.build_request_body(&request, false);

        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["temperature"], 0.5);
        assert_eq!(body["system"], "You are a helpful assistant.");
        assert!(body.get("stream").is_none());

        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1); // System message extracted, only user remains.
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_stream() {
        let provider = make_provider();

        let request = CompletionRequest {
            messages: vec![Message::user("Hello")],
            tools: None,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: vec![],
            model: None,
        };

        let body = provider.build_request_body(&request, true);
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 4096); // Default when not specified.
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = make_provider();

        let tools = vec![ToolDefinition {
            name: "file_read".to_string(),
            description: "Read a file from disk".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        }];

        let request = CompletionRequest {
            messages: vec![Message::user("Read main.rs")],
            tools: Some(tools),
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: vec![],
            model: None,
        };

        let body = provider.build_request_body(&request, false);
        let tools_json = body["tools"].as_array().unwrap();
        assert_eq!(tools_json.len(), 1);
        assert_eq!(tools_json[0]["name"], "file_read");
        assert_eq!(tools_json[0]["description"], "Read a file from disk");
        assert!(tools_json[0].get("input_schema").is_some());
    }

    #[test]
    fn test_build_request_body_with_stop_sequences() {
        let provider = make_provider();

        let request = CompletionRequest {
            messages: vec![Message::user("Hello")],
            tools: None,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: vec!["STOP".to_string(), "END".to_string()],
            model: None,
        };

        let body = provider.build_request_body(&request, false);
        let stop = body["stop_sequences"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], "STOP");
        assert_eq!(stop[1], "END");
    }

    #[test]
    fn test_build_request_body_model_override() {
        let provider = make_provider();

        let request = CompletionRequest {
            messages: vec![Message::user("Hello")],
            tools: None,
            temperature: 0.7,
            max_tokens: None,
            stop_sequences: vec![],
            model: Some("claude-3-5-haiku-20241022".to_string()),
        };

        let body = provider.build_request_body(&request, false);
        assert_eq!(body["model"], "claude-3-5-haiku-20241022");
    }

    #[test]
    fn test_provider_properties() {
        let provider = make_provider();

        assert_eq!(provider.model_name(), "claude-sonnet-4-20250514");
        assert_eq!(provider.context_window(), 200_000);
        assert!(provider.supports_tools());

        let (input_cost, output_cost) = provider.cost_per_token();
        // $3 / 1M = 0.000003 per token
        assert!((input_cost - 3.0 / 1_000_000.0).abs() < 1e-12);
        // $15 / 1M = 0.000015 per token
        assert!((output_cost - 15.0 / 1_000_000.0).abs() < 1e-12);
    }

    #[test]
    fn test_estimate_tokens() {
        let provider = make_provider();
        let messages = vec![
            Message::system("You are a coding assistant."),
            Message::user("What is Rust?"),
        ];
        let tokens = provider.estimate_tokens(&messages);
        assert!(tokens > 0);
        assert!(tokens < 200);
    }

    #[test]
    fn test_tool_definition_to_json() {
        let tool = ToolDefinition {
            name: "shell_exec".to_string(),
            description: "Execute a shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {"type": "string"}
                },
                "required": ["command"]
            }),
        };

        let json = AnthropicProvider::tool_definition_to_json(&tool);
        assert_eq!(json["name"], "shell_exec");
        assert_eq!(json["description"], "Execute a shell command");
        assert_eq!(json["input_schema"]["type"], "object");
        assert!(json["input_schema"]["properties"]["command"].is_object());
    }

    #[test]
    fn test_parse_empty_content_response() {
        let response_json = serde_json::json!({
            "id": "msg_empty",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "content": [],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 0
            }
        });

        let result = AnthropicProvider::parse_response(&response_json).unwrap();
        assert_eq!(result.message.content.as_text(), Some(""));
    }

    #[test]
    fn test_parse_response_missing_content() {
        let response_json = serde_json::json!({
            "id": "msg_bad",
            "type": "message",
            "role": "assistant",
            "model": "claude-sonnet-4-20250514",
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 0
            }
        });

        let result = AnthropicProvider::parse_response(&response_json);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ResponseParse { message } => {
                assert!(message.contains("content"));
            }
            other => panic!("Expected ResponseParse, got {:?}", other),
        }
    }

    #[test]
    fn test_content_to_anthropic_json_multipart() {
        let content = Content::MultiPart {
            parts: vec![
                Content::text("Here is the file:"),
                Content::tool_call(
                    "toolu_01multi",
                    "file_read",
                    serde_json::json!({"path": "/tmp/test.rs"}),
                ),
            ],
        };

        let json = AnthropicProvider::content_to_anthropic_json(&content);
        let blocks = json.as_array().unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[0]["text"], "Here is the file:");
        assert_eq!(blocks[1]["type"], "tool_use");
        assert_eq!(blocks[1]["id"], "toolu_01multi");
    }
}
