//! Communication style tracking for channel senders.
//!
//! Analyzes message patterns per-sender to learn communication preferences:
//! message length, formality, emoji usage, common greetings, and topics.
//! Every N messages, generates `Fact` entries for long-term memory.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A tracked style profile for a single sender.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SenderStyleProfile {
    /// Sender identifier (e.g., Slack user ID).
    pub sender_id: String,
    /// Channel type (e.g., "slack", "email").
    pub channel_type: String,
    /// Total messages analyzed.
    pub message_count: usize,
    /// Average message length in characters.
    pub avg_message_length: f64,
    /// Formality score: 0.0 (very casual) to 1.0 (very formal).
    pub formality_score: f64,
    /// Whether the sender frequently uses emoji.
    pub uses_emoji: bool,
    /// Common greeting patterns observed.
    pub common_greetings: Vec<String>,
    /// Frequently discussed topics/keywords.
    pub frequent_topics: Vec<String>,
    /// Average response time in seconds (if measurable).
    pub avg_response_time_secs: Option<f64>,
}

/// Tracks communication styles across multiple senders.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CommunicationStyleTracker {
    /// Per-sender style profiles.
    pub profiles: HashMap<String, SenderStyleProfile>,
    /// Total messages processed.
    pub total_messages: usize,
    /// Threshold: generate facts every N messages per sender.
    pub fact_threshold: usize,
}

impl CommunicationStyleTracker {
    /// Create a new tracker with the given fact generation threshold.
    pub fn new(fact_threshold: usize) -> Self {
        Self {
            profiles: HashMap::new(),
            total_messages: 0,
            fact_threshold,
        }
    }

    /// Analyze a message and update the sender's style profile.
    ///
    /// Returns a list of fact strings if the threshold is reached.
    pub fn track_message(
        &mut self,
        sender_id: &str,
        channel_type: &str,
        message: &str,
    ) -> Vec<String> {
        self.total_messages += 1;

        let profile = self
            .profiles
            .entry(sender_id.to_string())
            .or_insert_with(|| SenderStyleProfile {
                sender_id: sender_id.to_string(),
                channel_type: channel_type.to_string(),
                ..Default::default()
            });

        let msg_len = message.len() as f64;
        let old_count = profile.message_count as f64;
        profile.message_count += 1;
        let new_count = profile.message_count as f64;

        // Running average of message length
        profile.avg_message_length = (profile.avg_message_length * old_count + msg_len) / new_count;

        // Formality heuristic
        let formality = compute_formality(message);
        profile.formality_score = (profile.formality_score * old_count + formality) / new_count;

        // Emoji detection
        if contains_emoji(message) {
            profile.uses_emoji = true;
        }

        // Greeting detection
        let greeting = detect_greeting(message);
        if let Some(g) = greeting
            && !profile.common_greetings.contains(&g)
            && profile.common_greetings.len() < 5
        {
            profile.common_greetings.push(g);
        }

        // Generate facts at threshold
        let mut facts = Vec::new();
        if profile.message_count > 0
            && profile.message_count % self.fact_threshold == 0
            && self.fact_threshold > 0
        {
            facts.push(format!(
                "Sender '{}' on {} typically writes {} messages (avg {} chars). \
                 Formality: {:.1}/1.0. Uses emoji: {}.",
                profile.sender_id,
                profile.channel_type,
                if profile.avg_message_length > 200.0 {
                    "long"
                } else if profile.avg_message_length > 50.0 {
                    "medium"
                } else {
                    "short"
                },
                profile.avg_message_length as usize,
                profile.formality_score,
                profile.uses_emoji,
            ));

            if !profile.common_greetings.is_empty() {
                facts.push(format!(
                    "Sender '{}' commonly greets with: {}",
                    profile.sender_id,
                    profile.common_greetings.join(", ")
                ));
            }
        }

        facts
    }

    /// Get a sender's style profile.
    pub fn get_profile(&self, sender_id: &str) -> Option<&SenderStyleProfile> {
        self.profiles.get(sender_id)
    }

    /// Get all tracked profiles.
    pub fn all_profiles(&self) -> &HashMap<String, SenderStyleProfile> {
        &self.profiles
    }
}

