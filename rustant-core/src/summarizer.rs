//! LLM-based context summarization.
//!
//! When the conversation grows beyond the context window, older messages
//! are summarized into a compact representation to preserve important context
//! while reducing token usage.

use crate::brain::{Brain, LlmProvider};
use crate::types::{CompletionRequest, Content, Message, Role};
use std::sync::Arc;

/// Summary of conversation context for compression.
#[derive(Debug, Clone)]
pub struct ContextSummary {
    /// The generated summary text.
    pub text: String,
    /// Number of messages that were summarized.
    pub messages_summarized: usize,
    /// Estimated tokens saved.
    pub tokens_saved: usize,
}

/// Generates summaries of conversation history using the LLM.
pub struct ContextSummarizer {
    /// LLM provider for generating summaries.
    provider: Arc<dyn LlmProvider>,
}

impl ContextSummarizer {
    /// Create a new summarizer with the given LLM provider.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Generate a summary of the given messages.
    pub async fn summarize(&self, messages: &[Message]) -> Result<ContextSummary, SummarizeError> {
        if messages.is_empty() {
            return Ok(ContextSummary {
                text: String::new(),
                messages_summarized: 0,
                tokens_saved: 0,
            });
        }

        let prompt = build_summarization_prompt(messages);

        let request = CompletionRequest {
            messages: vec![Message::user(prompt)],
            tools: None,
            temperature: 0.3,
            max_tokens: Some(500),
            stop_sequences: Vec::new(),
            model: None,
            ..Default::default()
        };

        let response = self
            .provider
            .complete(request)
            .await
            .map_err(|e| SummarizeError::LlmError(e.to_string()))?;

        let summary_text = match &response.message.content {
            Content::Text { text } => text.clone(),
            _ => String::from("[Summary unavailable]"),
        };

        // Estimate tokens saved (rough: original messages minus summary)
        let original_tokens: usize = messages.iter().map(estimate_message_tokens).sum();
        let summary_tokens = summary_text.len() / 4; // rough estimate

        Ok(ContextSummary {
            text: summary_text,
            messages_summarized: messages.len(),
            tokens_saved: original_tokens.saturating_sub(summary_tokens),
        })
    }

    /// Check if summarization is needed based on context usage.
    pub fn should_summarize(context_ratio: f32, threshold: f32) -> bool {
        context_ratio >= threshold
    }
}

/// Build the prompt for summarizing messages.
fn build_summarization_prompt(messages: &[Message]) -> String {
    let mut prompt = String::from(
        "Summarize the following conversation concisely, preserving:\n\
         - Key decisions and conclusions\n\
         - Important facts and data points\n\
         - Tool results and their outcomes\n\
         - Current task goals and progress\n\n\
         Conversation:\n",
    );

    for msg in messages {
        let role = match msg.role {
            Role::User => "User",
            Role::Assistant => "Assistant",
            Role::System => "System",
            Role::Tool => "Tool",
        };
        let text = match &msg.content {
            Content::Text { text } => text.clone(),
            Content::ToolCall {
                name, arguments, ..
            } => format!("[Tool Call: {} ({})]", name, arguments),
            Content::ToolResult { output, .. } => {
                format!("[Tool Result: {}]", output)
            }
            Content::MultiPart { parts } => parts
                .iter()
                .filter_map(|p| {
                    if let Content::Text { text } = p {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            Content::Thinking { thinking, .. } => format!("[Thinking: {}]", thinking),
            Content::Image { media_type, .. } => format!("[Image: {}]", media_type),
            Content::Citation { cited_text, .. } => format!("[Citation: {}]", cited_text),
            Content::CodeExecution { code, .. } => {
                format!("[Code: {}]", &code[..code.len().min(100)])
            }
            Content::SearchResult { query, .. } => format!("[Search: {}]", query),
        };
        prompt.push_str(&format!("{}: {}\n", role, text));
    }

    prompt.push_str("\nProvide a concise summary (3-5 sentences) capturing the essential context:");
    prompt
}

/// Rough token estimation for a message.
fn estimate_message_tokens(msg: &Message) -> usize {
    let text_len = match &msg.content {
        Content::Text { text } => text.len(),
        Content::ToolCall { arguments, .. } => arguments.to_string().len(),
        Content::ToolResult { output, .. } => output.len(),
        Content::MultiPart { parts } => parts
            .iter()
            .map(|p| match p {
                Content::Text { text } => text.len(),
                _ => 0,
            })
            .sum(),
        Content::Image { .. } => 300, // images are roughly 85 tokens
        Content::Thinking { thinking, .. } => thinking.len(),
        Content::Citation { cited_text, .. } => cited_text.len(),
        Content::CodeExecution {
            code,
            output,
            error,
            ..
        } => {
            code.len()
                + output.as_deref().map_or(0, |s| s.len())
                + error.as_deref().map_or(0, |s| s.len())
        }
        Content::SearchResult { query, results } => {
            query.len() + results.iter().map(|r| r.snippet.len()).sum::<usize>()
        }
    };
    text_len / 4 + 4 // rough: 4 chars per token + overhead
}

/// Errors during summarization.
#[derive(Debug, thiserror::Error)]
pub enum SummarizeError {
    #[error("LLM error during summarization: {0}")]
    LlmError(String),
}

/// Token budget alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenAlert {
    /// Context is within normal range.
    Normal,
    /// Context is getting large (> 50%).
    Warning,
    /// Context is critically full (> 80%).
    Critical,
    /// Context is near overflow (> 95%).
    Overflow,
}

impl TokenAlert {
    /// Determine the alert level from a context ratio.
    pub fn from_ratio(ratio: f32) -> Self {
        if ratio > 0.95 {
            TokenAlert::Overflow
        } else if ratio > 0.80 {
            TokenAlert::Critical
        } else if ratio > 0.50 {
            TokenAlert::Warning
        } else {
            TokenAlert::Normal
        }
    }
}

impl std::fmt::Display for TokenAlert {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenAlert::Normal => write!(f, "OK"),
            TokenAlert::Warning => write!(f, "WARNING"),
            TokenAlert::Critical => write!(f, "CRITICAL"),
            TokenAlert::Overflow => write!(f, "OVERFLOW"),
        }
    }
}

