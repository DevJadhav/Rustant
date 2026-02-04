//! Auto-Reply Engine for channel intelligence.
//!
//! Generates and manages automatic responses to classified channel messages.
//! Respects the configured `AutoReplyMode` and gates all outgoing replies
//! through the `SafetyGuardian` approval system.
//!
//! # Reply Flow
//!
//! 1. Receive a `ClassifiedMessage` with a suggested action
//! 2. Generate a reply draft (via LLM or template)
//! 3. Apply safety gating based on `AutoReplyMode` and priority
//! 4. Either send, queue for approval, or store as draft
//! 5. Record outcome for learning feedback

use super::intelligence::{ClassifiedMessage, SuggestedAction};
use crate::config::{AutoReplyMode, IntelligenceConfig, MessagePriority};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a pending auto-reply in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplyStatus {
    /// LLM is generating the response.
    Drafting,
    /// Queued for user approval.
    PendingApproval,
    /// User approved, ready to send.
    Approved,
    /// Successfully delivered to channel.
    Sent,
    /// User rejected the reply.
    Rejected,
    /// Timed out waiting for approval.
    Expired,
}

/// A pending auto-reply awaiting approval or delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingReply {
    /// Unique identifier for this reply.
    pub id: Uuid,
    /// The channel through which the reply will be sent.
    pub channel_name: String,
    /// The sender to reply to.
    pub recipient: String,
    /// The original message summary.
    pub original_summary: String,
    /// The original message priority.
    pub priority: MessagePriority,
    /// The generated draft response text.
    pub draft_response: String,
    /// Current status of the reply.
    pub status: ReplyStatus,
    /// When the reply was created.
    pub created_at: DateTime<Utc>,
    /// When the reply was last updated.
    pub updated_at: DateTime<Utc>,
    /// Reasoning for the auto-reply decision (for audit).
    pub reasoning: String,
}

impl PendingReply {
    /// Create a new pending reply in drafting state.
    pub fn new(
        channel_name: impl Into<String>,
        recipient: impl Into<String>,
        original_summary: impl Into<String>,
        priority: MessagePriority,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            channel_name: channel_name.into(),
            recipient: recipient.into(),
            original_summary: original_summary.into(),
            priority,
            draft_response: String::new(),
            status: ReplyStatus::Drafting,
            created_at: now,
            updated_at: now,
            reasoning: String::new(),
        }
    }

    /// Set the draft response text.
    pub fn with_draft(mut self, draft: impl Into<String>) -> Self {
        self.draft_response = draft.into();
        self.status = ReplyStatus::PendingApproval;
        self.updated_at = Utc::now();
        self
    }

    /// Set the reasoning for the auto-reply decision.
    pub fn with_reasoning(mut self, reasoning: impl Into<String>) -> Self {
        self.reasoning = reasoning.into();
        self
    }

    /// Attempt to approve the reply. Only valid from `PendingApproval` state.
    pub fn try_approve(&mut self) -> Result<(), &'static str> {
        match self.status {
            ReplyStatus::PendingApproval => {
                self.status = ReplyStatus::Approved;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err("can only approve a reply in PendingApproval state"),
        }
    }

    /// Attempt to reject the reply. Only valid from `PendingApproval` or `Drafting` state.
    pub fn try_reject(&mut self) -> Result<(), &'static str> {
        match self.status {
            ReplyStatus::PendingApproval | ReplyStatus::Drafting => {
                self.status = ReplyStatus::Rejected;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err("can only reject a reply in PendingApproval or Drafting state"),
        }
    }

    /// Attempt to mark the reply as sent. Only valid from `Approved` state.
    pub fn try_mark_sent(&mut self) -> Result<(), &'static str> {
        match self.status {
            ReplyStatus::Approved => {
                self.status = ReplyStatus::Sent;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err("can only mark as sent a reply in Approved state"),
        }
    }

    /// Attempt to expire the reply. Only valid from `PendingApproval` or `Drafting` state.
    pub fn try_expire(&mut self) -> Result<(), &'static str> {
        match self.status {
            ReplyStatus::PendingApproval | ReplyStatus::Drafting => {
                self.status = ReplyStatus::Expired;
                self.updated_at = Utc::now();
                Ok(())
            }
            _ => Err("can only expire a reply in PendingApproval or Drafting state"),
        }
    }

    /// Check if the reply is actionable (can be sent or needs approval).
    pub fn is_actionable(&self) -> bool {
        matches!(
            self.status,
            ReplyStatus::PendingApproval | ReplyStatus::Approved
        )
    }
}

