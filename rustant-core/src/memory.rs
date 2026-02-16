//! Three-tier memory system for the Rustant agent.
//!
//! - **Working Memory**: Current task state and scratch data (single task lifetime).
//! - **Short-Term Memory**: Sliding window of recent conversation with summarization.
//! - **Long-Term Memory**: Persistent facts and preferences across sessions.

use crate::error::MemoryError;
use crate::search::{HybridSearchEngine, SearchConfig};
use crate::types::Message;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
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
    /// Set of absolute message indices that are pinned (survive compression).
    pinned: std::collections::HashSet<usize>,
    /// Running count of messages removed by compression (for index mapping).
    compressed_offset: usize,
}

impl ShortTermMemory {
    pub fn new(window_size: usize) -> Self {
        Self {
            messages: VecDeque::new(),
            window_size,
            summarized_prefix: None,
            total_messages_seen: 0,
            pinned: std::collections::HashSet::new(),
            compressed_offset: 0,
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

        // Include recent messages within the window.
        // Pinned messages that fall before the window start are still included.
        let start = if self.messages.len() > self.window_size {
            self.messages.len() - self.window_size
        } else {
            0
        };

        for (i, msg) in self.messages.iter().enumerate() {
            if i >= start || self.is_pinned(i) {
                result.push(msg.clone());
            }
        }

        result
    }

    /// Check whether compression is needed based on message count.
    pub fn needs_compression(&self) -> bool {
        self.messages.len() >= self.window_size * 2
    }

    /// Compress older messages by replacing them with a summary.
    /// Pinned messages are preserved and moved to the front of the window.
    /// Returns the number of messages that were compressed.
    pub fn compress(&mut self, summary: String) -> usize {
        if self.messages.len() <= self.window_size {
            return 0;
        }

        let to_remove = self.messages.len() - self.window_size;

        // Separate pinned messages from those to be removed, and collect
        // the absolute indices of messages that survive the window (not removed).
        let mut preserved = Vec::new();
        let mut removed_count = 0;

        for i in 0..to_remove {
            let abs_idx = self.compressed_offset + i;
            if self.pinned.contains(&abs_idx) {
                if let Some(msg) = self.messages.get(i) {
                    preserved.push(msg.clone());
                }
            } else {
                removed_count += 1;
            }
        }

        // Collect absolute indices that are pinned and remain in the window
        // (i.e. messages beyond to_remove that were pinned).
        let mut surviving_pinned: Vec<usize> = Vec::new();
        for i in to_remove..self.messages.len() {
            let abs_idx = self.compressed_offset + i;
            if self.pinned.contains(&abs_idx) {
                surviving_pinned.push(i - to_remove);
            }
        }

        // Remove the to_remove oldest messages
        self.messages.drain(..to_remove);
        self.compressed_offset += to_remove;

        // Re-insert pinned messages at the front of the window using batch operation.
        // This avoids O(p^2) sequential insert() calls by building a new VecDeque.
        let preserved_count = preserved.len();
        if !preserved.is_empty() {
            let mut new_messages = VecDeque::with_capacity(preserved_count + self.messages.len());
            for msg in preserved {
                new_messages.push_back(msg);
            }
            new_messages.append(&mut self.messages);
            self.messages = new_messages;
        }

        // Rebuild the pinned set with correct absolute indices.
        // Preserved messages are at positions 0..preserved_count.
        // Surviving pinned messages shifted by preserved_count.
        let mut new_pinned = HashSet::new();
        for i in 0..preserved_count {
            new_pinned.insert(self.compressed_offset + i);
        }
        for pos in surviving_pinned {
            new_pinned.insert(self.compressed_offset + preserved_count + pos);
        }
        self.pinned = new_pinned;

        // Merge with existing summary
        if let Some(ref existing) = self.summarized_prefix {
            self.summarized_prefix = Some(format!("{}\n\n{}", existing, summary));
        } else {
            self.summarized_prefix = Some(summary);
        }

        removed_count
    }

    /// Pin a message by its position in the current message list (0-based).
    /// Pinned messages survive compression.
    pub fn pin(&mut self, position: usize) -> bool {
        if position >= self.messages.len() {
            return false;
        }
        let abs_idx = self.compressed_offset + position;
        self.pinned.insert(abs_idx);
        true
    }

    /// Unpin a message by its position in the current message list.
    pub fn unpin(&mut self, position: usize) -> bool {
        if position >= self.messages.len() {
            return false;
        }
        let abs_idx = self.compressed_offset + position;
        self.pinned.remove(&abs_idx)
    }

    /// Check if a message at the given position is pinned.
    pub fn is_pinned(&self, position: usize) -> bool {
        let abs_idx = self.compressed_offset + position;
        self.pinned.contains(&abs_idx)
    }

    /// Number of currently pinned messages.
    pub fn pinned_count(&self) -> usize {
        // Count pinned messages that are still in the current window
        (0..self.messages.len())
            .filter(|&i| self.is_pinned(i))
            .count()
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
        self.pinned.clear();
        self.compressed_offset = 0;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongTermMemory {
    pub facts: Vec<Fact>,
    pub preferences: HashMap<String, String>,
    pub corrections: Vec<Correction>,
    /// Maximum number of facts to retain. When exceeded, the oldest fact is evicted.
    #[serde(default = "LongTermMemory::default_max_facts")]
    pub max_facts: usize,
    /// Maximum number of corrections to retain. When exceeded, the oldest correction is evicted.
    #[serde(default = "LongTermMemory::default_max_corrections")]
    pub max_corrections: usize,
}

impl Default for LongTermMemory {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            preferences: HashMap::new(),
            corrections: Vec::new(),
            max_facts: Self::default_max_facts(),
            max_corrections: Self::default_max_corrections(),
        }
    }
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

    fn default_max_facts() -> usize {
        10_000
    }

    fn default_max_corrections() -> usize {
        1_000
    }

    pub fn add_fact(&mut self, fact: Fact) {
        if self.facts.len() >= self.max_facts {
            self.facts.remove(0);
        }
        self.facts.push(fact);
    }

    pub fn set_preference(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.preferences.insert(key.into(), value.into());
    }

    pub fn get_preference(&self, key: &str) -> Option<&str> {
        self.preferences.get(key).map(|s| s.as_str())
    }

    pub fn add_correction(&mut self, original: String, corrected: String, context: String) {
        if self.corrections.len() >= self.max_corrections {
            self.corrections.remove(0);
        }
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
                    || f.tags
                        .iter()
                        .any(|t| t.to_lowercase().contains(&query_lower))
            })
            .collect()
    }
}

