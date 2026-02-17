//! OpenAI-compatible LLM provider.
//!
//! Supports OpenAI, Azure OpenAI, Ollama, vLLM, LM Studio, and any
//! endpoint that follows the OpenAI chat completions API format.

use crate::brain::{LlmProvider, TokenCounter};
use crate::config::LlmConfig;
use crate::error::LlmError;
use crate::types::{
    CompletionRequest, CompletionResponse, Content, Message, Role, StreamEvent, TokenUsage,
    ToolDefinition,
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};
use std::collections::HashSet;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// Metadata for a known model.
struct ModelMeta {
    context_window: usize,
    input_cost_per_million: f64,
    output_cost_per_million: f64,
    supports_tools: bool,
}

/// Look up known model metadata. Returns None for unknown models.
fn known_model_meta(model: &str) -> Option<ModelMeta> {
    match model {
        // OpenAI models
        "gpt-4o" | "gpt-4o-2024-11-20" | "gpt-4o-2024-08-06" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 2.50,
            output_cost_per_million: 10.0,
            supports_tools: true,
        }),
        "gpt-4o-mini" | "gpt-4o-mini-2024-07-18" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 0.15,
            output_cost_per_million: 0.60,
            supports_tools: true,
        }),
        "gpt-4-turbo" | "gpt-4-turbo-2024-04-09" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 10.0,
            output_cost_per_million: 30.0,
            supports_tools: true,
        }),
        "gpt-3.5-turbo" | "gpt-3.5-turbo-0125" => Some(ModelMeta {
            context_window: 16_385,
            input_cost_per_million: 0.50,
            output_cost_per_million: 1.50,
            supports_tools: true,
        }),
        // Ollama / local models (zero cost)
        "qwen2.5:14b" | "qwen2.5:32b" | "qwen2.5:7b" | "qwen2.5:72b" => Some(ModelMeta {
            context_window: 32_768,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            supports_tools: true,
        }),
        "llama3.1:8b" | "llama3.1:70b" | "llama3.1:405b" | "llama3.2:3b" | "llama3.2:1b" => {
            Some(ModelMeta {
                context_window: 128_000,
                input_cost_per_million: 0.0,
                output_cost_per_million: 0.0,
                supports_tools: true,
            })
        }
        "mistral-nemo:12b" | "mistral:7b" | "mixtral:8x7b" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            supports_tools: true,
        }),
        "deepseek-coder-v2:16b" | "deepseek-coder:6.7b" | "deepseek-coder:33b" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            supports_tools: true,
        }),
        "phi-3:14b" | "phi-3:3.8b" => Some(ModelMeta {
            context_window: 128_000,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            supports_tools: true,
        }),
        "codellama:70b" | "codellama:34b" | "codellama:13b" | "codellama:7b" => Some(ModelMeta {
            context_window: 16_384,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            supports_tools: false,
        }),
        _ => None,
    }
}

/// OpenAI-compatible LLM provider.
pub struct OpenAiCompatibleProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    context_window: usize,
    cost_input: f64,
    cost_output: f64,
    supports_tools: bool,
    token_counter: TokenCounter,
}