/// Token and cost tracking display data.
#[derive(Debug, Clone)]
pub struct TokenCostDisplay {
    /// Total input tokens used.
    pub input_tokens: usize,
    /// Total output tokens used.
    pub output_tokens: usize,
    /// Total tokens.
    pub total_tokens: usize,
    /// Context window size.
    pub context_window: usize,
    /// Context fill ratio.
    pub context_ratio: f32,
    /// Total cost in USD.
    pub total_cost: f64,
    /// Alert level.
    pub alert: TokenAlert,
}

impl TokenCostDisplay {
    /// Create from brain statistics.
    ///
    /// Uses total usage tokens as a ratio against the context window.
    pub fn from_brain(brain: &Brain) -> Self {
        let usage = brain.total_usage();
        let cost = brain.total_cost();
        let context_window = brain.context_window();
        let ratio = if context_window > 0 {
            usage.total() as f32 / context_window as f32
        } else {
            0.0
        };

        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total(),
            context_window,
            context_ratio: ratio,
            total_cost: cost.total(),
            alert: TokenAlert::from_ratio(ratio),
        }
    }

    /// Format as a display string.
    pub fn format_display(&self) -> String {
        format!(
            "Tokens: {} in / {} out ({} total) | Context: {:.0}% of {} | Cost: ${:.4} | {}",
            self.input_tokens,
            self.output_tokens,
            self.total_tokens,
            self.context_ratio * 100.0,
            self.context_window,
            self.total_cost,
            self.alert,
        )
    }
}

