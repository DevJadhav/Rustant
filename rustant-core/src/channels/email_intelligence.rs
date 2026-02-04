//! Email-specific intelligence enhancements.
//!
//! Provides richer classification for email messages by analyzing sender
//! patterns, thread positions, and email-specific metadata (subject lines,
//! mailing list headers, CC counts).
//!
//! Integrates with the general `MessageClassifier` and adds:
//! - Email category detection (NeedsReply, ActionRequired, FYI, Newsletter, etc.)
//! - Sender profile learning from long-term memory
//! - Thread position detection (new thread vs reply vs follow-up)
//! - Background IMAP polling via the heartbeat system

use super::intelligence::{ClassifiedMessage, MessageType};
use super::types::{ChannelMessage, MessageContent};
use crate::config::MessagePriority;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Email-specific message categories beyond general classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmailCategory {
    /// Requires a response from the user.
    NeedsReply,
    /// User needs to take a specific action (approve, review, etc.).
    ActionRequired,
    /// Informational only, no action needed.
    FYI,
    /// Automated newsletter or marketing email.
    Newsletter,
    /// Automated system notification (CI/CD, monitoring, etc.).
    Automated,
    /// From a known important contact.
    PersonalImportant,
    /// Unable to categorize.
    Unknown,
}

/// Position within an email thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadPosition {
    /// First message in a new thread.
    NewThread,
    /// Reply within an existing thread.
    Reply,
    /// Follow-up after a period of inactivity.
    FollowUp,
}

/// Learned profile for an email sender based on historical interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SenderProfile {
    /// The sender's email address.
    pub address: String,
    /// Display name, if known.
    pub name: Option<String>,
    /// Typical priority of messages from this sender.
    pub typical_priority: MessagePriority,
    /// How often the user replies to this sender (0.0 - 1.0).
    pub response_rate: f32,
    /// Average response time in seconds, if applicable.
    pub avg_response_time_secs: Option<u64>,
    /// Labels/tags associated with this sender.
    pub labels: Vec<String>,
    /// Total number of messages received from this sender.
    pub message_count: usize,
    /// When the profile was last updated.
    pub last_seen: DateTime<Utc>,
    /// S15: Whether this sender has been verified (e.g., via DKIM/SPF authentication headers).
    /// Unverified senders require higher thresholds to be considered "important".
    #[serde(default)]
    pub verified: bool,
}

impl SenderProfile {
    /// Create a new sender profile for a first-time sender.
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            name: None,
            typical_priority: MessagePriority::Normal,
            response_rate: 0.5, // Unknown, default to 50%
            avg_response_time_secs: None,
            labels: Vec::new(),
            message_count: 0,
            last_seen: Utc::now(),
            verified: false,
        }
    }

    /// Set the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Record a new message from this sender.
    pub fn record_message(&mut self) {
        self.message_count += 1;
        self.last_seen = Utc::now();
    }

    /// Update the response rate based on whether the user replied.
    pub fn update_response_rate(&mut self, replied: bool) {
        // S14: Guard against NaN/Inf propagation from corrupted data
        if !self.response_rate.is_finite() {
            self.response_rate = 0.5; // Reset to neutral default
        }
        // Exponential moving average with alpha = 0.1
        let value = if replied { 1.0 } else { 0.0 };
        self.response_rate = (self.response_rate * 0.9 + value * 0.1).clamp(0.0, 1.0);
    }

    /// Check if this sender is considered important (high response rate or many messages).
    ///
    /// S15: Unverified senders require a higher message count threshold (>50) to be
    /// considered important, reducing spoofing risk from unknown senders.
    pub fn is_important(&self) -> bool {
        if self.verified {
            self.response_rate > 0.7 || self.message_count > 20
        } else {
            self.response_rate > 0.7 || self.message_count > 50
        }
    }

    /// Add a label to this sender.
    pub fn add_label(&mut self, label: impl Into<String>) {
        let label = label.into();
        if !self.labels.iter().any(|l| l.eq_ignore_ascii_case(&label)) {
            self.labels.push(label);
        }
    }
}