/// The auto-reply engine manages reply generation and lifecycle.
///
/// It determines whether to auto-send, queue for approval, or draft-only
/// based on the `AutoReplyMode` and the message priority.
///
/// **Important**: Rate limiting is per-instance. The system should use a single
/// shared `AutoReplyEngine` instance to enforce rate limits correctly. Creating
/// multiple instances would bypass the per-channel rate limiting.
pub struct AutoReplyEngine {
    /// Per-channel intelligence configuration defaults.
    config: IntelligenceConfig,
    /// Queue of pending replies.
    pending_replies: Vec<PendingReply>,
    /// Timestamps of sent replies for time-based windowed rate limiting.
    reply_timestamps: std::collections::VecDeque<DateTime<Utc>>,
    /// Maximum replies per hour per channel (rate limit).
    max_replies_per_hour: usize,
}

impl AutoReplyEngine {
    /// Create a new auto-reply engine with the given intelligence config.
    pub fn new(config: IntelligenceConfig) -> Self {
        let max_replies = config.max_reply_tokens / 100; // rough heuristic
        Self {
            config,
            pending_replies: Vec::new(),
            reply_timestamps: std::collections::VecDeque::new(),
            max_replies_per_hour: max_replies.max(10),
        }
    }

    /// Check if the rate limit has been reached within the sliding 1-hour window.
    fn is_rate_limited(&mut self) -> bool {
        let cutoff = Utc::now() - chrono::Duration::hours(1);
        while self.reply_timestamps.front().is_some_and(|t| *t < cutoff) {
            self.reply_timestamps.pop_front();
        }
        self.reply_timestamps.len() >= self.max_replies_per_hour
    }

    /// Process a classified message and determine the reply action.
    ///
    /// Returns `Some(PendingReply)` if a reply should be generated,
    /// or `None` if no reply is needed.
    pub fn process_classified(
        &mut self,
        classified: &ClassifiedMessage,
        channel_name: &str,
    ) -> Option<PendingReply> {
        // Check rate limit (sliding 1-hour window)
        if self.is_rate_limited() {
            return None;
        }

        let channel_config = self.config.for_channel(channel_name);

        match &classified.suggested_action {
            SuggestedAction::AutoReply => {
                let reply = self.create_reply(classified, channel_name, &channel_config.auto_reply);
                Some(reply)
            }
            SuggestedAction::DraftReply => {
                let mut reply =
                    self.create_reply(classified, channel_name, &AutoReplyMode::DraftOnly);
                reply.status = ReplyStatus::PendingApproval;
                Some(reply)
            }
            _ => None,
        }
    }

