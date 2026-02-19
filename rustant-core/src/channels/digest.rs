//! Channel Digest System.
//!
//! Collects classified messages over configurable time windows and generates
//! summarized digests. Digests are delivered via three outputs:
//! - TUI/REPL callback (`on_channel_digest`)
//! - Configurable channel (send as a message)
//! - Markdown file export to `.rustant/digests/`
//!
//! Digest frequency is controlled per-channel via `DigestFrequency`.

use super::intelligence::{ClassifiedMessage, MessageType};
use crate::config::{DigestFrequency, MessagePriority};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A highlight entry within a digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestHighlight {
    /// The channel the message came from.
    pub channel: String,
    /// The sender's display name or ID.
    pub sender: String,
    /// A brief summary of the message.
    pub summary: String,
    /// The classified priority.
    pub priority: MessagePriority,
}

/// An action item extracted from classified messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DigestActionItem {
    /// Description of what needs to be done.
    pub description: String,
    /// The channel the action came from.
    pub source_channel: String,
    /// The sender who triggered the action.
    pub source_sender: String,
    /// Optional deadline extracted from the message.
    pub deadline: Option<DateTime<Utc>>,
    /// Whether a reminder has been scheduled for this item.
    pub scheduled: bool,
}

/// A generated channel digest covering a time period.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelDigest {
    /// Unique identifier for this digest.
    pub id: Uuid,
    /// Start of the covered period.
    pub period_start: DateTime<Utc>,
    /// End of the covered period.
    pub period_end: DateTime<Utc>,
    /// Channels covered by this digest.
    pub channels_covered: Vec<String>,
    /// Total number of messages in the period.
    pub total_messages: usize,
    /// Generated summary text.
    pub summary: String,
    /// Notable messages highlighted for attention.
    pub highlights: Vec<DigestHighlight>,
    /// Action items extracted from messages.
    pub action_items: Vec<DigestActionItem>,
    /// Per-channel message counts.
    pub channel_counts: HashMap<String, usize>,
}

impl ChannelDigest {
    /// Generate a markdown representation of the digest.
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!(
            "# Channel Digest — {} to {}\n\n",
            self.period_start.format("%Y-%m-%d %H:%M"),
            self.period_end.format("%Y-%m-%d %H:%M"),
        ));

        md.push_str("## Summary\n\n");
        md.push_str(&format!(
            "Received {} messages across {} channels.",
            self.total_messages,
            self.channels_covered.len(),
        ));
        if !self.highlights.is_empty() {
            md.push_str(&format!(" {} need attention.", self.highlights.len()));
        }
        md.push_str("\n\n");

        if !self.summary.is_empty() {
            md.push_str(&self.summary);
            md.push_str("\n\n");
        }

        if !self.highlights.is_empty() {
            md.push_str("## Highlights\n\n");
            for h in &self.highlights {
                md.push_str(&format!(
                    "- **[{}]** {}: {} ({:?})\n",
                    crate::sanitize::escape_markdown(&h.channel),
                    crate::sanitize::escape_markdown(&h.sender),
                    crate::sanitize::escape_markdown(&h.summary),
                    h.priority,
                ));
            }
            md.push('\n');
        }

        if !self.action_items.is_empty() {
            md.push_str("## Action Items\n\n");
            for item in &self.action_items {
                let checkbox = if item.scheduled { "[x]" } else { "[ ]" };
                let deadline_str = item
                    .deadline
                    .map(|d| format!(" — deadline: {}", d.format("%Y-%m-%d")))
                    .unwrap_or_default();
                md.push_str(&format!(
                    "- {} {} ({}, {}){}\n",
                    checkbox,
                    crate::sanitize::escape_markdown(&item.description),
                    crate::sanitize::escape_markdown(&item.source_channel),
                    crate::sanitize::escape_markdown(&item.source_sender),
                    deadline_str,
                ));
            }
            md.push('\n');
        }

        if !self.channel_counts.is_empty() {
            md.push_str("## Channel Breakdown\n\n");
            let mut counts: Vec<_> = self.channel_counts.iter().collect();
            counts.sort_by(|a, b| b.1.cmp(a.1));
            for (channel, count) in counts {
                md.push_str(&format!(
                    "- **{}**: {} messages\n",
                    crate::sanitize::escape_markdown(channel),
                    count
                ));
            }
            md.push('\n');
        }

        md
    }
}

/// An entry in the digest collector, grouping messages by channel.
#[derive(Debug, Clone)]
struct DigestEntry {
    channel_name: String,
    sender: String,
    summary: String,
    priority: MessagePriority,
    message_type: MessageType,
    #[allow(dead_code)]
    timestamp: DateTime<Utc>,
}