/// The unified memory system combining all three tiers.
pub struct MemorySystem {
    pub working: WorkingMemory,
    pub short_term: ShortTermMemory,
    pub long_term: LongTermMemory,
    /// Optional hybrid search engine for fact retrieval.
    search_engine: Option<HybridSearchEngine>,
    /// Optional automatic flusher for periodic persistence.
    flusher: Option<MemoryFlusher>,
}

impl MemorySystem {
    pub fn new(window_size: usize) -> Self {
        Self {
            working: WorkingMemory::new(),
            short_term: ShortTermMemory::new(window_size),
            long_term: LongTermMemory::new(),
            search_engine: None,
            flusher: None,
        }
    }

    /// Create a memory system with hybrid search enabled.
    pub fn with_search(
        window_size: usize,
        search_config: SearchConfig,
    ) -> Result<Self, crate::search::SearchError> {
        let engine = HybridSearchEngine::open(search_config)?;
        Ok(Self {
            working: WorkingMemory::new(),
            short_term: ShortTermMemory::new(window_size),
            long_term: LongTermMemory::new(),
            search_engine: Some(engine),
            flusher: None,
        })
    }

    /// Attach an automatic flusher to this memory system (builder pattern).
    pub fn with_flusher(mut self, config: FlushConfig) -> Self {
        self.flusher = Some(MemoryFlusher::new(config));
        self
    }

    /// Get all messages for the LLM context.
    pub fn context_messages(&self) -> Vec<Message> {
        self.short_term.to_messages()
    }

    /// Add a message to the conversation.
    pub fn add_message(&mut self, message: Message) {
        self.short_term.add(message);
        // Notify flusher
        if let Some(ref mut flusher) = self.flusher {
            flusher.on_message_added();
        }
    }

    /// Add a fact to long-term memory, also indexing it in the search engine.
    pub fn add_fact(&mut self, fact: Fact) {
        if let Some(ref mut engine) = self.search_engine {
            let _ = engine.index_fact(&fact.id.to_string(), &fact.content);
        }
        self.long_term.add_fact(fact);
    }

    /// Search facts using the hybrid engine (falls back to keyword search).
    pub fn search_facts_hybrid(&self, query: &str) -> Vec<&Fact> {
        if let Some(ref engine) = self.search_engine {
            if let Ok(results) = engine.search(query) {
                let ids: Vec<String> = results.iter().map(|r| r.fact_id.clone()).collect();
                let found: Vec<&Fact> = self
                    .long_term
                    .facts
                    .iter()
                    .filter(|f| ids.contains(&f.id.to_string()))
                    .collect();
                if !found.is_empty() {
                    return found;
                }
            }
        }
        // Fallback to keyword search
        self.long_term.search_facts(query)
    }

    /// Check if auto-flush should happen, and flush if needed.
    ///
    /// Uses the `Option::take()` pattern to avoid borrow conflicts.
    pub fn check_auto_flush(&mut self) -> Result<bool, MemoryError> {
        let mut flusher = match self.flusher.take() {
            Some(f) => f,
            None => return Ok(false),
        };
        let result = if flusher.should_flush() {
            flusher.flush(self)?;
            Ok(true)
        } else {
            Ok(false)
        };
        self.flusher = Some(flusher);
        result
    }

    /// Force a flush regardless of triggers.
    pub fn force_flush(&mut self) -> Result<(), MemoryError> {
        let mut flusher = match self.flusher.take() {
            Some(f) => f,
            None => return Ok(()),
        };
        let result = flusher.force_flush(self);
        self.flusher = Some(flusher);
        result
    }

    /// Whether the flusher has unflushed data.
    pub fn flusher_is_dirty(&self) -> bool {
        self.flusher.as_ref().is_some_and(|f| f.is_dirty())
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

    /// Get a breakdown of context usage for the UI.
    pub fn context_breakdown(&self, context_window: usize) -> ContextBreakdown {
        let summary_chars = self.short_term.summary().map(|s| s.len()).unwrap_or(0);
        let message_chars: usize = self
            .short_term
            .messages()
            .iter()
            .map(|m| m.content_length())
            .sum();
        let total_chars = summary_chars + message_chars;

        // Rough token estimate: ~4 chars per token
        let summary_tokens = summary_chars / 4;
        let message_tokens = message_chars / 4;
        let total_tokens = total_chars / 4;
        let remaining_tokens = context_window.saturating_sub(total_tokens);

        ContextBreakdown {
            summary_tokens,
            message_tokens,
            total_tokens,
            context_window,
            remaining_tokens,
            message_count: self.short_term.len(),
            total_messages_seen: self.short_term.total_messages_seen(),
            pinned_count: self.short_term.pinned_count(),
            has_summary: self.short_term.summary().is_some(),
            facts_count: self.long_term.facts.len(),
            rules_count: 0, // Populated separately if knowledge distiller is available
        }
    }

    /// Pin a message in short-term memory by position.
    pub fn pin_message(&mut self, position: usize) -> bool {
        self.short_term.pin(position)
    }

    /// Unpin a message in short-term memory by position.
    pub fn unpin_message(&mut self, position: usize) -> bool {
        self.short_term.unpin(position)
    }
}

/// Breakdown of context window usage for the UI.
#[derive(Debug, Clone, Default)]
pub struct ContextBreakdown {
    /// Estimated tokens used by the summarized prefix.
    pub summary_tokens: usize,
    /// Estimated tokens used by active messages.
    pub message_tokens: usize,
    /// Total estimated tokens in use.
    pub total_tokens: usize,
    /// Total context window size (from config).
    pub context_window: usize,
    /// Remaining available tokens.
    pub remaining_tokens: usize,
    /// Number of messages currently in the window.
    pub message_count: usize,
    /// Total messages seen in the session.
    pub total_messages_seen: usize,
    /// Number of pinned messages.
    pub pinned_count: usize,
    /// Whether a summary prefix exists.
    pub has_summary: bool,
    /// Number of facts in long-term memory.
    pub facts_count: usize,
    /// Number of active behavioral rules.
    pub rules_count: usize,
}

impl ContextBreakdown {
    /// Context usage as a ratio (0.0 to 1.0).
    pub fn usage_ratio(&self) -> f32 {
        if self.context_window == 0 {
            return 0.0;
        }
        (self.total_tokens as f32 / self.context_window as f32).clamp(0.0, 1.0)
    }

