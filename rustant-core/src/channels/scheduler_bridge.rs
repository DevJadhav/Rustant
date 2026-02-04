//! Smart Scheduling bridge for channel intelligence.
//!
//! Bridges classified messages to the cron scheduler system and generates
//! ICS calendar files for follow-up reminders.
//!
//! # Scheduling Flow
//!
//! 1. Message classified as `SuggestedAction::ScheduleFollowUp { minutes }`
//! 2. `SchedulerBridge.schedule_followup()` creates a `FollowUpReminder`
//! 3. Reminder is persisted to `.rustant/reminders/index.json`
//! 4. An ICS file is generated at `.rustant/reminders/{id}.ics`
//! 5. When the reminder fires, `AgentCallback::on_reminder()` is called

use crate::config::MessagePriority;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Status of a follow-up reminder in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReminderStatus {
    /// Reminder is scheduled and waiting.
    Pending,
    /// Reminder has fired.
    Triggered,
    /// User dismissed the reminder.
    Dismissed,
    /// User marked the action as completed.
    Completed,
}

/// A scheduled follow-up reminder for a classified message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowUpReminder {
    /// Unique identifier.
    pub id: Uuid,
    /// Summary of the original message.
    pub source_message: String,
    /// The channel the message came from.
    pub source_channel: String,
    /// The sender of the original message.
    pub source_sender: String,
    /// When the reminder should fire.
    pub remind_at: DateTime<Utc>,
    /// Human-readable description of what needs follow-up.
    pub description: String,
    /// Current status.
    pub status: ReminderStatus,
    /// The priority of the original message.
    pub priority: MessagePriority,
    /// When the reminder was created.
    pub created_at: DateTime<Utc>,
}

impl FollowUpReminder {
    /// Create a new pending reminder.
    pub fn new(
        source_message: impl Into<String>,
        source_channel: impl Into<String>,
        source_sender: impl Into<String>,
        remind_at: DateTime<Utc>,
        description: impl Into<String>,
        priority: MessagePriority,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            source_message: source_message.into(),
            source_channel: source_channel.into(),
            source_sender: source_sender.into(),
            remind_at,
            description: description.into(),
            status: ReminderStatus::Pending,
            priority,
            created_at: Utc::now(),
        }
    }

    /// Mark the reminder as triggered.
    pub fn trigger(&mut self) {
        self.status = ReminderStatus::Triggered;
    }

    /// Mark the reminder as dismissed.
    pub fn dismiss(&mut self) {
        self.status = ReminderStatus::Dismissed;
    }

    /// Mark the reminder as completed.
    pub fn complete(&mut self) {
        self.status = ReminderStatus::Completed;
    }

    /// Check if the reminder is due (past its fire time and still pending).
    pub fn is_due(&self) -> bool {
        self.status == ReminderStatus::Pending && Utc::now() >= self.remind_at
    }

    /// Check if the reminder is active (pending or triggered).
    pub fn is_active(&self) -> bool {
        matches!(
            self.status,
            ReminderStatus::Pending | ReminderStatus::Triggered
        )
    }

    /// Generate an ICS (iCalendar) representation of this reminder.
    ///
    /// All user-controlled fields are escaped via [`crate::sanitize::escape_ics_field`]
    /// to prevent CRLF injection of rogue ICS properties.
    pub fn to_ics(&self) -> String {
        use crate::sanitize::escape_ics_field;

        let dtstart = self.remind_at.format("%Y%m%dT%H%M%SZ");
        let dtend = (self.remind_at + chrono::Duration::minutes(30)).format("%Y%m%dT%H%M%SZ");
        let dtstamp = self.created_at.format("%Y%m%dT%H%M%SZ");

        format!(
            "BEGIN:VCALENDAR\r\n\
             VERSION:2.0\r\n\
             PRODID:-//Rustant//Channel Intelligence//EN\r\n\
             BEGIN:VEVENT\r\n\
             DTSTART:{dtstart}\r\n\
             DTEND:{dtend}\r\n\
             DTSTAMP:{dtstamp}\r\n\
             SUMMARY:Follow up: {description} ({channel} from {sender})\r\n\
             DESCRIPTION:Original message: {msg}. Priority: {priority:?}.\r\n\
             UID:{uid}@rustant\r\n\
             END:VEVENT\r\n\
             END:VCALENDAR\r\n",
            dtstart = dtstart,
            dtend = dtend,
            dtstamp = dtstamp,
            description = escape_ics_field(&self.description),
            channel = escape_ics_field(&self.source_channel),
            sender = escape_ics_field(&self.source_sender),
            msg = escape_ics_field(&self.source_message),
            priority = self.priority,
            uid = self.id,
        )
    }
}