/// Maximum number of digest entries held before oldest are dropped to prevent unbounded memory growth.
const MAX_DIGEST_ENTRIES: usize = 10_000;

/// Collects classified messages and generates periodic digests.
pub struct DigestCollector {
    /// Accumulated message entries grouped by channel.
    entries: Vec<DigestEntry>,
    /// When the current collection period started.
    period_start: DateTime<Utc>,
    /// Configured digest frequency.
    frequency: DigestFrequency,
    /// Directory for markdown file export.
    digest_dir: PathBuf,
}

impl DigestCollector {
    /// Create a new collector with the given frequency and export directory.
    pub fn new(frequency: DigestFrequency, digest_dir: PathBuf) -> Self {
        Self {
            entries: Vec::new(),
            period_start: Utc::now(),
            frequency,
            digest_dir,
        }
    }

    /// Add a classified message to the collector.
    pub fn add_message(&mut self, classified: &ClassifiedMessage, channel_name: &str) {
        let sender = classified
            .original
            .sender
            .display_name
            .clone()
            .unwrap_or_else(|| classified.original.sender.id.clone());

        let summary = match &classified.original.content {
            super::types::MessageContent::Text { text } => {
                if text.chars().count() > 120 {
                    format!("{}...", text.chars().take(120).collect::<String>())
                } else {
                    text.clone()
                }
            }
            super::types::MessageContent::Command { command, args } => {
                format!("/{} {}", command, args.join(" "))
            }
            super::types::MessageContent::File { filename, .. } => {
                format!("[File: {filename}]")
            }
            _ => "[media]".to_string(),
        };

        self.entries.push(DigestEntry {
            channel_name: channel_name.to_string(),
            sender,
            summary,
            priority: classified.priority,
            message_type: classified.message_type.clone(),
            timestamp: classified.classified_at,
        });

        // Evict oldest entries if over capacity to prevent unbounded memory growth
        if self.entries.len() > MAX_DIGEST_ENTRIES {
            let excess = self.entries.len() - MAX_DIGEST_ENTRIES;
            self.entries.drain(..excess);
        }
    }

    /// Check if a digest should be generated based on the configured frequency.
    pub fn should_generate(&self) -> bool {
        let elapsed = Utc::now() - self.period_start;
        match self.frequency {
            DigestFrequency::Off => false,
            DigestFrequency::Hourly => elapsed.num_hours() >= 1,
            DigestFrequency::Daily => elapsed.num_hours() >= 24,
            DigestFrequency::Weekly => elapsed.num_days() >= 7,
        }
    }

    /// Generate a digest from the collected messages and reset the collector.
    pub fn generate(&mut self) -> Option<ChannelDigest> {
        if self.entries.is_empty() || self.frequency == DigestFrequency::Off {
            return None;
        }

        let now = Utc::now();

        // Compute per-channel counts
        let mut channel_counts: HashMap<String, usize> = HashMap::new();
        for entry in &self.entries {
            *channel_counts
                .entry(entry.channel_name.clone())
                .or_default() += 1;
        }

        let channels_covered: Vec<String> = channel_counts.keys().cloned().collect();

        // Extract highlights (High/Urgent messages)
        let highlights: Vec<DigestHighlight> = self
            .entries
            .iter()
            .filter(|e| e.priority >= MessagePriority::High)
            .map(|e| DigestHighlight {
                channel: e.channel_name.clone(),
                sender: e.sender.clone(),
                summary: e.summary.clone(),
                priority: e.priority,
            })
            .collect();

        // Extract action items
        let action_items: Vec<DigestActionItem> = self
            .entries
            .iter()
            .filter(|e| e.message_type == MessageType::ActionRequired)
            .map(|e| DigestActionItem {
                description: e.summary.clone(),
                source_channel: e.channel_name.clone(),
                source_sender: e.sender.clone(),
                deadline: None,
                scheduled: false,
            })
            .collect();

        let total = self.entries.len();

        // Generate summary
        let summary = format!(
            "Processed {} messages across {} channels. {} highlights, {} action items.",
            total,
            channels_covered.len(),
            highlights.len(),
            action_items.len(),
        );

        let digest = ChannelDigest {
            id: Uuid::new_v4(),
            period_start: self.period_start,
            period_end: now,
            channels_covered,
            total_messages: total,
            summary,
            highlights,
            action_items,
            channel_counts,
        };

        // Reset for next period
        self.entries.clear();
        self.period_start = now;

        Some(digest)
    }

    /// Generate the file path for a digest export.
    pub fn digest_file_path(&self, digest: &ChannelDigest) -> PathBuf {
        let filename = format!("digest_{}.md", digest.period_end.format("%Y-%m-%d_%H%M"),);
        self.digest_dir.join(filename)
    }

