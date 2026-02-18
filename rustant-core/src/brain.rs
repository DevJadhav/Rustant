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
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

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

    /// Whether this provider supports prompt caching.
    fn supports_caching(&self) -> bool {
        false
    }

    /// Cost per token for cache operations: (cache_read_per_token, cache_write_per_token).
    /// Cache reads are discounted; cache writes may have a premium.
    fn cache_cost_per_token(&self) -> (f64, f64) {
        let (input, _) = self.cost_per_token();
        // Default: cache reads at 10% of input cost, cache writes at 125% of input cost
        (input * 0.1, input * 1.25)
    }

    // --- Capability discovery (default: all false) ---

    /// Whether this provider supports vision/image inputs.
    fn supports_vision(&self) -> bool {
        false
    }

    /// Whether this provider supports extended thinking / chain-of-thought.
    fn supports_thinking(&self) -> bool {
        false
    }

    /// Whether this provider supports structured output (JSON schema).
    fn supports_structured_output(&self) -> bool {
        false
    }

    /// Whether this provider supports inline citations.
    fn supports_citations(&self) -> bool {
        false
    }

    /// Whether this provider supports code execution sandbox.
    fn supports_code_execution(&self) -> bool {
        false
    }

    /// Whether this provider supports web grounding / search.
    fn supports_grounding(&self) -> bool {
        false
    }

    /// Exact token count via provider API (e.g., Anthropic count_tokens endpoint).
    /// Returns `None` if not supported.
    async fn count_tokens_exact(&self, _request: &CompletionRequest) -> Option<usize> {
        None
    }
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

    /// Estimate the token count for a set of tool definitions.
    ///
    /// Each tool definition adds overhead for the JSON schema structure
    /// (type/function wrapper), plus the name, description, and parameters.
    pub fn count_tool_definitions(&self, tools: &[ToolDefinition]) -> usize {
        let mut total = 0;
        for tool in tools {
            total += 10; // struct overhead (type, function wrapper, required fields)
            total += self.count(&tool.name);
            total += self.count(&tool.description);
            total += self.count(&tool.parameters.to_string());
        }
        total
    }

    /// Estimate the token count for a set of messages.
    /// Adds overhead for message structure (role, separators).
    pub fn count_messages(&self, messages: &[Message]) -> usize {
        let mut total = 0;
        for msg in messages {
            // Each message has overhead: role token + separators (~4 tokens)
            total += 4;
            total += self.count_content(&msg.content);
        }
        total + 3 // reply priming overhead
    }

    /// Estimate the token count for a single Content value.
    fn count_content(&self, content: &Content) -> usize {
        match content {
            Content::Text { text } => self.count(text),
            Content::ToolCall {
                name, arguments, ..
            } => self.count(name) + self.count(&arguments.to_string()),
            Content::ToolResult { output, .. } => self.count(output),
            Content::MultiPart { parts } => parts.iter().map(|p| self.count_content(p)).sum(),
            Content::Image { .. } => 85, // vision tokens approximation (low detail)
            Content::Thinking { thinking, .. } => self.count(thinking),
            Content::Citation { cited_text, .. } => self.count(cited_text),
            Content::CodeExecution {
                code,
                output,
                error,
                ..
            } => {
                self.count(code)
                    + output.as_deref().map_or(0, |s| self.count(s))
                    + error.as_deref().map_or(0, |s| self.count(s))
            }
            Content::SearchResult { query, results } => {
                self.count(query)
                    + results
                        .iter()
                        .map(|r| self.count(&r.title) + self.count(&r.snippet))
                        .sum::<usize>()
            }
        }
    }
}