    /// Create a pending reply for the classified message.
    fn create_reply(
        &self,
        classified: &ClassifiedMessage,
        channel_name: &str,
        mode: &AutoReplyMode,
    ) -> PendingReply {
        let recipient = classified
            .original
            .sender
            .display_name
            .clone()
            .unwrap_or_else(|| classified.original.sender.id.clone());

        let original_summary = match &classified.original.content {
            super::types::MessageContent::Text { text } => {
                if text.chars().count() > 100 {
                    let truncated: String = text.chars().take(100).collect();
                    format!("{}...", truncated)
                } else {
                    text.clone()
                }
            }
            _ => format!("{:?}", classified.message_type),
        };

        let status = match (mode, &classified.priority) {
            // FullAuto + Low/Normal -> auto-approve (will be sent immediately)
            (AutoReplyMode::FullAuto, MessagePriority::Low)
            | (AutoReplyMode::FullAuto, MessagePriority::Normal) => ReplyStatus::Approved,
            // FullAuto + High/Urgent -> needs approval
            (AutoReplyMode::FullAuto, _) => ReplyStatus::PendingApproval,
            // AutoWithApproval -> always needs approval
            (AutoReplyMode::AutoWithApproval, _) => ReplyStatus::PendingApproval,
            // DraftOnly -> just a draft
            (AutoReplyMode::DraftOnly, _) => ReplyStatus::PendingApproval,
            // Disabled -> shouldn't reach here, but treat as draft
            (AutoReplyMode::Disabled, _) => ReplyStatus::PendingApproval,
        };

        let reasoning = format!(
            "Auto-reply mode={:?}, priority={:?}, type={:?}, classification_confidence={:.2}",
            mode, classified.priority, classified.message_type, classified.confidence,
        );

        PendingReply {
            id: Uuid::new_v4(),
            channel_name: channel_name.to_string(),
            recipient,
            original_summary,
            priority: classified.priority,
            draft_response: String::new(), // Will be filled by LLM
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            reasoning,
        }
    }

    /// Add a reply to the pending queue.
    pub fn enqueue(&mut self, reply: PendingReply) {
        self.pending_replies.push(reply);
    }

    /// Get all pending replies that need approval.
    pub fn pending_approval(&self) -> Vec<&PendingReply> {
        self.pending_replies
            .iter()
            .filter(|r| r.status == ReplyStatus::PendingApproval)
            .collect()
    }

    /// Get all approved replies ready to send.
    pub fn ready_to_send(&self) -> Vec<&PendingReply> {
        self.pending_replies
            .iter()
            .filter(|r| r.status == ReplyStatus::Approved)
            .collect()
    }

    /// Approve a pending reply by ID. Returns false if not found or invalid state.
    pub fn approve_reply(&mut self, id: Uuid) -> bool {
        if let Some(reply) = self.pending_replies.iter_mut().find(|r| r.id == id) {
            reply.try_approve().is_ok()
        } else {
            false
        }
    }

    /// Reject a pending reply by ID. Returns false if not found or invalid state.
    pub fn reject_reply(&mut self, id: Uuid) -> bool {
        if let Some(reply) = self.pending_replies.iter_mut().find(|r| r.id == id) {
            reply.try_reject().is_ok()
        } else {
            false
        }
    }