/// Compute a formality score from 0.0 (casual) to 1.0 (formal).
fn compute_formality(message: &str) -> f64 {
    let mut score = 0.5_f64; // neutral baseline

    // Formal indicators
    if message.contains("Dear ") || message.contains("Regards") || message.contains("Sincerely") {
        score += 0.2_f64;
    }
    if message.ends_with('.') || message.ends_with('!') {
        score += 0.05_f64;
    }
    // Starts with capital letter
    if message
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        score += 0.05_f64;
    }

    // Casual indicators
    if message.contains("lol") || message.contains("haha") || message.contains("lmao") {
        score -= 0.2_f64;
    }
    if message == message.to_lowercase() && message.len() > 10 {
        score -= 0.1_f64;
    }
    if contains_emoji(message) {
        score -= 0.05_f64;
    }

    score.clamp(0.0_f64, 1.0_f64)
}

/// Check if a string contains emoji characters.
fn contains_emoji(s: &str) -> bool {
    s.chars().any(|c| {
        let cp = c as u32;
        (0x1F600..=0x1F64F).contains(&cp) // Emoticons
            || (0x1F300..=0x1F5FF).contains(&cp) // Misc symbols
            || (0x1F680..=0x1F6FF).contains(&cp) // Transport
            || (0x1F900..=0x1F9FF).contains(&cp) // Supplemental
            || (0x2600..=0x26FF).contains(&cp) // Misc symbols
            || (0x2700..=0x27BF).contains(&cp) // Dingbats
    })
}

/// Detect greeting patterns at the start of a message.
fn detect_greeting(message: &str) -> Option<String> {
    let lower = message.to_lowercase();
    let _first_word = lower.split_whitespace().next().unwrap_or("");

    let greetings = [
        "hi",
        "hello",
        "hey",
        "good morning",
        "good afternoon",
        "good evening",
        "greetings",
        "howdy",
        "sup",
        "yo",
    ];

    for g in &greetings {
        if lower.starts_with(g) {
            return Some(g.to_string());
        }
    }

    if lower.starts_with("dear") {
        return Some("dear".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_single_message() {
        let mut tracker = CommunicationStyleTracker::new(50);
        let facts = tracker.track_message("user1", "slack", "Hello, how are you today?");
        assert!(facts.is_empty()); // Under threshold
        assert_eq!(tracker.total_messages, 1);

        let profile = tracker.get_profile("user1").unwrap();
        assert_eq!(profile.message_count, 1);
        assert!(profile.avg_message_length > 0.0);
    }

    #[test]
    fn test_formality_formal() {
        let score = compute_formality("Dear John, I hope this message finds you well. Regards.");
        assert!(score > 0.6);
    }

    #[test]
    fn test_formality_casual() {
        let score = compute_formality("hey lol whats up haha");
        assert!(score < 0.4);
    }

    #[test]
    fn test_contains_emoji() {
        assert!(!contains_emoji("Hello world"));
        // Unicode emoji tests - using escape sequences
        assert!(contains_emoji("Hello \u{1F600}"));
    }

    #[test]
    fn test_detect_greeting() {
        assert_eq!(detect_greeting("Hello there!"), Some("hello".to_string()));
        assert_eq!(detect_greeting("hey what's up"), Some("hey".to_string()));
        assert_eq!(detect_greeting("Thanks for the update"), None);
    }

    #[test]
    fn test_fact_generation_at_threshold() {
        let mut tracker = CommunicationStyleTracker::new(3);
        tracker.track_message("user1", "slack", "Message 1");
        tracker.track_message("user1", "slack", "Message 2");
        let facts = tracker.track_message("user1", "slack", "Message 3");
        assert!(!facts.is_empty()); // Should generate facts at count 3
    }

    #[test]
    fn test_greeting_tracking() {
        let mut tracker = CommunicationStyleTracker::new(50);
        tracker.track_message("user1", "slack", "Hello everyone!");
        tracker.track_message("user1", "slack", "Hey, quick question");
        let profile = tracker.get_profile("user1").unwrap();
        assert!(profile.common_greetings.contains(&"hello".to_string()));
        assert!(profile.common_greetings.contains(&"hey".to_string()));
    }
}