impl OpenAiCompatibleProvider {
    /// Create a new provider from configuration.
    ///
    /// Reads the API key from the environment variable specified in `config.api_key_env`.
    pub fn new(config: &LlmConfig) -> Result<Self, LlmError> {
        let is_local = config
            .base_url
            .as_ref()
            .map(|u| u.contains("localhost") || u.contains("127.0.0.1"))
            .unwrap_or(false);

        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var(&config.api_key_env).ok())
            .or_else(|| {
                if is_local {
                    // Local providers (Ollama, vLLM, LM Studio) don't require an API key
                    debug!("No API key set for local provider; using dummy bearer token");
                    Some("ollama".to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| LlmError::AuthFailed {
                provider: format!(
                    "OpenAI-compatible: env var '{}' not set",
                    config.api_key_env
                ),
            })?;
        Self::new_with_key(config, api_key)
    }

    /// Create a new provider with an explicitly provided API key.
    ///
    /// Use this when the API key has been resolved externally (e.g., from a credential store).
    pub fn new_with_key(config: &LlmConfig, api_key: String) -> Result<Self, LlmError> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        // Use known model metadata if available, otherwise use config values
        let meta = known_model_meta(&config.model);
        let context_window = meta
            .as_ref()
            .map(|m| m.context_window)
            .unwrap_or(config.context_window);
        // Use centralized model pricing first, then local meta, then config
        let (cost_input, cost_output) = crate::providers::models::model_pricing(&config.model)
            .map(|(i, o)| (i / 1_000_000.0, o / 1_000_000.0))
            .unwrap_or_else(|| {
                meta.as_ref()
                    .map(|m| {
                        (
                            m.input_cost_per_million / 1_000_000.0,
                            m.output_cost_per_million / 1_000_000.0,
                        )
                    })
                    .unwrap_or((
                        config.input_cost_per_million / 1_000_000.0,
                        config.output_cost_per_million / 1_000_000.0,
                    ))
            });
        let supports_tools = meta.as_ref().map(|m| m.supports_tools).unwrap_or(true);

        Ok(Self {
            client: Client::new(),
            base_url,
            api_key,
            model: config.model.clone(),
            context_window,
            cost_input,
            cost_output,
            supports_tools,
            token_counter: TokenCounter::for_model(&config.model),
        })
    }

    /// Convert internal messages to OpenAI JSON format.
    fn messages_to_json(messages: &[Message]) -> Vec<Value> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::System => "system",
                    Role::Tool => "tool",
                };
                match &msg.content {
                    Content::Text { text } => json!({
                        "role": role,
                        "content": text,
                    }),
                    Content::ToolCall {
                        id,
                        name,
                        arguments,
                    } => json!({
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": arguments.to_string(),
                            }
                        }]
                    }),
                    Content::ToolResult {
                        call_id, output, ..
                    } => json!({
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": output,
                    }),
                    Content::MultiPart { parts } => {
                        // Collect text parts and tool calls
                        let mut text_parts = Vec::new();
                        let mut tool_calls = Vec::new();
                        for part in parts {
                            match part {
                                Content::Text { text } => text_parts.push(text.clone()),
                                Content::ToolCall {
                                    id,
                                    name,
                                    arguments,
                                } => {
                                    tool_calls.push(json!({
                                        "id": id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": arguments.to_string(),
                                        }
                                    }));
                                }
                                _ => {}
                            }
                        }
                        if !tool_calls.is_empty() {
                            json!({
                                "role": "assistant",
                                "content": if text_parts.is_empty() { Value::Null } else { Value::String(text_parts.join("\n")) },
                                "tool_calls": tool_calls,
                            })
                        } else {
                            json!({
                                "role": role,
                                "content": text_parts.join("\n"),
                            })
                        }
                    }
                }
            })
            .collect()
    }

    /// Convert tool definitions to OpenAI format.
    fn tools_to_json(tools: &[ToolDefinition]) -> Vec<Value> {
        tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
                })
            })
            .collect()
    }

    /// Parse an OpenAI-format response body into a CompletionResponse.
    fn parse_response(body: &Value, model: &str) -> Result<CompletionResponse, LlmError> {
        let choice =
            body.get("choices")
                .and_then(|c| c.get(0))
                .ok_or_else(|| LlmError::ResponseParse {
                    message: "No choices in response".to_string(),
                })?;

        let message = choice
            .get("message")
            .ok_or_else(|| LlmError::ResponseParse {
                message: "No message in choice".to_string(),
            })?;

        let finish_reason = choice
            .get("finish_reason")
            .and_then(|f| f.as_str())
            .map(|s| s.to_string());

        // Parse content: either text content or tool calls
        let content = if let Some(tool_calls) = message.get("tool_calls") {
            let calls: Vec<Content> = tool_calls
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .filter_map(|tc| {
                    let id = tc.get("id")?.as_str()?.to_string();
                    let func = tc.get("function")?;
                    let name = func.get("name")?.as_str()?.to_string();
                    let args_str = func.get("arguments")?.as_str()?;
                    let arguments: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    Some(Content::ToolCall {
                        id,
                        name,
                        arguments,
                    })
                })
                .collect();

            if calls.len() == 1 {
                calls.into_iter().next().unwrap()
            } else if calls.is_empty() {
                Content::text(
                    message
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or(""),
                )
            } else {
                // Multiple tool calls: wrap in MultiPart
                // Optionally include text if present
                let mut parts = Vec::new();
                if let Some(text) = message.get("content").and_then(|c| c.as_str())
                    && !text.is_empty()
                {
                    parts.push(Content::text(text));
                }
                parts.extend(calls);
                Content::MultiPart { parts }
            }
        } else {
            Content::text(
                message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or(""),
            )
        };

        // Parse usage
        let usage_obj = body.get("usage");
        let usage = TokenUsage {
            input_tokens: usage_obj
                .and_then(|u| u.get("prompt_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize,
            output_tokens: usage_obj
                .and_then(|u| u.get("completion_tokens"))
                .and_then(|t| t.as_u64())
                .unwrap_or(0) as usize,
        };

        let resp_model = body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or(model)
            .to_string();

        Ok(CompletionResponse {
            message: Message::new(Role::Assistant, content),
            usage,
            model: resp_model,
            finish_reason,
        })
    }

    /// Parse a single SSE data line. Returns the parsed JSON if valid.
    fn parse_sse_line(line: &str) -> Option<Value> {
        let data = line.strip_prefix("data: ")?;
        if data == "[DONE]" {
            return None;
        }
        serde_json::from_str(data).ok()
    }

    /// Fix OpenAI-format message turn ordering issues.
    ///
    /// OpenAI requires that every "tool" role message has a matching tool_call in a
    /// preceding "assistant" message, and that no system/user messages appear between
    /// an assistant's tool_calls and their corresponding tool results.
    ///
    /// This method:
    /// 1. Collects all tool_call IDs from assistant messages
    /// 2. Removes orphaned "tool" messages (no matching tool_call)
    /// 3. Relocates system/user messages that appear between tool_call and tool_result
    fn fix_openai_turns(messages: Vec<Value>) -> Vec<Value> {
        if messages.is_empty() {
            return messages;
        }

        // --- Pass 1: Collect all tool_call IDs ---
        let mut tool_call_ids: HashSet<String> = HashSet::new();
        for msg in &messages {
            if msg["role"].as_str() == Some("assistant")
                && let Some(calls) = msg["tool_calls"].as_array()
            {
                for call in calls {
                    if let Some(id) = call["id"].as_str() {
                        tool_call_ids.insert(id.to_string());
                    }
                }
            }
        }

        // --- Pass 2: Remove orphaned tool messages ---
        let mut result: Vec<Value> = Vec::with_capacity(messages.len());
        for msg in messages {
            if msg["role"].as_str() == Some("tool") {
                if let Some(call_id) = msg["tool_call_id"].as_str() {
                    if tool_call_ids.contains(call_id) {
                        result.push(msg);
                    } else {
                        warn!(
                            call_id = call_id,
                            "Removing orphaned tool message (no matching tool_call)"
                        );
                    }
                } else {
                    warn!("Removing tool message without tool_call_id");
                }
            } else {
                result.push(msg);
            }
        }

        // --- Pass 3: Relocate non-tool messages between assistant[tool_calls] and tool results ---
        let mut i = 0;
        while i + 1 < result.len() {
            let has_tool_calls = result[i]["role"].as_str() == Some("assistant")
                && result[i]["tool_calls"].is_array()
                && !result[i]["tool_calls"]
                    .as_array()
                    .map(|a| a.is_empty())
                    .unwrap_or(true);

            if has_tool_calls {
                // Move any non-tool messages that appear between this assistant and
                // its tool results to before the assistant message
                let mut j = i + 1;
                let mut to_relocate = Vec::new();
                while j < result.len() {
                    let role = result[j]["role"].as_str().unwrap_or("");
                    if role == "tool" {
                        j += 1;
                        continue;
                    }
                    // Check if there are still tool results after this non-tool message
                    let has_tool_after =
                        (j + 1..result.len()).any(|k| result[k]["role"].as_str() == Some("tool"));
                    if has_tool_after && (role == "system" || role == "user") {
                        to_relocate.push(j);
                        j += 1;
                    } else {
                        break;
                    }
                }

                // Move them before the assistant message
                if !to_relocate.is_empty() {
                    let mut extracted: Vec<Value> = Vec::new();
                    for &idx in to_relocate.iter().rev() {
                        extracted.push(result.remove(idx));
                    }
                    extracted.reverse();
                    for (offset, msg) in extracted.into_iter().enumerate() {
                        result.insert(i + offset, msg);
                        i += 1;
                    }
                }
            }
            i += 1;
        }

        result
    }

    /// Map an HTTP status code to the appropriate LlmError.
    fn map_http_error(status: reqwest::StatusCode, body: &str) -> LlmError {
        match status.as_u16() {
            401 => {
                debug!(body = %body, "Authentication failed (401)");
                LlmError::AuthFailed {
                    provider: "OpenAI-compatible".to_string(),
                }
            }
            429 => {
                // Try to parse retry-after from response
                let retry_secs = serde_json::from_str::<Value>(body)
                    .ok()
                    .and_then(|v| {
                        v.get("error")?
                            .get("message")?
                            .as_str()
                            .map(|s| s.to_string())
                    })
                    .and_then(|msg| {
                        // Try to extract number from "Rate limit... try again in Xs"
                        msg.split("in ")
                            .last()
                            .and_then(|s| s.trim_end_matches('s').parse::<u64>().ok())
                    })
                    .unwrap_or(5);
                LlmError::RateLimited {
                    retry_after_secs: retry_secs,
                }
            }
            status if status >= 500 => LlmError::ApiRequest {
                message: format!("Server error ({}): {}", status, body),
            },
            _ => LlmError::ApiRequest {
                message: format!("HTTP {}: {}", status, body),
            },
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatibleProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let url = format!("{}/chat/completions", self.base_url);

        let messages_json = Self::fix_openai_turns(Self::messages_to_json(&request.messages));
        let mut body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "messages": messages_json,
            "temperature": request.temperature,
            "stream": false,
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if !request.stop_sequences.is_empty() {
            body["stop"] = json!(request.stop_sequences);
        }
        if let Some(tools) = &request.tools
            && !tools.is_empty()
        {
            body["tools"] = json!(Self::tools_to_json(tools));
        }

        debug!(url = %url, model = %self.model, "Sending OpenAI completion request");

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Request failed: {}", e),
            })?;

        let status = response.status();
        let response_body = response.text().await.map_err(|e| LlmError::ApiRequest {
            message: format!("Failed to read response body: {}", e),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &response_body));
        }

        let json: Value =
            serde_json::from_str(&response_body).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON: {}", e),
            })?;

        Self::parse_response(&json, &self.model)
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let url = format!("{}/chat/completions", self.base_url);

        let messages_json = Self::fix_openai_turns(Self::messages_to_json(&request.messages));
        let mut body = json!({
            "model": request.model.as_deref().unwrap_or(&self.model),
            "messages": messages_json,
            "temperature": request.temperature,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if let Some(max_tokens) = request.max_tokens {
            body["max_tokens"] = json!(max_tokens);
        }
        if !request.stop_sequences.is_empty() {
            body["stop"] = json!(request.stop_sequences);
        }
        if let Some(tools) = &request.tools
            && !tools.is_empty()
        {
            body["tools"] = json!(Self::tools_to_json(tools));
        }

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::Streaming {
                message: format!("Request failed: {}", e),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body_text));
        }

        let mut usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
        };
        // Track active tool calls for streaming
        let mut active_tool_calls: std::collections::HashMap<usize, (String, String)> =
            std::collections::HashMap::new();

        // Read the response body as text chunks (SSE)
        let full_body = response.text().await.map_err(|e| LlmError::Streaming {
            message: format!("Failed to read stream: {}", e),
        })?;

        for line in full_body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if line == "data: [DONE]" {
                break;
            }
            if let Some(data) = Self::parse_sse_line(line) {
                // Check for usage in the final chunk
                if let Some(u) = data.get("usage") {
                    usage.input_tokens =
                        u.get("prompt_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as usize;
                    usage.output_tokens = u
                        .get("completion_tokens")
                        .and_then(|t| t.as_u64())
                        .unwrap_or(0) as usize;
                }

                if let Some(choice) = data.get("choices").and_then(|c| c.get(0)) {
                    let empty_obj = json!({});
                    let delta = choice.get("delta").unwrap_or(&empty_obj);

                    // Content token
                    if let Some(content) = delta.get("content").and_then(|c| c.as_str())
                        && !content.is_empty()
                    {
                        let _ = tx.send(StreamEvent::Token(content.to_string())).await;
                    }

                    // Tool calls
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                        for tc in tool_calls {
                            let index =
                                tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;

                            if let Some(func) = tc.get("function") {
                                // New tool call start
                                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                    let id = tc
                                        .get("id")
                                        .and_then(|i| i.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    active_tool_calls.insert(index, (id.clone(), name.to_string()));
                                    let _ = tx
                                        .send(StreamEvent::ToolCallStart {
                                            id,
                                            name: name.to_string(),
                                            raw_function_call: None,
                                        })
                                        .await;
                                }
                                // Arguments delta
                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str())
                                    && !args.is_empty()
                                    && let Some((id, _)) = active_tool_calls.get(&index)
                                {
                                    let _ = tx
                                        .send(StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_delta: args.to_string(),
                                        })
                                        .await;
                                }
                            }
                        }
                    }

                    // Finish reason
                    if let Some(finish) = choice.get("finish_reason").and_then(|f| f.as_str())
                        && finish == "tool_calls"
                    {
                        // Send ToolCallEnd for all active calls
                        for (_, (id, _)) in active_tool_calls.drain() {
                            let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
                        }
                    }
                }
            }
        }

        let _ = tx.send(StreamEvent::Done { usage }).await;
        Ok(())
    }

    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        self.token_counter.count_messages(messages)
    }

    fn context_window(&self) -> usize {
        self.context_window
    }

    fn supports_tools(&self) -> bool {
        self.supports_tools
    }

    fn cost_per_token(&self) -> (f64, f64) {
        (self.cost_input, self.cost_output)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LlmConfig {
        LlmConfig {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            api_key_env: "RUSTANT_TEST_OPENAI_KEY".to_string(),
            base_url: None,
            max_tokens: 4096,
            temperature: 0.7,
            context_window: 128_000,
            input_cost_per_million: 2.5,
            output_cost_per_million: 10.0,
            use_streaming: false,
            fallback_providers: Vec::new(),
            credential_store_key: None,
            auth_method: String::new(),
            api_key: None,
            retry: crate::config::RetryConfig::default(),
        }
    }

    #[test]
    fn test_messages_to_json_text() {
        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there"),
        ];
        let json = OpenAiCompatibleProvider::messages_to_json(&messages);
        assert_eq!(json.len(), 3);
        assert_eq!(json[0]["role"], "system");
        assert_eq!(json[0]["content"], "You are helpful");
        assert_eq!(json[1]["role"], "user");
        assert_eq!(json[2]["role"], "assistant");
    }

    #[test]
    fn test_messages_to_json_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            Content::tool_call("call_123", "file_read", json!({"path": "/tmp/test"})),
        );
        let json = OpenAiCompatibleProvider::messages_to_json(&[msg]);
        assert_eq!(json[0]["role"], "assistant");
        assert!(json[0]["tool_calls"].is_array());
        assert_eq!(json[0]["tool_calls"][0]["id"], "call_123");
        assert_eq!(json[0]["tool_calls"][0]["function"]["name"], "file_read");
    }

    #[test]
    fn test_messages_to_json_tool_result() {
        let msg = Message::new(
            Role::Tool,
            Content::ToolResult {
                call_id: "call_123".to_string(),
                output: "file contents here".to_string(),
                is_error: false,
            },
        );
        let json = OpenAiCompatibleProvider::messages_to_json(&[msg]);
        assert_eq!(json[0]["role"], "tool");
        assert_eq!(json[0]["tool_call_id"], "call_123");
        assert_eq!(json[0]["content"], "file contents here");
    }

    #[test]
    fn test_tools_to_json() {
        let tools = vec![ToolDefinition {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        }];
        let json = OpenAiCompatibleProvider::tools_to_json(&tools);
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["type"], "function");
        assert_eq!(json[0]["function"]["name"], "file_read");
    }

    #[test]
    fn test_parse_text_response() {
        let body = json!({
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 8,
                "total_tokens": 18
            },
            "model": "gpt-4o"
        });
        let resp = OpenAiCompatibleProvider::parse_response(&body, "gpt-4o").unwrap();
        assert_eq!(
            resp.message.content.as_text().unwrap(),
            "Hello! How can I help?"
        );
        assert_eq!(resp.usage.input_tokens, 10);
        assert_eq!(resp.usage.output_tokens, 8);
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
        assert_eq!(resp.model, "gpt-4o");
    }

    #[test]
    fn test_parse_tool_call_response() {
        let body = json!({
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_abc",
                        "type": "function",
                        "function": {
                            "name": "file_read",
                            "arguments": "{\"path\":\"/tmp/test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 15
            },
            "model": "gpt-4o"
        });
        let resp = OpenAiCompatibleProvider::parse_response(&body, "gpt-4o").unwrap();
        match &resp.message.content {
            Content::ToolCall {
                id,
                name,
                arguments,
            } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "file_read");
                assert_eq!(arguments["path"], "/tmp/test.txt");
            }
            other => panic!("Expected ToolCall, got {:?}", other),
        }
        assert_eq!(resp.finish_reason.as_deref(), Some("tool_calls"));
    }

    #[test]
    fn test_parse_response_no_choices() {
        let body = json!({"choices": []});
        let result = OpenAiCompatibleProvider::parse_response(&body, "gpt-4o");
        assert!(result.is_err());
    }

    #[test]
    fn test_http_error_mapping_401() {
        let err = OpenAiCompatibleProvider::map_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            "Unauthorized",
        );
        match err {
            LlmError::AuthFailed { .. } => {}
            other => panic!("Expected AuthFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_http_error_mapping_429() {
        let err = OpenAiCompatibleProvider::map_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Rate limit exceeded"}}"#,
        );
        match err {
            LlmError::RateLimited { .. } => {}
            other => panic!("Expected RateLimited, got {:?}", other),
        }
    }

    #[test]
    fn test_http_error_mapping_500() {
        let err = OpenAiCompatibleProvider::map_http_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "Internal server error",
        );
        match err {
            LlmError::ApiRequest { message } => {
                assert!(message.contains("500"));
            }
            other => panic!("Expected ApiRequest, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_sse_line_valid() {
        let line = r#"data: {"id":"chatcmpl-123","choices":[{"delta":{"content":"Hello"}}]}"#;
        let parsed = OpenAiCompatibleProvider::parse_sse_line(line);
        assert!(parsed.is_some());
        let val = parsed.unwrap();
        assert_eq!(val["id"], "chatcmpl-123");
    }

    #[test]
    fn test_parse_sse_line_done() {
        let line = "data: [DONE]";
        assert!(OpenAiCompatibleProvider::parse_sse_line(line).is_none());
    }

    #[test]
    fn test_parse_sse_line_not_data() {
        let line = "event: message";
        assert!(OpenAiCompatibleProvider::parse_sse_line(line).is_none());
    }

    #[test]
    fn test_new_reads_env() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_OPENAI_KEY", "sk-test-key") };
        let config = test_config();
        let provider = OpenAiCompatibleProvider::new(&config).unwrap();
        assert_eq!(provider.model_name(), "gpt-4o");
        assert_eq!(provider.context_window(), 128_000);
        assert!(provider.supports_tools());
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OPENAI_KEY") };
    }

    #[test]
    fn test_new_missing_key() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OPENAI_KEY_MISSING") };
        let mut config = test_config();
        config.api_key_env = "RUSTANT_TEST_OPENAI_KEY_MISSING".to_string();
        let result = OpenAiCompatibleProvider::new(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_known_model_metadata() {
        let meta = known_model_meta("gpt-4o").unwrap();
        assert_eq!(meta.context_window, 128_000);
        assert!(meta.supports_tools);
        assert!(meta.input_cost_per_million > 0.0);

        let meta = known_model_meta("gpt-4o-mini").unwrap();
        assert_eq!(meta.context_window, 128_000);
        assert!(meta.input_cost_per_million < 1.0);

        assert!(known_model_meta("unknown-model").is_none());
    }

    #[test]
    fn test_custom_base_url() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("RUSTANT_TEST_OPENAI_KEY", "test-key") };
        let mut config = test_config();
        config.base_url = Some("http://localhost:11434/v1".to_string());
        let provider = OpenAiCompatibleProvider::new(&config).unwrap();
        assert_eq!(provider.base_url, "http://localhost:11434/v1");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OPENAI_KEY") };
    }

    #[test]
    fn test_ollama_provider_no_api_key_required() {
        // Ollama on localhost should not require an API key env var
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OLLAMA_KEY_NONEXISTENT") };
        let mut config = test_config();
        config.api_key_env = "RUSTANT_TEST_OLLAMA_KEY_NONEXISTENT".to_string();
        config.base_url = Some("http://localhost:11434/v1".to_string());
        config.model = "qwen2.5:14b".to_string();
        let result = OpenAiCompatibleProvider::new(&config);
        assert!(
            result.is_ok(),
            "Ollama localhost should not require API key"
        );
        let provider = result.unwrap();
        assert_eq!(provider.model_name(), "qwen2.5:14b");
    }

    #[test]
    fn test_ollama_127_0_0_1_no_api_key_required() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OLLAMA_KEY_NONEXISTENT2") };
        let mut config = test_config();
        config.api_key_env = "RUSTANT_TEST_OLLAMA_KEY_NONEXISTENT2".to_string();
        config.base_url = Some("http://127.0.0.1:11434/v1".to_string());
        config.model = "llama3.1:8b".to_string();
        let result = OpenAiCompatibleProvider::new(&config);
        assert!(result.is_ok(), "127.0.0.1 should not require API key");
    }

    #[test]
    fn test_remote_provider_still_requires_api_key() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_REMOTE_KEY_NONEXISTENT") };
        let mut config = test_config();
        config.api_key_env = "RUSTANT_TEST_REMOTE_KEY_NONEXISTENT".to_string();
        config.base_url = None; // defaults to api.openai.com
        let result = OpenAiCompatibleProvider::new(&config);
        assert!(result.is_err(), "Remote provider must require API key");
    }

    #[test]
    fn test_known_model_meta_ollama_models() {
        let meta = known_model_meta("qwen2.5:14b").expect("qwen2.5:14b should be known");
        assert_eq!(meta.input_cost_per_million, 0.0);
        assert_eq!(meta.output_cost_per_million, 0.0);
        assert!(meta.supports_tools);
        assert_eq!(meta.context_window, 32_768);

        let meta = known_model_meta("llama3.1:8b").expect("llama3.1:8b should be known");
        assert_eq!(meta.input_cost_per_million, 0.0);
        assert!(meta.supports_tools);

        let meta = known_model_meta("qwen2.5:32b").expect("qwen2.5:32b should be known");
        assert_eq!(meta.context_window, 32_768);

        let meta = known_model_meta("mistral-nemo:12b").expect("mistral-nemo:12b should be known");
        assert!(meta.supports_tools);

        let meta = known_model_meta("deepseek-coder-v2:16b")
            .expect("deepseek-coder-v2:16b should be known");
        assert_eq!(meta.input_cost_per_million, 0.0);
    }

    #[test]
    fn test_ollama_model_zero_cost() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("RUSTANT_TEST_OLLAMA_COST_KEY") };
        let mut config = test_config();
        config.api_key_env = "RUSTANT_TEST_OLLAMA_COST_KEY".to_string();
        config.base_url = Some("http://localhost:11434/v1".to_string());
        config.model = "qwen2.5:14b".to_string();
        let provider = OpenAiCompatibleProvider::new(&config).unwrap();
        let (input_cost, output_cost) = provider.cost_per_token();
        assert_eq!(input_cost, 0.0);
        assert_eq!(output_cost, 0.0);
    }

    // --- fix_openai_turns tests ---

    #[test]
    fn test_fix_openai_turns_removes_orphaned_tool() {
        let messages = vec![
            json!({"role": "user", "content": "Hello"}),
            json!({"role": "assistant", "content": "Hi"}),
            json!({"role": "tool", "tool_call_id": "nonexistent_call", "content": "orphan"}),
        ];

        let result = OpenAiCompatibleProvider::fix_openai_turns(messages);

        // Orphaned tool message should be removed
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
    }

    #[test]
    fn test_fix_openai_turns_preserves_valid_sequence() {
        let messages = vec![
            json!({"role": "user", "content": "Read file"}),
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "file_read", "arguments": "{\"path\":\"x.rs\"}"}
                }]
            }),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "fn main(){}"}),
            json!({"role": "assistant", "content": "Here is the file."}),
        ];

        let result = OpenAiCompatibleProvider::fix_openai_turns(messages);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[2]["role"], "tool");
        assert_eq!(result[3]["role"], "assistant");
    }

    #[test]
    fn test_fix_openai_turns_relocates_system_between_call_and_result() {
        let messages = vec![
            json!({"role": "user", "content": "do something"}),
            json!({
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "file_read", "arguments": "{}"}
                }]
            }),
            json!({"role": "system", "content": "routing hint"}),
            json!({"role": "tool", "tool_call_id": "call_1", "content": "result"}),
        ];

        let result = OpenAiCompatibleProvider::fix_openai_turns(messages);

        // System message should be moved before the assistant tool_call
        assert_eq!(result.len(), 4);
        // The tool message should immediately follow the assistant
        let assistant_idx = result
            .iter()
            .position(|m| m["role"] == "assistant" && m["tool_calls"].is_array())
            .unwrap();
        assert_eq!(result[assistant_idx + 1]["role"], "tool");
    }

    #[test]
    fn test_fix_openai_turns_empty() {
        let messages: Vec<Value> = vec![];
        let result = OpenAiCompatibleProvider::fix_openai_turns(messages);
        assert!(result.is_empty());
    }
}