    /// Mark a reply as sent and record the timestamp for rate limiting. Returns false if not found or invalid state.
    pub fn mark_sent(&mut self, id: Uuid) -> bool {
        if let Some(reply) = self.pending_replies.iter_mut().find(|r| r.id == id) {
            if reply.try_mark_sent().is_ok() {
                self.reply_timestamps.push_back(Utc::now());
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    /// Remove all completed (sent, rejected, expired) replies from the queue.
    pub fn cleanup_completed(&mut self) -> usize {
        let before = self.pending_replies.len();
        self.pending_replies.retain(|r| {
            !matches!(
                r.status,
                ReplyStatus::Sent | ReplyStatus::Rejected | ReplyStatus::Expired
            )
        });
        before - self.pending_replies.len()
    }

    /// Get the total number of pending replies.
    pub fn pending_count(&self) -> usize {
        self.pending_replies.len()
    }

    /// Get the number of replies sent within the current 1-hour window.
    pub fn sent_count(&self) -> usize {
        let cutoff = Utc::now() - chrono::Duration::hours(1);
        self.reply_timestamps
            .iter()
            .filter(|t| **t >= cutoff)
            .count()
    }

    /// Reset the rate limit by clearing all timestamps. Kept for backward compatibility.
    pub fn reset_rate_limit(&mut self) {
        self.reply_timestamps.clear();
    }

    /// Expire all pending replies older than the given duration.
    pub fn expire_old_replies(&mut self, max_age_secs: i64) -> usize {
        let cutoff = Utc::now() - chrono::Duration::seconds(max_age_secs);
        let mut expired = 0;
        for reply in &mut self.pending_replies {
            if reply.created_at < cutoff && reply.try_expire().is_ok() {
                expired += 1;
            }
        }
        expired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::intelligence::{ClassifiedMessage, MessageType, SuggestedAction};
    use crate::channels::types::{
        ChannelMessage, ChannelType, ChannelUser, MessageContent, MessageId,
    };
    use crate::config::{IntelligenceConfig, MessagePriority};
    use std::collections::HashMap;

    fn make_classified(
        text: &str,
        priority: MessagePriority,
        msg_type: MessageType,
        action: SuggestedAction,
    ) -> ClassifiedMessage {
        let msg = ChannelMessage {
            id: MessageId::random(),
            channel_type: ChannelType::Slack,
            channel_id: "C123".to_string(),
            sender: ChannelUser::new("alice", ChannelType::Slack).with_name("Alice"),
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
            suggested_action: action,
            confidence: 0.85,
            reasoning: "test classification".to_string(),
            classified_at: Utc::now(),
        }
    }

    fn default_engine() -> AutoReplyEngine {
        AutoReplyEngine::new(IntelligenceConfig::default())
    }

    // --- PendingReply Tests ---

    #[test]
    fn test_pending_reply_new() {
        let reply = PendingReply::new("slack", "alice", "Hello?", MessagePriority::Normal);
        assert_eq!(reply.channel_name, "slack");
        assert_eq!(reply.recipient, "alice");
        assert_eq!(reply.status, ReplyStatus::Drafting);
        assert!(reply.draft_response.is_empty());
    }

    #[test]
    fn test_pending_reply_with_draft() {
        let reply = PendingReply::new("slack", "alice", "Hello?", MessagePriority::Normal)
            .with_draft("Hi there! How can I help?");
        assert_eq!(reply.status, ReplyStatus::PendingApproval);
        assert_eq!(reply.draft_response, "Hi there! How can I help?");
    }

    #[test]
    fn test_pending_reply_lifecycle() {
        let mut reply = PendingReply::new("slack", "alice", "Hello?", MessagePriority::Normal)
            .with_draft("Reply text");

        assert_eq!(reply.status, ReplyStatus::PendingApproval);
        assert!(reply.is_actionable());

        reply.try_approve().unwrap();
        assert_eq!(reply.status, ReplyStatus::Approved);
        assert!(reply.is_actionable());

        reply.try_mark_sent().unwrap();
        assert_eq!(reply.status, ReplyStatus::Sent);
        assert!(!reply.is_actionable());
    }

    #[test]
    fn test_pending_reply_reject() {
        let mut reply = PendingReply::new("slack", "alice", "Hello?", MessagePriority::Normal)
            .with_draft("Reply text");
        reply.try_reject().unwrap();
        assert_eq!(reply.status, ReplyStatus::Rejected);
        assert!(!reply.is_actionable());
    }

    #[test]
    fn test_pending_reply_expire() {
        let mut reply = PendingReply::new("slack", "alice", "Hello?", MessagePriority::Normal)
            .with_draft("Reply text");
        reply.try_expire().unwrap();
        assert_eq!(reply.status, ReplyStatus::Expired);
        assert!(!reply.is_actionable());
    }

    #[test]
    fn test_try_approve_from_rejected_fails() {
        let mut reply =
            PendingReply::new("slack", "alice", "Q", MessagePriority::Normal).with_draft("Reply");
        reply.try_reject().unwrap();
        assert!(reply.try_approve().is_err());
    }

    #[test]
    fn test_try_mark_sent_from_pending_fails() {
        let mut reply =
            PendingReply::new("slack", "alice", "Q", MessagePriority::Normal).with_draft("Reply");
        assert!(reply.try_mark_sent().is_err());
    }

    #[test]
    fn test_try_expire_from_sent_fails() {
        let mut reply =
            PendingReply::new("slack", "alice", "Q", MessagePriority::Normal).with_draft("Reply");
        reply.try_approve().unwrap();
        reply.try_mark_sent().unwrap();
        assert!(reply.try_expire().is_err());
    }

    #[test]
    fn test_valid_full_lifecycle_path() {
        let mut reply = PendingReply::new("slack", "alice", "Q", MessagePriority::Normal);
        assert_eq!(reply.status, ReplyStatus::Drafting);
        // Drafting -> PendingApproval (via with_draft)
        reply = reply.with_draft("Draft text");
        assert_eq!(reply.status, ReplyStatus::PendingApproval);
        // PendingApproval -> Approved
        reply.try_approve().unwrap();
        assert_eq!(reply.status, ReplyStatus::Approved);
        // Approved -> Sent
        reply.try_mark_sent().unwrap();
        assert_eq!(reply.status, ReplyStatus::Sent);
    }

    // --- AutoReplyEngine Tests ---

    #[test]
    fn test_engine_process_auto_reply_full_auto_normal() {
        let mut engine = default_engine();
        let classified = make_classified(
            "What time is the meeting?",
            MessagePriority::Normal,
            MessageType::Question,
            SuggestedAction::AutoReply,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(reply.is_some());
        let reply = reply.unwrap();
        // FullAuto + Normal -> auto-approved
        assert_eq!(reply.status, ReplyStatus::Approved);
        assert_eq!(reply.recipient, "Alice");
    }

    #[test]
    fn test_engine_process_auto_reply_full_auto_urgent() {
        let mut engine = default_engine();
        let classified = make_classified(
            "URGENT: production is down",
            MessagePriority::Urgent,
            MessageType::Question,
            SuggestedAction::AutoReply,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(reply.is_some());
        let reply = reply.unwrap();
        // FullAuto + Urgent -> needs approval
        assert_eq!(reply.status, ReplyStatus::PendingApproval);
    }

    #[test]
    fn test_engine_process_draft_reply() {
        let mut engine = default_engine();
        let classified = make_classified(
            "Can you explain how this works?",
            MessagePriority::Normal,
            MessageType::Question,
            SuggestedAction::DraftReply,
        );
        let reply = engine.process_classified(&classified, "email");
        assert!(reply.is_some());
        let reply = reply.unwrap();
        assert_eq!(reply.status, ReplyStatus::PendingApproval);
    }

    #[test]
    fn test_engine_process_non_reply_action() {
        let mut engine = default_engine();
        let classified = make_classified(
            "Interesting news",
            MessagePriority::Low,
            MessageType::Notification,
            SuggestedAction::AddToDigest,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(reply.is_none());
    }

    #[test]
    fn test_engine_process_ignore_action() {
        let mut engine = default_engine();
        let classified = make_classified(
            "spam spam spam",
            MessagePriority::Low,
            MessageType::Spam,
            SuggestedAction::Ignore,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(reply.is_none());
    }

    #[test]
    fn test_engine_enqueue_and_query() {
        let mut engine = default_engine();
        let reply1 = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        let reply2 =
            PendingReply::new("slack", "bob", "Q2", MessagePriority::Normal).with_draft("Reply 2");

        engine.enqueue(reply1);
        engine.enqueue(reply2);

        assert_eq!(engine.pending_count(), 2);
        assert_eq!(engine.pending_approval().len(), 2);
        assert_eq!(engine.ready_to_send().len(), 0);
    }

    #[test]
    fn test_engine_approve_and_send() {
        let mut engine = default_engine();
        let reply = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        let id = reply.id;
        engine.enqueue(reply);

        assert!(engine.approve_reply(id));
        assert_eq!(engine.ready_to_send().len(), 1);

        assert!(engine.mark_sent(id));
        assert_eq!(engine.sent_count(), 1);
        assert_eq!(engine.ready_to_send().len(), 0);
    }

    #[test]
    fn test_engine_reject_reply() {
        let mut engine = default_engine();
        let reply = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        let id = reply.id;
        engine.enqueue(reply);

        assert!(engine.reject_reply(id));
        assert_eq!(engine.pending_approval().len(), 0);
    }

    #[test]
    fn test_engine_approve_nonexistent() {
        let mut engine = default_engine();
        assert!(!engine.approve_reply(Uuid::new_v4()));
    }

    #[test]
    fn test_engine_cleanup_completed() {
        let mut engine = default_engine();

        let mut reply1 = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        reply1.try_approve().unwrap();
        reply1.try_mark_sent().unwrap();
        engine.enqueue(reply1);

        let mut reply2 =
            PendingReply::new("slack", "bob", "Q2", MessagePriority::Normal).with_draft("Reply 2");
        reply2.try_reject().unwrap();
        engine.enqueue(reply2);

        let reply3 = PendingReply::new("slack", "carol", "Q3", MessagePriority::Normal)
            .with_draft("Reply 3");
        engine.enqueue(reply3);

        assert_eq!(engine.pending_count(), 3);
        let cleaned = engine.cleanup_completed();
        assert_eq!(cleaned, 2);
        assert_eq!(engine.pending_count(), 1);
    }

    #[test]
    fn test_engine_rate_limiting() {
        let config = IntelligenceConfig {
            max_reply_tokens: 200, // Low token limit -> low rate limit
            ..IntelligenceConfig::default()
        };
        let mut engine = AutoReplyEngine::new(config);
        // max_replies_per_hour = max(200/100, 10) = 10
        // Exhaust rate limit by adding 10 timestamps within the last hour
        for _ in 0..10 {
            engine.reply_timestamps.push_back(Utc::now());
        }
        let classified = make_classified(
            "What time?",
            MessagePriority::Normal,
            MessageType::Question,
            SuggestedAction::AutoReply,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(reply.is_none(), "Should be rate limited");
    }

    #[test]
    fn test_engine_reset_rate_limit() {
        let mut engine = default_engine();
        for _ in 0..5 {
            engine.reply_timestamps.push_back(Utc::now());
        }
        engine.reset_rate_limit();
        assert_eq!(engine.sent_count(), 0);
    }

    #[test]
    fn test_engine_rate_limit_window_expiry() {
        let config = IntelligenceConfig {
            max_reply_tokens: 200,
            ..IntelligenceConfig::default()
        };
        let mut engine = AutoReplyEngine::new(config);
        // Add 10 timestamps from 2 hours ago (outside the 1-hour window)
        let old = Utc::now() - chrono::Duration::hours(2);
        for _ in 0..10 {
            engine.reply_timestamps.push_back(old);
        }
        // Should NOT be rate limited because all timestamps are outside the window
        let classified = make_classified(
            "What time?",
            MessagePriority::Normal,
            MessageType::Question,
            SuggestedAction::AutoReply,
        );
        let reply = engine.process_classified(&classified, "slack");
        assert!(
            reply.is_some(),
            "Old timestamps should have expired from the window"
        );
    }

    #[test]
    fn test_engine_expire_old_replies() {
        let mut engine = default_engine();
        let mut reply = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        // Set creation time to 2 hours ago
        reply.created_at = Utc::now() - chrono::Duration::hours(2);
        engine.enqueue(reply);

        let expired = engine.expire_old_replies(3600); // 1 hour
        assert_eq!(expired, 1);
        assert_eq!(engine.pending_approval().len(), 0);
    }

    #[test]
    fn test_engine_expire_does_not_expire_recent() {
        let mut engine = default_engine();
        let reply = PendingReply::new("slack", "alice", "Q1", MessagePriority::Normal)
            .with_draft("Reply 1");
        engine.enqueue(reply);

        let expired = engine.expire_old_replies(3600);
        assert_eq!(expired, 0);
        assert_eq!(engine.pending_approval().len(), 1);
    }
}