/// Email-specific classification result extending the base classification.
#[derive(Debug, Clone)]
pub struct EmailClassification {
    /// The base classification from the general classifier.
    pub base: ClassifiedMessage,
    /// Email-specific category.
    pub category: EmailCategory,
    /// Whether the email has file attachments.
    pub has_attachments: bool,
    /// Position within the email thread.
    pub thread_position: ThreadPosition,
    /// Suggested labels based on content analysis.
    pub suggested_labels: Vec<String>,
}

/// Email intelligence engine for enhanced email processing.
///
/// **Thread safety**: This struct is designed for single-threaded use. For concurrent
/// access (e.g., from multiple async tasks), wrap in `Arc<tokio::sync::Mutex<_>>`.
pub struct EmailIntelligence {
    /// Known sender profiles (keyed by email address).
    known_senders: HashMap<String, SenderProfile>,
    /// Newsletter/mailing list patterns (detected from headers).
    newsletter_patterns: Vec<String>,
}

impl EmailIntelligence {
    /// Create a new email intelligence engine.
    pub fn new() -> Self {
        Self {
            known_senders: HashMap::new(),
            newsletter_patterns: default_newsletter_patterns(),
        }
    }

    /// Classify an email message with email-specific enhancements.
    pub fn classify_email(&mut self, classified: ClassifiedMessage) -> EmailClassification {
        let sender_address = classified.original.sender.id.clone();
        let has_attachments = self.detect_attachments(&classified.original);
        let thread_position = self.detect_thread_position(&classified.original);

        // Update sender profile
        {
            let profile = self
                .known_senders
                .entry(sender_address.clone())
                .or_insert_with(|| SenderProfile::new(&sender_address));
            profile.record_message();
            if let Some(name) = &classified.original.sender.display_name {
                profile.name = Some(name.clone());
            }
        }

        // Get a snapshot of the profile for classification (avoids borrow conflict)
        let profile_snapshot = self.known_senders.get(&sender_address).cloned().unwrap();

        // Determine email category
        let category = self.categorize_email(&classified, &profile_snapshot, &thread_position);

        // Generate suggested labels
        let suggested_labels = self.suggest_labels(&classified, &category, &profile_snapshot);

        EmailClassification {
            base: classified,
            category,
            has_attachments,
            thread_position,
            suggested_labels,
        }
    }

    /// Categorize an email based on content, sender profile, and metadata.
    fn categorize_email(
        &self,
        classified: &ClassifiedMessage,
        profile: &SenderProfile,
        _thread_position: &ThreadPosition,
    ) -> EmailCategory {
        let text = extract_email_text(&classified.original);
        let text_lower = text.to_lowercase();

        // Check for newsletter/automated patterns
        if self.is_newsletter(&classified.original) {
            return EmailCategory::Newsletter;
        }

        if self.is_automated(&text_lower, &classified.original) {
            return EmailCategory::Automated;
        }

        // Check for important senders
        if profile.is_important() {
            return EmailCategory::PersonalImportant;
        }

        // Check based on message type
        match classified.message_type {
            MessageType::Question => EmailCategory::NeedsReply,
            MessageType::ActionRequired => EmailCategory::ActionRequired,
            MessageType::Notification => EmailCategory::FYI,
            _ => {
                // Fall back to checking content
                if has_reply_indicators(&text_lower) {
                    EmailCategory::NeedsReply
                } else {
                    EmailCategory::FYI
                }
            }
        }
    }

    /// Check if the email appears to be a newsletter.
    fn is_newsletter(&self, msg: &ChannelMessage) -> bool {
        // Check metadata for mailing list headers
        // S19: Clean header values to strip injected CRLF before comparison
        if msg.metadata.contains_key("list-unsubscribe")
            || msg.metadata.contains_key("list-id")
            || msg.metadata.get("precedence").is_some_and(|p| {
                let clean = clean_header_value(p);
                clean == "bulk" || clean == "list"
            })
        {
            return true;
        }

        // Check sender against newsletter patterns
        let sender_lower = clean_header_value(&msg.sender.id).to_lowercase();
        self.newsletter_patterns
            .iter()
            .any(|p| sender_lower.contains(p))
    }