/// Truncate a string to at most `max` characters (byte-safe via char boundary).
fn truncate_str(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    // Find a valid char boundary at or before `max`
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Smart fallback summary that preserves structured information when LLM-based
/// summarization fails. Instead of naive truncation, it extracts tool names,
/// results, and preserves the first/last messages for continuity.
pub fn smart_fallback_summary(messages: &[Message], max_chars: usize) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let quarter = max_chars / 4;
    let mut parts = Vec::new();

    // Always include first message (the initial request context)
    if let Some(first) = messages.first()
        && let Some(text) = first.content.as_text()
    {
        parts.push(format!("[Start] {}", truncate_str(text, quarter)));
    }

    // Extract tool call summaries (tool name + brief result)
    for msg in messages.iter() {
        match &msg.content {
            Content::ToolCall { name, .. } => {
                parts.push(format!("[Tool: {}]", name));
            }
            Content::ToolResult { output, .. } => {
                parts.push(format!("[Result: {}]", truncate_str(output, 80)));
            }
            _ => {}
        }
    }

    // Always include last message if different from first
    if messages.len() > 1
        && let Some(last) = messages.last()
        && let Some(text) = last.content.as_text()
    {
        parts.push(format!("[Latest] {}", truncate_str(text, quarter)));
    }

    let joined = parts.join("\n");
    if joined.len() > max_chars {
        format!("{}...", truncate_str(&joined, max_chars))
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MockLlmProvider;

    #[test]
    fn test_token_alert_from_ratio() {
        assert_eq!(TokenAlert::from_ratio(0.0), TokenAlert::Normal);
        assert_eq!(TokenAlert::from_ratio(0.3), TokenAlert::Normal);
        assert_eq!(TokenAlert::from_ratio(0.51), TokenAlert::Warning);
        assert_eq!(TokenAlert::from_ratio(0.81), TokenAlert::Critical);
        assert_eq!(TokenAlert::from_ratio(0.96), TokenAlert::Overflow);
    }

    #[test]
    fn test_token_alert_display() {
        assert_eq!(TokenAlert::Normal.to_string(), "OK");
        assert_eq!(TokenAlert::Warning.to_string(), "WARNING");
        assert_eq!(TokenAlert::Critical.to_string(), "CRITICAL");
        assert_eq!(TokenAlert::Overflow.to_string(), "OVERFLOW");
    }

    #[test]
    fn test_should_summarize() {
        assert!(!ContextSummarizer::should_summarize(0.5, 0.8));
        assert!(ContextSummarizer::should_summarize(0.85, 0.8));
        assert!(ContextSummarizer::should_summarize(1.0, 0.8));
    }

    #[test]
    fn test_build_summarization_prompt() {
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there")];
        let prompt = build_summarization_prompt(&messages);
        assert!(prompt.contains("User: Hello"));
        assert!(prompt.contains("Assistant: Hi there"));
        assert!(prompt.contains("Summarize"));
    }

    #[test]
    fn test_estimate_message_tokens() {
        let msg = Message::user("Hello world, this is a test message");
        let tokens = estimate_message_tokens(&msg);
        assert!(tokens > 0);
    }

    #[tokio::test]
    async fn test_summarize_empty() {
        let provider = Arc::new(MockLlmProvider::new());
        let summarizer = ContextSummarizer::new(provider);
        let result = summarizer.summarize(&[]).await.unwrap();
        assert_eq!(result.messages_summarized, 0);
        assert!(result.text.is_empty());
    }

    #[tokio::test]
    async fn test_summarize_messages() {
        let provider = Arc::new(MockLlmProvider::new());
        let summarizer = ContextSummarizer::new(provider);
        let messages = vec![
            Message::user("Write a function"),
            Message::assistant("Here's the function..."),
        ];
        let result = summarizer.summarize(&messages).await.unwrap();
        assert_eq!(result.messages_summarized, 2);
        assert!(!result.text.is_empty());
    }

    #[test]
    fn test_token_cost_display_format() {
        let display = TokenCostDisplay {
            input_tokens: 1000,
            output_tokens: 500,
            total_tokens: 1500,
            context_window: 128000,
            context_ratio: 0.45,
            total_cost: 0.0123,
            alert: TokenAlert::Normal,
        };
        let formatted = display.format_display();
        assert!(formatted.contains("1000 in"));
        assert!(formatted.contains("500 out"));
        assert!(formatted.contains("$0.0123"));
        assert!(formatted.contains("OK"));
    }

    // --- Gap 2: Smart fallback summary tests ---

    #[test]
    fn test_smart_fallback_preserves_tool_names() {
        let messages = vec![
            Message::user("fix the bug"),
            Message::new(
                Role::Assistant,
                Content::tool_call(
                    "c1",
                    "file_read",
                    serde_json::json!({"path": "src/main.rs"}),
                ),
            ),
            Message::new(
                Role::Tool,
                Content::tool_result("c1", "fn main() { println!(\"hello\"); }", false),
            ),
            Message::assistant("I found the issue."),
        ];

        let summary = smart_fallback_summary(&messages, 500);

        assert!(
            summary.contains("file_read"),
            "Summary should contain tool name: {}",
            summary
        );
        assert!(
            summary.contains("fix the bug"),
            "Summary should contain first message: {}",
            summary
        );
    }

    #[test]
    fn test_smart_fallback_preserves_first_and_last() {
        let messages = vec![
            Message::user("initial request about authentication"),
            Message::assistant("Let me look into that."),
            Message::user("follow up about tokens"),
            Message::assistant("Here is the solution for token handling"),
        ];

        let summary = smart_fallback_summary(&messages, 500);

        assert!(
            summary.contains("initial request"),
            "Summary should contain first message: {}",
            summary
        );
        assert!(
            summary.contains("token handling"),
            "Summary should contain last message: {}",
            summary
        );
    }

    #[test]
    fn test_smart_fallback_respects_limit() {
        let long_text = "a".repeat(1000);
        let messages = vec![Message::user(&long_text)];

        let summary = smart_fallback_summary(&messages, 100);

        assert!(
            summary.len() <= 110, // small margin for formatting
            "Summary should respect limit: len={} > 110",
            summary.len()
        );
    }

    #[test]
    fn test_smart_fallback_empty_messages() {
        let summary = smart_fallback_summary(&[], 500);
        assert!(
            summary.is_empty(),
            "Empty messages should give empty summary"
        );
    }

    #[test]
    fn test_smart_fallback_different_limits() {
        let messages = vec![Message::user("x".repeat(1000))];

        let short = smart_fallback_summary(&messages, 50);
        let long = smart_fallback_summary(&messages, 800);

        assert!(short.len() <= 60);
        assert!(long.len() <= 810);
        assert!(long.len() > short.len());
    }
}