/// Bridge between channel intelligence and the scheduling system.
///
/// Manages follow-up reminders, persists them to disk, and generates
/// ICS calendar files.
pub struct SchedulerBridge {
    /// Active reminders.
    reminders: Vec<FollowUpReminder>,
    /// Directory for ICS file export and index.
    reminders_dir: PathBuf,
}

impl SchedulerBridge {
    /// Create a new scheduler bridge with the given reminders directory.
    pub fn new(reminders_dir: PathBuf) -> Self {
        Self {
            reminders: Vec::new(),
            reminders_dir,
        }
    }

    /// Schedule a follow-up reminder for a classified message.
    ///
    /// Creates a `FollowUpReminder`, adds it to the active list,
    /// and generates an ICS file.
    pub fn schedule_followup(
        &mut self,
        source_message: &str,
        source_channel: &str,
        source_sender: &str,
        minutes: u32,
        priority: MessagePriority,
    ) -> FollowUpReminder {
        let remind_at = Utc::now() + chrono::Duration::minutes(i64::from(minutes));
        let description = if source_message.chars().count() > 80 {
            format!("{}...", source_message.chars().take(80).collect::<String>())
        } else {
            source_message.to_string()
        };

        let reminder = FollowUpReminder::new(
            source_message,
            source_channel,
            source_sender,
            remind_at,
            description,
            priority,
        );

        self.reminders.push(reminder.clone());
        reminder
    }

    /// Get all active (pending or triggered) reminders.
    pub fn active_reminders(&self) -> Vec<&FollowUpReminder> {
        self.reminders.iter().filter(|r| r.is_active()).collect()
    }

    /// Get all reminders that are currently due.
    pub fn due_reminders(&self) -> Vec<&FollowUpReminder> {
        self.reminders.iter().filter(|r| r.is_due()).collect()
    }

    /// Trigger all due reminders and return them.
    pub fn trigger_due(&mut self) -> Vec<FollowUpReminder> {
        let mut triggered = Vec::new();
        for reminder in &mut self.reminders {
            if reminder.is_due() {
                reminder.trigger();
                triggered.push(reminder.clone());
            }
        }
        triggered
    }

