//! Google Gemini API provider implementation.
//!
//! Implements the `LlmProvider` trait for the native Google Gemini API,
//! supporting Gemini model families with both synchronous and streaming completions.
//!
//! Key differences from OpenAI-compatible APIs:
//! - Auth via `?key=API_KEY` query parameter (not header-based)
//! - System instruction is a top-level `system_instruction` field
//! - Roles are `"user"` / `"model"` (not `"assistant"`)
//! - Tool calls use `functionCall` / `functionResponse` content parts
//! - Streaming uses `?alt=sse` query parameter

use crate::brain::{LlmProvider, TokenCounter};
use crate::config::LlmConfig;
use crate::error::LlmError;
use crate::types::{
    CompletionRequest, CompletionResponse, Content, Message, Role, StreamEvent, TokenUsage,
    ToolDefinition,
};
use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde_json::Value;
use tokio::sync::mpsc;
use tracing::{debug, warn};

/// The default Google Gemini API base URL.
const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// How authentication is performed against the Gemini API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GeminiAuthMode {
    /// Traditional API key via `?key=` query parameter.
    ApiKey,
    /// OAuth Bearer token via `Authorization` header.
    Bearer,
}

/// Google Gemini API provider.
///
/// Communicates with the Gemini API to perform completions using
/// Gemini models. Supports both full and streaming responses, including tool use.
pub struct GeminiProvider {
    client: Client,
    base_url: String,
    api_key: String,
    model: String,
    context_window: usize,
    cost_input: f64,
    cost_output: f64,
    token_counter: TokenCounter,
    auth_mode: GeminiAuthMode,
    /// Cached content name from the Gemini Context Caching API.
    /// Set after a successful `create_cached_content()` call, and reused
    /// in subsequent requests to avoid re-processing long system prompts.
    pub cached_content_name: Option<String>,
}

impl GeminiProvider {
    /// Create a new Gemini provider from configuration.
    ///
    /// Reads the API key from the environment variable specified in `config.api_key_env`.
    /// Returns `LlmError::AuthFailed` if the environment variable is not set.
    pub fn new(config: &LlmConfig) -> Result<Self, LlmError> {
        let api_key = config
            .api_key
            .clone()
            .or_else(|| std::env::var(&config.api_key_env).ok())
            .ok_or_else(|| LlmError::AuthFailed {
                provider: format!("Gemini (env var '{}' not set)", config.api_key_env),
            })?;
        Self::new_with_key(config, api_key)
    }