    /// Whether context is at warning level (>= 70%).
    /// Aligned with agent's `ContextHealthEvent::Warning` threshold.
    pub fn is_warning(&self) -> bool {
        self.usage_ratio() >= 0.7
    }
}

/// Metadata about a saved session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub task_summary: Option<String>,
}

impl SessionMetadata {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            created_at: now,
            updated_at: now,
            task_summary: None,
        }
    }
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self::new()
    }
}

/// A persistable session containing the memory state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub metadata: SessionMetadata,
    pub working: WorkingMemory,
    pub long_term: LongTermMemory,
    pub messages: Vec<Message>,
    pub window_size: usize,
}

impl MemorySystem {
    /// Save the current memory state to a JSON file.
    pub fn save_session(&self, path: &Path) -> Result<(), MemoryError> {
        let session = Session {
            metadata: SessionMetadata {
                id: Uuid::new_v4(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                task_summary: self.working.current_goal.clone(),
            },
            working: self.working.clone(),
            long_term: self.long_term.clone(),
            messages: self.short_term.messages().iter().cloned().collect(),
            window_size: self.short_term.window_size(),
        };

        let json =
            serde_json::to_string_pretty(&session).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to serialize session: {}", e),
            })?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to create directory: {}", e),
            })?;
        }

        std::fs::write(path, json).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to write session file: {}", e),
        })?;

        Ok(())
    }

    /// Load a session from a JSON file.
    pub fn load_session(path: &Path) -> Result<Self, MemoryError> {
        let json = std::fs::read_to_string(path).map_err(|e| MemoryError::SessionLoadFailed {
            message: format!("Failed to read session file: {}", e),
        })?;

        let session: Session =
            serde_json::from_str(&json).map_err(|e| MemoryError::SessionLoadFailed {
                message: format!("Failed to deserialize session: {}", e),
            })?;

        let mut memory = MemorySystem::new(session.window_size);
        memory.working = session.working;
        memory.long_term = session.long_term;
        for msg in session.messages {
            memory.short_term.add(msg);
        }

        Ok(memory)
    }
}

impl ShortTermMemory {
    /// Get the window size.
    pub fn window_size(&self) -> usize {
        self.window_size
    }
}

/// Result of a context compression operation.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    pub messages_before: usize,
    pub messages_after: usize,
    pub compressed_count: usize,
}

// ---------------------------------------------------------------------------
// Memory Flusher
// ---------------------------------------------------------------------------

/// Configuration for automatic memory flushing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlushConfig {
    /// Whether automatic flushing is enabled.
    pub enabled: bool,
    /// Flush interval in seconds (0 = disabled).
    pub interval_secs: u64,
    /// Number of messages that triggers an auto-flush (0 = disabled).
    pub flush_on_message_count: usize,
    /// Path where flushed data is written.
    pub flush_path: Option<std::path::PathBuf>,
}

impl Default for FlushConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_secs: 300, // 5 minutes
            flush_on_message_count: 50,
            flush_path: None,
        }
    }
}

/// Tracks dirty state and triggers for automatic memory persistence.
#[derive(Debug, Clone)]
pub struct MemoryFlusher {
    config: FlushConfig,
    dirty: bool,
    messages_since_flush: usize,
    last_flush: DateTime<Utc>,
    total_flushes: usize,
}

impl MemoryFlusher {
    /// Create a new flusher with the given configuration.
    pub fn new(config: FlushConfig) -> Self {
        Self {
            config,
            dirty: false,
            messages_since_flush: 0,
            last_flush: Utc::now(),
            total_flushes: 0,
        }
    }

    /// Notify the flusher that a message was added.
    pub fn on_message_added(&mut self) {
        self.dirty = true;
        self.messages_since_flush += 1;
    }

    /// Check whether a flush should happen based on the configured triggers.
    pub fn should_flush(&self) -> bool {
        if !self.config.enabled || !self.dirty {
            return false;
        }

        // Message-count trigger
        if self.config.flush_on_message_count > 0
            && self.messages_since_flush >= self.config.flush_on_message_count
        {
            return true;
        }

        // Time-based trigger
        if self.config.interval_secs > 0 {
            let elapsed = (Utc::now() - self.last_flush).num_seconds();
            if elapsed >= self.config.interval_secs as i64 {
                return true;
            }
        }

        false
    }

    /// Perform a flush of the memory system to disk.
    pub fn flush(&mut self, memory: &MemorySystem) -> Result<(), MemoryError> {
        let path =
            self.config
                .flush_path
                .as_ref()
                .ok_or_else(|| MemoryError::PersistenceError {
                    message: "No flush path configured".to_string(),
                })?;

        memory.save_session(path)?;
        self.mark_flushed();
        Ok(())
    }

