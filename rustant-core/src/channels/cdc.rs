//! Change Data Capture (CDC) for channel message processing.
//!
//! Provides stateful polling with cursor-based tracking, reply-chain detection,
//! and a background polling loop that feeds the classification -> auto-reply pipeline.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::style_tracker::CommunicationStyleTracker;

/// Per-channel cursor state for tracking which messages have been processed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CdcState {
    /// Per-channel cursors (channel_name -> cursor string).
    pub cursors: HashMap<String, String>,
    /// Message IDs we've sent (for reply-chain detection).
    /// Maps channel -> Vec<SentMessageRecord>.
    pub sent_messages: HashMap<String, Vec<SentMessageRecord>>,
}

/// Record of a message sent by the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentMessageRecord {
    pub message_id: String,
    pub channel: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl CdcState {
    /// Load state from disk.
    pub fn load(workspace: &Path) -> Self {
        let path = workspace.join(".rustant").join("cdc").join("state.json");
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            Self::default()
        }
    }

    /// Persist state to disk (atomic write).
    pub fn save(&self, workspace: &Path) -> Result<(), String> {
        let dir = workspace.join(".rustant").join("cdc");
        std::fs::create_dir_all(&dir).map_err(|e| format!("Create CDC dir: {e}"))?;
        let path = dir.join("state.json");
        let tmp = path.with_extension("json.tmp");
        let json =
            serde_json::to_string_pretty(self).map_err(|e| format!("Serialize CDC state: {e}"))?;
        std::fs::write(&tmp, &json).map_err(|e| format!("Write CDC state: {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("Rename CDC state: {e}"))?;
        Ok(())
    }

    /// Get the cursor for a specific channel.
    pub fn cursor_for(&self, channel: &str) -> Option<&str> {
        self.cursors.get(channel).map(|s| s.as_str())
    }

    /// Update the cursor for a channel.
    pub fn set_cursor(&mut self, channel: &str, cursor: String) {
        self.cursors.insert(channel.to_string(), cursor);
    }

    /// Record a sent message for reply-chain detection.
    pub fn record_sent(&mut self, channel: &str, message_id: &str) {
        let records = self.sent_messages.entry(channel.to_string()).or_default();
        records.push(SentMessageRecord {
            message_id: message_id.to_string(),
            channel: channel.to_string(),
            timestamp: chrono::Utc::now(),
        });
    }

    /// Check if a message is a reply to one of our sent messages.
    pub fn is_reply_to_us(&self, channel: &str, reply_to: &str) -> bool {
        self.sent_messages
            .get(channel)
            .map(|records| records.iter().any(|r| r.message_id == reply_to))
            .unwrap_or(false)
    }

    /// Expire sent message records older than `ttl_days`.
    pub fn expire_sent_records(&mut self, ttl_days: u64) {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(ttl_days as i64);
        for records in self.sent_messages.values_mut() {
            records.retain(|r| r.timestamp > cutoff);
        }
        // Remove empty channels
        self.sent_messages.retain(|_, v| !v.is_empty());
    }
}

/// Action emitted by the CDC processor for the REPL/TUI to handle.
#[derive(Debug, Clone)]
pub enum CdcAction {
    /// Auto-reply ready to send (channel, message_text, reply_to_id).
    Reply {
        channel: String,
        text: String,
        reply_to: Option<String>,
    },
    /// Message requires user attention (escalation).
    Escalate {
        channel: String,
        sender: String,
        summary: String,
    },
    /// Message added to digest for later review.
    AddToDigest {
        channel: String,
        sender: String,
        preview: String,
    },
    /// Status update for display.
    StatusUpdate(String),
}

/// Configuration for the CDC polling system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcConfig {
    /// Whether CDC polling is enabled.
    pub enabled: bool,
    /// Default polling interval in seconds.
    pub default_interval_secs: u64,
    /// Per-channel polling interval overrides.
    #[serde(default)]
    pub channel_intervals: HashMap<String, u64>,
    /// Per-channel enable/disable.
    #[serde(default)]
    pub channel_enabled: HashMap<String, bool>,
    /// How long to keep sent message records (days).
    pub sent_record_ttl_days: u64,
    /// Number of messages before generating style facts.
    pub style_fact_threshold: usize,
}