    /// Create a new Gemini provider with an explicitly provided API key.
    ///
    /// Use this when the API key has been resolved externally (e.g., from a credential store).
    pub fn new_with_key(config: &LlmConfig, api_key: String) -> Result<Self, LlmError> {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let client = super::shared_http_client();
        let token_counter = TokenCounter::for_model(&config.model);

        let auth_mode = if config.auth_method == "oauth" {
            GeminiAuthMode::Bearer
        } else {
            GeminiAuthMode::ApiKey
        };

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
            auth_mode,
            cached_content_name: None,
        })
    }

    /// Build the JSON request body for the Gemini API.
    ///
    /// Extracts any system messages and places them as the top-level
    /// `system_instruction` field. All other messages are converted to
    /// Gemini's `contents` format.
    fn build_request_body(&self, request: &CompletionRequest, _stream: bool) -> Value {
        let max_tokens = request.max_tokens.unwrap_or(4096);

        // Extract system message(s) from the messages list.
        let (system_text, non_system_messages) =
            Self::extract_system_instruction(&request.messages);

        // Convert messages to Gemini format and fix sequencing.
        let raw_contents: Vec<Value> = non_system_messages
            .iter()
            .map(|msg| Self::message_to_gemini_json(msg))
            .collect();
        let contents = Self::fix_gemini_turns(raw_contents);

        let mut body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "maxOutputTokens": max_tokens,
                "temperature": request.temperature,
            },
        });

        // Add system instruction as top-level field if present.
        if let Some(system) = &system_text {
            body["system_instruction"] = serde_json::json!({
                "parts": [{"text": system}]
            });
        }

        // Add stop sequences if provided.
        if !request.stop_sequences.is_empty() {
            body["generationConfig"]["stopSequences"] = serde_json::json!(request.stop_sequences);
        }

        // Add tools if provided.
        if let Some(tools) = &request.tools {
            let function_declarations: Vec<Value> =
                tools.iter().map(Self::tool_definition_to_json).collect();
            let mut tools_array = vec![serde_json::json!({
                "functionDeclarations": function_declarations
            })];
            // Add grounding tools (Google Search, etc.)
            for gt in &request.grounding_tools {
                match gt {
                    crate::types::GroundingTool::GoogleSearch => {
                        tools_array.push(serde_json::json!({"googleSearch": {}}));
                    }
                    crate::types::GroundingTool::UrlContext { urls } => {
                        tools_array.push(serde_json::json!({"urlContext": {"urls": urls}}));
                    }
                }
            }
            body["tools"] = Value::Array(tools_array);

            // Restrict Gemini to only call functions we declared.
            // Without this, Gemini can hallucinate calls to functions
            // not in the declaration list (e.g., knowledge_graph when only
            // arxiv_research is provided).
            let allowed_names: Vec<String> = tools.iter().map(|t| t.name.clone()).collect();
            body["toolConfig"] = serde_json::json!({
                "functionCallingConfig": {
                    "mode": "AUTO",
                    "allowedFunctionNames": allowed_names
                }
            });
        } else if !request.grounding_tools.is_empty() {
            // Grounding tools without function tools
            let tools_array: Vec<Value> = request
                .grounding_tools
                .iter()
                .map(|gt| match gt {
                    crate::types::GroundingTool::GoogleSearch => {
                        serde_json::json!({"googleSearch": {}})
                    }
                    crate::types::GroundingTool::UrlContext { urls } => {
                        serde_json::json!({"urlContext": {"urls": urls}})
                    }
                })
                .collect();
            body["tools"] = Value::Array(tools_array);
        }

        // Add code execution tool if enabled
        if request.enable_code_execution {
            let tools = body["tools"].as_array_mut().map(|a| {
                a.push(serde_json::json!({"codeExecution": {}}));
            });
            if tools.is_none() {
                body["tools"] = serde_json::json!([{"codeExecution": {}}]);
            }
        }

        // Enable thinking mode if configured
        if let Some(ref thinking) = request.thinking
            && thinking.enabled
        {
            if let Some(budget) = thinking.budget_tokens {
                body["generationConfig"]["thinkingConfig"] =
                    serde_json::json!({"thinkingBudget": budget});
            }
            if let Some(ref level) = thinking.level {
                body["generationConfig"]["thinkingConfig"] =
                    serde_json::json!({"thinkingLevel": level});
            }
        }

        // Structured output (JSON mode)
        if let Some(ref response_format) = request.response_format {
            match response_format {
                crate::types::ResponseFormat::JsonObject => {
                    body["generationConfig"]["responseMimeType"] =
                        Value::String("application/json".into());
                }
                crate::types::ResponseFormat::JsonSchema { schema, .. } => {
                    body["generationConfig"]["responseMimeType"] =
                        Value::String("application/json".into());
                    body["generationConfig"]["responseSchema"] = schema.clone();
                }
                crate::types::ResponseFormat::Text => {}
            }
        }

        body
    }

    /// Extract system messages from the messages list.
    ///
    /// Returns a tuple of (optional concatenated system text, non-system messages).
    fn extract_system_instruction(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
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

    /// Convert a single `Message` to Gemini JSON format.
    ///
    /// Maps our roles to Gemini roles:
    /// - `User` / `Tool` -> `"user"`
    /// - `Assistant` -> `"model"`
    ///
    /// For assistant messages containing function calls, uses stored raw Gemini
    /// parts (if available) to preserve `thought_signature` fields required by
    /// Gemini's thinking models.
    fn message_to_gemini_json(msg: &Message) -> Value {
        let role = match msg.role {
            Role::User | Role::Tool => "user",
            Role::Assistant => "model",
            Role::System => "user", // Should not reach here after extraction
        };

        // For assistant messages with stored raw Gemini parts (preserving
        // thought_signature), use them verbatim instead of reconstructing.
        if msg.role == Role::Assistant
            && let Some(raw_parts) = msg.metadata.get("gemini_raw_parts")
        {
            return serde_json::json!({
                "role": role,
                "parts": raw_parts,
            });
        }

        let parts = Self::content_to_gemini_parts(&msg.content);

        serde_json::json!({
            "role": role,
            "parts": parts,
        })
    }

    /// Convert a `Content` enum to Gemini parts array.
    fn content_to_gemini_parts(content: &Content) -> Value {
        match content {
            Content::Text { text } => {
                serde_json::json!([{"text": text}])
            }
            Content::ToolCall {
                id: _,
                name,
                arguments,
            } => {
                serde_json::json!([{
                    "functionCall": {
                        "name": name,
                        "args": arguments,
                    }
                }])
            }
            Content::ToolResult {
                call_id: _,
                output,
                is_error: _,
            } => {
                // Parse the output as JSON; Gemini requires response to be a Struct (object).
                let response_value = match serde_json::from_str::<Value>(output) {
                    Ok(Value::Object(map)) => Value::Object(map),
                    Ok(other) => serde_json::json!({"result": other}),
                    Err(_) => serde_json::json!({"result": output}),
                };
                serde_json::json!([{
                    "functionResponse": {
                        "name": "tool",
                        "response": response_value,
                    }
                }])
            }
            Content::MultiPart { parts } => {
                let gemini_parts: Vec<Value> = parts
                    .iter()
                    .flat_map(|part| match Self::content_to_gemini_parts(part) {
                        Value::Array(arr) => arr,
                        other => vec![other],
                    })
                    .collect();
                Value::Array(gemini_parts)
            }
            Content::Image {
                source, media_type, ..
            } => {
                match source {
                    crate::types::ImageSource::Base64(data) => {
                        serde_json::json!([{
                            "inlineData": {
                                "mimeType": media_type,
                                "data": data,
                            }
                        }])
                    }
                    crate::types::ImageSource::Url(url) => {
                        serde_json::json!([{
                            "fileData": {
                                "mimeType": media_type,
                                "fileUri": url,
                            }
                        }])
                    }
                    crate::types::ImageSource::FilePath(_) => {
                        // File paths should be resolved to base64 before reaching provider
                        serde_json::json!([{"text": "[Image: file path not resolved]"}])
                    }
                }
            }
            Content::Thinking { thinking, .. } => {
                // Gemini uses "thought" parts
                serde_json::json!([{"text": thinking}])
            }
            Content::Citation { cited_text, .. } => {
                serde_json::json!([{"text": cited_text}])
            }
            Content::CodeExecution {
                code,
                output,
                error,
                ..
            } => {
                let mut parts = vec![
                    serde_json::json!({"executableCode": {"code": code, "language": "PYTHON"}}),
                ];
                if let Some(out) = output {
                    parts.push(serde_json::json!({"codeExecutionResult": {"output": out, "outcome": "OUTCOME_OK"}}));
                }
                if let Some(err) = error {
                    parts.push(serde_json::json!({"codeExecutionResult": {"output": err, "outcome": "OUTCOME_FAILED"}}));
                }
                Value::Array(parts)
            }
            Content::SearchResult { query, results } => {
                let text = format!(
                    "[Search: {}]\n{}",
                    query,
                    results
                        .iter()
                        .map(|r| format!("- {}: {}", r.title, r.snippet))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                serde_json::json!([{"text": text}])
            }
        }
    }

    /// Post-process Gemini contents to fix API sequencing requirements:
    /// 1. Merge consecutive same-role turns (e.g., multiple tool results)
    /// 2. Fix `functionResponse.name` to match the preceding `functionCall.name`
    /// 3. Remove orphaned `functionResponse` parts (no matching `functionCall`)
    /// 4. Filter out turns with empty parts arrays
    /// 5. Ensure the first message has `"user"` role
    fn fix_gemini_turns(contents: Vec<Value>) -> Vec<Value> {
        if contents.is_empty() {
            return contents;
        }

        // Pass 1: Merge consecutive same-role turns.
        let mut merged: Vec<Value> = Vec::with_capacity(contents.len());
        for entry in contents {
            let role = entry["role"].as_str().unwrap_or("").to_string();
            let should_merge = if let Some(last) = merged.last() {
                last["role"].as_str().unwrap_or("") == role
            } else {
                false
            };

            if should_merge {
                // Extend the last entry's parts with this entry's parts.
                let last = merged.last_mut().unwrap();
                if let (Some(existing), Some(new)) =
                    (last["parts"].as_array().cloned(), entry["parts"].as_array())
                {
                    let mut combined = existing;
                    combined.extend(new.iter().cloned());
                    last["parts"] = Value::Array(combined);
                }
            } else {
                merged.push(entry);
            }
        }

        // Pass 2: Fix functionResponse names to match preceding functionCall names.
        for i in 0..merged.len().saturating_sub(1) {
            let call_names: Vec<String> = merged[i]["parts"]
                .as_array()
                .map(|parts| {
                    parts
                        .iter()
                        .filter_map(|p| {
                            p.get("functionCall")
                                .and_then(|fc| fc["name"].as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();

            if call_names.is_empty() {
                continue;
            }

            // Fix the functionResponse entries in the next turn.
            if let Some(parts) = merged[i + 1]["parts"].as_array_mut() {
                let mut name_idx = 0;
                for part in parts.iter_mut() {
                    if part.get("functionResponse").is_some() && name_idx < call_names.len() {
                        part["functionResponse"]["name"] =
                            Value::String(call_names[name_idx].clone());
                        name_idx += 1;
                    }
                }
            }
        }

        // Pass 3: Remove orphaned functionResponse parts (no matching functionCall).
        {
            // Collect all functionCall names from "model" role turns.
            let mut function_call_names: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for entry in &merged {
                if entry["role"].as_str() == Some("model")
                    && let Some(parts) = entry["parts"].as_array()
                {
                    for part in parts {
                        if let Some(name) =
                            part.get("functionCall").and_then(|fc| fc["name"].as_str())
                        {
                            function_call_names.insert(name.to_string());
                        }
                    }
                }
            }

            // Filter out functionResponse parts whose names don't match any functionCall.
            for entry in &mut merged {
                if entry["role"].as_str() != Some("user") {
                    continue;
                }
                if let Some(parts) = entry["parts"].as_array() {
                    let filtered: Vec<Value> = parts
                        .iter()
                        .filter(|part| {
                            if let Some(name) = part
                                .get("functionResponse")
                                .and_then(|fr| fr["name"].as_str())
                            {
                                function_call_names.contains(name)
                            } else {
                                true // keep non-functionResponse parts
                            }
                        })
                        .cloned()
                        .collect();
                    if filtered.len() != parts.len() {
                        entry["parts"] = Value::Array(filtered);
                    }
                }
            }
        }

        // Pass 4: Filter out turns with empty parts arrays.
        merged.retain(|entry| {
            entry["parts"]
                .as_array()
                .map(|parts| !parts.is_empty())
                .unwrap_or(true) // keep entries without parts array (shouldn't happen)
        });

        // Guard against all turns being filtered out.
        if merged.is_empty() {
            return merged;
        }

        // Pass 5: Ensure the first message is "user" role.
        if merged
            .first()
            .and_then(|m| m["role"].as_str())
            .unwrap_or("")
            != "user"
        {
            merged.insert(
                0,
                serde_json::json!({"role": "user", "parts": [{"text": "Hello"}]}),
            );
        }

        merged
    }

    /// Convert a `ToolDefinition` to Gemini function declaration JSON.
    ///
    /// Sanitizes the parameters schema to remove fields unsupported by the
    /// Gemini API (e.g., `additionalProperties`, `default`, `$schema`).
    fn tool_definition_to_json(tool: &ToolDefinition) -> Value {
        serde_json::json!({
            "name": tool.name,
            "description": tool.description,
            "parameters": Self::sanitize_schema(&tool.parameters),
        })
    }

    /// Recursively strip JSON Schema fields that the Gemini API does not support.
    ///
    /// Gemini function declarations support: `type`, `description`, `properties`,
    /// `required`, `enum`, `items`, `format`, `nullable`.
    /// Everything else (e.g., `additionalProperties`, `default`, `$schema`, `$ref`,
    /// `title`, `examples`, `pattern`, `minimum`, `maximum`, etc.) is removed.
    fn sanitize_schema(schema: &Value) -> Value {
        const ALLOWED_KEYS: &[&str] = &[
            "type",
            "description",
            "properties",
            "required",
            "enum",
            "items",
            "format",
            "nullable",
        ];

        match schema {
            Value::Object(map) => {
                let mut clean = serde_json::Map::new();
                for (key, value) in map {
                    if !ALLOWED_KEYS.contains(&key.as_str()) {
                        continue;
                    }
                    // Recurse into nested schemas (properties, items)
                    let cleaned_value = match key.as_str() {
                        "properties" => {
                            if let Value::Object(props) = value {
                                let cleaned_props: serde_json::Map<String, Value> = props
                                    .iter()
                                    .map(|(k, v)| (k.clone(), Self::sanitize_schema(v)))
                                    .collect();
                                Value::Object(cleaned_props)
                            } else {
                                value.clone()
                            }
                        }
                        "items" => Self::sanitize_schema(value),
                        _ => value.clone(),
                    };
                    clean.insert(key.clone(), cleaned_value);
                }
                Value::Object(clean)
            }
            other => other.clone(),
        }
    }

    /// Parse a Gemini API response JSON into a `CompletionResponse`.
    fn parse_response(body: &Value) -> Result<CompletionResponse, LlmError> {
        let candidates = body["candidates"]
            .as_array()
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'candidates' array in response".to_string(),
            })?;

        if candidates.is_empty() {
            return Err(LlmError::ResponseParse {
                message: "Empty 'candidates' array in response".to_string(),
            });
        }

        let candidate = &candidates[0];
        let content = &candidate["content"];
        let parts = content["parts"]
            .as_array()
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'parts' array in candidate content".to_string(),
            })?;

        let parsed_content = Self::parse_parts(parts)?;

        let finish_reason = candidate["finishReason"].as_str().map(|s| s.to_string());

        // Extract usage metadata.
        let usage_metadata = &body["usageMetadata"];
        let usage = TokenUsage {
            input_tokens: usage_metadata["promptTokenCount"].as_u64().unwrap_or(0) as usize,
            output_tokens: usage_metadata["candidatesTokenCount"].as_u64().unwrap_or(0) as usize,
            cache_read_tokens: usage_metadata["cachedContentTokenCount"]
                .as_u64()
                .unwrap_or(0) as usize,
            cache_creation_tokens: 0,
        };

        let model = body["modelVersion"]
            .as_str()
            .unwrap_or("gemini")
            .to_string();

        let mut message = Message::new(Role::Assistant, parsed_content);

        // Store raw Gemini parts to preserve thought_signature for function calls.
        // When converting messages back to Gemini format, these raw parts are used
        // verbatim instead of reconstructing from Content (which loses the signature).
        let has_function_calls = parts.iter().any(|p| p.get("functionCall").is_some());
        if has_function_calls {
            message = message.with_metadata("gemini_raw_parts", Value::Array(parts.to_vec()));
        }

        Ok(CompletionResponse {
            message,
            usage,
            model,
            finish_reason,
            rate_limit_headers: None,
        })
    }

    /// Parse an array of Gemini parts into a `Content` value.
    fn parse_parts(parts: &[Value]) -> Result<Content, LlmError> {
        let mut content_parts: Vec<Content> = Vec::new();

        for part in parts {
            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                content_parts.push(Content::Text {
                    text: text.to_string(),
                });
            } else if let Some(fc) = part.get("functionCall") {
                let name = fc["name"].as_str().unwrap_or("").to_string();
                let args = fc["args"].clone();
                // Generate a unique call ID since Gemini doesn't provide one.
                let id = format!("gemini_call_{}", uuid::Uuid::new_v4());
                content_parts.push(Content::ToolCall {
                    id,
                    name,
                    arguments: args,
                });
            } else {
                debug!(?part, "Ignoring unknown Gemini part type");
            }
        }

        match content_parts.len() {
            0 => Ok(Content::text("")),
            1 => Ok(content_parts.into_iter().next().unwrap()),
            _ => Ok(Content::MultiPart {
                parts: content_parts,
            }),
        }
    }

    /// Create a cached content entry via the Gemini Context Caching API.
    ///
    /// Long-lived system prompts and tool definitions can be cached server-side
    /// to reduce TTFT on subsequent requests. The returned cache name should be
    /// stored in `cached_content_name` and referenced in future requests via
    /// `CacheHint.cached_content_ref`.
    ///
    /// # Arguments
    /// * `system_prompt` - The system instruction text to cache.
    /// * `tools` - Tool definitions in Gemini JSON format (functionDeclarations).
    /// * `ttl_secs` - Time-to-live for the cached content in seconds.
    ///
    /// # Returns
    /// The `name` of the created cached content resource (e.g., `cachedContents/abc123`).
    pub async fn create_cached_content(
        &mut self,
        system_prompt: &str,
        tools: &[Value],
        ttl_secs: u64,
    ) -> Result<String, LlmError> {
        let url = match self.auth_mode {
            GeminiAuthMode::ApiKey => {
                format!("{}/cachedContents?key={}", self.base_url, self.api_key)
            }
            GeminiAuthMode::Bearer => {
                format!("{}/cachedContents", self.base_url)
            }
        };

        let mut body = serde_json::json!({
            "model": format!("models/{}", self.model),
            "contents": [],
            "systemInstruction": {
                "parts": [{"text": system_prompt}]
            },
            "ttl": format!("{ttl_secs}s"),
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!([{
                "functionDeclarations": tools
            }]);
        }

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            ttl_secs = ttl_secs,
            "Creating cached content via Gemini API"
        );

        let response = self
            .build_authed_request(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Cached content creation request to Gemini API failed: {e}"),
            })?;

        let status = response.status();
        let body_text = response.text().await.map_err(|e| LlmError::ResponseParse {
            message: format!("Failed to read cached content response body: {e}"),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &body_text, None));
        }

        let response_json: Value =
            serde_json::from_str(&body_text).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON in cached content response: {e}"),
            })?;

        let name = response_json["name"]
            .as_str()
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'name' in cachedContents response".to_string(),
            })?
            .to_string();

        self.cached_content_name = Some(name.clone());

        debug!(
            cached_content_name = name.as_str(),
            "Successfully created cached content"
        );

        Ok(name)
    }

    /// Map an HTTP status code to the appropriate `LlmError`.
    ///
    /// When `headers` are available, parses the `retry-after` header for 429
    /// responses instead of hardcoding a default.
    fn map_http_error(
        status: reqwest::StatusCode,
        body_text: &str,
        headers: Option<&reqwest::header::HeaderMap>,
    ) -> LlmError {
        match status.as_u16() {
            401 | 403 => LlmError::AuthFailed {
                provider: "Gemini".to_string(),
            },
            429 => {
                // Parse Retry-After header if available (seconds or HTTP-date).
                let retry_after = headers
                    .and_then(|h| h.get(reqwest::header::RETRY_AFTER))
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(30);
                LlmError::RateLimited {
                    retry_after_secs: retry_after,
                }
            }
            _ => LlmError::ApiRequest {
                message: format!("HTTP {status} from Gemini API: {body_text}"),
            },
        }
    }

    /// Build the endpoint URL for a Gemini API call.
    ///
    /// In `ApiKey` mode, the key is appended as a `?key=` query parameter.
    /// In `Bearer` mode, the URL contains no key (auth is via header).
    fn endpoint_url(&self, model: &str, method: &str) -> String {
        match self.auth_mode {
            GeminiAuthMode::ApiKey => {
                format!(
                    "{}/models/{}:{}?key={}",
                    self.base_url, model, method, self.api_key
                )
            }
            GeminiAuthMode::Bearer => {
                format!("{}/models/{}:{}", self.base_url, model, method)
            }
        }
    }

    /// Build a request with the appropriate auth header/params.
    fn build_authed_request(&self, url: &str) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .post(url)
            .header("content-type", "application/json");
        match self.auth_mode {
            GeminiAuthMode::ApiKey => builder,
            GeminiAuthMode::Bearer => {
                builder.header("Authorization", format!("Bearer {}", self.api_key))
            }
        }
    }

    /// Process a parsed SSE event and send the appropriate `StreamEvent` on the channel.
    async fn process_stream_chunk(
        data: &Value,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<Option<TokenUsage>, LlmError> {
        let candidates = match data["candidates"].as_array() {
            Some(c) => c,
            None => return Ok(None),
        };

        if candidates.is_empty() {
            return Ok(None);
        }

        let candidate = &candidates[0];
        if let Some(parts) = candidate["content"]["parts"].as_array() {
            for part in parts {
                if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        let _ = tx.send(StreamEvent::Token(text.to_string())).await;
                    }
                } else if let Some(fc) = part.get("functionCall") {
                    let name = fc["name"].as_str().unwrap_or("").to_string();
                    let id = format!("gemini_call_{}", uuid::Uuid::new_v4());
                    let args = fc["args"].to_string();

                    // Preserve the raw functionCall JSON (includes thought_signature)
                    let _ = tx
                        .send(StreamEvent::ToolCallStart {
                            id: id.clone(),
                            name,
                            raw_function_call: Some(part.clone()),
                        })
                        .await;
                    let _ = tx
                        .send(StreamEvent::ToolCallDelta {
                            id: id.clone(),
                            arguments_delta: args,
                        })
                        .await;
                    let _ = tx.send(StreamEvent::ToolCallEnd { id }).await;
                }
            }
        }

        // Extract usage metadata if present.
        let usage_metadata = &data["usageMetadata"];
        if usage_metadata.is_object() {
            let usage = TokenUsage {
                input_tokens: usage_metadata["promptTokenCount"].as_u64().unwrap_or(0) as usize,
                output_tokens: usage_metadata["candidatesTokenCount"].as_u64().unwrap_or(0)
                    as usize,
                cache_read_tokens: usage_metadata["cachedContentTokenCount"]
                    .as_u64()
                    .unwrap_or(0) as usize,
                cache_creation_tokens: 0,
            };
            return Ok(Some(usage));
        }

        Ok(None)
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    /// Perform a full (non-streaming) completion via the Gemini API.
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let model = request.model.as_deref().unwrap_or(&self.model);
        let body = self.build_request_body(&request, false);
        let url = self.endpoint_url(model, "generateContent");

        debug!(
            model = self.model.as_str(),
            url = url.as_str(),
            "Sending Gemini completion request"
        );

        let response = self
            .build_authed_request(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Request to Gemini API failed: {e}"),
            })?;

        let status = response.status();
        let headers = response.headers().clone();
        let body_text = response.text().await.map_err(|e| LlmError::ResponseParse {
            message: format!("Failed to read response body: {e}"),
        })?;

        if !status.is_success() {
            return Err(Self::map_http_error(status, &body_text, Some(&headers)));
        }

        let response_json: Value =
            serde_json::from_str(&body_text).map_err(|e| LlmError::ResponseParse {
                message: format!("Invalid JSON in response: {e}"),
            })?;

        Self::parse_response(&response_json)
    }

    /// Perform a streaming completion via the Gemini API.
    ///
    /// Uses the `streamGenerateContent` endpoint with `?alt=sse`.
    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let model = request.model.as_deref().unwrap_or(&self.model);
        let body = self.build_request_body(&request, true);
        let url = match self.auth_mode {
            GeminiAuthMode::ApiKey => format!(
                "{}/models/{}:streamGenerateContent?alt=sse&key={}",
                self.base_url, model, self.api_key
            ),
            GeminiAuthMode::Bearer => format!(
                "{}/models/{}:streamGenerateContent?alt=sse",
                self.base_url, model
            ),
        };

        debug!(
            model = self.model.as_str(),
            "Sending Gemini streaming request"
        );

        let response = self
            .build_authed_request(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::ApiRequest {
                message: format!("Streaming request to Gemini API failed: {e}"),
            })?;

        let status = response.status();
        if !status.is_success() {
            let headers = response.headers().clone();
            let body_text = response.text().await.unwrap_or_default();
            return Err(Self::map_http_error(status, &body_text, Some(&headers)));
        }

        // Stream SSE events incrementally using bytes_stream.
        let mut byte_stream = response.bytes_stream();
        let mut total_usage = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            ..Default::default()
        };
        let mut line_buffer = String::new();

        while let Some(chunk_result) = byte_stream.next().await {
            let chunk = chunk_result.map_err(|e| LlmError::Streaming {
                message: format!("Failed to read streaming chunk: {e}"),
            })?;

            let chunk_str = String::from_utf8_lossy(&chunk);
            line_buffer.push_str(&chunk_str);

            // Process complete lines from the buffer.
            while let Some(newline_pos) = line_buffer.find('\n') {
                let line = line_buffer[..newline_pos].trim().to_string();
                line_buffer = line_buffer[newline_pos + 1..].to_string();

                if line.is_empty() || line.starts_with("event:") {
                    continue;
                }

                if let Some(data_str) = line.strip_prefix("data: ") {
                    match serde_json::from_str::<Value>(data_str) {
                        Ok(data_json) => match Self::process_stream_chunk(&data_json, &tx).await {
                            Ok(Some(usage)) => {
                                total_usage = usage;
                            }
                            Ok(None) => {}
                            Err(e) => {
                                warn!(error = %e, "Error processing Gemini stream chunk");
                                return Err(e);
                            }
                        },
                        Err(e) => {
                            let preview = if data_str.len() > 200 {
                                &data_str[..200]
                            } else {
                                data_str
                            };
                            warn!(
                                error = %e,
                                data_preview = preview,
                                "Failed to parse Gemini SSE JSON chunk"
                            );
                        }
                    }
                }
            }
        }

        // Process any remaining data in the buffer.
        let remaining = line_buffer.trim().to_string();
        if !remaining.is_empty()
            && let Some(data_str) = remaining.strip_prefix("data: ")
        {
            match serde_json::from_str::<Value>(data_str) {
                Ok(data_json) => {
                    if let Ok(Some(usage)) = Self::process_stream_chunk(&data_json, &tx).await {
                        total_usage = usage;
                    }
                }
                Err(e) => {
                    let preview = if data_str.len() > 200 {
                        &data_str[..200]
                    } else {
                        data_str
                    };
                    warn!(
                        error = %e,
                        data_preview = preview,
                        "Failed to parse final Gemini SSE JSON chunk"
                    );
                }
            }
        }

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

    /// Gemini models support function calling.
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

    /// Gemini supports caching via the CachedContent API.
    fn supports_caching(&self) -> bool {
        true
    }

    /// Gemini cache pricing: reads at 25% of input cost, no creation premium.
    fn cache_cost_per_token(&self) -> (f64, f64) {
        (self.cost_input * 0.25, self.cost_input)
    }

    fn supports_vision(&self) -> bool {
        // Gemini Pro and Flash models support vision
        self.model.contains("gemini-1.5") || self.model.contains("gemini-2")
    }

    fn supports_thinking(&self) -> bool {
        // Gemini 2.0+ models support thinking
        self.model.contains("gemini-2")
    }

    fn supports_structured_output(&self) -> bool {
        // Gemini 1.5+ supports JSON mode
        self.model.contains("gemini-1.5") || self.model.contains("gemini-2")
    }

    fn supports_code_execution(&self) -> bool {
        // Gemini 1.5+ supports code execution
        self.model.contains("gemini-1.5") || self.model.contains("gemini-2")
    }

    fn supports_grounding(&self) -> bool {
        // Gemini 1.5+ supports Google Search grounding
        self.model.contains("gemini-1.5") || self.model.contains("gemini-2")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a test config with a given env var name.
    fn test_config(api_key_env: &str) -> LlmConfig {
        LlmConfig {
            provider: "gemini".to_string(),
            model: "gemini-2.0-flash".to_string(),
            api_key_env: api_key_env.to_string(),
            base_url: None,
            max_tokens: 4096,
            temperature: 0.7,
            context_window: 1_000_000,
            input_cost_per_million: 0.075,
            output_cost_per_million: 0.30,
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
    fn make_provider() -> GeminiProvider {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var("GEMINI_TEST_KEY_UNIT", "test-gemini-key-12345") };
        let config = test_config("GEMINI_TEST_KEY_UNIT");
        GeminiProvider::new(&config).expect("Provider creation should succeed")
    }

    #[test]
    fn test_new_reads_env() {
        let env_var = "GEMINI_TEST_KEY_NEW_READS";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "my-gemini-api-key") };
        let config = test_config(env_var);
        let provider = GeminiProvider::new(&config).unwrap();
        assert_eq!(provider.api_key, "my-gemini-api-key");
        assert_eq!(provider.model, "gemini-2.0-flash");
        assert_eq!(provider.base_url, DEFAULT_BASE_URL);
        assert_eq!(provider.context_window, 1_000_000);
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }

    #[test]
    fn test_new_missing_env_returns_auth_failed() {
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var("GEMINI_MISSING_KEY_XYZ") };
        let config = test_config("GEMINI_MISSING_KEY_XYZ");
        let result = GeminiProvider::new(&config);
        assert!(result.is_err());
        let err = result.err().unwrap();
        match err {
            LlmError::AuthFailed { provider } => {
                assert!(provider.contains("GEMINI_MISSING_KEY_XYZ"));
            }
            other => panic!("Expected AuthFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_new_custom_base_url() {
        let env_var = "GEMINI_TEST_KEY_CUSTOM_URL";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "test-key") };
        let mut config = test_config(env_var);
        config.base_url = Some("https://my-proxy.example.com/v1".to_string());
        let provider = GeminiProvider::new(&config).unwrap();
        assert_eq!(provider.base_url, "https://my-proxy.example.com/v1");
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }

    #[test]
    fn test_new_with_key() {
        let config = test_config("UNUSED_ENV_VAR");
        let provider = GeminiProvider::new_with_key(&config, "explicit-key".to_string()).unwrap();
        assert_eq!(provider.api_key, "explicit-key");
    }

    #[test]
    fn test_system_instruction_extraction() {
        let messages = vec![
            Message::system("You are a helpful coding assistant."),
            Message::user("Hello!"),
            Message::assistant("Hi there!"),
        ];

        let (system_text, non_system) = GeminiProvider::extract_system_instruction(&messages);

        assert_eq!(
            system_text,
            Some("You are a helpful coding assistant.".to_string())
        );
        assert_eq!(non_system.len(), 2);
        assert_eq!(non_system[0].role, Role::User);
        assert_eq!(non_system[1].role, Role::Assistant);
    }

    #[test]
    fn test_system_instruction_extraction_multiple() {
        let messages = vec![
            Message::system("First instruction."),
            Message::system("Second instruction."),
            Message::user("Hello!"),
        ];

        let (system_text, non_system) = GeminiProvider::extract_system_instruction(&messages);

        assert_eq!(
            system_text,
            Some("First instruction.\n\nSecond instruction.".to_string())
        );
        assert_eq!(non_system.len(), 1);
    }

    #[test]
    fn test_system_instruction_extraction_none() {
        let messages = vec![Message::user("Hello!"), Message::assistant("Hi!")];

        let (system_text, non_system) = GeminiProvider::extract_system_instruction(&messages);

        assert!(system_text.is_none());
        assert_eq!(non_system.len(), 2);
    }

    #[test]
    fn test_message_to_gemini_json_user() {
        let msg = Message::user("What is Rust?");
        let json = GeminiProvider::message_to_gemini_json(&msg);

        assert_eq!(json["role"], "user");
        let parts = json["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["text"], "What is Rust?");
    }

    #[test]
    fn test_message_to_gemini_json_assistant() {
        let msg = Message::assistant("Rust is a systems programming language.");
        let json = GeminiProvider::message_to_gemini_json(&msg);

        assert_eq!(json["role"], "model");
        let parts = json["parts"].as_array().unwrap();
        assert_eq!(parts[0]["text"], "Rust is a systems programming language.");
    }

    #[test]
    fn test_message_to_gemini_json_tool_call() {
        let msg = Message::new(
            Role::Assistant,
            Content::tool_call(
                "call_01abc",
                "file_read",
                serde_json::json!({"path": "/src/main.rs"}),
            ),
        );
        let json = GeminiProvider::message_to_gemini_json(&msg);

        assert_eq!(json["role"], "model");
        let parts = json["parts"].as_array().unwrap();
        assert!(parts[0].get("functionCall").is_some());
        assert_eq!(parts[0]["functionCall"]["name"], "file_read");
        assert_eq!(parts[0]["functionCall"]["args"]["path"], "/src/main.rs");
    }

    #[test]
    fn test_message_to_gemini_json_tool_result() {
        let msg = Message::tool_result("call_01abc", "fn main() { }", false);
        let json = GeminiProvider::message_to_gemini_json(&msg);

        // Tool results are sent as user role in Gemini API.
        assert_eq!(json["role"], "user");
        let parts = json["parts"].as_array().unwrap();
        assert!(parts[0].get("functionResponse").is_some());
    }

    #[test]
    fn test_parse_text_response() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello! How can I help you today?"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 25,
                "candidatesTokenCount": 10,
                "totalTokenCount": 35
            },
            "modelVersion": "gemini-2.0-flash"
        });

        let result = GeminiProvider::parse_response(&response_json).unwrap();
        assert_eq!(
            result.message.content.as_text(),
            Some("Hello! How can I help you today?")
        );
        assert_eq!(result.model, "gemini-2.0-flash");
        assert_eq!(result.usage.input_tokens, 25);
        assert_eq!(result.usage.output_tokens, 10);
        assert_eq!(result.finish_reason, Some("STOP".to_string()));
        assert_eq!(result.message.role, Role::Assistant);
    }

    #[test]
    fn test_parse_function_call_response() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "file_read",
                            "args": {"path": "/src/main.rs"}
                        }
                    }],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 50,
                "candidatesTokenCount": 30,
                "totalTokenCount": 80
            }
        });

        let result = GeminiProvider::parse_response(&response_json).unwrap();
        match &result.message.content {
            Content::ToolCall {
                name, arguments, ..
            } => {
                assert_eq!(name, "file_read");
                assert_eq!(arguments["path"], "/src/main.rs");
            }
            _ => panic!("Expected ToolCall content"),
        }
    }

    #[test]
    fn test_parse_multipart_response() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "I'll read that file for you."},
                        {
                            "functionCall": {
                                "name": "file_read",
                                "args": {"path": "/src/main.rs"}
                            }
                        }
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 50,
                "candidatesTokenCount": 30,
                "totalTokenCount": 80
            }
        });

        let result = GeminiProvider::parse_response(&response_json).unwrap();
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
                        name, arguments, ..
                    } => {
                        assert_eq!(name, "file_read");
                        assert_eq!(arguments["path"], "/src/main.rs");
                    }
                    _ => panic!("Expected ToolCall part"),
                }
            }
            _ => panic!("Expected MultiPart content"),
        }
    }

    #[test]
    fn test_parse_empty_candidates() {
        let response_json = serde_json::json!({
            "candidates": [],
            "usageMetadata": {}
        });

        let result = GeminiProvider::parse_response(&response_json);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ResponseParse { message } => {
                assert!(message.contains("Empty"));
            }
            other => panic!("Expected ResponseParse, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_missing_candidates() {
        let response_json = serde_json::json!({"error": "bad request"});
        let result = GeminiProvider::parse_response(&response_json);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ResponseParse { message } => {
                assert!(message.contains("candidates"));
            }
            other => panic!("Expected ResponseParse, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_empty_parts() {
        let response_json = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        });

        let result = GeminiProvider::parse_response(&response_json).unwrap();
        assert_eq!(result.message.content.as_text(), Some(""));
    }

    #[test]
    fn test_http_error_mapping() {
        // 401 -> AuthFailed
        let err = GeminiProvider::map_http_error(
            reqwest::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"Invalid API key"}}"#,
            None,
        );
        match err {
            LlmError::AuthFailed { provider } => {
                assert_eq!(provider, "Gemini");
            }
            _ => panic!("Expected AuthFailed, got {err:?}"),
        }

        // 403 -> AuthFailed
        let err = GeminiProvider::map_http_error(
            reqwest::StatusCode::FORBIDDEN,
            r#"{"error":{"message":"Forbidden"}}"#,
            None,
        );
        assert!(matches!(err, LlmError::AuthFailed { .. }));

        // 429 -> RateLimited (no headers → falls back to 30s default)
        let err = GeminiProvider::map_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Rate limited"}}"#,
            None,
        );
        match err {
            LlmError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 30);
            }
            _ => panic!("Expected RateLimited, got {err:?}"),
        }

        // 429 with Retry-After header → parses actual value
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::RETRY_AFTER, "5".parse().unwrap());
        let err = GeminiProvider::map_http_error(
            reqwest::StatusCode::TOO_MANY_REQUESTS,
            r#"{"error":{"message":"Rate limited"}}"#,
            Some(&headers),
        );
        match err {
            LlmError::RateLimited { retry_after_secs } => {
                assert_eq!(retry_after_secs, 5);
            }
            _ => panic!("Expected RateLimited with 5s, got {err:?}"),
        }

        // 500 -> ApiRequest
        let err = GeminiProvider::map_http_error(
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":{"message":"Internal server error"}}"#,
            None,
        );
        match err {
            LlmError::ApiRequest { message } => {
                assert!(message.contains("500"));
            }
            _ => panic!("Expected ApiRequest, got {err:?}"),
        }
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

        assert_eq!(body["generationConfig"]["maxOutputTokens"], 1024);
        assert_eq!(body["generationConfig"]["temperature"], 0.5);
        assert_eq!(
            body["system_instruction"]["parts"][0]["text"],
            "You are a helpful assistant."
        );

        let contents = body["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1); // System message extracted, only user remains.
        assert_eq!(contents[0]["role"], "user");
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
        let func_decls = tools_json[0]["functionDeclarations"].as_array().unwrap();
        assert_eq!(func_decls.len(), 1);
        assert_eq!(func_decls[0]["name"], "file_read");
        assert_eq!(func_decls[0]["description"], "Read a file from disk");

        // Verify toolConfig restricts Gemini to only declared functions
        let tool_config = &body["toolConfig"];
        assert_eq!(
            tool_config["functionCallingConfig"]["mode"], "AUTO",
            "toolConfig should use AUTO mode"
        );
        let allowed = tool_config["functionCallingConfig"]["allowedFunctionNames"]
            .as_array()
            .expect("allowedFunctionNames should be an array");
        assert_eq!(allowed.len(), 1);
        assert_eq!(allowed[0], "file_read");
    }

    #[test]
    fn test_build_request_body_allowed_function_names_multiple_tools() {
        let provider = make_provider();

        let tools = vec![
            ToolDefinition {
                name: "arxiv_research".to_string(),
                description: "Search arXiv papers".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
            ToolDefinition {
                name: "web_fetch".to_string(),
                description: "Fetch a URL".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
            ToolDefinition {
                name: "file_read".to_string(),
                description: "Read a file".to_string(),
                parameters: serde_json::json!({"type": "object", "properties": {}}),
            },
        ];

        let request = CompletionRequest {
            messages: vec![Message::user("Do something")],
            tools: Some(tools),
            ..Default::default()
        };

        let body = provider.build_request_body(&request, false);
        let allowed = body["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"]
            .as_array()
            .expect("allowedFunctionNames should be present");
        assert_eq!(allowed.len(), 3);
        let names: Vec<&str> = allowed.iter().filter_map(|v| v.as_str()).collect();
        assert!(names.contains(&"arxiv_research"));
        assert!(names.contains(&"web_fetch"));
        assert!(names.contains(&"file_read"));
        // knowledge_graph is NOT in the list (not declared)
        assert!(!names.contains(&"knowledge_graph"));
    }

    #[test]
    fn test_build_request_body_no_tool_config_without_tools() {
        let provider = make_provider();

        let request = CompletionRequest {
            messages: vec![Message::user("Hello")],
            tools: None,
            ..Default::default()
        };

        let body = provider.build_request_body(&request, false);
        assert!(
            body.get("toolConfig").is_none() || body["toolConfig"].is_null(),
            "toolConfig should not be present when no tools are declared"
        );
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
        let stop = body["generationConfig"]["stopSequences"]
            .as_array()
            .unwrap();
        assert_eq!(stop.len(), 2);
        assert_eq!(stop[0], "STOP");
        assert_eq!(stop[1], "END");
    }

    #[test]
    fn test_endpoint_url() {
        let provider = make_provider();
        let url = provider.endpoint_url("gemini-2.0-flash", "generateContent");
        assert!(url.contains("gemini-2.0-flash"));
        assert!(url.contains("generateContent"));
        assert!(url.contains("key="));
    }

    #[test]
    fn test_provider_properties() {
        let provider = make_provider();

        assert_eq!(provider.model_name(), "gemini-2.0-flash");
        assert_eq!(provider.context_window(), 1_000_000);
        assert!(provider.supports_tools());

        let (input_cost, output_cost) = provider.cost_per_token();
        // model_pricing returns $0.10/$0.40 for gemini-2.0-flash
        assert!((input_cost - 0.10 / 1_000_000.0).abs() < 1e-12);
        assert!((output_cost - 0.40 / 1_000_000.0).abs() < 1e-12);
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

        let json = GeminiProvider::tool_definition_to_json(&tool);
        assert_eq!(json["name"], "shell_exec");
        assert_eq!(json["description"], "Execute a shell command");
        assert_eq!(json["parameters"]["type"], "object");
    }

    #[test]
    fn test_sanitize_schema_strips_unsupported_fields() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["get", "post"],
                    "description": "HTTP method"
                },
                "headers": {
                    "type": "object",
                    "description": "Custom headers",
                    "additionalProperties": { "type": "string" }
                },
                "count": {
                    "type": "integer",
                    "default": 5,
                    "description": "Number of results"
                }
            },
            "required": ["action"],
            "additionalProperties": false
        });

        let sanitized = GeminiProvider::sanitize_schema(&schema);

        // Allowed fields are preserved
        assert_eq!(sanitized["type"], "object");
        assert!(sanitized["required"].is_array());
        assert_eq!(sanitized["properties"]["action"]["type"], "string");
        assert_eq!(sanitized["properties"]["action"]["enum"][0], "get");
        assert_eq!(
            sanitized["properties"]["action"]["description"],
            "HTTP method"
        );

        // Unsupported fields are removed
        assert!(sanitized.get("additionalProperties").is_none());
        assert!(
            sanitized["properties"]["headers"]
                .get("additionalProperties")
                .is_none()
        );
        assert!(sanitized["properties"]["count"].get("default").is_none());

        // But supported fields within those properties still exist
        assert_eq!(sanitized["properties"]["headers"]["type"], "object");
        assert_eq!(sanitized["properties"]["count"]["type"], "integer");
        assert_eq!(
            sanitized["properties"]["count"]["description"],
            "Number of results"
        );
    }

    #[test]
    fn test_content_to_gemini_parts_multipart() {
        let content = Content::MultiPart {
            parts: vec![
                Content::text("Here is the file:"),
                Content::tool_call(
                    "call_01multi",
                    "file_read",
                    serde_json::json!({"path": "/tmp/test.rs"}),
                ),
            ],
        };

        let json = GeminiProvider::content_to_gemini_parts(&content);
        let parts = json.as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "Here is the file:");
        assert!(parts[1].get("functionCall").is_some());
        assert_eq!(parts[1]["functionCall"]["name"], "file_read");
    }

    #[tokio::test]
    async fn test_process_stream_chunk_text() {
        let (tx, mut rx) = mpsc::channel(32);

        let data = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello world"}],
                    "role": "model"
                }
            }]
        });

        let result = GeminiProvider::process_stream_chunk(&data, &tx).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::Token(text) => assert_eq!(text, "Hello world"),
            _ => panic!("Expected Token event"),
        }
    }

    #[tokio::test]
    async fn test_process_stream_chunk_function_call() {
        let (tx, mut rx) = mpsc::channel(32);

        let data = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{
                        "functionCall": {
                            "name": "file_read",
                            "args": {"path": "/src/main.rs"}
                        }
                    }],
                    "role": "model"
                }
            }]
        });

        let result = GeminiProvider::process_stream_chunk(&data, &tx).await;
        assert!(result.is_ok());

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ToolCallStart { name, .. } => assert_eq!(name, "file_read"),
            _ => panic!("Expected ToolCallStart event"),
        }

        let event = rx.recv().await.unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                arguments_delta, ..
            } => {
                assert!(arguments_delta.contains("main.rs"));
            }
            _ => panic!("Expected ToolCallDelta event"),
        }

        let event = rx.recv().await.unwrap();
        assert!(matches!(event, StreamEvent::ToolCallEnd { .. }));
    }

    #[tokio::test]
    async fn test_process_stream_chunk_with_usage() {
        let (tx, _rx) = mpsc::channel(32);

        let data = serde_json::json!({
            "candidates": [{
                "content": {
                    "parts": [{"text": "Done"}],
                    "role": "model"
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 42,
                "totalTokenCount": 142
            }
        });

        let result = GeminiProvider::process_stream_chunk(&data, &tx)
            .await
            .unwrap();
        assert!(result.is_some());
        let usage = result.unwrap();
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 42);
    }

    #[test]
    fn test_fix_gemini_turns_merges_consecutive_user() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": [{"text": "Hello"}]}),
            serde_json::json!({"role": "user", "parts": [{"text": "World"}]}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        assert_eq!(fixed.len(), 1);
        let parts = fixed[0]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["text"], "Hello");
        assert_eq!(parts[1]["text"], "World");
    }

    #[test]
    fn test_fix_gemini_turns_fixes_function_response_name() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": [{"text": "Read the file"}]}),
            serde_json::json!({"role": "model", "parts": [{"functionCall": {"name": "file_read", "args": {"path": "/tmp/test"}}}]}),
            serde_json::json!({"role": "user", "parts": [{"functionResponse": {"name": "tool", "response": {"result": "contents"}}}]}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        assert_eq!(fixed.len(), 3);
        // The functionResponse name should be fixed to match the functionCall name
        assert_eq!(
            fixed[2]["parts"][0]["functionResponse"]["name"],
            "file_read"
        );
    }

    #[test]
    fn test_fix_gemini_turns_multi_tool_call() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": [{"text": "Read two files"}]}),
            serde_json::json!({"role": "model", "parts": [
                {"functionCall": {"name": "file_read", "args": {"path": "/a"}}},
                {"functionCall": {"name": "file_write", "args": {"path": "/b", "content": "x"}}}
            ]}),
            // Two separate tool result turns that should get merged
            serde_json::json!({"role": "user", "parts": [{"functionResponse": {"name": "tool", "response": {"result": "aaa"}}}]}),
            serde_json::json!({"role": "user", "parts": [{"functionResponse": {"name": "tool", "response": {"result": "bbb"}}}]}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        // The two user tool-result turns should be merged into one
        assert_eq!(fixed.len(), 3);
        let parts = fixed[2]["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 2);
        // Names should be fixed to match the functionCalls
        assert_eq!(parts[0]["functionResponse"]["name"], "file_read");
        assert_eq!(parts[1]["functionResponse"]["name"], "file_write");
    }

    #[test]
    fn test_fix_gemini_turns_prepends_user() {
        let contents =
            vec![serde_json::json!({"role": "model", "parts": [{"text": "Hello from model"}]})];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0]["role"], "user");
        assert_eq!(fixed[0]["parts"][0]["text"], "Hello");
        assert_eq!(fixed[1]["role"], "model");
    }

    #[test]
    fn test_fix_gemini_turns_no_op() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": [{"text": "Hello"}]}),
            serde_json::json!({"role": "model", "parts": [{"text": "Hi there"}]}),
            serde_json::json!({"role": "user", "parts": [{"text": "How are you?"}]}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents.clone());
        assert_eq!(fixed.len(), 3);
        assert_eq!(fixed[0]["role"], "user");
        assert_eq!(fixed[1]["role"], "model");
        assert_eq!(fixed[2]["role"], "user");
    }

    #[test]
    fn test_fix_gemini_turns_empty_parts_filtered() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": [{"text": "Hello"}]}),
            serde_json::json!({"role": "model", "parts": []}),
            serde_json::json!({"role": "user", "parts": [{"text": "Still here"}]}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        // The empty-parts model turn should be removed
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0]["role"], "user");
        assert_eq!(fixed[1]["role"], "user");
    }

    #[test]
    fn test_fix_gemini_turns_all_empty_parts() {
        let contents = vec![
            serde_json::json!({"role": "user", "parts": []}),
            serde_json::json!({"role": "model", "parts": []}),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        // All turns have empty parts — result should be empty
        assert!(fixed.is_empty());
    }

    #[test]
    fn test_fix_gemini_turns_removes_orphaned_function_response() {
        let contents = vec![
            serde_json::json!({
                "role": "user",
                "parts": [{"text": "Hello"}]
            }),
            serde_json::json!({
                "role": "model",
                "parts": [{"text": "Hi there"}]
            }),
            // Orphaned functionResponse — no matching functionCall in model turn
            serde_json::json!({
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "name": "nonexistent_tool",
                        "response": {"result": "orphan"}
                    }
                }]
            }),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        // The orphaned functionResponse turn should be removed (empty parts → filtered)
        assert_eq!(fixed.len(), 2);
        assert_eq!(fixed[0]["role"], "user");
        assert_eq!(fixed[1]["role"], "model");
    }

    #[test]
    fn test_fix_gemini_turns_preserves_valid_function_response() {
        let contents = vec![
            serde_json::json!({
                "role": "user",
                "parts": [{"text": "Read file"}]
            }),
            serde_json::json!({
                "role": "model",
                "parts": [{
                    "functionCall": {
                        "name": "file_read",
                        "args": {"path": "main.rs"}
                    }
                }]
            }),
            serde_json::json!({
                "role": "user",
                "parts": [{
                    "functionResponse": {
                        "name": "file_read",
                        "response": {"result": "fn main(){}"}
                    }
                }]
            }),
        ];
        let fixed = GeminiProvider::fix_gemini_turns(contents);
        assert_eq!(fixed.len(), 3);
        // functionResponse should still be present
        assert!(fixed[2]["parts"][0].get("functionResponse").is_some());
    }

    #[test]
    fn test_provider_has_timeout() {
        // Verify provider creates successfully with timeout-enabled client
        let env_var = "GEMINI_TEST_KEY_TIMEOUT";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "test-key-timeout") };
        let config = test_config(env_var);
        let provider = GeminiProvider::new(&config);
        assert!(
            provider.is_ok(),
            "Provider with timeout should create successfully"
        );
        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }

    #[test]
    fn test_cached_content_name_initially_none() {
        let provider = make_provider();
        assert!(
            provider.cached_content_name.is_none(),
            "cached_content_name should be None initially"
        );
    }

    #[tokio::test]
    async fn test_create_cached_content_connection_error() {
        // We cannot call the real API in unit tests, but we can verify the method
        // exists and handles error responses correctly by pointing at a non-routable
        // server. This validates the request construction and error mapping path.
        let env_var = "GEMINI_TEST_KEY_CACHED_CONTENT";
        // SAFETY: test-only env var manipulation
        unsafe { std::env::set_var(env_var, "test-gemini-cache-key") };
        let mut config = test_config(env_var);
        // Point to a non-routable address so the request fails fast
        config.base_url = Some("http://127.0.0.1:1".to_string());
        let mut provider = GeminiProvider::new(&config).unwrap();

        let tools = vec![serde_json::json!({
            "name": "file_read",
            "description": "Read a file",
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}
        })];

        let result = provider
            .create_cached_content("You are a helpful assistant.", &tools, 3600)
            .await;

        // Should fail with a connection error since we are hitting a non-routable address
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            LlmError::ApiRequest { message } => {
                assert!(
                    message.contains("Cached content creation request"),
                    "Expected cached content error, got: {message}"
                );
            }
            other => panic!("Expected ApiRequest error, got {other:?}"),
        }

        // cached_content_name should remain None on failure
        assert!(provider.cached_content_name.is_none());

        // SAFETY: test-only env var manipulation
        unsafe { std::env::remove_var(env_var) };
    }
}
