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
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var(&config.api_key_env).ok())
            .ok_or_else(|| LlmError::AuthFailed {
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

        let client = super::shared_http_client();
        let token_counter = TokenCounter::for_model(&config.model);

        // Use centralized model pricing, falling back to config values
        let (cost_in, cost_out) =
            crate::providers::models::model_pricing(&config.model).unwrap_or((
                config.input_cost_per_million,
                config.output_cost_per_million,
            ));

        Ok(Self {
            client,
            base_url,
            api_key,
            model: config.model.clone(),
            context_window: config.context_window,
            cost_input: cost_in / 1_000_000.0,
            cost_output: cost_out / 1_000_000.0,
            token_counter,
        })
    }

    /// Build the JSON request body for the Anthropic Messages API.
    ///
    /// Extracts any system messages from the messages list and places them
    /// as the top-level `system` field. All other messages are converted to
    /// Anthropic's message format, then sanitized via [`fix_anthropic_turns`].
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

        // Sanitize: merge consecutive same-role, remove orphaned tool_results,
        // ensure strict user/assistant alternation.
        let mut messages_json = Self::fix_anthropic_turns(messages_json);

        // Add a third cache breakpoint on the first user message.
        // This creates 3 stable cache layers: system + tools + first task.
        // In multi-turn conversations, the first user message (task description)
        // is stable, maximizing cache hits on subsequent turns.
        if request.cache_hint.enable_prompt_cache {
            if let Some(first_user) = messages_json
                .iter_mut()
                .find(|m| m["role"].as_str() == Some("user"))
            {
                if let Some(content) = first_user["content"].as_array_mut() {
                    if let Some(last_block) = content.last_mut() {
                        last_block["cache_control"] =
                            serde_json::json!({"type": "ephemeral"});
                    }
                }
            }
        }

        let mut body = serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "temperature": request.temperature,
            "messages": messages_json,
        });

        // Add system message as top-level field if present.
        // When caching is enabled, use array-of-content-blocks format with cache_control.
        if let Some(system) = &system_text {
            if request.cache_hint.enable_prompt_cache {
                body["system"] = serde_json::json!([{
                    "type": "text",
                    "text": system,
                    "cache_control": {"type": "ephemeral"}
                }]);
            } else {
                body["system"] = Value::String(system.clone());
            }
        }

        // Add stop sequences if provided.
        if !request.stop_sequences.is_empty() {
            body["stop_sequences"] = serde_json::json!(request.stop_sequences);
        }

        // Add tools if provided.
        // When there are many tools (>20), use Anthropic Tool Search with defer_loading.
        // If MoE precision hints are available, map them to defer_loading decisions:
        //   Full precision → non-deferred (in prompt)
        //   Half/Quarter precision → deferred (discovered via tool search)
        if let Some(tools) = &request.tools {
            let use_tool_search = tools.len() > 20;
            let has_precision_hints = !request.tool_precision_hints.is_empty();

            let mut tools_json: Vec<Value> = if has_precision_hints {
                // Hybrid MoE + Tool Search: use precision hints to decide defer_loading
                tools
                    .iter()
                    .map(|tool| Self::tool_definition_to_json_hybrid(tool, &request.tool_precision_hints))
                    .collect()
            } else if use_tool_search {
                // Legacy path: defer non-core tools
                tools
                    .iter()
                    .map(Self::tool_definition_to_json_with_search)
                    .collect()
            } else {
                tools.iter().map(Self::tool_definition_to_json).collect()
            };

            // Inject the tool_search_tool entry when using deferred tools.
            // This gives Claude the ability to discover deferred tools on demand.
            if use_tool_search || has_precision_hints {
                let has_deferred = tools_json.iter().any(|t| {
                    t.get("defer_loading").and_then(|v| v.as_bool()).unwrap_or(false)
                });
                if has_deferred {
                    // Insert the BM25 tool search tool (better for natural language queries)
                    tools_json.insert(0, serde_json::json!({
                        "type": "tool_search_tool_bm25_20251119",
                        "name": "tool_search"
                    }));
                }
            }

            // When caching is enabled, mark the last tool with cache_control
            // so the tool definitions prefix is also cached.
            if request.cache_hint.enable_prompt_cache
                && let Some(last) = tools_json.last_mut()
            {
                last["cache_control"] = serde_json::json!({"type": "ephemeral"});
            }
            body["tools"] = Value::Array(tools_json);
        }

        // Enable extended thinking if configured.
        if let Some(ref thinking) = request.thinking
            && thinking.enabled
        {
            let budget = thinking.budget_tokens.unwrap_or(10000);
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget
            });
            // Anthropic requires temperature = 1.0 for thinking mode
            body["temperature"] = serde_json::json!(1.0);
        }

        // Set tool_choice if not Auto
        match &request.tool_choice {
            crate::types::ToolChoice::Auto => {}
            crate::types::ToolChoice::Required => {
                body["tool_choice"] = serde_json::json!({"type": "any"});
            }
            crate::types::ToolChoice::None => {
                body["tool_choice"] = serde_json::json!({"type": "none"});
            }
            crate::types::ToolChoice::Specific(name) => {
                body["tool_choice"] = serde_json::json!({"type": "tool", "name": name});
            }
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
            Content::Image {
                source, media_type, ..
            } => {
                let source_json = match source {
                    crate::types::ImageSource::Base64(data) => serde_json::json!({
                        "type": "base64",
                        "media_type": media_type,
                        "data": data,
                    }),
                    crate::types::ImageSource::Url(url) => serde_json::json!({
                        "type": "url",
                        "url": url,
                    }),
                    crate::types::ImageSource::FilePath(_) => {
                        // File paths should be converted to base64 before reaching here
                        serde_json::json!({"type": "base64", "media_type": media_type, "data": ""})
                    }
                };
                serde_json::json!([{
                    "type": "image",
                    "source": source_json,
                }])
            }
            Content::Thinking {
                thinking,
                signature,
            } => {
                let mut block = serde_json::json!({
                    "type": "thinking",
                    "thinking": thinking,
                });
                if let Some(sig) = signature {
                    block["signature"] = Value::String(sig.clone());
                }
                serde_json::json!([block])
            }
            Content::Citation { cited_text, .. } => {
                // Citations are rendered as text references in the Anthropic format
                serde_json::json!([{
                    "type": "text",
                    "text": cited_text,
                }])
            }
            Content::CodeExecution {
                code,
                output,
                error,
                ..
            } => {
                let text = format!(
                    "[Code Execution]\n```\n{}\n```\nOutput: {}\n{}",
                    code,
                    output.as_deref().unwrap_or("(none)"),
                    error
                        .as_deref()
                        .map_or(String::new(), |e| format!("Error: {e}"))
                );
                serde_json::json!([{"type": "text", "text": text}])
            }
            Content::SearchResult { query, results } => {
                let text = format!(
                    "[Search: {}]\n{}",
                    query,
                    results
                        .iter()
                        .map(|r| format!("- {} ({}): {}", r.title, r.url, r.snippet))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                serde_json::json!([{"type": "text", "text": text}])
            }
        }
    }

    /// Convert a `ToolDefinition` to Anthropic tool JSON format.
    /// Core tools that should be loaded upfront (not deferred).
    /// These are the most commonly used tools across all task types.
    const CORE_TOOLS: &'static [&'static str] = &[
        "file_read",
        "file_write",
        "file_list",
        "file_search",
        "file_patch",
        "shell_exec",
        "git_status",
        "git_diff",
        "git_commit",
        "echo",
        "datetime",
        "calculator",
        "ask_user",
        "web_search",
        "codebase_search",
        "smart_edit",
    ];

    fn tool_definition_to_json(tool: &ToolDefinition) -> Value {
        serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.parameters,
        })
    }

    /// Convert a tool definition to JSON with optional `defer_loading` for
    /// Anthropic's Tool Search Tool feature. Non-core tools are marked with
    /// `defer_loading: true` so Claude discovers them on demand, reducing
    /// upfront token cost by ~85%.
    fn tool_definition_to_json_with_search(tool: &ToolDefinition) -> Value {
        let mut json = serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.parameters,
        });
        if !Self::CORE_TOOLS.contains(&tool.name.as_str()) {
            json["defer_loading"] = serde_json::json!(true);
        }
        json
    }

    /// Hybrid MoE + Tool Search: use precision hints from Sparse MoE routing
    /// to decide which tools are deferred.
    ///
    /// - Shared tools (in CORE_TOOLS) and Full-precision tools → non-deferred (in prompt)
    /// - Half/Quarter-precision tools → deferred (discovered via tool search on demand)
    ///
    /// This maps the DeepSeek V3-style mixed-precision tiers directly to
    /// Anthropic's Tool Search defer_loading mechanism, achieving zero prompt
    /// token cost for secondary/tertiary expert tools while preserving full
    /// schema definitions for accurate search matching.
    fn tool_definition_to_json_hybrid(
        tool: &ToolDefinition,
        precision_hints: &std::collections::HashMap<String, crate::moe::ToolPrecision>,
    ) -> Value {
        let mut json = serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "input_schema": tool.parameters,
        });

        // Core/shared tools are never deferred regardless of precision hints
        if Self::CORE_TOOLS.contains(&tool.name.as_str()) {
            return json;
        }

        // Check MoE precision hint for this tool
        match precision_hints.get(&tool.name) {
            Some(crate::moe::ToolPrecision::Full) => {
                // Primary expert tools: keep in prompt (non-deferred)
            }
            Some(crate::moe::ToolPrecision::Half | crate::moe::ToolPrecision::Quarter) => {
                // Secondary/tertiary expert tools: defer for tool search discovery
                json["defer_loading"] = serde_json::json!(true);
            }
            None => {
                // No hint — tool not in MoE route, defer by default
                json["defer_loading"] = serde_json::json!(true);
            }
        }
        json
    }

    /// Check whether tool search should be enabled for this request.
    ///
    /// Tool search is enabled when:
    /// 1. MoE precision hints are provided (hybrid mode), OR
    /// 2. There are more than 20 tools (legacy path)
    ///
    /// AND the request actually produces deferred tools.
    fn should_use_tool_search(request: &CompletionRequest) -> bool {
        let has_many_tools = request.tools.as_ref().is_some_and(|t| t.len() > 20);
        let has_precision_hints = !request.tool_precision_hints.is_empty();
        has_many_tools || has_precision_hints
    }

    /// Fix Anthropic message turn ordering issues.
    ///
    /// Anthropic requires strict user/assistant alternation. This method:
    /// 1. Merges consecutive same-role messages (e.g., two "user" messages → one with combined content)
    /// 2. Removes orphaned tool_result blocks (no preceding tool_use in assistant message)
    /// 3. Ensures alternating user/assistant pattern by merging or inserting placeholder messages
    fn fix_anthropic_turns(messages: Vec<Value>) -> Vec<Value> {
        if messages.is_empty() {
            return messages;
        }

        // --- Pass 1: Merge consecutive same-role messages ---
        let mut merged: Vec<Value> = Vec::with_capacity(messages.len());
        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("").to_string();
            let should_merge = if let Some(last) = merged.last() {
                last["role"].as_str().unwrap_or("") == role
            } else {
                false
            };

            if should_merge {
                // Merge content arrays
                let last = merged.last_mut().unwrap();
                if let (Some(existing), Some(new)) =
                    (last["content"].as_array_mut(), msg["content"].as_array())
                {
                    existing.extend(new.iter().cloned());
                }
            } else {
                merged.push(msg);
            }
        }

        // --- Pass 2: Remove orphaned tool_result blocks ---
        // Collect all tool_use IDs from assistant messages
        let mut tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for msg in &merged {
            if msg["role"].as_str() == Some("assistant")
                && let Some(content) = msg["content"].as_array()
            {
                for block in content {
                    if block["type"].as_str() == Some("tool_use")
                        && let Some(id) = block["id"].as_str()
                    {
                        tool_use_ids.insert(id.to_string());
                    }
                }
            }
        }

        // Filter out orphaned tool_result blocks from user messages
        for msg in &mut merged {
            if msg["role"].as_str() != Some("user") {
                continue;
            }
            if let Some(content) = msg["content"].as_array() {
                let filtered: Vec<Value> = content
                    .iter()
                    .filter(|block| {
                        if block["type"].as_str() == Some("tool_result") {
                            if let Some(id) = block["tool_use_id"].as_str() {
                                return tool_use_ids.contains(id);
                            }
                            return false;
                        }
                        true
                    })
                    .cloned()
                    .collect();
                if filtered.len() != content.len() {
                    msg["content"] = Value::Array(filtered);
                }
            }
        }

        // Remove messages with empty content arrays after filtering
        merged.retain(|msg| {
            if let Some(content) = msg["content"].as_array() {
                !content.is_empty()
            } else {
                true
            }
        });

        // --- Pass 3: Ensure alternating user/assistant pattern ---
        // Anthropic requires the first message to be "user" and strict alternation after.
        let mut result: Vec<Value> = Vec::with_capacity(merged.len());
        for msg in merged {
            let role = msg["role"].as_str().unwrap_or("user").to_string();
            let last_role = result.last().and_then(|m| m["role"].as_str()).unwrap_or("");

            if result.is_empty() && role != "user" {
                // Insert a placeholder user message before the first non-user message
                result.push(serde_json::json!({
                    "role": "user",
                    "content": [{"type": "text", "text": "Continue."}]
                }));
            } else if !result.is_empty() && last_role == role {
                // Same role as previous — merge content
                let last = result.last_mut().unwrap();
                if let (Some(existing), Some(new)) =
                    (last["content"].as_array_mut(), msg["content"].as_array())
                {
                    existing.extend(new.iter().cloned());
                }
                continue;
            }
            result.push(msg);
        }

        result
    }

    /// Parse an Anthropic API response JSON into a `CompletionResponse`.
    fn parse_response(body: &Value) -> Result<CompletionResponse, LlmError> {
        let model = body["model"].as_str().unwrap_or("unknown").to_string();

        let finish_reason = body["stop_reason"].as_str().map(|s| s.to_string());

        let usage = TokenUsage {
            input_tokens: body["usage"]["input_tokens"].as_u64().unwrap_or(0) as usize,
            output_tokens: body["usage"]["output_tokens"].as_u64().unwrap_or(0) as usize,
            cache_read_tokens: body["usage"]["cache_read_input_tokens"]
                .as_u64()
                .unwrap_or(0) as usize,
            cache_creation_tokens: body["usage"]["cache_creation_input_tokens"]
                .as_u64()
                .unwrap_or(0) as usize,
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
            rate_limit_headers: None,
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
                "thinking" => {
                    let thinking = block["thinking"].as_str().unwrap_or("").to_string();
                    let signature = block["signature"].as_str().map(|s| s.to_string());
                    parts.push(Content::Thinking {
                        thinking,
                        signature,
                    });
                }
                // Tool Search responses: server_tool_use is an internal tool call
                // that Claude makes to search for deferred tools. We skip it since
                // the API handles expansion automatically.
                "server_tool_use" => {
                    debug!(
                        name = block["name"].as_str().unwrap_or(""),
                        "Tool search invoked (server-side)"
                    );
                }
                // Tool search results contain tool_reference blocks that the API
                // expands into full definitions. We skip the result blocks themselves.
                "tool_search_tool_result" => {
                    debug!("Tool search result received (server-side expansion)");
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

    /// Build the combined `anthropic-beta` header value.
    ///
    /// Combines prompt caching, token-efficient tool use, and tool search headers.
    /// Claude 4+ models have token-efficient tool use built-in (no header needed),
    /// but Claude 3.7 Sonnet needs the explicit beta header.
    fn build_beta_header(caching_enabled: bool, model: &str, tool_search_enabled: bool) -> String {
        let mut parts: Vec<&str> = Vec::new();
        if caching_enabled {
            parts.push("prompt-caching-2024-07-31");
        }
        // Token-efficient tool use for pre-Claude-4 models
        let is_claude_3 = model.contains("claude-3");
        if is_claude_3 {
            parts.push("token-efficient-tools-2025-02-19");
        }
        // Tool search for deferred tool discovery (Sonnet 4+, Opus 4+)
        if tool_search_enabled {
            parts.push("tool-search-tool-2025-10-19");
        }
        parts.join(",")
    }

    /// Count tokens exactly using the Anthropic `/v1/messages/count_tokens` endpoint.
    ///
    /// This provides server-side token counting for pre-flight budget validation,
    /// which is more accurate than local tiktoken estimates — especially for
    /// tool definitions and cached content.
    ///
    /// # Arguments
    /// * `messages` - The messages array in Anthropic JSON format.
    /// * `system` - Optional system prompt text.
    /// * `tools` - Optional tool definitions in Anthropic JSON format.
    ///
    /// # Returns
    /// The exact `input_tokens` count from the API.
    pub async fn count_tokens_exact(
        &self,
        messages: &[Value],
        system: Option<&str>,
        tools: Option<&[Value]>,
    ) -> Result<usize, LlmError> {
        let url = format!("{}/messages/count_tokens", self.base_url);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        if let Some(system_text) = system {
            body["system"] = Value::String(system_text.to_string());
        }

        if let Some(tools_list) = tools {
            body["tools"] = Value::Array(tools_list.to_vec());
        }

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            "Counting tokens via Anthropic API"
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
                message: format!("Token count request to Anthropic API failed: {e}"),
            })?;

        let status = response.status();
        let body_text = response.text().await.map_err(|e| LlmError::ResponseParse {
            message: format!("Failed to read token count response body: {e}"),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &body_text));
        }

        let response_json: Value =
            serde_json::from_str(&body_text).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON in token count response: {e}"),
            })?;

        response_json["input_tokens"]
            .as_u64()
            .map(|n| n as usize)
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'input_tokens' in count_tokens response".to_string(),
            })
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
                message: format!("HTTP {status} from Anthropic API: {body_text}"),
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

                    let _ = tx
                        .send(StreamEvent::ToolCallStart {
                            id,
                            name,
                            raw_function_call: None,
                        })
                        .await;
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
                if current_block_type.as_deref() == Some("tool_use")
                    && let Some(id) = current_block_id.take()
                {
                    let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
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
                    ..Default::default()
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
        let caching_enabled = request.cache_hint.enable_prompt_cache;
        let tool_search_enabled = Self::should_use_tool_search(&request);
        let body = self.build_request_body(&request, false);
        let url = format!("{}/messages", self.base_url);

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            caching = caching_enabled,
            tool_search = tool_search_enabled,
            "Sending Anthropic completion request"
        );

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        // Build combined beta header string
        let beta_header = Self::build_beta_header(caching_enabled, &self.model, tool_search_enabled);
        if !beta_header.is_empty() {
            req_builder = req_builder.header("anthropic-beta", &beta_header);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Request to Anthropic API failed: {e}"),
            })?;

        let status = response.status();
        // Capture rate limit headers BEFORE consuming the response body.
        let rl_headers =
            crate::providers::rate_limiter::parse_rate_limit_headers(response.headers());
        let body_text = response.text().await.map_err(|e| LlmError::ResponseParse {
            message: format!("Failed to read response body: {e}"),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &body_text));
        }

        let response_json: Value =
            serde_json::from_str(&body_text).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON in response: {e}"),
            })?;

        let mut resp = Self::parse_response(&response_json)?;
        resp.rate_limit_headers = Some(crate::types::RateLimitHeaders {
            itpm_limit: rl_headers.0,
            rpm_limit: rl_headers.2,
            otpm_limit: rl_headers.1,
        });
        Ok(resp)
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
        let caching_enabled = request.cache_hint.enable_prompt_cache;
        let tool_search_enabled = Self::should_use_tool_search(&request);
        let body = self.build_request_body(&request, true);
        let url = format!("{}/messages", self.base_url);

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            caching = caching_enabled,
            tool_search = tool_search_enabled,
            "Sending Anthropic streaming request"
        );

        let mut req_builder = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");

        // Build combined beta header string
        let beta_header = Self::build_beta_header(caching_enabled, &self.model, tool_search_enabled);
        if !beta_header.is_empty() {
            req_builder = req_builder.header("anthropic-beta", &beta_header);
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Streaming request to Anthropic API failed: {e}"),
            })?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body_text));
        }

        // Read the SSE stream line by line.
        let body_text = response.text().await.map_err(|e| LlmError::Streaming {
            message: format!("Failed to read streaming response: {e}"),
        })?;

        let mut current_block_id: Option<String> = None;
        let mut current_block_type: Option<String> = None;
        let mut total_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            ..Default::default()
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

                    // Extract input tokens and cache usage from message_start event.
                    if event_type == "message_start" {
                        let usage = &data_json["message"]["usage"];
                        if let Some(input_tokens) = usage["input_tokens"].as_u64() {
                            total_usage.input_tokens = input_tokens as usize;
                        }
                        if let Some(cache_read) = usage["cache_read_input_tokens"].as_u64() {
                            total_usage.cache_read_tokens = cache_read as usize;
                        }
                        if let Some(cache_creation) = usage["cache_creation_input_tokens"].as_u64()
                        {
                            total_usage.cache_creation_tokens = cache_creation as usize;
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

    /// Anthropic supports prompt caching via cache_control breakpoints.
    fn supports_caching(&self) -> bool {
        true
    }

    /// Anthropic cache pricing: reads at 10% of input cost, writes at 125% of input cost.
    fn cache_cost_per_token(&self) -> (f64, f64) {
        (self.cost_input * 0.1, self.cost_input * 1.25)
    }

    fn supports_vision(&self) -> bool {
        // All Claude 3+ models support vision
        self.model.contains("claude-3") || self.model.contains("claude-4")
    }

    fn supports_thinking(&self) -> bool {
        // Extended thinking available on Claude 3.5+ and Claude 4+
        self.model.contains("claude-3-5")
            || self.model.contains("claude-3.5")
            || self.model.contains("claude-4")
    }

    fn supports_citations(&self) -> bool {
        // Citations available on Claude 3.5+ models
        self.model.contains("claude-3-5")
            || self.model.contains("claude-3.5")
            || self.model.contains("claude-4")
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
            retry: crate::config::RetryConfig::default(),
            rate_limits: None,
        }
    }

    /// Helper to create a provider with a fake API key in the environment.
    fn make_provider() -> AnthropicProvider {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("ANTHROPIC_TEST_KEY_UNIT", "sk-ant-test-key-12345") };
        let config = test_config("ANTHROPIC_TEST_KEY_UNIT");
        AnthropicProvider::new(&config).expect("Provider creation should succeed")
    }

    #[test]
    fn test_new_reads_env() {
        let env_var = "ANTHROPIC_TEST_KEY_NEW_READS";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "sk-ant-my-secret-key") };
        let config = test_config(env_var);
        let provider = AnthropicProvider::new(&config).unwrap();
        assert_eq!(provider.api_key, "sk-ant-my-secret-key");
        assert_eq!(provider.model, "claude-sonnet-4-20250514");
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.context_window, 200_000);
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }

    #[test]
    fn test_new_missing_env_returns_auth_failed() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("ANTHROPIC_MISSING_KEY_XYZ") };
        let config = test_config("ANTHROPIC_MISSING_KEY_XYZ");
        let result = AnthropicProvider::new(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            LlmError::AuthFailed { provider } => {
                assert!(provider.contains("ANTHROPIC_MISSING_KEY_XYZ"));
            }
            other => panic!("Expected AuthFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_new_custom_base_url() {
        let env_var = "ANTHROPIC_TEST_KEY_CUSTOM_URL";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "test-key") };
        let mut config = test_config(env_var);
        config.base_url = Some("https://my-proxy.example.com/v1".to_string());
        let provider = AnthropicProvider::new(&config).unwrap();
        assert_eq!(provider.base_url, "https://my-proxy.example.com/v1");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
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
            _ => panic!("Expected AuthFailed, got {err:?}"),
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
            _ => panic!("Expected RateLimited, got {err:?}"),
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
            _ => panic!("Expected RateLimited, got {err:?}"),
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
            _ => panic!("Expected ApiRequest, got {err:?}"),
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
            StreamEvent::ToolCallStart { id, name, .. } => {
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            other => panic!("Expected ResponseParse, got {other:?}"),
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

    // --- fix_anthropic_turns tests ---

    #[test]
    fn test_fix_anthropic_turns_merges_consecutive_user() {
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Hello"}]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "t1",
                    "name": "file_read",
                    "input": {"path": "x.rs"}
                }]
            }),
            // Two consecutive user messages (text + tool_result) should be merged
            serde_json::json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "result"}]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Thanks"}]
            }),
        ];

        let result = AnthropicProvider::fix_anthropic_turns(messages);

        // The two consecutive user messages should merge into one
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[2]["role"], "user");
        let content = result[2]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2); // tool_result + text merged
    }

    #[test]
    fn test_fix_anthropic_turns_removes_orphaned_tool_result() {
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Hello"}]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{"type": "text", "text": "Hi"}]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "nonexistent", "content": "orphan"}]
            }),
        ];

        let result = AnthropicProvider::fix_anthropic_turns(messages);

        // The orphaned tool_result message should be removed (empty content after filter)
        // Only user + assistant should remain
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
    }

    #[test]
    fn test_fix_anthropic_turns_preserves_valid_sequence() {
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Read file"}]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "toolu_01",
                    "name": "file_read",
                    "input": {"path": "main.rs"}
                }]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "toolu_01", "content": "fn main(){}"}]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [{"type": "text", "text": "Here is the file."}]
            }),
        ];

        let result = AnthropicProvider::fix_anthropic_turns(messages);

        assert_eq!(result.len(), 4);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[2]["role"], "user");
        assert_eq!(result[3]["role"], "assistant");
    }

    #[test]
    fn test_fix_anthropic_turns_handles_empty() {
        let messages: Vec<Value> = vec![];
        let result = AnthropicProvider::fix_anthropic_turns(messages);
        assert!(result.is_empty());
    }

    #[test]
    fn test_fix_anthropic_turns_ensures_user_first() {
        let messages = vec![serde_json::json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "I'll help."}]
        })];

        let result = AnthropicProvider::fix_anthropic_turns(messages);

        // Should insert a placeholder user message first
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
    }

    #[test]
    fn test_fix_anthropic_turns_handles_multipart_tool_result() {
        let messages = vec![
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "Do two things"}]
            }),
            serde_json::json!({
                "role": "assistant",
                "content": [
                    {"type": "tool_use", "id": "t1", "name": "file_read", "input": {"path": "a.rs"}},
                    {"type": "tool_use", "id": "t2", "name": "file_read", "input": {"path": "b.rs"}}
                ]
            }),
            // Two separate user messages with tool results (should merge)
            serde_json::json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "a contents"}]
            }),
            serde_json::json!({
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t2", "content": "b contents"}]
            }),
        ];

        let result = AnthropicProvider::fix_anthropic_turns(messages);

        // The two user tool_result messages should be merged
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["role"], "user");
        assert_eq!(result[1]["role"], "assistant");
        assert_eq!(result[2]["role"], "user");
        let content = result[2]["content"].as_array().unwrap();
        assert_eq!(content.len(), 2); // Both tool_results merged
    }

    #[tokio::test]
    async fn test_count_tokens_exact_builds_request() {
        // We cannot call the real API in unit tests, but we can verify the method
        // exists and handles error responses correctly by pointing at a non-existent
        // server. This validates the request construction and error mapping path.
        let env_var = "ANTHROPIC_TEST_KEY_COUNT_TOKENS";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "sk-ant-test-count-key") };
        let mut config = test_config(env_var);
        // Point to a non-routable address so the request fails fast
        config.base_url = Some("http://127.0.0.1:1".to_string());
        let provider = AnthropicProvider::new(&config).unwrap();

        let messages = vec![serde_json::json!({
            "role": "user",
            "content": [{"type": "text", "text": "Hello"}]
        })];

        let result = provider
            .count_tokens_exact(&messages, Some("You are helpful"), None)
            .await;

        // Should fail with a connection error since we are hitting a non-routable address
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            LlmError::ApiRequest { message } => {
                assert!(
                    message.contains("Token count request"),
                    "Expected token count error, got: {message}"
                );
            }
            other => panic!("Expected ApiRequest error, got {other:?}"),
        }
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }

    // -----------------------------------------------------------------------
    // Hybrid Tool Search + MoE tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_build_beta_header_with_tool_search() {
        // No features enabled
        let header = AnthropicProvider::build_beta_header(false, "claude-sonnet-4-20250514", false);
        assert!(header.is_empty());

        // Caching only
        let header = AnthropicProvider::build_beta_header(true, "claude-sonnet-4-20250514", false);
        assert_eq!(header, "prompt-caching-2024-07-31");

        // Tool search only
        let header = AnthropicProvider::build_beta_header(false, "claude-sonnet-4-20250514", true);
        assert_eq!(header, "tool-search-tool-2025-10-19");

        // Both caching and tool search
        let header = AnthropicProvider::build_beta_header(true, "claude-sonnet-4-20250514", true);
        assert!(header.contains("prompt-caching-2024-07-31"));
        assert!(header.contains("tool-search-tool-2025-10-19"));

        // Claude 3 model gets token-efficient-tools too
        let header = AnthropicProvider::build_beta_header(true, "claude-3-5-sonnet-20241022", true);
        assert!(header.contains("prompt-caching-2024-07-31"));
        assert!(header.contains("token-efficient-tools-2025-02-19"));
        assert!(header.contains("tool-search-tool-2025-10-19"));
    }

    #[test]
    fn test_tool_definition_hybrid_shared_tool_never_deferred() {
        let tool = ToolDefinition {
            name: "file_read".to_string(),
            description: "Read a file".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let mut hints = std::collections::HashMap::new();
        hints.insert(
            "file_read".to_string(),
            crate::moe::ToolPrecision::Quarter,
        );

        let json = AnthropicProvider::tool_definition_to_json_hybrid(&tool, &hints);
        // Even with Quarter precision hint, CORE_TOOLS are never deferred
        assert!(json.get("defer_loading").is_none());
    }

    #[test]
    fn test_tool_definition_hybrid_full_precision_not_deferred() {
        let tool = ToolDefinition {
            name: "ml_train".to_string(),
            description: "Train a model".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let mut hints = std::collections::HashMap::new();
        hints.insert("ml_train".to_string(), crate::moe::ToolPrecision::Full);

        let json = AnthropicProvider::tool_definition_to_json_hybrid(&tool, &hints);
        assert!(json.get("defer_loading").is_none());
    }

    #[test]
    fn test_tool_definition_hybrid_half_precision_deferred() {
        let tool = ToolDefinition {
            name: "security_scan".to_string(),
            description: "Scan for vulnerabilities".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let mut hints = std::collections::HashMap::new();
        hints.insert(
            "security_scan".to_string(),
            crate::moe::ToolPrecision::Half,
        );

        let json = AnthropicProvider::tool_definition_to_json_hybrid(&tool, &hints);
        assert_eq!(json["defer_loading"], serde_json::json!(true));
    }

    #[test]
    fn test_tool_definition_hybrid_quarter_precision_deferred() {
        let tool = ToolDefinition {
            name: "kubernetes".to_string(),
            description: "K8s operations".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let mut hints = std::collections::HashMap::new();
        hints.insert(
            "kubernetes".to_string(),
            crate::moe::ToolPrecision::Quarter,
        );

        let json = AnthropicProvider::tool_definition_to_json_hybrid(&tool, &hints);
        assert_eq!(json["defer_loading"], serde_json::json!(true));
    }

    #[test]
    fn test_tool_definition_hybrid_no_hint_defaults_to_deferred() {
        let tool = ToolDefinition {
            name: "obscure_tool".to_string(),
            description: "Something rare".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        };
        let hints = std::collections::HashMap::new(); // empty hints

        let json = AnthropicProvider::tool_definition_to_json_hybrid(&tool, &hints);
        // Non-core tool with no MoE hint → deferred
        assert_eq!(json["defer_loading"], serde_json::json!(true));
    }

    #[test]
    fn test_should_use_tool_search() {
        // Few tools, no hints → no search
        let req = CompletionRequest {
            tools: Some(vec![ToolDefinition {
                name: "file_read".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            ..Default::default()
        };
        assert!(!AnthropicProvider::should_use_tool_search(&req));

        // Many tools → search enabled
        let many_tools: Vec<ToolDefinition> = (0..25)
            .map(|i| ToolDefinition {
                name: format!("tool_{i}"),
                description: format!("Tool {i}"),
                parameters: serde_json::json!({"type": "object"}),
            })
            .collect();
        let req = CompletionRequest {
            tools: Some(many_tools),
            ..Default::default()
        };
        assert!(AnthropicProvider::should_use_tool_search(&req));

        // Few tools but with precision hints → search enabled
        let mut hints = std::collections::HashMap::new();
        hints.insert("ml_train".to_string(), crate::moe::ToolPrecision::Half);
        let req = CompletionRequest {
            tools: Some(vec![ToolDefinition {
                name: "ml_train".to_string(),
                description: "Train".to_string(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            tool_precision_hints: hints,
            ..Default::default()
        };
        assert!(AnthropicProvider::should_use_tool_search(&req));
    }

    #[test]
    fn test_build_request_body_injects_tool_search_entry() {
        let provider = make_provider();
        let mut hints = std::collections::HashMap::new();
        hints.insert(
            "security_scan".to_string(),
            crate::moe::ToolPrecision::Half,
        );
        hints.insert("file_read".to_string(), crate::moe::ToolPrecision::Full);

        let request = CompletionRequest {
            messages: vec![Message::user("scan for vulnerabilities")],
            tools: Some(vec![
                ToolDefinition {
                    name: "file_read".to_string(),
                    description: "Read a file".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                },
                ToolDefinition {
                    name: "security_scan".to_string(),
                    description: "Scan for vulnerabilities".to_string(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            ]),
            tool_precision_hints: hints,
            ..Default::default()
        };

        let body = provider.build_request_body(&request, false);
        let tools = body["tools"].as_array().unwrap();

        // First entry should be the tool_search_tool (BM25 variant)
        assert_eq!(
            tools[0]["type"],
            "tool_search_tool_bm25_20251119"
        );
        assert_eq!(tools[0]["name"], "tool_search");

        // file_read (core tool) should NOT be deferred
        let file_read = tools.iter().find(|t| t["name"] == "file_read").unwrap();
        assert!(file_read.get("defer_loading").is_none());

        // security_scan (Half precision) should be deferred
        let sec_scan = tools
            .iter()
            .find(|t| t["name"] == "security_scan")
            .unwrap();
        assert_eq!(sec_scan["defer_loading"], true);
    }

    #[test]
    fn test_parse_content_blocks_handles_server_tool_use() {
        let blocks = vec![
            serde_json::json!({"type": "server_tool_use", "id": "srvtoolu_01ABC", "name": "tool_search", "input": {"query": "security"}}),
            serde_json::json!({"type": "tool_search_tool_result", "tool_use_id": "srvtoolu_01ABC", "content": {"type": "tool_search_tool_search_result"}}),
            serde_json::json!({"type": "text", "text": "I found the security scan tool."}),
            serde_json::json!({"type": "tool_use", "id": "toolu_01XYZ", "name": "security_scan", "input": {"path": "."}}),
        ];

        let content = AnthropicProvider::parse_content_blocks(&blocks).unwrap();
        // server_tool_use and tool_search_tool_result are silently skipped;
        // only the text and tool_use blocks are parsed
        match content {
            Content::MultiPart { ref parts } => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(&parts[0], Content::Text { text } if text.contains("security")));
                assert!(matches!(&parts[1], Content::ToolCall { name, .. } if name == "security_scan"));
            }
            _ => panic!("Expected MultiPart content"),
        }
    }
}