    /// Export a digest to a markdown file.
    ///
    /// Returns the path the file was written to, or an error.
    pub fn export_markdown(&self, digest: &ChannelDigest) -> Result<PathBuf, std::io::Error> {
        let path = self.digest_file_path(digest);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, digest.to_markdown())?;
        Ok(path)
    }

    /// Get the number of messages collected in the current period.
    pub fn message_count(&self) -> usize {
        self.entries.len()
    }

    /// Get the configured digest frequency.
    pub fn frequency(&self) -> &DigestFrequency {
        &self.frequency
    }

    /// Get the digest export directory.
    pub fn digest_dir(&self) -> &Path {
        &self.digest_dir
    }

    /// Check if the collector is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::intelligence::{ClassifiedMessage, MessageType, SuggestedAction};
    use crate::channels::types::{
        ChannelMessage, ChannelType, ChannelUser, MessageContent, MessageId,
    };
    use crate::config::MessagePriority;
    use std::collections::HashMap;

    fn make_classified(
        text: &str,
        priority: MessagePriority,
        msg_type: MessageType,
        channel_type: ChannelType,
        sender_name: &str,
    ) -> ClassifiedMessage {
        let msg = ChannelMessage {
            id: MessageId::random(),
            channel_type,
            channel_id: "C123".to_string(),
            sender: ChannelUser::new("user1", channel_type).with_name(sender_name),
            content: MessageContent::Text {
                text: text.to_string(),
            },
            timestamp: Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: HashMap::new(),
        };
        ClassifiedMessage {
            original: msg,
            priority,
            message_type: msg_type,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        }
    }

    fn test_collector() -> DigestCollector {
        DigestCollector::new(
            DigestFrequency::Hourly,
            PathBuf::from("/tmp/rustant-test-digests"),
        )
    }

    #[test]
    fn test_collector_new_empty() {
        let collector = test_collector();
        assert!(collector.is_empty());
        assert_eq!(collector.message_count(), 0);
    }

    #[test]
    fn test_collector_add_message() {
        let mut collector = test_collector();
        let classified = make_classified(
            "Hello world",
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        collector.add_message(&classified, "slack");
        assert_eq!(collector.message_count(), 1);
        assert!(!collector.is_empty());
    }

    #[test]
    fn test_collector_add_multiple_channels() {
        let mut collector = test_collector();
        let msg1 = make_classified(
            "Slack message",
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        let msg2 = make_classified(
            "Email message",
            MessagePriority::High,
            MessageType::ActionRequired,
            ChannelType::Email,
            "Bob",
        );
        collector.add_message(&msg1, "slack");
        collector.add_message(&msg2, "email");
        assert_eq!(collector.message_count(), 2);
    }

    #[test]
    fn test_collector_should_not_generate_when_off() {
        let collector = DigestCollector::new(DigestFrequency::Off, PathBuf::from("/tmp"));
        assert!(!collector.should_generate());
    }

    #[test]
    fn test_collector_should_not_generate_too_soon() {
        let collector = test_collector(); // Hourly
        assert!(!collector.should_generate());
    }

    #[test]
    fn test_generate_empty_returns_none() {
        let mut collector = test_collector();
        assert!(collector.generate().is_none());
    }

    #[test]
    fn test_generate_off_returns_none() {
        let mut collector = DigestCollector::new(DigestFrequency::Off, PathBuf::from("/tmp"));
        let msg = make_classified(
            "Hello",
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        collector.add_message(&msg, "slack");
        assert!(collector.generate().is_none());
    }

    #[test]
    fn test_generate_digest() {
        let mut collector = test_collector();

        // Add various messages
        let msg1 = make_classified(
            "Normal notification",
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        let msg2 = make_classified(
            "URGENT: production down!",
            MessagePriority::Urgent,
            MessageType::ActionRequired,
            ChannelType::Slack,
            "Bob",
        );
        let msg3 = make_classified(
            "Please review PR #456",
            MessagePriority::Normal,
            MessageType::ActionRequired,
            ChannelType::Email,
            "Carol",
        );

        collector.add_message(&msg1, "slack");
        collector.add_message(&msg2, "slack");
        collector.add_message(&msg3, "email");

        let digest = collector.generate().unwrap();

        assert_eq!(digest.total_messages, 3);
        assert_eq!(digest.channels_covered.len(), 2);
        assert_eq!(digest.highlights.len(), 1); // Only the urgent one
        assert_eq!(digest.action_items.len(), 2); // Both ActionRequired
        assert_eq!(*digest.channel_counts.get("slack").unwrap(), 2);
        assert_eq!(*digest.channel_counts.get("email").unwrap(), 1);
    }

    #[test]
    fn test_generate_resets_collector() {
        let mut collector = test_collector();
        let msg = make_classified(
            "Hello",
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        collector.add_message(&msg, "slack");
        assert_eq!(collector.message_count(), 1);

        let _digest = collector.generate();
        assert_eq!(collector.message_count(), 0);
        assert!(collector.is_empty());
    }

    #[test]
    fn test_digest_to_markdown() {
        let digest = ChannelDigest {
            id: Uuid::new_v4(),
            period_start: Utc::now() - chrono::Duration::hours(1),
            period_end: Utc::now(),
            channels_covered: vec!["slack".to_string(), "email".to_string()],
            total_messages: 15,
            summary: "Active day with multiple action items.".to_string(),
            highlights: vec![DigestHighlight {
                channel: "slack".to_string(),
                sender: "Alice".to_string(),
                summary: "Production deployment scheduled".to_string(),
                priority: MessagePriority::High,
            }],
            action_items: vec![DigestActionItem {
                description: "Review PR #456".to_string(),
                source_channel: "email".to_string(),
                source_sender: "Bob".to_string(),
                deadline: None,
                scheduled: false,
            }],
            channel_counts: {
                let mut m = HashMap::new();
                m.insert("slack".to_string(), 10);
                m.insert("email".to_string(), 5);
                m
            },
        };

        let md = digest.to_markdown();
        assert!(md.contains("# Channel Digest"));
        assert!(md.contains("15 messages across 2 channels"));
        assert!(md.contains("## Highlights"));
        assert!(md.contains("Alice"));
        assert!(md.contains("## Action Items"));
        assert!(md.contains("Review PR \\#456"));
        assert!(md.contains("## Channel Breakdown"));
    }

    #[test]
    fn test_digest_file_path() {
        let collector = test_collector();
        let digest = ChannelDigest {
            id: Uuid::new_v4(),
            period_start: Utc::now(),
            period_end: Utc::now(),
            channels_covered: vec![],
            total_messages: 0,
            summary: String::new(),
            highlights: vec![],
            action_items: vec![],
            channel_counts: HashMap::new(),
        };
        let path = collector.digest_file_path(&digest);
        assert!(path.to_str().unwrap().contains("digest_"));
        assert!(path.to_str().unwrap().ends_with(".md"));
    }

    #[test]
    fn test_digest_highlights_only_high_priority() {
        let mut collector = test_collector();

        let low = make_classified(
            "Low priority",
            MessagePriority::Low,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        let normal = make_classified(
            "Normal priority",
            MessagePriority::Normal,
            MessageType::Question,
            ChannelType::Slack,
            "Bob",
        );
        let high = make_classified(
            "High priority",
            MessagePriority::High,
            MessageType::ActionRequired,
            ChannelType::Email,
            "Carol",
        );
        let urgent = make_classified(
            "Urgent!",
            MessagePriority::Urgent,
            MessageType::ActionRequired,
            ChannelType::Email,
            "Dave",
        );

        collector.add_message(&low, "slack");
        collector.add_message(&normal, "slack");
        collector.add_message(&high, "email");
        collector.add_message(&urgent, "email");

        let digest = collector.generate().unwrap();
        assert_eq!(digest.highlights.len(), 2); // High + Urgent only
    }

    #[test]
    fn test_digest_multibyte_utf8_truncation() {
        let mut collector = test_collector();
        // 130 CJK characters — each is 3 bytes in UTF-8, so byte length = 390
        // but char count = 130 > 120, so it should be truncated
        let cjk_text: String = "漢".repeat(130);
        let msg = make_classified(
            &cjk_text,
            MessagePriority::Normal,
            MessageType::Notification,
            ChannelType::Slack,
            "Alice",
        );
        collector.add_message(&msg, "slack");
        // Should not panic — this was the bug
        let digest = collector.generate().unwrap();
        assert_eq!(digest.total_messages, 1);
    }

    #[test]
    fn test_digest_emoji_truncation() {
        let mut collector = test_collector();
        // 130 emoji — each is 4 bytes in UTF-8
        let emoji_text: String = "🎉".repeat(130);
        let msg = make_classified(
            &emoji_text,
            MessagePriority::High,
            MessageType::ActionRequired,
            ChannelType::Email,
            "Bob",
        );
        collector.add_message(&msg, "email");
        // Should not panic
        let digest = collector.generate().unwrap();
        assert_eq!(digest.highlights.len(), 1);
        // The highlight summary should end with "..."
        assert!(digest.highlights[0].summary.ends_with("..."));
    }
}