    /// Force a flush regardless of triggers.
    pub fn force_flush(&mut self, memory: &MemorySystem) -> Result<(), MemoryError> {
        if !self.dirty {
            return Ok(()); // nothing to flush
        }
        self.flush(memory)
    }

    /// Whether there is unflushed data.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Messages added since the last flush.
    pub fn messages_since_flush(&self) -> usize {
        self.messages_since_flush
    }

    /// Total number of flushes performed.
    pub fn total_flushes(&self) -> usize {
        self.total_flushes
    }

    /// Mark the flush as completed (reset counters).
    fn mark_flushed(&mut self) {
        self.dirty = false;
        self.messages_since_flush = 0;
        self.last_flush = Utc::now();
        self.total_flushes += 1;
    }
}

// ---------------------------------------------------------------------------
// Knowledge Distiller — Cross-Session Learning
// ---------------------------------------------------------------------------

/// A distilled behavioral rule generated from accumulated corrections and facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralRule {
    /// Unique identifier.
    pub id: Uuid,
    /// Human-readable rule description (injected into system prompt).
    pub rule: String,
    /// Source entries (correction/fact IDs) that contributed to this rule.
    pub source_ids: Vec<Uuid>,
    /// How many source entries support this rule (higher = more confidence).
    pub support_count: usize,
    /// When this rule was distilled.
    pub created_at: DateTime<Utc>,
}

/// Persistent knowledge store containing distilled behavioral rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KnowledgeStore {
    pub rules: Vec<BehavioralRule>,
    /// Correction IDs already processed by the distiller.
    pub processed_correction_ids: Vec<Uuid>,
    /// Fact IDs already processed by the distiller.
    pub processed_fact_ids: Vec<Uuid>,
}

impl KnowledgeStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a knowledge store from a JSON file.
    pub fn load(path: &std::path::Path) -> Result<Self, MemoryError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let json = std::fs::read_to_string(path).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to read knowledge store: {}", e),
        })?;
        serde_json::from_str(&json).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to parse knowledge store: {}", e),
        })
    }

    /// Save the knowledge store to a JSON file.
    pub fn save(&self, path: &std::path::Path) -> Result<(), MemoryError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to create knowledge directory: {}", e),
            })?;
        }
        let json =
            serde_json::to_string_pretty(self).map_err(|e| MemoryError::PersistenceError {
                message: format!("Failed to serialize knowledge store: {}", e),
            })?;
        std::fs::write(path, json).map_err(|e| MemoryError::PersistenceError {
            message: format!("Failed to write knowledge store: {}", e),
        })
    }
}

/// The `KnowledgeDistiller` processes accumulated corrections and facts from
/// `LongTermMemory` to generate compressed behavioral rules. These rules
/// are injected into the system prompt to influence future agent behavior,
/// creating a cross-session learning loop.
pub struct KnowledgeDistiller {
    store: KnowledgeStore,
    max_rules: usize,
    min_entries: usize,
    store_path: Option<std::path::PathBuf>,
}

impl KnowledgeDistiller {
    /// Create a new distiller from config. If `config` is None, creates a
    /// disabled distiller that returns no rules.
    pub fn new(config: Option<&crate::config::KnowledgeConfig>) -> Self {
        match config {
            Some(cfg) if cfg.enabled => {
                let store = cfg
                    .knowledge_path
                    .as_ref()
                    .and_then(|p| KnowledgeStore::load(p).ok())
                    .unwrap_or_default();
                Self {
                    store,
                    max_rules: cfg.max_rules,
                    min_entries: cfg.min_entries_for_distillation,
                    store_path: cfg.knowledge_path.clone(),
                }
            }
            _ => Self {
                store: KnowledgeStore::new(),
                max_rules: 0,
                min_entries: usize::MAX,
                store_path: None,
            },
        }
    }

    /// Run the distillation process over long-term memory.
    ///
    /// Groups corrections by context patterns and generates compressed rules.
    /// Only processes entries not yet seen by the distiller.
    pub fn distill(&mut self, long_term: &LongTermMemory) {
        // Collect new (unprocessed) corrections
        let new_corrections: Vec<&Correction> = long_term
            .corrections
            .iter()
            .filter(|c| !self.store.processed_correction_ids.contains(&c.id))
            .collect();

        // Collect new (unprocessed) facts
        let new_facts: Vec<&Fact> = long_term
            .facts
            .iter()
            .filter(|f| !self.store.processed_fact_ids.contains(&f.id))
            .collect();

        let total_new = new_corrections.len() + new_facts.len();
        if total_new < self.min_entries {
            return; // Not enough new data to distill
        }

        // --- Distill corrections into rules ---
        // Group corrections by common patterns in their context field.
        let mut context_groups: HashMap<String, Vec<&Correction>> = HashMap::new();
        for correction in &new_corrections {
            // Normalize the context to a group key (first 50 chars, lowercased)
            let key = correction
                .context
                .chars()
                .take(50)
                .collect::<String>()
                .to_lowercase();
            context_groups.entry(key).or_default().push(correction);
        }

        // Generate a rule per group that has enough entries
        for group in context_groups.values() {
            if group.len() >= 2 {
                // Multiple corrections with similar context → a behavioral rule
                let corrected_patterns: Vec<&str> =
                    group.iter().map(|c| c.corrected.as_str()).collect();
                let rule_text = format!(
                    "Based on {} previous corrections: prefer {}",
                    group.len(),
                    corrected_patterns.join("; ")
                );
                let source_ids: Vec<Uuid> = group.iter().map(|c| c.id).collect();
                self.store.rules.push(BehavioralRule {
                    id: Uuid::new_v4(),
                    rule: rule_text,
                    source_ids,
                    support_count: group.len(),
                    created_at: Utc::now(),
                });
            } else {
                // Single correction → direct rule
                for c in group {
                    self.store.rules.push(BehavioralRule {
                        id: Uuid::new_v4(),
                        rule: format!("Instead of '{}', prefer '{}'", c.original, c.corrected),
                        source_ids: vec![c.id],
                        support_count: 1,
                        created_at: Utc::now(),
                    });
                }
            }
        }

        // --- Distill preferences from facts ---
        // Facts tagged with "preference" or containing directive language get
        // turned into rules directly.
        for fact in &new_facts {
            let is_preference = fact.tags.iter().any(|t| t == "preference")
                || fact.content.starts_with("Prefer")
                || fact.content.starts_with("Always")
                || fact.content.starts_with("Never")
                || fact.content.starts_with("Don't")
                || fact.content.starts_with("Use ");
            if is_preference {
                self.store.rules.push(BehavioralRule {
                    id: Uuid::new_v4(),
                    rule: fact.content.clone(),
                    source_ids: vec![fact.id],
                    support_count: 1,
                    created_at: Utc::now(),
                });
            }
        }

        // Mark all new entries as processed
        for c in &new_corrections {
            self.store.processed_correction_ids.push(c.id);
        }
        for f in &new_facts {
            self.store.processed_fact_ids.push(f.id);
        }

        // Trim rules to max_rules, keeping highest support_count
        if self.store.rules.len() > self.max_rules {
            self.store
                .rules
                .sort_by(|a, b| b.support_count.cmp(&a.support_count));
            self.store.rules.truncate(self.max_rules);
        }

        // Persist if a store path is configured
        if let Some(ref path) = self.store_path {
            let _ = self.store.save(path);
        }
    }