    /// Check if the email is from an automated system.
    fn is_automated(&self, text_lower: &str, msg: &ChannelMessage) -> bool {
        // S19: Clean sender to strip injected CRLF
        let sender_lower = clean_header_value(&msg.sender.id).to_lowercase();
        let automated_patterns = [
            "noreply",
            "no-reply",
            "notifications@",
            "alerts@",
            "monitoring@",
            "ci@",
            "builds@",
            "deploy@",
            "system@",
            "automation@",
            "mailer-daemon",
        ];
        if automated_patterns.iter().any(|p| sender_lower.contains(p)) {
            return true;
        }

        // Check for automated content patterns
        let auto_content = [
            "this is an automated message",
            "do not reply to this email",
            "this email was sent automatically",
            "automated notification",
        ];
        auto_content.iter().any(|p| text_lower.contains(p))
    }

    /// Detect if the email has file attachments based on metadata.
    fn detect_attachments(&self, msg: &ChannelMessage) -> bool {
        matches!(msg.content, MessageContent::File { .. })
            || msg
                .metadata
                .get("has_attachments")
                .is_some_and(|v| v == "true")
    }

    /// Detect the position within an email thread.
    fn detect_thread_position(&self, msg: &ChannelMessage) -> ThreadPosition {
        if msg.thread_id.is_some() {
            if msg.reply_to.is_some() {
                ThreadPosition::Reply
            } else {
                ThreadPosition::FollowUp
            }
        } else {
            ThreadPosition::NewThread
        }
    }

    /// Suggest labels for the email based on classification.
    fn suggest_labels(
        &self,
        classified: &ClassifiedMessage,
        category: &EmailCategory,
        profile: &SenderProfile,
    ) -> Vec<String> {
        let mut labels = Vec::new();

        match category {
            EmailCategory::NeedsReply => labels.push("needs-reply".to_string()),
            EmailCategory::ActionRequired => labels.push("action-required".to_string()),
            EmailCategory::Newsletter => labels.push("newsletter".to_string()),
            EmailCategory::Automated => labels.push("automated".to_string()),
            EmailCategory::PersonalImportant => labels.push("important".to_string()),
            EmailCategory::FYI => labels.push("fyi".to_string()),
            EmailCategory::Unknown => {}
        }

        if classified.priority >= MessagePriority::High {
            labels.push("priority".to_string());
        }

        // Include sender labels
        for label in &profile.labels {
            if !labels.contains(label) {
                labels.push(label.clone());
            }
        }

        labels
    }

    /// Get a sender profile by email address.
    pub fn get_sender_profile(&self, address: &str) -> Option<&SenderProfile> {
        self.known_senders.get(address)
    }

    /// Get a mutable sender profile by email address.
    pub fn get_sender_profile_mut(&mut self, address: &str) -> Option<&mut SenderProfile> {
        self.known_senders.get_mut(address)
    }

    /// Add or update a sender profile.
    pub fn update_sender_profile(&mut self, profile: SenderProfile) {
        self.known_senders.insert(profile.address.clone(), profile);
    }

    /// Get the total number of known senders.
    pub fn known_sender_count(&self) -> usize {
        self.known_senders.len()
    }
}

impl Default for EmailIntelligence {
    fn default() -> Self {
        Self::new()
    }
}

// --- Helper Functions ---

/// Extract text content from an email ChannelMessage.
fn extract_email_text(msg: &ChannelMessage) -> String {
    match &msg.content {
        MessageContent::Text { text } => text.clone(),
        MessageContent::Command { command, args } => format!("/{} {}", command, args.join(" ")),
        _ => String::new(),
    }
}

/// S19: Strip CR/LF from email header values to prevent CRLF injection.
///
/// Email headers can contain injected line breaks that could be used to
/// smuggle extra headers or bypass classification logic.
fn clean_header_value(value: &str) -> String {
    value.chars().filter(|c| *c != '\r' && *c != '\n').collect()
}

/// Check if text contains indicators that a reply is expected.
fn has_reply_indicators(text: &str) -> bool {
    const REPLY_INDICATORS: &[&str] = &[
        "please reply",
        "please respond",
        "your thoughts",
        "your opinion",
        "let me know",
        "get back to me",
        "waiting for your",
        "your feedback",
        "your input",
        "please confirm",
        "please advise",
    ];
    REPLY_INDICATORS.iter().any(|p| text.contains(p))
}