/// Sanitize tool_call → tool_result ordering in a message sequence.
///
/// This runs provider-agnostically *before* messages are sent to any LLM provider,
/// ensuring that:
/// 1. Every tool_result has a matching tool_call earlier in the sequence.
/// 2. No non-tool messages (system hints, summaries) appear between an assistant's
///    tool_call message and its corresponding user/tool tool_result message.
/// 3. Orphaned tool_results (no matching tool_call) are removed.
///
/// This fixes issues caused by compression moving pinned tool_results out of order,
/// system routing hints persisting between call/result, and summary injection.
pub fn sanitize_tool_sequence(messages: &mut Vec<Message>) {
    // --- Pass 1: Collect all tool_call IDs from assistant messages ---
    let mut tool_call_ids: HashSet<String> = HashSet::new();
    for msg in messages.iter() {
        if msg.role != Role::Assistant {
            continue;
        }
        collect_tool_call_ids(&msg.content, &mut tool_call_ids);
    }

    // --- Pass 2: Remove orphaned tool_results (no matching tool_call) ---
    messages.retain(|msg| {
        if msg.role != Role::Tool {
            // Check user messages too — Anthropic sends tool_result as user role
            if msg.role == Role::User
                && let Content::ToolResult { call_id, .. } = &msg.content
                && !tool_call_ids.contains(call_id)
            {
                warn!(
                    call_id = call_id.as_str(),
                    "Removing orphaned tool_result (no matching tool_call)"
                );
                return false;
            }
            return true;
        }
        match &msg.content {
            Content::ToolResult { call_id, .. } => {
                if tool_call_ids.contains(call_id) {
                    true
                } else {
                    warn!(
                        call_id = call_id.as_str(),
                        "Removing orphaned tool_result (no matching tool_call)"
                    );
                    false
                }
            }
            Content::MultiPart { parts } => {
                // Keep the message if at least one tool_result has a matching call
                let has_valid = parts.iter().any(|p| {
                    if let Content::ToolResult { call_id, .. } = p {
                        tool_call_ids.contains(call_id)
                    } else {
                        true
                    }
                });
                if !has_valid {
                    warn!("Removing multipart tool message with all orphaned tool_results");
                }
                has_valid
            }
            _ => true,
        }
    });

    // --- Pass 3: Relocate system messages that appear between tool_call and tool_result ---
    // Strategy: find each assistant message with tool_call(s), then check if the
    // immediately following message is a system message. If so, move the system message
    // before the assistant message.
    let mut i = 0;
    while i + 1 < messages.len() {
        let has_tool_call =
            messages[i].role == Role::Assistant && content_has_tool_call(&messages[i].content);

        if has_tool_call {
            // Check if next message is a system message (should be tool_result instead)
            let mut j = i + 1;
            let mut system_messages_to_relocate = Vec::new();
            while j < messages.len() && messages[j].role == Role::System {
                system_messages_to_relocate.push(j);
                j += 1;
            }
            // Move system messages before the assistant tool_call message
            if !system_messages_to_relocate.is_empty() {
                // Extract system messages in reverse order to maintain relative order
                let mut extracted: Vec<Message> = Vec::new();
                for &idx in system_messages_to_relocate.iter().rev() {
                    extracted.push(messages.remove(idx));
                }
                extracted.reverse();
                // Insert them before position i
                for (offset, msg) in extracted.into_iter().enumerate() {
                    messages.insert(i + offset, msg);
                    i += 1; // adjust i to still point to the assistant message
                }
            }
        }
        i += 1;
    }
}

/// Extract all tool_call IDs from a Content value into the given set.
fn collect_tool_call_ids(content: &Content, ids: &mut HashSet<String>) {
    match content {
        Content::ToolCall { id, .. } => {
            ids.insert(id.clone());
        }
        Content::MultiPart { parts } => {
            for part in parts {
                collect_tool_call_ids(part, ids);
            }
        }
        _ => {}
    }
}