    /// Get the current distilled rules formatted for system prompt injection.
    ///
    /// Returns an empty string if no rules exist.
    pub fn rules_for_prompt(&self) -> String {
        if self.store.rules.is_empty() {
            return String::new();
        }
        let mut prompt = String::from(
            "\n\n## Learned Behavioral Rules\n\
             The following rules were distilled from previous sessions. Follow them:\n",
        );
        for (i, rule) in self.store.rules.iter().enumerate() {
            prompt.push_str(&format!("{}. {}\n", i + 1, rule.rule));
        }
        prompt
    }

    /// Get the number of distilled rules.
    pub fn rule_count(&self) -> usize {
        self.store.rules.len()
    }

    /// Get the knowledge store reference (for diagnostics/REPL).
    pub fn store(&self) -> &KnowledgeStore {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Content, Role};

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
        assert_eq!(
            wm.scratchpad.get("finding").map(|s| s.as_str()),
            Some("uses basic auth currently")
        );

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
        assert!(messages[0]
            .content
            .as_text()
            .unwrap()
            .contains("Summary of"));
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

    // --- Session persistence tests ---

    #[test]
    fn test_session_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("session.json");

        // Build a memory system with data
        let mut mem = MemorySystem::new(10);
        mem.start_new_task("fix bug #42");
        mem.add_message(Message::user("fix the bug"));
        mem.add_message(Message::assistant("Looking into it."));
        mem.long_term.add_fact(Fact::new("Uses Rust", "analysis"));
        mem.long_term.set_preference("style", "concise");

        // Save
        mem.save_session(&session_path).unwrap();
        assert!(session_path.exists());

        // Load
        let loaded = MemorySystem::load_session(&session_path).unwrap();
        assert_eq!(loaded.working.current_goal.as_deref(), Some("fix bug #42"));
        assert_eq!(loaded.short_term.len(), 2);
        assert_eq!(loaded.long_term.facts.len(), 1);
        assert_eq!(loaded.long_term.get_preference("style"), Some("concise"));

