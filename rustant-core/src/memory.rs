//! Three-tier memory system for the Rustant agent.
//!
//! - **Working Memory**: Current task state and scratch data (single task lifetime).
//! - **Short-Term Memory**: Sliding window of recent conversation with summarization.
//! - **Long-Term Memory**: Persistent facts and preferences across sessions.

use crate::types::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use uuid::Uuid;

/// Working memory for the currently executing task.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkingMemory {
    pub current_goal: Option<String>,
    pub sub_tasks: Vec<String>,
    pub scratchpad: HashMap<String, String>,
    pub active_files: Vec<String>,
}

impl WorkingMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_goal(&mut self, goal: impl Into<String>) {
        self.current_goal = Some(goal.into());
    }

    pub fn add_sub_task(&mut self, task: impl Into<String>) {
        self.sub_tasks.push(task.into());
    }

    pub fn note(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.scratchpad.insert(key.into(), value.into());
    }

    pub fn add_active_file(&mut self, path: impl Into<String>) {
        let path = path.into();
        if !self.active_files.contains(&path) {
            self.active_files.push(path);
        }
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Short-term memory: sliding window of recent messages with summarization support.
#[derive(Debug, Clone)]
pub struct ShortTermMemory {
    messages: VecDeque<Message>,
    window_size: usize,
    summarized_prefix: Option<String>,
    total_messages_seen: usize,
}

impl ShortTermMemory {
    pub fn new(window_size: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            window_size,
            summarized_prefix: None,
            total_messages_seen: 0,
        }
    }

    /// Add a message to short-term memory.
    pub fn add(&mut self, message: Message) {
        self.messages.push_back(message);
        self.total_messages_seen += 1;
    }

    /// Get all messages that should be sent to the LLM, including any summary.
    pub fn to_messages(&self) -> Vec<Message> {
        let mut result = Vec::new();

        // Include summary of older context if available
        if let Some(ref summary) = self.summarized_prefix {
            result.push(Message::system(format!(
                "[Summary of earlier conversation]\n{}",
                summary
            )));
        }

        // Include recent messages within the window
        let start = if self.messages.len() > self.window_size {
            self.messages.len() - self.window_size
        } else {
            0
        };

        for msg in self.messages.iter().skip(start) {
            result.push(msg.clone());
        }

        result
    }

    /// Check whether compression is needed based on message count.
    pub fn needs_compression(&self) -> bool {
        self.messages.len() >= self.window_size * 2
    }

    /// Compress older messages by replacing them with a summary.
    /// Returns the number of messages that were compressed.
    pub fn compress(&mut self, summary: String) -> usize {
        if self.messages.len() <= self.window_size {
            return 0;
        }

        let to_remove = self.messages.len() - self.window_size;
        let removed: Vec<Message> = self.messages.drain(..to_remove).collect();

        // Merge with existing summary
        if let Some(ref existing) = self.summarized_prefix {
            self.summarized_prefix = Some(format!("{}\n\n{}", existing, summary));
        } else {
            self.summarized_prefix = Some(summary);
        }

        removed.len()
    }

    /// Get messages that should be summarized (older than window).
    pub fn messages_to_summarize(&self) -> Vec<&Message> {
        if self.messages.len() <= self.window_size {
            return Vec::new();
        }
        let to_summarize = self.messages.len() - self.window_size;
        self.messages.iter().take(to_summarize).collect()
    }

    /// Get the total number of messages currently held.
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if memory is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get total messages seen across the session.
    pub fn total_messages_seen(&self) -> usize {
        self.total_messages_seen
    }

    /// Clear all messages and summary.
    pub fn clear(&mut self) {
        self.messages.clear();
        self.summarized_prefix = None;
        self.total_messages_seen = 0;
    }

    /// Get a reference to all current messages.
    pub fn messages(&self) -> &VecDeque<Message> {
        &self.messages
    }

    /// Get the summary prefix, if any.
    pub fn summary(&self) -> Option<&str> {
        self.summarized_prefix.as_deref()
    }
}

/// A fact extracted from conversation for long-term storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: Uuid,
    pub content: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

impl Fact {
    pub fn new(content: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            source: source.into(),
            created_at: Utc::now(),
            tags: Vec::new(),
        }
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }
}

/// Long-term memory persisted across sessions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub facts: Vec<Fact>,
    pub preferences: HashMap<String, String>,
    pub corrections: Vec<Correction>,
}