/// Check whether a Content value contains at least one tool_call.
fn content_has_tool_call(content: &Content) -> bool {
    match content {
        Content::ToolCall { .. } => true,
        Content::MultiPart { parts } => parts.iter().any(content_has_tool_call),
        _ => false,
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
    /// Optional knowledge addendum appended to system prompt from distilled rules.
    knowledge_addendum: String,
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
            knowledge_addendum: String::new(),
        }
    }

    /// Set knowledge addendum (distilled rules) to append to the system prompt.
    pub fn set_knowledge_addendum(&mut self, addendum: String) {
        self.knowledge_addendum = addendum;
    }

    /// Estimate token count for messages using tiktoken-rs.
    pub fn estimate_tokens(&self, messages: &[Message]) -> usize {
        self.token_counter.count_messages(messages)
    }

    /// Estimate token count for messages plus tool definitions.
    pub fn estimate_tokens_with_tools(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> usize {
        let mut total = self.token_counter.count_messages(messages);
        if let Some(tool_defs) = tools {
            total += self.token_counter.count_tool_definitions(tool_defs);
        }
        total
    }

    /// Construct messages for the LLM with system prompt prepended.
    ///
    /// If a knowledge addendum has been set via `set_knowledge_addendum()`,
    /// it is automatically appended to the system prompt.
    ///
    /// After assembly, [`sanitize_tool_sequence`] runs to ensure tool_call→tool_result
    /// ordering is never broken regardless of compression, pinning, or system message injection.
    pub fn build_messages(&self, conversation: &[Message]) -> Vec<Message> {
        let mut messages = Vec::with_capacity(conversation.len() + 1);
        if self.knowledge_addendum.is_empty() {
            messages.push(Message::system(&self.system_prompt));
        } else {
            let augmented = format!("{}{}", self.system_prompt, self.knowledge_addendum);
            messages.push(Message::system(&augmented));
        }
        messages.extend_from_slice(conversation);
        sanitize_tool_sequence(&mut messages);
        messages
    }

    /// Send a completion request and return the response, tracking usage.
    pub async fn think(
        &mut self,
        conversation: &[Message],
        tools: Option<Vec<ToolDefinition>>,
    ) -> Result<CompletionResponse, LlmError> {
        let messages = self.build_messages(conversation);
        let mut token_estimate = self.provider.estimate_tokens(&messages);
        if let Some(ref tool_defs) = tools {
            token_estimate += self.token_counter.count_tool_definitions(tool_defs);
        }
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
            cache_hint: crate::cache::CacheHint {
                enable_prompt_cache: self.provider.supports_caching(),
                cached_content_ref: None,
            },
            ..Default::default()
        };

        let response = self.provider.complete(request).await?;

        // Track usage (cache-aware)
        self.track_usage(&response.usage);

        info!(
            input_tokens = response.usage.input_tokens,
            output_tokens = response.usage.output_tokens,
            cost = format!("${:.4}", self.total_cost.total()),
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
    pub fn is_retryable(error: &LlmError) -> bool {
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
            cache_hint: crate::cache::CacheHint {
                enable_prompt_cache: self.provider.supports_caching(),
                cached_content_ref: None,
            },
            ..Default::default()
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

    /// Get cost rates (input_per_token, output_per_token) from the provider.
    pub fn provider_cost_rates(&self) -> (f64, f64) {
        self.provider.cost_per_token()
    }

    /// Get a reference to the underlying LLM provider.
    pub fn provider(&self) -> &dyn LlmProvider {
        &*self.provider
    }

    /// Get a cloneable Arc handle to the underlying LLM provider.
    pub fn provider_arc(&self) -> Arc<dyn LlmProvider> {
        Arc::clone(&self.provider)
    }

    /// Track usage and cost from an external completion (e.g., streaming).
    pub fn track_usage(&mut self, usage: &TokenUsage) {
        self.total_usage.accumulate(usage);
        let (input_rate, output_rate) = self.provider.cost_per_token();
        let (cache_read_rate, cache_write_rate) = self.provider.cache_cost_per_token();

        // Cache read tokens are charged at the discounted rate instead of full input rate.
        // Cache creation tokens are charged at the premium rate.
        // Savings = (full_price - discounted_price) for cache read tokens.
        let cache_read_cost = usage.cache_read_tokens as f64 * cache_read_rate;
        let cache_creation_cost = usage.cache_creation_tokens as f64 * cache_write_rate;
        let cache_savings = usage.cache_read_tokens as f64 * (input_rate - cache_read_rate);

        let cost = CostEstimate {
            input_cost: usage.input_tokens as f64 * input_rate
                + cache_read_cost
                + cache_creation_cost,
            output_cost: usage.output_tokens as f64 * output_rate,
            cache_savings,
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

    /// Create a MockLlmProvider that always returns the given text.
    ///
    /// Queues multiple copies of the response so it can handle multiple calls.
    pub fn with_response(text: &str) -> Self {
        let provider = Self::new();
        for _ in 0..20 {
            provider.queue_response(Self::text_response(text));
        }
        provider
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
                ..Default::default()
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
                ..Default::default()
            },
            model: "mock-model".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        }
    }

    /// Create a multipart response (text + tool call) for testing.
    pub fn multipart_response(
        text: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> CompletionResponse {
        let call_id = format!("call_{}", uuid::Uuid::new_v4());
        CompletionResponse {
            message: Message::new(
                Role::Assistant,
                Content::MultiPart {
                    parts: vec![
                        Content::text(text),
                        Content::tool_call(&call_id, tool_name, arguments),
                    ],
                },
            ),
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 50,
                ..Default::default()
            },
            model: "mock-model".to_string(),
            finish_reason: Some("tool_calls".to_string()),
        }
    }
}

/// Rough token estimate for mock provider (~4 chars per token).
fn estimate_content_tokens_mock(content: &Content) -> usize {
    match content {
        Content::Text { text } => text.len() / 4,
        Content::ToolCall { arguments, .. } => arguments.to_string().len() / 4,
        Content::ToolResult { output, .. } => output.len() / 4,
        Content::MultiPart { parts } => parts.iter().map(estimate_content_tokens_mock).sum(),
        Content::Image { .. } => 85,
        Content::Thinking { thinking, .. } => thinking.len() / 4,
        Content::Citation { cited_text, .. } => cited_text.len() / 4,
        Content::CodeExecution { code, .. } => code.len() / 4,
        Content::SearchResult { results, .. } => results
            .iter()
            .map(|r| (r.snippet.len() + r.title.len()) / 4)
            .sum(),
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
            .map(|m| estimate_content_tokens_mock(&m.content))
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
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are Rustant, a privacy-first autonomous personal assistant built in Rust. You help users with software engineering, daily productivity, and macOS automation tasks.

CRITICAL — Tool selection rules:
- You MUST use the dedicated tool for each task. Do NOT use shell_exec when a dedicated tool exists.
- For clipboard: call macos_clipboard with {"action":"read"} or {"action":"write","content":"..."}
- For battery/disk/CPU/version: call macos_system_info with {"action":"battery"}, {"action":"version"}, etc.
- For running apps: call macos_app_control with {"action":"list_running"}
- For calendar: call macos_calendar. For reminders: call macos_reminders. For notes: call macos_notes.
- For screenshots: call macos_screenshot. For Spotlight search: call macos_spotlight.
- shell_exec is a last resort — only use it for commands that have no dedicated tool.
- Do NOT use document_read for clipboard or system operations — it reads document files only.
- If a tool call fails, try a different tool or action — do NOT ask the user whether to proceed. Act autonomously.
- Never call ask_user more than once per task unless the user's answer was genuinely unclear.

Other behaviors:
- Always read a file before modifying it
- Prefer small, focused changes over large rewrites
- Respect file boundaries and permissions

Tools:
- Use the tools provided to you. Each tool has a name, description, and parameter schema.
- For arXiv/paper searches: ALWAYS use arxiv_research (built-in API client), never safari/curl.
- For web searches: use web_search (DuckDuckGo), not safari/shell.
- For fetching URLs: use web_fetch, not safari/shell.
- For meeting recording: use macos_meeting_recorder with 'record_and_transcribe' for full flow.

Workflows (structured multi-step templates — run via shell_exec "rustant workflow run <name>"):
  code_review, refactor, test_generation, documentation, dependency_update,
  security_scan, deployment, incident_response, morning_briefing, pr_review,
  dependency_audit, changelog, meeting_recorder, daily_briefing_full,
  end_of_day_summary, app_automation, email_triage, arxiv_research,
  knowledge_graph, experiment_tracking, code_analysis, content_pipeline,
  skill_development, career_planning, system_monitoring, life_planning,
  privacy_audit, self_improvement_loop
When a user asks for one of these tasks by name or description, execute the workflow or accomplish it step by step.

Security rules:
- Never execute commands that could damage the system or leak credentials
- Do not read or write files containing secrets (.env, *.key, *.pem) unless explicitly asked
- Sanitize all user input before passing to shell or AppleScript commands
- When unsure about a destructive action, use ask_user to confirm first"#;

// ---------------------------------------------------------------------------
// Token Budget Manager
// ---------------------------------------------------------------------------

/// Tracks token usage against configurable budgets and predicts costs
/// before execution. Can warn or halt when budgets are exceeded.
pub struct TokenBudgetManager {
    session_limit_usd: f64,
    task_limit_usd: f64,
    session_token_limit: usize,
    halt_on_exceed: bool,
    session_cost: f64,
    task_cost: f64,
    session_tokens: usize,
}

/// The result of a pre-call budget check.
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetCheckResult {
    /// Budget is within limits, proceed.
    Ok,
    /// Budget warning — approaching limit but not exceeded.
    Warning { message: String, usage_pct: f64 },
    /// Budget exceeded — should halt if configured.
    Exceeded { message: String },
}

impl TokenBudgetManager {
    /// Create a new budget manager from config. Passing `None` creates
    /// an unlimited manager that always returns `Ok`.
    pub fn new(config: Option<&crate::config::BudgetConfig>) -> Self {
        match config {
            Some(cfg) => Self {
                session_limit_usd: cfg.session_limit_usd,
                task_limit_usd: cfg.task_limit_usd,
                session_token_limit: cfg.session_token_limit,
                halt_on_exceed: cfg.halt_on_exceed,
                session_cost: 0.0,
                task_cost: 0.0,
                session_tokens: 0,
            },
            None => Self {
                session_limit_usd: 0.0,
                task_limit_usd: 0.0,
                session_token_limit: 0,
                halt_on_exceed: false,
                session_cost: 0.0,
                task_cost: 0.0,
                session_tokens: 0,
            },
        }
    }

    /// Reset task-level tracking (call at start of each new task).
    pub fn reset_task(&mut self) {
        self.task_cost = 0.0;
    }

    /// Record usage after an LLM call completes.
    pub fn record_usage(&mut self, usage: &TokenUsage, cost: &CostEstimate) {
        self.session_cost += cost.total();
        self.task_cost += cost.total();
        self.session_tokens += usage.total();
    }

    /// Estimate cost for an upcoming LLM call and check against budgets.
    ///
    /// `estimated_input_tokens` is the count of tokens in the request.
    /// `input_rate` and `output_rate` are the per-token costs from the provider.
    /// Output tokens are estimated at 0.5x input as a heuristic.
    pub fn check_budget(
        &self,
        estimated_input_tokens: usize,
        input_rate: f64,
        output_rate: f64,
    ) -> BudgetCheckResult {
        // Predict output tokens as ~50% of input (heuristic)
        let predicted_output = estimated_input_tokens / 2;
        let predicted_cost =
            (estimated_input_tokens as f64 * input_rate) + (predicted_output as f64 * output_rate);

        let projected_session_cost = self.session_cost + predicted_cost;
        let projected_task_cost = self.task_cost + predicted_cost;
        let projected_session_tokens =
            self.session_tokens + estimated_input_tokens + predicted_output;

        // Check session cost limit
        if self.session_limit_usd > 0.0 && projected_session_cost > self.session_limit_usd {
            return BudgetCheckResult::Exceeded {
                message: format!(
                    "Session cost ${:.4} would exceed limit ${:.4}",
                    projected_session_cost, self.session_limit_usd
                ),
            };
        }

        // Check task cost limit
        if self.task_limit_usd > 0.0 && projected_task_cost > self.task_limit_usd {
            return BudgetCheckResult::Exceeded {
                message: format!(
                    "Task cost ${:.4} would exceed limit ${:.4}",
                    projected_task_cost, self.task_limit_usd
                ),
            };
        }

        // Check session token limit
        if self.session_token_limit > 0 && projected_session_tokens > self.session_token_limit {
            return BudgetCheckResult::Exceeded {
                message: format!(
                    "Session tokens {} would exceed limit {}",
                    projected_session_tokens, self.session_token_limit
                ),
            };
        }

        // Check if approaching limits (>80%)
        if self.session_limit_usd > 0.0 {
            let pct = projected_session_cost / self.session_limit_usd;
            if pct > 0.8 {
                return BudgetCheckResult::Warning {
                    message: format!(
                        "Session cost at {:.0}% of ${:.4} limit",
                        pct * 100.0,
                        self.session_limit_usd
                    ),
                    usage_pct: pct,
                };
            }
        }

        BudgetCheckResult::Ok
    }

    /// Whether budget enforcement should halt execution on exceed.
    pub fn should_halt_on_exceed(&self) -> bool {
        self.halt_on_exceed
    }

    /// Current session cost.
    pub fn session_cost(&self) -> f64 {
        self.session_cost
    }

    /// Current task cost.
    pub fn task_cost(&self) -> f64 {
        self.task_cost
    }

    /// Current session token count.
    pub fn session_tokens(&self) -> usize {
        self.session_tokens
    }
}

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
            ..Default::default()
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

    // --- sanitize_tool_sequence tests ---

    #[test]
    fn test_sanitize_removes_orphaned_tool_results() {
        let mut messages = vec![
            Message::system("You are a helper."),
            Message::user("do something"),
            // Orphaned tool_result — no matching tool_call
            Message::tool_result("call_orphan_123", "some result", false),
            Message::assistant("Done!"),
        ];

        super::sanitize_tool_sequence(&mut messages);

        // The orphaned tool_result should be removed
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, Role::System);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[2].role, Role::Assistant);
    }

    #[test]
    fn test_sanitize_preserves_valid_sequence() {
        let mut messages = vec![
            Message::system("system prompt"),
            Message::user("read main.rs"),
            Message::new(
                Role::Assistant,
                Content::tool_call(
                    "call_1",
                    "file_read",
                    serde_json::json!({"path": "main.rs"}),
                ),
            ),
            Message::tool_result("call_1", "fn main() {}", false),
            Message::assistant("Here is the file content."),
        ];

        super::sanitize_tool_sequence(&mut messages);

        // All messages should be preserved
        assert_eq!(messages.len(), 5);
    }

    #[test]
    fn test_sanitize_handles_system_between_call_and_result() {
        let mut messages = vec![
            Message::system("system prompt"),
            Message::user("do something"),
            Message::new(
                Role::Assistant,
                Content::tool_call("call_1", "file_read", serde_json::json!({"path": "x.rs"})),
            ),
            // System message injected between tool_call and tool_result
            Message::system("routing hint: use file_read"),
            Message::tool_result("call_1", "file contents", false),
            Message::assistant("Done"),
        ];

        super::sanitize_tool_sequence(&mut messages);

        // System message should be moved before the assistant tool_call
        // Find the assistant tool_call message
        let assistant_idx = messages
            .iter()
            .position(|m| m.role == Role::Assistant && super::content_has_tool_call(&m.content))
            .unwrap();

        // The message right after the assistant tool_call should be the tool_result
        let next = &messages[assistant_idx + 1];
        assert!(
            matches!(&next.content, Content::ToolResult { .. })
                || next.role == Role::Tool
                || next.role == Role::User,
            "Expected tool_result after tool_call, got {:?}",
            next.role
        );
    }

    #[test]
    fn test_sanitize_multipart_tool_call() {
        let mut messages = vec![
            Message::user("do two things"),
            Message::new(
                Role::Assistant,
                Content::MultiPart {
                    parts: vec![
                        Content::text("I'll read both files."),
                        Content::tool_call(
                            "call_a",
                            "file_read",
                            serde_json::json!({"path": "a.rs"}),
                        ),
                        Content::tool_call(
                            "call_b",
                            "file_read",
                            serde_json::json!({"path": "b.rs"}),
                        ),
                    ],
                },
            ),
            Message::tool_result("call_a", "contents of a", false),
            Message::tool_result("call_b", "contents of b", false),
            // Orphaned tool_result
            Message::tool_result("call_nonexistent", "orphan", false),
        ];

        super::sanitize_tool_sequence(&mut messages);

        // Orphaned result removed, valid ones preserved
        assert_eq!(messages.len(), 4);
    }

    #[test]
    fn test_sanitize_empty_messages() {
        let mut messages: Vec<Message> = vec![];
        super::sanitize_tool_sequence(&mut messages);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_sanitize_no_tool_messages() {
        let mut messages = vec![
            Message::system("prompt"),
            Message::user("hello"),
            Message::assistant("hi"),
        ];
        super::sanitize_tool_sequence(&mut messages);
        assert_eq!(messages.len(), 3);
    }

    #[test]
    fn test_count_tool_definitions() {
        let counter = TokenCounter::for_model("gpt-4");
        let tools = vec![
            ToolDefinition {
                name: "calculator".to_string(),
                description: "Perform arithmetic calculations".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "expression": { "type": "string", "description": "Math expression" }
                    },
                    "required": ["expression"]
                }),
            },
            ToolDefinition {
                name: "file_read".to_string(),
                description: "Read a file from the filesystem".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path" }
                    },
                    "required": ["path"]
                }),
            },
        ];

        let token_count = counter.count_tool_definitions(&tools);
        // Each tool: 10 overhead + name tokens + description tokens + params tokens
        // Should be a meaningful positive number
        assert!(
            token_count > 40,
            "Two tool definitions should count as >40 tokens, got {}",
            token_count
        );
        // Sanity: shouldn't be absurdly large
        assert!(
            token_count < 500,
            "Two simple tool definitions should be <500 tokens, got {}",
            token_count
        );
    }

    #[test]
    fn test_count_tool_definitions_empty() {
        let counter = TokenCounter::for_model("gpt-4");
        assert_eq!(counter.count_tool_definitions(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_with_tools() {
        let provider = Arc::new(MockLlmProvider::new());
        let brain = Brain::new(provider, "system prompt");

        let messages = vec![Message::user("hello")];
        let tools = vec![ToolDefinition {
            name: "echo".to_string(),
            description: "Echo text back".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];

        let without_tools = brain.estimate_tokens(&messages);
        let with_tools = brain.estimate_tokens_with_tools(&messages, Some(&tools));
        assert!(
            with_tools > without_tools,
            "Token estimate with tools ({}) should be greater than without ({})",
            with_tools,
            without_tools
        );
    }
}