impl Default for CdcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_interval_secs: 60,
            channel_intervals: HashMap::new(),
            channel_enabled: HashMap::new(),
            sent_record_ttl_days: 7,
            style_fact_threshold: 50,
        }
    }
}

impl CdcConfig {
    /// Get the polling interval for a specific channel.
    pub fn interval_for(&self, channel: &str) -> u64 {
        self.channel_intervals
            .get(channel)
            .copied()
            .unwrap_or(self.default_interval_secs)
    }

    /// Check if a specific channel is enabled for CDC.
    pub fn is_channel_enabled(&self, channel: &str) -> bool {
        self.channel_enabled.get(channel).copied().unwrap_or(true) // enabled by default
    }
}

/// The CDC processor that coordinates polling, classification, and action emission.
pub struct CdcProcessor {
    pub config: CdcConfig,
    pub state: CdcState,
    pub style_tracker: CommunicationStyleTracker,
    workspace: PathBuf,
}

impl CdcProcessor {
    /// Create a new CDC processor.
    pub fn new(config: CdcConfig, workspace: PathBuf) -> Self {
        let state = CdcState::load(&workspace);
        let style_tracker = CommunicationStyleTracker::new(config.style_fact_threshold);
        Self {
            config,
            state,
            style_tracker,
            workspace,
        }
    }

    /// Process a batch of new messages from a channel.
    ///
    /// Returns CDC actions and any style facts generated.
    pub fn process_messages(
        &mut self,
        channel: &str,
        messages: &[(String, String, String, Option<String>)], // (id, sender, text, reply_to)
    ) -> (Vec<CdcAction>, Vec<String>) {
        let mut actions = Vec::new();
        let mut facts = Vec::new();

        for (msg_id, sender, text, reply_to) in messages {
            // Track communication style
            let style_facts = self.style_tracker.track_message(sender, channel, text);
            facts.extend(style_facts);

            // Check if this is a reply to one of our messages
            let is_reply_to_us = reply_to
                .as_ref()
                .map(|rt| self.state.is_reply_to_us(channel, rt))
                .unwrap_or(false);

            // Simple heuristic classification
            if is_reply_to_us {
                // Replies to us get escalated for attention
                actions.push(CdcAction::Escalate {
                    channel: channel.to_string(),
                    sender: sender.clone(),
                    summary: truncate(text, 100),
                });
            } else if looks_like_question(text) {
                // Questions might need auto-reply
                actions.push(CdcAction::Reply {
                    channel: channel.to_string(),
                    text: "Received your question. Processing...".to_string(),
                    reply_to: Some(msg_id.clone()),
                });
            } else {
                // Other messages go to digest
                actions.push(CdcAction::AddToDigest {
                    channel: channel.to_string(),
                    sender: sender.clone(),
                    preview: truncate(text, 80),
                });
            }
        }

        // Update cursor to the last message ID
        if let Some((last_id, _, _, _)) = messages.last() {
            self.state.set_cursor(channel, last_id.clone());
        }

        // Expire old sent records
        self.state
            .expire_sent_records(self.config.sent_record_ttl_days);

        // Persist state
        if let Err(e) = self.state.save(&self.workspace) {
            tracing::warn!("Failed to save CDC state: {}", e);
        }

        (actions, facts)
    }

    /// Record that we sent a message (for reply-chain detection).
    pub fn record_sent_message(&mut self, channel: &str, message_id: &str) {
        self.state.record_sent(channel, message_id);
        let _ = self.state.save(&self.workspace);
    }

    /// Get the current CDC state summary for display.
    pub fn status_summary(&self) -> String {
        let mut output = String::from("CDC Status:\n");
        output.push_str(&format!("  Enabled: {}\n", self.config.enabled));
        output.push_str(&format!(
            "  Default interval: {}s\n",
            self.config.default_interval_secs
        ));
        output.push_str(&format!(
            "  Channels with cursors: {}\n",
            self.state.cursors.len()
        ));
        for (ch, cursor) in &self.state.cursors {
            output.push_str(&format!("    {ch} -> {cursor}\n"));
        }
        output.push_str(&format!(
            "  Style profiles tracked: {}\n",
            self.style_tracker.profiles.len()
        ));
        output.push_str(&format!(
            "  Total messages processed: {}\n",
            self.style_tracker.total_messages
        ));
        output
    }
}