    /// Dismiss a reminder by ID.
    pub fn dismiss(&mut self, id: Uuid) -> bool {
        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
            r.dismiss();
            true
        } else {
            false
        }
    }

    /// Complete a reminder by ID.
    pub fn complete(&mut self, id: Uuid) -> bool {
        if let Some(r) = self.reminders.iter_mut().find(|r| r.id == id) {
            r.complete();
            true
        } else {
            false
        }
    }

    /// Remove all completed/dismissed reminders.
    pub fn cleanup(&mut self) -> usize {
        let before = self.reminders.len();
        self.reminders.retain(|r| r.is_active());
        before - self.reminders.len()
    }

    /// Get the total number of reminders (all statuses).
    pub fn total_count(&self) -> usize {
        self.reminders.len()
    }

    /// Export a reminder as an ICS file.
    pub fn export_ics(&self, reminder: &FollowUpReminder) -> Result<PathBuf, std::io::Error> {
        let path = self.reminders_dir.join(format!("{}.ics", reminder.id));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, reminder.to_ics())?;
        Ok(path)
    }

    /// Save the reminder index to JSON.
    pub fn save_index(&self) -> Result<PathBuf, std::io::Error> {
        let path = self.reminders_dir.join("index.json");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&self.reminders).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load reminders from the JSON index.
    pub fn load_index(&mut self) -> Result<usize, std::io::Error> {
        let path = self.reminders_dir.join("index.json");
        if !path.exists() {
            return Ok(0);
        }
        let json = std::fs::read_to_string(&path)?;
        let loaded: Vec<FollowUpReminder> = serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        let count = loaded.len();
        self.reminders = loaded;
        Ok(count)
    }

    /// Get the reminders directory path.
    pub fn reminders_dir(&self) -> &Path {
        &self.reminders_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MessagePriority;

    fn test_bridge() -> SchedulerBridge {
        SchedulerBridge::new(PathBuf::from("/tmp/rustant-test-reminders"))
    }

    // --- FollowUpReminder Tests ---

    #[test]
    fn test_reminder_new() {
        let reminder = FollowUpReminder::new(
            "Please review PR #123",
            "slack",
            "alice",
            Utc::now() + chrono::Duration::hours(1),
            "Review PR #123",
            MessagePriority::Normal,
        );
        assert_eq!(reminder.status, ReminderStatus::Pending);
        assert_eq!(reminder.source_channel, "slack");
        assert!(reminder.is_active());
        assert!(!reminder.is_due()); // 1 hour in the future
    }

    #[test]
    fn test_reminder_lifecycle() {
        let mut reminder = FollowUpReminder::new(
            "Test message",
            "email",
            "bob",
            Utc::now() - chrono::Duration::minutes(5), // Already past
            "Follow up on test",
            MessagePriority::High,
        );
        assert!(reminder.is_due());
        assert!(reminder.is_active());

        reminder.trigger();
        assert_eq!(reminder.status, ReminderStatus::Triggered);
        assert!(reminder.is_active());

        reminder.complete();
        assert_eq!(reminder.status, ReminderStatus::Completed);
        assert!(!reminder.is_active());
    }

    #[test]
    fn test_reminder_dismiss() {
        let mut reminder = FollowUpReminder::new(
            "Test",
            "slack",
            "alice",
            Utc::now() + chrono::Duration::hours(1),
            "Test reminder",
            MessagePriority::Low,
        );
        reminder.dismiss();
        assert_eq!(reminder.status, ReminderStatus::Dismissed);
        assert!(!reminder.is_active());
    }

    #[test]
    fn test_reminder_to_ics() {
        let reminder = FollowUpReminder::new(
            "Review the Q1 report",
            "email",
            "boss@corp.com",
            Utc::now() + chrono::Duration::hours(2),
            "Q1 report review",
            MessagePriority::High,
        );
        let ics = reminder.to_ics();
        assert!(ics.contains("BEGIN:VCALENDAR"));
        assert!(ics.contains("BEGIN:VEVENT"));
        assert!(ics.contains("END:VEVENT"));
        assert!(ics.contains("END:VCALENDAR"));
        assert!(ics.contains("SUMMARY:Follow up:"));
        assert!(ics.contains("Q1 report review"));
        assert!(ics.contains("@rustant"));
    }

    #[test]
    fn test_reminder_ics_no_newlines_in_description() {
        let reminder = FollowUpReminder::new(
            "Line one\nLine two\nLine three",
            "slack",
            "alice",
            Utc::now(),
            "Multi-line test",
            MessagePriority::Normal,
        );
        let ics = reminder.to_ics();
        // Newlines in the source message should be replaced
        assert!(!ics.contains("Line one\nLine two"));
    }

    // --- SchedulerBridge Tests ---

    #[test]
    fn test_bridge_new_empty() {
        let bridge = test_bridge();
        assert_eq!(bridge.total_count(), 0);
        assert!(bridge.active_reminders().is_empty());
        assert!(bridge.due_reminders().is_empty());
    }

    #[test]
    fn test_bridge_schedule_followup() {
        let mut bridge = test_bridge();
        let reminder = bridge.schedule_followup(
            "Review PR #123",
            "slack",
            "alice",
            60, // 60 minutes
            MessagePriority::Normal,
        );
        assert_eq!(reminder.source_channel, "slack");
        assert_eq!(reminder.source_sender, "alice");
        assert_eq!(bridge.total_count(), 1);
        assert_eq!(bridge.active_reminders().len(), 1);
    }

    #[test]
    fn test_bridge_trigger_due() {
        let mut bridge = test_bridge();
        // Schedule one for the past (already due)
        let reminder = FollowUpReminder::new(
            "Past message",
            "slack",
            "alice",
            Utc::now() - chrono::Duration::minutes(5),
            "Past followup",
            MessagePriority::Normal,
        );
        bridge.reminders.push(reminder);

        // Schedule one for the future (not due)
        bridge.schedule_followup(
            "Future message",
            "email",
            "bob",
            60,
            MessagePriority::Normal,
        );

        let triggered = bridge.trigger_due();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].source_sender, "alice");
    }

    #[test]
    fn test_bridge_dismiss() {
        let mut bridge = test_bridge();
        let reminder = bridge.schedule_followup("Test", "slack", "alice", 60, MessagePriority::Low);
        let id = reminder.id;

        assert!(bridge.dismiss(id));
        assert!(bridge.active_reminders().is_empty());
    }

    #[test]
    fn test_bridge_complete() {
        let mut bridge = test_bridge();
        let reminder =
            bridge.schedule_followup("Test", "slack", "alice", 60, MessagePriority::Normal);
        let id = reminder.id;

        assert!(bridge.complete(id));
        assert!(bridge.active_reminders().is_empty());
    }

    #[test]
    fn test_bridge_dismiss_nonexistent() {
        let mut bridge = test_bridge();
        assert!(!bridge.dismiss(Uuid::new_v4()));
    }

    #[test]
    fn test_bridge_cleanup() {
        let mut bridge = test_bridge();
        let _r1 = bridge.schedule_followup("Active", "slack", "alice", 60, MessagePriority::Normal);
        let r2 = bridge.schedule_followup("To dismiss", "email", "bob", 30, MessagePriority::Low);

        bridge.dismiss(r2.id);
        let cleaned = bridge.cleanup();
        assert_eq!(cleaned, 1);
        assert_eq!(bridge.total_count(), 1);
    }

    #[test]
    fn test_bridge_long_message_truncated() {
        let mut bridge = test_bridge();
        let long_message = "a".repeat(200);
        let reminder =
            bridge.schedule_followup(&long_message, "slack", "alice", 60, MessagePriority::Normal);
        assert!(reminder.description.len() <= 83); // 80 + "..."
    }

    #[test]
    fn test_bridge_save_and_load_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = SchedulerBridge::new(dir.path().to_path_buf());

        bridge.schedule_followup(
            "Test message 1",
            "slack",
            "alice",
            60,
            MessagePriority::Normal,
        );
        bridge.schedule_followup("Test message 2", "email", "bob", 120, MessagePriority::High);

        let saved_path = bridge.save_index().unwrap();
        assert!(saved_path.exists());

        // Load into a new bridge
        let mut bridge2 = SchedulerBridge::new(dir.path().to_path_buf());
        let loaded = bridge2.load_index().unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(bridge2.total_count(), 2);
    }

    #[test]
    fn test_bridge_export_ics() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = SchedulerBridge::new(dir.path().to_path_buf());

        let reminder = FollowUpReminder::new(
            "Review PR",
            "slack",
            "alice",
            Utc::now() + chrono::Duration::hours(1),
            "PR review",
            MessagePriority::Normal,
        );

        let path = bridge.export_ics(&reminder).unwrap();
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("BEGIN:VCALENDAR"));
    }

    #[test]
    fn test_bridge_load_nonexistent_index() {
        let dir = tempfile::tempdir().unwrap();
        let mut bridge = SchedulerBridge::new(dir.path().to_path_buf());
        let count = bridge.load_index().unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_bridge_multibyte_utf8_truncation() {
        let mut bridge = test_bridge();
        // 100 CJK characters (each 3 bytes in UTF-8, total 300 bytes > 80 byte threshold)
        // char count = 100 > 80, so description should be truncated
        let cjk_message: String = "漢".repeat(100);
        let reminder =
            bridge.schedule_followup(&cjk_message, "slack", "alice", 60, MessagePriority::Normal);
        // Should not panic — this was the bug with byte indexing
        assert!(reminder.description.ends_with("..."));
        // 80 chars + "..." = 83 chars total
        assert_eq!(reminder.description.chars().count(), 83);
    }

    #[test]
    fn test_bridge_emoji_message_truncation() {
        let mut bridge = test_bridge();
        let emoji_message: String = "🎉".repeat(90);
        let reminder =
            bridge.schedule_followup(&emoji_message, "email", "bob", 30, MessagePriority::High);
        // Should not panic
        assert!(reminder.description.ends_with("..."));
        assert_eq!(reminder.description.chars().count(), 83);
    }

    #[test]
    fn test_bridge_short_message_no_truncation() {
        let mut bridge = test_bridge();
        let short = "Short msg";
        let reminder =
            bridge.schedule_followup(short, "slack", "alice", 60, MessagePriority::Normal);
        assert_eq!(reminder.description, "Short msg");
        assert!(!reminder.description.ends_with("..."));
    }

    // --- S2: CRLF Injection in ICS Tests ---

    #[test]
    fn test_ics_crlf_injection_in_source_message() {
        let reminder = FollowUpReminder::new(
            "Review PR\r\nATTACH:http://attacker.com/malware\r\nDESCRIPTION:fake",
            "slack",
            "alice",
            Utc::now(),
            "Injected desc",
            MessagePriority::Normal,
        );
        let ics = reminder.to_ics();
        // The CRLF must NOT produce a separate ICS property line starting with \r\nATTACH:
        // After escaping, \r is stripped and \n becomes literal \n, so the result should be
        // on a single DESCRIPTION: line, not split across multiple property lines.
        assert!(
            !ics.contains("\r\nATTACH:"),
            "CRLF injection should be prevented"
        );
        // The escaped content should use literal \n not actual newline
        let desc_line = ics
            .lines()
            .find(|l| l.trim().starts_with("DESCRIPTION:"))
            .expect("DESCRIPTION line should exist");
        assert!(
            desc_line.contains("\\n"),
            "Newlines should be escaped to literal \\n"
        );
    }

    #[test]
    fn test_ics_escapes_semicolons_in_sender() {
        let reminder = FollowUpReminder::new(
            "Test message",
            "slack",
            "alice;ATTENDEE:evil@bad.com",
            Utc::now(),
            "Test",
            MessagePriority::Normal,
        );
        let ics = reminder.to_ics();
        // The semicolon should be escaped to \; so it's not interpreted
        // as an ICS parameter separator by calendar parsers.
        assert!(
            ics.contains("\\;ATTENDEE"),
            "Semicolons should be escaped with backslash"
        );
        // Verify the SUMMARY line contains the escaped sender in the expected format
        let summary_line = ics
            .lines()
            .find(|l| l.trim().starts_with("SUMMARY:"))
            .expect("SUMMARY line should exist");
        assert!(summary_line.contains("alice\\;ATTENDEE"));
    }

    #[test]
    fn test_ics_escapes_cr_in_channel() {
        let reminder = FollowUpReminder::new(
            "Test message",
            "slack\r\nX-INJECT:bad",
            "alice",
            Utc::now(),
            "Test",
            MessagePriority::Normal,
        );
        let ics = reminder.to_ics();
        // CRLF in channel name should NOT produce a new ICS property line
        assert!(
            !ics.contains("\r\nX-INJECT:"),
            "CRLF injection in channel name should be prevented"
        );
    }
}