/// Default newsletter sender patterns.
fn default_newsletter_patterns() -> Vec<String> {
    vec![
        "newsletter".to_string(),
        "digest@".to_string(),
        "updates@".to_string(),
        "marketing@".to_string(),
        "info@".to_string(),
        "news@".to_string(),
        "weekly@".to_string(),
        "daily@".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::intelligence::{ClassifiedMessage, MessageType, SuggestedAction};
    use crate::channels::types::{
        ChannelMessage, ChannelType, ChannelUser, MessageContent, MessageId, ThreadId,
    };
    use crate::config::MessagePriority;

    fn make_email_message(text: &str, sender: &str) -> ChannelMessage {
        ChannelMessage {
            id: MessageId::random(),
            channel_type: ChannelType::Email,
            channel_id: "inbox".to_string(),
            sender: ChannelUser::new(sender, ChannelType::Email).with_name(sender),
            content: MessageContent::Text {
                text: text.to_string(),
            },
            timestamp: Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: HashMap::new(),
        }
    }

    fn make_classified_email(
        text: &str,
        sender: &str,
        priority: MessagePriority,
        msg_type: MessageType,
    ) -> ClassifiedMessage {
        ClassifiedMessage {
            original: make_email_message(text, sender),
            priority,
            message_type: msg_type,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        }
    }

    // --- SenderProfile Tests ---

    #[test]
    fn test_sender_profile_new() {
        let profile = SenderProfile::new("alice@example.com");
        assert_eq!(profile.address, "alice@example.com");
        assert_eq!(profile.message_count, 0);
        assert!(!profile.is_important());
    }

    #[test]
    fn test_sender_profile_record_message() {
        let mut profile = SenderProfile::new("alice@example.com");
        profile.record_message();
        profile.record_message();
        assert_eq!(profile.message_count, 2);
    }

    #[test]
    fn test_sender_profile_response_rate() {
        let mut profile = SenderProfile::new("alice@example.com");
        // Start at 0.5 default
        for _ in 0..20 {
            profile.update_response_rate(true);
        }
        assert!(profile.response_rate > 0.7);
        assert!(profile.is_important());
    }

    #[test]
    fn test_sender_profile_important_by_count_verified() {
        let mut profile = SenderProfile::new("alice@example.com");
        profile.verified = true;
        profile.message_count = 25;
        assert!(profile.is_important());
    }

    #[test]
    fn test_sender_profile_important_by_count_unverified() {
        let mut profile = SenderProfile::new("alice@example.com");
        // S15: Unverified senders need >50 messages to be important
        profile.message_count = 25;
        assert!(
            !profile.is_important(),
            "25 messages should not be enough for unverified sender"
        );
        profile.message_count = 55;
        assert!(
            profile.is_important(),
            "55 messages should be enough for unverified sender"
        );
    }

    #[test]
    fn test_sender_profile_add_label() {
        let mut profile = SenderProfile::new("alice@example.com");
        profile.add_label("work");
        profile.add_label("Work"); // Duplicate (case insensitive)
        profile.add_label("personal");
        assert_eq!(profile.labels.len(), 2);
    }

    // --- EmailIntelligence Tests ---

    #[test]
    fn test_email_intelligence_new() {
        let intel = EmailIntelligence::new();
        assert_eq!(intel.known_sender_count(), 0);
    }

    #[test]
    fn test_classify_question_email() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "Can you review the latest proposal?",
            "boss@corp.com",
            MessagePriority::Normal,
            MessageType::Question,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::NeedsReply);
        assert!(result.suggested_labels.contains(&"needs-reply".to_string()));
    }

    #[test]
    fn test_classify_action_email() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "Please approve the budget by EOD",
            "manager@corp.com",
            MessagePriority::High,
            MessageType::ActionRequired,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::ActionRequired);
        assert!(result
            .suggested_labels
            .contains(&"action-required".to_string()));
        assert!(result.suggested_labels.contains(&"priority".to_string()));
    }

    #[test]
    fn test_classify_newsletter_by_metadata() {
        let mut intel = EmailIntelligence::new();
        let mut msg = make_email_message("This week in tech...", "newsletter@techweekly.com");
        msg.metadata.insert(
            "list-unsubscribe".to_string(),
            "mailto:unsub@techweekly.com".to_string(),
        );

        let classified = ClassifiedMessage {
            original: msg,
            priority: MessagePriority::Low,
            message_type: MessageType::Notification,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        };

        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::Newsletter);
    }

    #[test]
    fn test_classify_newsletter_by_sender() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "Your weekly update...",
            "newsletter@company.com",
            MessagePriority::Low,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::Newsletter);
    }

    #[test]
    fn test_classify_automated_by_sender() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "Build #456 succeeded",
            "noreply@github.com",
            MessagePriority::Low,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::Automated);
    }

    #[test]
    fn test_classify_automated_by_content() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "This is an automated message. Your deployment completed successfully.",
            "deploy@corp.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::Automated);
    }

    #[test]
    fn test_classify_important_sender() {
        let mut intel = EmailIntelligence::new();

        // Build up sender profile as important
        let mut profile = SenderProfile::new("important@corp.com");
        profile.message_count = 50;
        profile.response_rate = 0.9;
        intel.update_sender_profile(profile);

        let classified = make_classified_email(
            "Hey, quick sync?",
            "important@corp.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.category, EmailCategory::PersonalImportant);
    }

    #[test]
    fn test_thread_position_new() {
        let mut intel = EmailIntelligence::new();
        let classified = make_classified_email(
            "Starting a new thread",
            "alice@example.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert_eq!(result.thread_position, ThreadPosition::NewThread);
    }

    #[test]
    fn test_thread_position_reply() {
        let mut intel = EmailIntelligence::new();
        let mut msg = make_email_message("Re: Previous subject", "alice@example.com");
        msg.thread_id = Some(ThreadId::new("thread-1"));
        msg.reply_to = Some(MessageId::new("msg-1"));

        let classified = ClassifiedMessage {
            original: msg,
            priority: MessagePriority::Normal,
            message_type: MessageType::Notification,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        };
        let result = intel.classify_email(classified);
        assert_eq!(result.thread_position, ThreadPosition::Reply);
    }

    #[test]
    fn test_thread_position_followup() {
        let mut intel = EmailIntelligence::new();
        let mut msg = make_email_message("Following up on this", "alice@example.com");
        msg.thread_id = Some(ThreadId::new("thread-1"));
        // reply_to is None -> follow-up

        let classified = ClassifiedMessage {
            original: msg,
            priority: MessagePriority::Normal,
            message_type: MessageType::Notification,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        };
        let result = intel.classify_email(classified);
        assert_eq!(result.thread_position, ThreadPosition::FollowUp);
    }

    #[test]
    fn test_detect_attachments_from_metadata() {
        let mut intel = EmailIntelligence::new();
        let mut msg = make_email_message("See attached", "alice@example.com");
        msg.metadata
            .insert("has_attachments".to_string(), "true".to_string());

        let classified = ClassifiedMessage {
            original: msg,
            priority: MessagePriority::Normal,
            message_type: MessageType::Notification,
            suggested_action: SuggestedAction::AddToDigest,
            confidence: 0.8,
            reasoning: "test".to_string(),
            classified_at: Utc::now(),
        };
        let result = intel.classify_email(classified);
        assert!(result.has_attachments);
    }

    #[test]
    fn test_sender_profile_tracked() {
        let mut intel = EmailIntelligence::new();

        // First email from alice
        let classified1 = make_classified_email(
            "First message",
            "alice@example.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        intel.classify_email(classified1);

        // Second email from alice
        let classified2 = make_classified_email(
            "Second message",
            "alice@example.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        intel.classify_email(classified2);

        let profile = intel.get_sender_profile("alice@example.com").unwrap();
        assert_eq!(profile.message_count, 2);
    }

    #[test]
    fn test_reply_indicators() {
        assert!(has_reply_indicators(
            "please reply at your earliest convenience"
        ));
        assert!(has_reply_indicators("let me know what you think"));
        assert!(has_reply_indicators("waiting for your response"));
        assert!(!has_reply_indicators("just wanted to say thanks"));
    }

    #[test]
    fn test_suggested_labels_include_sender_labels() {
        let mut intel = EmailIntelligence::new();

        let mut profile = SenderProfile::new("team@corp.com");
        profile.add_label("team");
        profile.add_label("work");
        intel.update_sender_profile(profile);

        let classified = make_classified_email(
            "Team update",
            "team@corp.com",
            MessagePriority::Normal,
            MessageType::Notification,
        );
        let result = intel.classify_email(classified);
        assert!(result.suggested_labels.contains(&"team".to_string()));
        assert!(result.suggested_labels.contains(&"work".to_string()));
    }

    // --- L5: SenderProfile response rate edge cases ---

    #[test]
    fn test_sender_profile_response_rate_zero_replies() {
        let mut profile = SenderProfile::new("test@example.com");
        // Initial rate is 0.5 (50%)
        assert_eq!(profile.response_rate, 0.5);

        // 10 non-replies should drive it toward 0
        for _ in 0..10 {
            profile.update_response_rate(false);
        }
        assert!(
            profile.response_rate < 0.2,
            "Rate should drop below 0.2 after 10 non-replies, got {}",
            profile.response_rate
        );
    }

    #[test]
    fn test_sender_profile_response_rate_single_reply() {
        let mut profile = SenderProfile::new("test@example.com");
        // Start at 0.5, one reply should nudge it slightly up
        profile.update_response_rate(true);
        assert!(profile.response_rate > 0.5);
        // Should be 0.5 * 0.9 + 1.0 * 0.1 = 0.55
        assert!((profile.response_rate - 0.55).abs() < 0.001);
    }

    #[test]
    fn test_sender_profile_response_rate_rapid_fire() {
        let mut profile = SenderProfile::new("test@example.com");
        // 100 consecutive replies should converge close to 1.0
        for _ in 0..100 {
            profile.update_response_rate(true);
        }
        assert!(
            profile.response_rate > 0.99,
            "Rate should converge near 1.0 after 100 replies, got {}",
            profile.response_rate
        );
    }

    #[test]
    fn test_sender_profile_response_rate_bounded() {
        let mut profile = SenderProfile::new("test@example.com");
        // EMA should always stay in [0.0, 1.0] range
        for _ in 0..1000 {
            profile.update_response_rate(true);
        }
        assert!(profile.response_rate <= 1.0);
        assert!(profile.response_rate >= 0.0);

        for _ in 0..1000 {
            profile.update_response_rate(false);
        }
        assert!(profile.response_rate >= 0.0);
        assert!(profile.response_rate <= 1.0);
    }

    // --- S14: NaN/Inf Guard Tests ---

    #[test]
    fn test_sender_profile_nan_recovery() {
        let mut profile = SenderProfile::new("test@example.com");
        profile.response_rate = f32::NAN;
        profile.update_response_rate(true);
        // Should have recovered from NaN to 0.5, then applied EMA
        assert!(profile.response_rate.is_finite());
        // 0.5 * 0.9 + 1.0 * 0.1 = 0.55
        assert!(
            (profile.response_rate - 0.55).abs() < 0.001,
            "Expected ~0.55 after NaN recovery + reply, got {}",
            profile.response_rate
        );
    }

    #[test]
    fn test_sender_profile_infinity_recovery() {
        let mut profile = SenderProfile::new("test@example.com");
        profile.response_rate = f32::INFINITY;
        profile.update_response_rate(false);
        assert!(profile.response_rate.is_finite());
        // 0.5 * 0.9 + 0.0 * 0.1 = 0.45
        assert!(
            (profile.response_rate - 0.45).abs() < 0.001,
            "Expected ~0.45 after Inf recovery + non-reply, got {}",
            profile.response_rate
        );
    }

    #[test]
    fn test_sender_profile_neg_infinity_recovery() {
        let mut profile = SenderProfile::new("test@example.com");
        profile.response_rate = f32::NEG_INFINITY;
        profile.update_response_rate(true);
        assert!(profile.response_rate.is_finite());
        assert!(profile.response_rate >= 0.0);
        assert!(profile.response_rate <= 1.0);
    }

    #[test]
    fn test_sender_profile_normal_values_unchanged() {
        let mut profile = SenderProfile::new("test@example.com");
        profile.response_rate = 0.7;
        profile.update_response_rate(true);
        // 0.7 * 0.9 + 1.0 * 0.1 = 0.73
        assert!(
            (profile.response_rate - 0.73).abs() < 0.001,
            "Normal values should not be affected by NaN guard, got {}",
            profile.response_rate
        );
    }
}