        // Verify message content
        let messages = loaded.context_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content.as_text(), Some("fix the bug"));
        assert_eq!(messages[1].content.as_text(), Some("Looking into it."));
    }

    #[test]
    fn test_session_load_missing_file() {
        let result = MemorySystem::load_session(Path::new("/nonexistent/session.json"));
        assert!(result.is_err());
    }

    #[test]
    fn test_session_load_corrupt_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json").unwrap();

        let result = MemorySystem::load_session(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_save_creates_directories() {
        let dir = tempfile::tempdir().unwrap();
        let session_path = dir.path().join("nested").join("dir").join("session.json");

        let mem = MemorySystem::new(5);
        mem.save_session(&session_path).unwrap();
        assert!(session_path.exists());
    }

    #[test]
    fn test_session_metadata() {
        let meta = SessionMetadata::new();
        assert!(meta.task_summary.is_none());
        assert!(meta.created_at <= Utc::now());

        let default_meta = SessionMetadata::default();
        assert!(default_meta.task_summary.is_none());
    }

    #[test]
    fn test_short_term_window_size() {
        let stm = ShortTermMemory::new(7);
        assert_eq!(stm.window_size(), 7);
    }

    // --- Pinning tests ---

    #[test]
    fn test_pin_message() {
        let mut stm = ShortTermMemory::new(5);
        stm.add(Message::user("msg 0"));
        stm.add(Message::user("msg 1"));
        stm.add(Message::user("msg 2"));

        assert!(stm.pin(1));
        assert!(stm.is_pinned(1));
        assert!(!stm.is_pinned(0));
        assert_eq!(stm.pinned_count(), 1);
    }

    #[test]
    fn test_pin_out_of_bounds() {
        let mut stm = ShortTermMemory::new(5);
        stm.add(Message::user("msg 0"));
        assert!(!stm.pin(5)); // out of bounds
    }

    #[test]
    fn test_unpin_message() {
        let mut stm = ShortTermMemory::new(5);
        stm.add(Message::user("msg 0"));
        stm.add(Message::user("msg 1"));

        stm.pin(0);
        assert!(stm.is_pinned(0));
        assert!(stm.unpin(0));
        assert!(!stm.is_pinned(0));
    }

    #[test]
    fn test_pinned_survives_compression() {
        let mut stm = ShortTermMemory::new(3);
        // Add enough messages to trigger compression
        stm.add(Message::user("old 0"));
        stm.add(Message::user("old 1"));
        stm.add(Message::user("important pinned"));
        stm.add(Message::user("msg 3"));
        stm.add(Message::user("msg 4"));
        stm.add(Message::user("msg 5"));

        // Pin the third message (index 2)
        stm.pin(2);
        assert!(stm.needs_compression());

        let removed = stm.compress("Summary of old messages".to_string());
        // The pinned message should be preserved
        assert!(removed < 3); // Not all 3 were removed because one was pinned

        // The pinned message should still be in the window
        let msgs = stm.to_messages();
        let has_pinned = msgs
            .iter()
            .any(|m| matches!(&m.content, Content::Text { text } if text == "important pinned"));
        assert!(has_pinned, "Pinned message should survive compression");
    }

    #[test]
    fn test_clear_resets_pins() {
        let mut stm = ShortTermMemory::new(5);
        stm.add(Message::user("msg 0"));
        stm.pin(0);
        assert_eq!(stm.pinned_count(), 1);

        stm.clear();
        assert_eq!(stm.pinned_count(), 0);
    }

    // --- Context Breakdown tests ---

    #[test]
    fn test_context_breakdown() {
        let mut memory = MemorySystem::new(10);
        memory.add_message(Message::user("hello world"));
        memory.add_message(Message::assistant("hi there!"));

        let ctx = memory.context_breakdown(8000);
        assert!(ctx.message_tokens > 0);
        assert_eq!(ctx.message_count, 2);
        assert_eq!(ctx.context_window, 8000);
        assert!(ctx.remaining_tokens > 0);
        assert!(!ctx.has_summary);
        assert_eq!(ctx.pinned_count, 0);
    }

    #[test]
    fn test_context_breakdown_ratio() {
        let ctx = ContextBreakdown {
            total_tokens: 4000,
            context_window: 8000,
            ..Default::default()
        };
        assert!((ctx.usage_ratio() - 0.5).abs() < 0.01);
        assert!(!ctx.is_warning());

        let ctx_high = ContextBreakdown {
            total_tokens: 7000,
            context_window: 8000,
            ..Default::default()
        };
        assert!(ctx_high.is_warning());
    }

    #[test]
    fn test_pin_message_via_memory_system() {
        let mut memory = MemorySystem::new(10);
        memory.add_message(Message::user("msg 0"));
        memory.add_message(Message::user("msg 1"));

        assert!(memory.pin_message(0));
        assert!(memory.short_term.is_pinned(0));
    }

    // --- Memory Flusher tests ---

    #[test]
    fn test_flusher_default_config() {
        let config = FlushConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.interval_secs, 300);
        assert_eq!(config.flush_on_message_count, 50);
        assert!(config.flush_path.is_none());
    }

    #[test]
    fn test_flusher_not_dirty_by_default() {
        let flusher = MemoryFlusher::new(FlushConfig::default());
        assert!(!flusher.is_dirty());
        assert_eq!(flusher.messages_since_flush(), 0);
        assert_eq!(flusher.total_flushes(), 0);
    }

    #[test]
    fn test_flusher_marks_dirty_on_message() {
        let mut flusher = MemoryFlusher::new(FlushConfig::default());
        flusher.on_message_added();
        assert!(flusher.is_dirty());
        assert_eq!(flusher.messages_since_flush(), 1);
    }

    #[test]
    fn test_flusher_disabled_never_triggers() {
        let mut flusher = MemoryFlusher::new(FlushConfig {
            enabled: false,
            ..FlushConfig::default()
        });
        for _ in 0..100 {
            flusher.on_message_added();
        }
        assert!(!flusher.should_flush());
    }

    #[test]
    fn test_flusher_message_count_trigger() {
        let mut flusher = MemoryFlusher::new(FlushConfig {
            enabled: true,
            flush_on_message_count: 5,
            interval_secs: 0,
            flush_path: None,
        });

        for _ in 0..4 {
            flusher.on_message_added();
        }
        assert!(!flusher.should_flush());

        flusher.on_message_added(); // 5th message
        assert!(flusher.should_flush());
    }

    #[test]
    fn test_flusher_not_dirty_no_trigger() {
        let flusher = MemoryFlusher::new(FlushConfig {
            enabled: true,
            flush_on_message_count: 1,
            interval_secs: 0,
            flush_path: None,
        });
        // Not dirty, so should_flush is false even though threshold is 1
        assert!(!flusher.should_flush());
    }

    #[test]
    fn test_flusher_flush_resets_state() {
        let dir = tempfile::tempdir().unwrap();
        let flush_path = dir.path().join("flush.json");

        let mut flusher = MemoryFlusher::new(FlushConfig {
            enabled: true,
            flush_on_message_count: 2,
            interval_secs: 0,
            flush_path: Some(flush_path.clone()),
        });

        let mut mem = MemorySystem::new(10);
        mem.add_message(Message::user("test"));

        flusher.on_message_added();
        flusher.on_message_added();
        assert!(flusher.should_flush());

        flusher.flush(&mem).unwrap();
        assert!(!flusher.is_dirty());
        assert_eq!(flusher.messages_since_flush(), 0);
        assert_eq!(flusher.total_flushes(), 1);
        assert!(flush_path.exists());
    }

    #[test]
    fn test_flusher_force_flush() {
        let dir = tempfile::tempdir().unwrap();
        let flush_path = dir.path().join("force.json");

        let mut flusher = MemoryFlusher::new(FlushConfig {
            enabled: true,
            flush_on_message_count: 100,
            interval_secs: 0,
            flush_path: Some(flush_path.clone()),
        });

        let mem = MemorySystem::new(10);

        // Not dirty - force_flush is a no-op
        flusher.force_flush(&mem).unwrap();
        assert_eq!(flusher.total_flushes(), 0);

        // Make dirty, then force
        flusher.on_message_added();
        flusher.force_flush(&mem).unwrap();
        assert_eq!(flusher.total_flushes(), 1);
        assert!(!flusher.is_dirty());
    }

    #[test]
    fn test_flusher_no_path_error() {
        let mut flusher = MemoryFlusher::new(FlushConfig {
            enabled: true,
            flush_on_message_count: 1,
            interval_secs: 0,
            flush_path: None,
        });
        flusher.on_message_added();

        let mem = MemorySystem::new(10);
        let result = flusher.flush(&mem);
        assert!(result.is_err());
    }

    #[test]
    fn test_flush_config_serialization() {
        let config = FlushConfig {
            enabled: true,
            interval_secs: 120,
            flush_on_message_count: 25,
            flush_path: Some(std::path::PathBuf::from("/tmp/flush.json")),
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: FlushConfig = serde_json::from_str(&json).unwrap();
        assert!(restored.enabled);
        assert_eq!(restored.interval_secs, 120);
        assert_eq!(restored.flush_on_message_count, 25);
    }

    // --- A4: HybridSearchEngine → MemorySystem integration tests ---

    #[test]
    fn test_memory_system_without_search_uses_keyword_fallback() {
        let mut mem = MemorySystem::new(10);
        mem.add_fact(Fact::new("Rust uses ownership for memory safety", "docs"));
        mem.add_fact(Fact::new("Python uses garbage collection", "docs"));

        let results = mem.search_facts_hybrid("ownership");
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("ownership"));
    }

    #[test]
    fn test_memory_system_with_search_engine() {
        let dir = tempfile::tempdir().unwrap();
        let config = SearchConfig {
            index_path: dir.path().join("idx"),
            db_path: dir.path().join("vec.db"),
            vector_dimensions: 64,
            full_text_weight: 0.5,
            vector_weight: 0.5,
            max_results: 10,
        };
        let mut mem = MemorySystem::with_search(10, config).unwrap();

        mem.add_fact(Fact::new("Rust uses ownership model", "analysis"));
        mem.add_fact(Fact::new("Python garbage collector", "analysis"));

        // The hybrid engine should find results (or fall back to keyword)
        let results = mem.search_facts_hybrid("ownership");
        assert!(!results.is_empty());
        assert!(results.iter().any(|f| f.content.contains("ownership")));
    }

    #[test]
    fn test_memory_system_search_empty_query() {
        let mut mem = MemorySystem::new(10);
        mem.add_fact(Fact::new("some fact", "source"));
        let results = mem.search_facts_hybrid("");
        // Empty query falls back to keyword search, which matches nothing
        // (no content matches empty substring in the keywords path)
        // Actually, empty string is contained in everything
        assert!(!results.is_empty());
    }

    #[test]
    fn test_memory_system_search_no_facts() {
        let mem = MemorySystem::new(10);
        let results = mem.search_facts_hybrid("anything");
        assert!(results.is_empty());
    }

    #[test]
    fn test_add_fact_indexes_into_search_engine() {
        let dir = tempfile::tempdir().unwrap();
        let config = SearchConfig {
            index_path: dir.path().join("idx"),
            db_path: dir.path().join("vec.db"),
            vector_dimensions: 64,
            full_text_weight: 0.5,
            vector_weight: 0.5,
            max_results: 10,
        };
        let mut mem = MemorySystem::with_search(10, config).unwrap();

        // Add multiple facts
        for i in 0..5 {
            mem.add_fact(Fact::new(format!("fact number {}", i), "test"));
        }

        // All 5 should be in long-term memory
        assert_eq!(mem.long_term.facts.len(), 5);
    }

    // --- A5: MemoryFlusher → MemorySystem integration tests ---

    #[test]
    fn test_memory_system_with_flusher() {
        let config = FlushConfig {
            enabled: true,
            flush_on_message_count: 5,
            interval_secs: 0,
            flush_path: None,
        };
        let mem = MemorySystem::new(10).with_flusher(config);
        assert!(!mem.flusher_is_dirty());
    }

    #[test]
    fn test_memory_system_add_message_notifies_flusher() {
        let config = FlushConfig {
            enabled: true,
            flush_on_message_count: 5,
            interval_secs: 0,
            flush_path: None,
        };
        let mut mem = MemorySystem::new(10).with_flusher(config);

        mem.add_message(Message::user("hello"));
        assert!(mem.flusher_is_dirty());
    }

    #[test]
    fn test_memory_system_check_auto_flush_no_flusher() {
        let mut mem = MemorySystem::new(10);
        // No flusher attached — should be a no-op returning Ok(false)
        let result = mem.check_auto_flush().unwrap();
        assert!(!result);
    }

    #[test]
    fn test_memory_system_check_auto_flush_triggers() {
        let dir = tempfile::tempdir().unwrap();
        let flush_path = dir.path().join("auto_flush.json");

        let config = FlushConfig {
            enabled: true,
            flush_on_message_count: 3,
            interval_secs: 0,
            flush_path: Some(flush_path.clone()),
        };
        let mut mem = MemorySystem::new(10).with_flusher(config);

        // Add messages below threshold
        mem.add_message(Message::user("msg 1"));
        mem.add_message(Message::user("msg 2"));
        assert!(!mem.check_auto_flush().unwrap());
        assert!(!flush_path.exists());

        // Hit the threshold
        mem.add_message(Message::user("msg 3"));
        assert!(mem.check_auto_flush().unwrap());
        assert!(flush_path.exists());

        // After flush, flusher should not be dirty
        assert!(!mem.flusher_is_dirty());
    }

    #[test]
    fn test_memory_system_force_flush() {
        let dir = tempfile::tempdir().unwrap();
        let flush_path = dir.path().join("force_flush.json");

        let config = FlushConfig {
            enabled: true,
            flush_on_message_count: 100, // high threshold
            interval_secs: 0,
            flush_path: Some(flush_path.clone()),
        };
        let mut mem = MemorySystem::new(10).with_flusher(config);

        mem.add_message(Message::user("important data"));
        assert!(mem.flusher_is_dirty());

        mem.force_flush().unwrap();
        assert!(!mem.flusher_is_dirty());
        assert!(flush_path.exists());
    }

    #[test]
    fn test_memory_system_force_flush_no_flusher() {
        let mut mem = MemorySystem::new(10);
        // Should be a no-op, not an error
        mem.force_flush().unwrap();
    }

    // --- Knowledge Distiller Tests ---

    #[test]
    fn test_knowledge_distiller_disabled() {
        let distiller = KnowledgeDistiller::new(None);
        assert_eq!(distiller.rule_count(), 0);
        assert!(distiller.rules_for_prompt().is_empty());
    }

    #[test]
    fn test_knowledge_distiller_no_data() {
        let config = crate::config::KnowledgeConfig::default();
        let mut distiller = KnowledgeDistiller::new(Some(&config));
        let ltm = LongTermMemory::new();
        distiller.distill(&ltm);
        assert_eq!(distiller.rule_count(), 0);
    }

    #[test]
    fn test_knowledge_distiller_corrections_below_threshold() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 5,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        ltm.add_correction(
            "unwrap()".into(),
            "? operator".into(),
            "error handling".into(),
        );
        ltm.add_correction("println!".into(), "tracing::info!".into(), "logging".into());

        distiller.distill(&ltm);
        // Only 2 entries, threshold is 5
        assert_eq!(distiller.rule_count(), 0);
    }

    #[test]
    fn test_knowledge_distiller_single_corrections() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 2,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        ltm.add_correction(
            "unwrap()".into(),
            "? operator".into(),
            "error handling".into(),
        );
        ltm.add_correction("println!".into(), "tracing::info!".into(), "logging".into());
        // Both have different contexts → separate rules

        distiller.distill(&ltm);
        assert_eq!(distiller.rule_count(), 2);

        let prompt = distiller.rules_for_prompt();
        assert!(prompt.contains("Learned Behavioral Rules"));
        assert!(prompt.contains("? operator"));
        assert!(prompt.contains("tracing::info!"));
    }

    #[test]
    fn test_knowledge_distiller_grouped_corrections() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 2,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        // Two corrections with same context prefix → should group
        ltm.add_correction(
            "unwrap()".into(),
            "? operator".into(),
            "error handling in Rust code".into(),
        );
        ltm.add_correction(
            "expect()".into(),
            "map_err()?".into(),
            "error handling in Rust code".into(),
        );

        distiller.distill(&ltm);
        assert_eq!(distiller.rule_count(), 1);
        let prompt = distiller.rules_for_prompt();
        assert!(prompt.contains("2 previous corrections"));
    }

    #[test]
    fn test_knowledge_distiller_preference_facts() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 1,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        ltm.add_fact(Fact::new("Prefer async/await over threads", "user"));
        ltm.add_fact(Fact::new("Project uses PostgreSQL", "session"));

        distiller.distill(&ltm);
        // Only the "Prefer..." fact becomes a rule
        assert_eq!(distiller.rule_count(), 1);
        let prompt = distiller.rules_for_prompt();
        assert!(prompt.contains("async/await"));
    }

    #[test]
    fn test_knowledge_distiller_max_rules_truncation() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 1,
            max_rules: 3,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        for i in 0..10 {
            ltm.add_correction(
                format!("old{}", i),
                format!("new{}", i),
                format!("context{}", i),
            );
        }

        distiller.distill(&ltm);
        assert!(distiller.rule_count() <= 3);
    }

    #[test]
    fn test_knowledge_distiller_idempotent() {
        let config = crate::config::KnowledgeConfig {
            min_entries_for_distillation: 1,
            ..Default::default()
        };
        let mut distiller = KnowledgeDistiller::new(Some(&config));

        let mut ltm = LongTermMemory::new();
        ltm.add_correction("old".into(), "new".into(), "ctx".into());

        distiller.distill(&ltm);
        let count_after_first = distiller.rule_count();

        // Distill again with same data — should not add duplicate rules
        distiller.distill(&ltm);
        assert_eq!(distiller.rule_count(), count_after_first);
    }

    #[test]
    fn test_knowledge_store_save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("knowledge.json");

        let mut store = KnowledgeStore::new();
        store.rules.push(BehavioralRule {
            id: Uuid::new_v4(),
            rule: "Prefer ? over unwrap".into(),
            source_ids: vec![Uuid::new_v4()],
            support_count: 3,
            created_at: Utc::now(),
        });

        store.save(&path).unwrap();
        let loaded = KnowledgeStore::load(&path).unwrap();
        assert_eq!(loaded.rules.len(), 1);
        assert_eq!(loaded.rules[0].rule, "Prefer ? over unwrap");
        assert_eq!(loaded.rules[0].support_count, 3);
    }

    #[test]
    fn test_knowledge_store_load_nonexistent() {
        let store =
            KnowledgeStore::load(std::path::Path::new("/nonexistent/knowledge.json")).unwrap();
        assert!(store.rules.is_empty());
    }

    #[test]
    fn test_unpin_out_of_bounds_returns_false() {
        let mut stm = ShortTermMemory::new(100);
        stm.add(Message::user("hello"));
        stm.add(Message::assistant("hi"));
        stm.add(Message::user("world"));

        // Pin a valid message
        assert!(stm.pin(1));
        assert!(stm.is_pinned(1));

        // Unpin out-of-bounds should return false
        assert!(!stm.unpin(999));
        assert!(!stm.unpin(3));

        // Original pin should still be intact
        assert!(stm.is_pinned(1));
    }

    #[test]
    fn test_unpin_at_exact_boundary() {
        let mut stm = ShortTermMemory::new(100);
        stm.add(Message::user("msg0"));
        stm.add(Message::user("msg1"));

        // Unpin at exactly len (2) should fail
        assert!(!stm.unpin(2));

        // Unpin at len-1 (1) should succeed even if not pinned (returns false from remove)
        assert!(!stm.unpin(1)); // not pinned, so remove returns false

        // Pin and then unpin at boundary
        assert!(stm.pin(1));
        assert!(stm.unpin(1));
        assert!(!stm.is_pinned(1));
    }
}
