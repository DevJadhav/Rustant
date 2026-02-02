//! Message normalizer — converts platform-specific message formats into
//! the unified `ChannelMessage` type.

use super::{ChannelMessage, ChannelType, ChannelUser, MessageContent};

/// Normalizes platform-specific messages into unified ChannelMessages.
pub struct MessageNormalizer;

impl MessageNormalizer {
    /// Normalize a raw text string from Telegram into a ChannelMessage.
    pub fn normalize_telegram(
        chat_id: i64,
        user_id: i64,
        user_name: &str,
        text: &str,
    ) -> ChannelMessage {
        let sender =
            ChannelUser::new(user_id.to_string(), ChannelType::Telegram).with_name(user_name);
        let content = if text.starts_with('/') {
            let parts: Vec<&str> = text.splitn(2, ' ').collect();
            let cmd = parts[0].to_string();
            let args = if parts.len() > 1 {
                parts[1].split_whitespace().map(String::from).collect()
            } else {
                Vec::new()
            };
            MessageContent::command(cmd, args)
        } else {
            MessageContent::text(text)
        };
        ChannelMessage {
            id: super::MessageId::random(),
            channel_type: ChannelType::Telegram,
            channel_id: chat_id.to_string(),
            sender,
            content,
            timestamp: chrono::Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Normalize a raw Discord message.
    pub fn normalize_discord(
        channel_id: &str,
        author_id: &str,
        author_name: &str,
        content: &str,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(author_id, ChannelType::Discord).with_name(author_name);
        ChannelMessage::text(ChannelType::Discord, channel_id, sender, content)
    }

    /// Normalize a raw Slack message.
    pub fn normalize_slack(
        channel: &str,
        user: &str,
        text: &str,
        thread_ts: Option<&str>,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(user, ChannelType::Slack);
        let mut msg = ChannelMessage::text(ChannelType::Slack, channel, sender, text);
        if let Some(ts) = thread_ts {
            msg = msg.with_metadata("thread_ts", ts);
        }
        msg
    }

    /// Normalize a raw email into a ChannelMessage.
    pub fn normalize_email(from: &str, subject: &str, body: &str) -> ChannelMessage {
        let sender = ChannelUser::new(from, ChannelType::Email);
        ChannelMessage::text(ChannelType::Email, from, sender, body)
            .with_metadata("subject", subject)
    }

    /// Normalize an iMessage incoming message.
    pub fn normalize_imessage(sender_id: &str, text: &str) -> ChannelMessage {
        let sender = ChannelUser::new(sender_id, ChannelType::IMessage);
        ChannelMessage::text(ChannelType::IMessage, sender_id, sender, text)
    }

    /// Normalize a Microsoft Teams message.
    pub fn normalize_teams(
        channel_id: &str,
        from_id: &str,
        from_name: &str,
        content: &str,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(from_id, ChannelType::Teams).with_name(from_name);
        ChannelMessage::text(ChannelType::Teams, channel_id, sender, content)
    }

    /// Normalize an SMS message.
    pub fn normalize_sms(from_number: &str, body: &str) -> ChannelMessage {
        let sender = ChannelUser::new(from_number, ChannelType::Sms);
        ChannelMessage::text(ChannelType::Sms, from_number, sender, body)
    }

    /// Normalize an IRC PRIVMSG.
    pub fn normalize_irc(nick: &str, channel: &str, text: &str) -> ChannelMessage {
        let sender = ChannelUser::new(nick, ChannelType::Irc);
        ChannelMessage::text(ChannelType::Irc, channel, sender, text)
    }

    /// Normalize a webhook payload.
    pub fn normalize_webhook(source: &str, body: &str) -> ChannelMessage {
        let sender = ChannelUser::new(source, ChannelType::Webhook);
        ChannelMessage::text(ChannelType::Webhook, source, sender, body)
    }

    /// Normalize a media message (audio, video, etc.) from any channel.
    pub fn normalize_media(
        channel_type: ChannelType,
        channel_id: &str,
        sender_id: &str,
        url: &str,
        mime_type: &str,
        caption: Option<&str>,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(sender_id, channel_type);
        ChannelMessage {
            id: super::MessageId::random(),
            channel_type,
            channel_id: channel_id.to_string(),
            sender,
            content: MessageContent::Media {
                url: url.to_string(),
                mime_type: mime_type.to_string(),
                caption: caption.map(|c| c.to_string()),
            },
            timestamp: chrono::Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Normalize a location message from any channel.
    pub fn normalize_location(
        channel_type: ChannelType,
        channel_id: &str,
        sender_id: &str,
        latitude: f64,
        longitude: f64,
        label: Option<&str>,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(sender_id, channel_type);
        ChannelMessage {
            id: super::MessageId::random(),
            channel_type,
            channel_id: channel_id.to_string(),
            sender,
            content: MessageContent::Location {
                latitude,
                longitude,
                label: label.map(|l| l.to_string()),
            },
            timestamp: chrono::Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Normalize a contact message from any channel.
    pub fn normalize_contact(
        channel_type: ChannelType,
        channel_id: &str,
        sender_id: &str,
        name: &str,
        phone: Option<&str>,
        email: Option<&str>,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(sender_id, channel_type);
        ChannelMessage {
            id: super::MessageId::random(),
            channel_type,
            channel_id: channel_id.to_string(),
            sender,
            content: MessageContent::Contact {
                name: name.to_string(),
                phone: phone.map(|p| p.to_string()),
                email: email.map(|e| e.to_string()),
            },
            timestamp: chrono::Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// Normalize a reaction message from any channel.
    pub fn normalize_reaction(
        channel_type: ChannelType,
        channel_id: &str,
        sender_id: &str,
        emoji: &str,
        target_message_id: super::MessageId,
    ) -> ChannelMessage {
        let sender = ChannelUser::new(sender_id, channel_type);
        ChannelMessage {
            id: super::MessageId::random(),
            channel_type,
            channel_id: channel_id.to_string(),
            sender,
            content: MessageContent::Reaction {
                emoji: emoji.to_string(),
                target_message_id,
            },
            timestamp: chrono::Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_telegram_text() {
        let msg = MessageNormalizer::normalize_telegram(12345, 42, "Alice", "hello world");
        assert_eq!(msg.channel_type, ChannelType::Telegram);
        assert_eq!(msg.channel_id, "12345");
        assert_eq!(msg.content.as_text(), Some("hello world"));
        assert_eq!(msg.sender.display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_normalize_telegram_command() {
        let msg = MessageNormalizer::normalize_telegram(12345, 42, "Alice", "/help topic");
        match &msg.content {
            MessageContent::Command { command, args } => {
                assert_eq!(command, "/help");
                assert_eq!(args, &["topic"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_normalize_discord() {
        let msg = MessageNormalizer::normalize_discord("ch1", "u1", "Bob", "hey discord");
        assert_eq!(msg.channel_type, ChannelType::Discord);
        assert_eq!(msg.content.as_text(), Some("hey discord"));
    }

    #[test]
    fn test_normalize_slack_with_thread() {
        let msg =
            MessageNormalizer::normalize_slack("general", "U123", "hi slack", Some("1234.5678"));
        assert_eq!(msg.channel_type, ChannelType::Slack);
        assert_eq!(msg.content.as_text(), Some("hi slack"));
        assert_eq!(
            msg.metadata.get("thread_ts").map(|s| s.as_str()),
            Some("1234.5678")
        );
    }

    #[test]
    fn test_normalize_email() {
        let msg = MessageNormalizer::normalize_email("alice@ex.com", "Subject Line", "body text");
        assert_eq!(msg.channel_type, ChannelType::Email);
        assert_eq!(msg.content.as_text(), Some("body text"));
        assert_eq!(
            msg.metadata.get("subject").map(|s| s.as_str()),
            Some("Subject Line")
        );
    }

    #[test]
    fn test_normalize_imessage() {
        let msg = MessageNormalizer::normalize_imessage("+1234567890", "hi from imsg");
        assert_eq!(msg.channel_type, ChannelType::IMessage);
        assert_eq!(msg.content.as_text(), Some("hi from imsg"));
        assert_eq!(msg.sender.id, "+1234567890");
    }

    #[test]
    fn test_normalize_teams() {
        let msg = MessageNormalizer::normalize_teams("ch1", "u1", "Alice", "teams msg");
        assert_eq!(msg.channel_type, ChannelType::Teams);
        assert_eq!(msg.content.as_text(), Some("teams msg"));
        assert_eq!(msg.sender.display_name.as_deref(), Some("Alice"));
    }

    #[test]
    fn test_normalize_sms() {
        let msg = MessageNormalizer::normalize_sms("+9876543210", "sms body");
        assert_eq!(msg.channel_type, ChannelType::Sms);
        assert_eq!(msg.content.as_text(), Some("sms body"));
    }

    #[test]
    fn test_normalize_irc() {
        let msg = MessageNormalizer::normalize_irc("nick42", "#channel", "irc msg");
        assert_eq!(msg.channel_type, ChannelType::Irc);
        assert_eq!(msg.channel_id, "#channel");
        assert_eq!(msg.content.as_text(), Some("irc msg"));
    }

    #[test]
    fn test_normalize_webhook() {
        let msg = MessageNormalizer::normalize_webhook("ext-system", "webhook data");
        assert_eq!(msg.channel_type, ChannelType::Webhook);
        assert_eq!(msg.content.as_text(), Some("webhook data"));
    }

    #[test]
    fn test_normalize_media_content() {
        let msg = MessageNormalizer::normalize_media(
            ChannelType::Telegram,
            "chat1",
            "user1",
            "https://example.com/video.mp4",
            "video/mp4",
            Some("My video"),
        );
        assert_eq!(msg.channel_type, ChannelType::Telegram);
        match &msg.content {
            MessageContent::Media {
                url,
                mime_type,
                caption,
            } => {
                assert_eq!(url, "https://example.com/video.mp4");
                assert_eq!(mime_type, "video/mp4");
                assert_eq!(caption.as_deref(), Some("My video"));
            }
            _ => panic!("Expected Media"),
        }
    }

    #[test]
    fn test_normalize_location_content() {
        let msg = MessageNormalizer::normalize_location(
            ChannelType::WhatsApp,
            "chat1",
            "user1",
            37.7749,
            -122.4194,
            Some("San Francisco"),
        );
        assert_eq!(msg.channel_type, ChannelType::WhatsApp);
        match &msg.content {
            MessageContent::Location {
                latitude,
                longitude,
                label,
            } => {
                assert!((latitude - 37.7749).abs() < f64::EPSILON);
                assert!((longitude - (-122.4194)).abs() < f64::EPSILON);
                assert_eq!(label.as_deref(), Some("San Francisco"));
            }
            _ => panic!("Expected Location"),
        }
    }

    #[test]
    fn test_normalize_contact_content() {
        let msg = MessageNormalizer::normalize_contact(
            ChannelType::Telegram,
            "chat1",
            "user1",
            "Jane Doe",
            Some("+1234567890"),
            Some("jane@example.com"),
        );
        assert_eq!(msg.channel_type, ChannelType::Telegram);
        match &msg.content {
            MessageContent::Contact { name, phone, email } => {
                assert_eq!(name, "Jane Doe");
                assert_eq!(phone.as_deref(), Some("+1234567890"));
                assert_eq!(email.as_deref(), Some("jane@example.com"));
            }
            _ => panic!("Expected Contact"),
        }
    }

    #[test]
    fn test_normalize_reaction_content() {
        let target_id = crate::channels::MessageId::random();
        let msg = MessageNormalizer::normalize_reaction(
            ChannelType::Slack,
            "general",
            "user1",
            "👍",
            target_id.clone(),
        );
        assert_eq!(msg.channel_type, ChannelType::Slack);
        match &msg.content {
            MessageContent::Reaction {
                emoji,
                target_message_id,
            } => {
                assert_eq!(emoji, "👍");
                assert_eq!(target_message_id, &target_id);
            }
            _ => panic!("Expected Reaction"),
        }
    }
}