/// Simple heuristic: does this message look like a question?
fn looks_like_question(text: &str) -> bool {
    text.trim().ends_with('?')
        || text.to_lowercase().starts_with("can ")
        || text.to_lowercase().starts_with("could ")
        || text.to_lowercase().starts_with("how ")
        || text.to_lowercase().starts_with("what ")
        || text.to_lowercase().starts_with("when ")
        || text.to_lowercase().starts_with("where ")
        || text.to_lowercase().starts_with("why ")
        || text.to_lowercase().starts_with("is ")
        || text.to_lowercase().starts_with("are ")
        || text.to_lowercase().starts_with("do ")
        || text.to_lowercase().starts_with("does ")
}

/// Truncate a string to max length with "...".
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cdc_state_roundtrip() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        let mut state = CdcState::default();
        state.set_cursor("slack", "123.456".into());
        state.record_sent("slack", "789.012");
        state.save(&workspace).unwrap();

        let loaded = CdcState::load(&workspace);
        assert_eq!(loaded.cursor_for("slack"), Some("123.456"));
        assert!(loaded.is_reply_to_us("slack", "789.012"));
    }

    #[test]
    fn test_cdc_config_defaults() {
        let config = CdcConfig::default();
        assert!(config.enabled);
        assert_eq!(config.default_interval_secs, 60);
        assert_eq!(config.interval_for("slack"), 60);
        assert!(config.is_channel_enabled("slack"));
    }

    #[test]
    fn test_cdc_config_channel_overrides() {
        let mut config = CdcConfig::default();
        config.channel_intervals.insert("slack".into(), 120);
        config.channel_enabled.insert("irc".into(), false);

        assert_eq!(config.interval_for("slack"), 120);
        assert_eq!(config.interval_for("email"), 60); // default
        assert!(!config.is_channel_enabled("irc"));
        assert!(config.is_channel_enabled("slack"));
    }

    #[test]
    fn test_cdc_processor_process_messages() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let config = CdcConfig {
            style_fact_threshold: 50,
            ..Default::default()
        };
        let mut processor = CdcProcessor::new(config, workspace);

        let messages = vec![
            (
                "1".into(),
                "user1".into(),
                "Can you help me with this?".into(),
                None,
            ),
            (
                "2".into(),
                "user2".into(),
                "Just an update on the project".into(),
                None,
            ),
        ];

        let (actions, _facts) = processor.process_messages("slack", &messages);
        assert_eq!(actions.len(), 2);
        // First is a question -> Reply
        assert!(matches!(&actions[0], CdcAction::Reply { .. }));
        // Second is not a question -> AddToDigest
        assert!(matches!(&actions[1], CdcAction::AddToDigest { .. }));
    }

    #[test]
    fn test_reply_chain_detection() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let config = CdcConfig::default();
        let mut processor = CdcProcessor::new(config, workspace);

        // Record that we sent a message
        processor.record_sent_message("slack", "our_msg_123");

        // Process a reply to our message
        let messages = vec![(
            "reply_1".into(),
            "user1".into(),
            "Thanks for that info!".into(),
            Some("our_msg_123".into()),
        )];

        let (actions, _) = processor.process_messages("slack", &messages);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], CdcAction::Escalate { .. }));
    }

    #[test]
    fn test_sent_record_expiry() {
        let mut state = CdcState::default();
        state.record_sent("slack", "old_msg");

        // Manually set timestamp to 10 days ago
        if let Some(records) = state.sent_messages.get_mut("slack") {
            records[0].timestamp = chrono::Utc::now() - chrono::Duration::days(10);
        }

        state.expire_sent_records(7);
        assert!(!state.is_reply_to_us("slack", "old_msg"));
    }

    #[test]
    fn test_looks_like_question() {
        assert!(looks_like_question("How do I do this?"));
        assert!(looks_like_question("Can you help me"));
        assert!(looks_like_question("What is the status?"));
        assert!(!looks_like_question("Just an update"));
        assert!(!looks_like_question("Thanks for the info"));
    }

    #[test]
    fn test_status_summary() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let config = CdcConfig::default();
        let processor = CdcProcessor::new(config, workspace);

        let summary = processor.status_summary();
        assert!(summary.contains("CDC Status"));
        assert!(summary.contains("Enabled: true"));
    }
}