/// A correction recorded from user feedback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub id: Uuid,
    pub original: String,
    pub corrected: String,
    pub context: String,
    pub timestamp: DateTime<Utc>,
}

impl LongTermMemory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_fact(&mut self, fact: Fact) {
        self.facts.push(fact);
    }

    pub fn set_preference(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.preferences.insert(key.into(), value.into());
    }

    pub fn get_preference(&self, key: &str) -> Option<&str> {
        self.preferences.get(key).map(|s| s.as_str())
    }

    pub fn add_correction(&mut self, original: String, corrected: String, context: String) {
        self.corrections.push(Correction {
            id: Uuid::new_v4(),
            original,
            corrected,
            context,
            timestamp: Utc::now(),
        });
    }

    /// Search facts by keyword.
    pub fn search_facts(&self, query: &str) -> Vec<&Fact> {
        let query_lower = query.to_lowercase();
        self.facts
            .iter()
            .filter(|f| {
                f.content.to_lowercase().contains(&query_lower)
                    || f.tags.iter().any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

/// The unified memory system combining all three tiers.
pub struct MemorySystem {
    pub working: WorkingMemory,
    pub short_term: ShortTermMemory,
    pub long_term: LongTermMemory,
}

impl MemorySystem {
    pub fn new(window_size: usize) -> Self {
        Self {
            working: WorkingMemory::new(),
            short_term: ShortTermMemory::new(window_size),
            long_term: LongTermMemory::new(),
        }
    }

    /// Get all messages for the LLM context.
    pub fn context_messages(&self) -> Vec<Message> {
        self.short_term.to_messages()
    }

    /// Add a message to the conversation.
    pub fn add_message(&mut self, message: Message) {
        self.short_term.add(message);
    }

    /// Reset working memory for a new task.
    pub fn start_new_task(&mut self, goal: impl Into<String>) {
        self.working.clear();
        self.working.set_goal(goal);
    }

    /// Clear everything except long-term memory.
    pub fn clear_session(&mut self) {
        self.working.clear();
        self.short_term.clear();
    }
}

/// Result of a context compression operation.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub messages_before: usize,
    pub messages_after: usize,
    pub compressed_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Role;

    #[test]
    fn test_working_memory_lifecycle() {
        let mut wm = WorkingMemory::new();
        assert!(wm.current_goal.is_none());

        wm.set_goal("refactor auth module");
        assert_eq!(wm.current_goal.as_deref(), Some("refactor auth module"));

        wm.add_sub_task("read current implementation");
        wm.add_sub_task("design new structure");
        assert_eq!(wm.sub_tasks.len(), 2);

        wm.note("finding", "uses basic auth currently");
        assert_eq!(wm.scratchpad.get("finding").map(|s| s.as_str()), Some("uses basic auth currently"));

        wm.add_active_file("src/auth/mod.rs");
        wm.add_active_file("src/auth/mod.rs"); // duplicate
        assert_eq!(wm.active_files.len(), 1);

        wm.clear();
        assert!(wm.current_goal.is_none());
        assert!(wm.sub_tasks.is_empty());
    }

    #[test]
    fn test_short_term_memory_basic() {
        let mut stm = ShortTermMemory::new(5);
        assert!(stm.is_empty());
        assert_eq!(stm.len(), 0);

        stm.add(Message::user("hello"));
        stm.add(Message::assistant("hi there"));
        assert_eq!(stm.len(), 2);
        assert_eq!(stm.total_messages_seen(), 2);

        let messages = stm.to_messages();
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_short_term_memory_window() {
        let mut stm = ShortTermMemory::new(3);

        for i in 0..6 {
            stm.add(Message::user(format!("message {}", i)));
        }

        assert_eq!(stm.len(), 6);
        let messages = stm.to_messages();
        // Should only include last 3 messages (window size)
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content.as_text(), Some("message 3"));
        assert_eq!(messages[2].content.as_text(), Some("message 5"));
    }

    #[test]
    fn test_short_term_memory_compression() {
        let mut stm = ShortTermMemory::new(3);

        for i in 0..6 {
            stm.add(Message::user(format!("message {}", i)));
        }

        assert!(stm.needs_compression());

        let to_summarize = stm.messages_to_summarize();
        assert_eq!(to_summarize.len(), 3); // messages 0, 1, 2

        let compressed = stm.compress("Summary of messages 0-2.".to_string());
        assert_eq!(compressed, 3);
        assert_eq!(stm.len(), 3); // only window remains

        let messages = stm.to_messages();
        // First message should be the summary
        assert_eq!(messages.len(), 4); // 1 summary + 3 recent
        assert!(messages[0].content.as_text().unwrap().contains("Summary of"));
        assert_eq!(messages[0].role, Role::System);
    }

    #[test]
    fn test_short_term_memory_double_compression() {
        let mut stm = ShortTermMemory::new(2);

        // First batch
        for i in 0..5 {
            stm.add(Message::user(format!("msg {}", i)));
        }
        stm.compress("First summary.".to_string());
        assert_eq!(stm.len(), 2);

        // Second batch
        for i in 5..8 {
            stm.add(Message::user(format!("msg {}", i)));
        }
        stm.compress("Second summary.".to_string());
        assert_eq!(stm.len(), 2);

        // Summary should merge
        let summary = stm.summary().unwrap();
        assert!(summary.contains("First summary."));
        assert!(summary.contains("Second summary."));
    }

    #[test]
    fn test_short_term_memory_clear() {
        let mut stm = ShortTermMemory::new(5);
        stm.add(Message::user("test"));
        stm.compress("summary".to_string());

        stm.clear();
        assert!(stm.is_empty());
        assert!(stm.summary().is_none());
        assert_eq!(stm.total_messages_seen(), 0);
    }

    #[test]
    fn test_fact_creation() {
        let fact = Fact::new("Project uses JWT auth", "code analysis")
            .with_tags(vec!["auth".to_string(), "jwt".to_string()]);
        assert_eq!(fact.content, "Project uses JWT auth");
        assert_eq!(fact.source, "code analysis");
        assert_eq!(fact.tags.len(), 2);
    }

    #[test]
    fn test_long_term_memory() {
        let mut ltm = LongTermMemory::new();

        ltm.add_fact(Fact::new("Uses Rust 2021 edition", "Cargo.toml"));
        ltm.set_preference("code_style", "rustfmt defaults");
        ltm.add_correction(
            "wrong import".to_string(),
            "correct import".to_string(),
            "editing main.rs".to_string(),
        );

        assert_eq!(ltm.facts.len(), 1);
        assert_eq!(ltm.get_preference("code_style"), Some("rustfmt defaults"));
        assert_eq!(ltm.corrections.len(), 1);
    }

    #[test]
    fn test_long_term_memory_search() {
        let mut ltm = LongTermMemory::new();
        ltm.add_fact(Fact::new("Project uses JWT authentication", "analysis"));
        ltm.add_fact(
            Fact::new("Database is PostgreSQL", "config").with_tags(vec!["database".to_string()]),
        );
        ltm.add_fact(Fact::new("Frontend uses React", "package.json"));

        let results = ltm.search_facts("JWT");
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("JWT"));

        let results = ltm.search_facts("database");
        assert_eq!(results.len(), 1);

        let results = ltm.search_facts("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_memory_system() {
        let mut mem = MemorySystem::new(5);

        mem.start_new_task("fix bug #42");
        assert_eq!(mem.working.current_goal.as_deref(), Some("fix bug #42"));

        mem.add_message(Message::user("fix the null pointer bug"));
        mem.add_message(Message::assistant("I'll look into that."));

        let ctx = mem.context_messages();
        assert_eq!(ctx.len(), 2);

        mem.clear_session();
        assert!(mem.short_term.is_empty());
        assert!(mem.working.current_goal.is_none());
    }

    #[test]
    fn test_compression_no_op_when_within_window() {
        let mut stm = ShortTermMemory::new(10);
        stm.add(Message::user("hello"));
        stm.add(Message::assistant("hi"));

        assert!(!stm.needs_compression());
        assert!(stm.messages_to_summarize().is_empty());

        let compressed = stm.compress("should not matter".to_string());
        assert_eq!(compressed, 0);
    }

    #[test]
    fn test_memory_system_new_task_preserves_long_term() {
        let mut mem = MemorySystem::new(5);
        mem.long_term.add_fact(Fact::new("important fact", "test"));
        mem.add_message(Message::user("task 1"));

        mem.start_new_task("task 2");
        assert_eq!(mem.working.current_goal.as_deref(), Some("task 2"));
        assert_eq!(mem.long_term.facts.len(), 1); // preserved
    }
}
