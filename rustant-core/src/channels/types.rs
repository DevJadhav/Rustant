//! Channel types and message protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Identifies a channel type / platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChannelType {
    Telegram,
    Discord,
    Slack,
    WebChat,
    Matrix,
    Signal,
    WhatsApp,
    Email,
    Irc,
    Webhook,
    Sms,
    Teams,
    IMessage,
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Telegram => write!(f, "telegram"),
            Self::Discord => write!(f, "discord"),
            Self::Slack => write!(f, "slack"),
            Self::WebChat => write!(f, "webchat"),
            Self::Matrix => write!(f, "matrix"),
            Self::Signal => write!(f, "signal"),
            Self::WhatsApp => write!(f, "whatsapp"),
            Self::Email => write!(f, "email"),
            Self::Irc => write!(f, "irc"),
            Self::Webhook => write!(f, "webhook"),
            Self::Sms => write!(f, "sms"),
            Self::Teams => write!(f, "teams"),
            Self::IMessage => write!(f, "imessage"),
        }
    }
}

/// Describes the capabilities that a channel supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelCapabilities {
    pub supports_threads: bool,
    pub supports_reactions: bool,
    pub supports_files: bool,
    pub supports_voice: bool,
    pub supports_video: bool,
    pub max_message_length: Option<usize>,
    pub supports_editing: bool,
    pub supports_deletion: bool,
}

/// How a channel receives incoming messages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamingMode {
    /// Periodic polling with a given interval.
    Polling { interval_ms: u64 },
    /// Persistent WebSocket connection.
    WebSocket,
    /// Server-sent events (HTTP streaming).
    ServerSentEvents,
    /// Long-polling (HTTP held open until data).
    LongPolling,
}

impl Default for StreamingMode {
    fn default() -> Self {
        Self::Polling { interval_ms: 5000 }
    }
}

/// Unique identifier for a thread within a channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

impl ThreadId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique identifier for a message across channels.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn random() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// A user within a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelUser {
    pub id: String,
    pub display_name: Option<String>,
    pub channel_type: ChannelType,
}

impl ChannelUser {
    pub fn new(id: impl Into<String>, channel_type: ChannelType) -> Self {
        Self {
            id: id.into(),
            display_name: None,
            channel_type,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }
}

/// Content carried by a channel message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MessageContent {
    /// Plain text content.
    Text { text: String },
    /// An image (URL or base64).
    Image {
        url: String,
        alt_text: Option<String>,
    },
    /// A file attachment.
    File {
        url: String,
        filename: String,
        size_bytes: Option<u64>,
    },
    /// A command (e.g., `/start`, `/help`).
    Command { command: String, args: Vec<String> },
    /// Generic media (audio, video, etc.).
    Media {
        url: String,
        mime_type: String,
        caption: Option<String>,
    },
    /// A geographic location.
    Location {
        latitude: f64,
        longitude: f64,
        label: Option<String>,
    },
    /// A contact card.
    Contact {
        name: String,
        phone: Option<String>,
        email: Option<String>,
    },
    /// A reaction to another message.
    Reaction {
        emoji: String,
        target_message_id: MessageId,
    },
}

impl MessageContent {
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    pub fn command(command: impl Into<String>, args: Vec<String>) -> Self {
        Self::Command {
            command: command.into(),
            args,
        }
    }

    /// Extract plain text content, if present.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }
}

/// Connection status of a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelStatus {
    /// Not yet connected.
    Disconnected,
    /// Actively connecting.
    Connecting,
    /// Connected and ready.
    Connected,
    /// Connection lost, will retry.
    Reconnecting,
    /// Permanently failed.
    Failed,
}

/// A message sent or received over a channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelMessage {
    pub id: MessageId,
    pub channel_type: ChannelType,
    pub channel_id: String,
    pub sender: ChannelUser,
    pub content: MessageContent,
    pub timestamp: DateTime<Utc>,
    pub reply_to: Option<MessageId>,
    pub thread_id: Option<ThreadId>,
    pub metadata: HashMap<String, String>,
}

impl ChannelMessage {
    /// Create a new text message.
    pub fn text(
        channel_type: ChannelType,
        channel_id: impl Into<String>,
        sender: ChannelUser,
        text: impl Into<String>,
    ) -> Self {
        Self {
            id: MessageId::random(),
            channel_type,
            channel_id: channel_id.into(),
            sender,
            content: MessageContent::text(text),
            timestamp: Utc::now(),
            reply_to: None,
            thread_id: None,
            metadata: HashMap::new(),
        }
    }

    /// Set a reply-to reference.
    pub fn with_reply_to(mut self, reply_to: MessageId) -> Self {
        self.reply_to = Some(reply_to);
        self
    }

    /// Set a thread ID for threaded conversations.
    pub fn with_thread(mut self, thread_id: ThreadId) -> Self {
        self.thread_id = Some(thread_id);
        self
    }

    /// Add a metadata key-value pair.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_display() {
        assert_eq!(ChannelType::Telegram.to_string(), "telegram");
        assert_eq!(ChannelType::Discord.to_string(), "discord");
        assert_eq!(ChannelType::Slack.to_string(), "slack");
        assert_eq!(ChannelType::WebChat.to_string(), "webchat");
        assert_eq!(ChannelType::Email.to_string(), "email");
    }

    #[test]
    fn test_message_id() {
        let id = MessageId::new("msg-123");
        assert_eq!(id.0, "msg-123");
        let random = MessageId::random();
        assert!(!random.0.is_empty());
    }

    #[test]
    fn test_channel_user() {
        let user = ChannelUser::new("user123", ChannelType::Telegram).with_name("Alice");
        assert_eq!(user.id, "user123");
        assert_eq!(user.display_name.as_deref(), Some("Alice"));
        assert_eq!(user.channel_type, ChannelType::Telegram);
    }

    #[test]
    fn test_message_content_text() {
        let content = MessageContent::text("Hello world");
        assert_eq!(content.as_text(), Some("Hello world"));

        let img = MessageContent::Image {
            url: "https://example.com/img.png".into(),
            alt_text: None,
        };
        assert_eq!(img.as_text(), None);
    }

    #[test]
    fn test_channel_message_construction() {
        let sender = ChannelUser::new("user1", ChannelType::Slack);
        let msg = ChannelMessage::text(ChannelType::Slack, "general", sender, "hello")
            .with_metadata("thread_ts", "123456.789");

        assert_eq!(msg.channel_type, ChannelType::Slack);
        assert_eq!(msg.channel_id, "general");
        assert_eq!(msg.content.as_text(), Some("hello"));
        assert_eq!(
            msg.metadata.get("thread_ts").map(|s| s.as_str()),
            Some("123456.789")
        );
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn test_channel_message_serialization() {
        let sender = ChannelUser::new("u1", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "chat1", sender, "hi");
        let json = serde_json::to_string(&msg).unwrap();
        let restored: ChannelMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.channel_type, ChannelType::Telegram);
        assert_eq!(restored.content.as_text(), Some("hi"));
    }

    #[test]
    fn test_channel_status_variants() {
        let statuses = [
            ChannelStatus::Disconnected,
            ChannelStatus::Connecting,
            ChannelStatus::Connected,
            ChannelStatus::Reconnecting,
            ChannelStatus::Failed,
        ];
        for status in &statuses {
            let json = serde_json::to_string(status).unwrap();
            let restored: ChannelStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(*status, restored);
        }
    }

    #[test]
    fn test_message_content_command() {
        let content = MessageContent::command("/help", vec!["topic".into()]);
        match content {
            MessageContent::Command { command, args } => {
                assert_eq!(command, "/help");
                assert_eq!(args, vec!["topic"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_channel_capabilities_default() {
        let caps = ChannelCapabilities::default();
        assert!(!caps.supports_threads);
        assert!(!caps.supports_reactions);
        assert!(!caps.supports_files);
        assert!(!caps.supports_voice);
        assert!(!caps.supports_video);
        assert!(caps.max_message_length.is_none());
        assert!(!caps.supports_editing);
        assert!(!caps.supports_deletion);
    }

    #[test]
    fn test_channel_capabilities_telegram() {
        let caps = ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_voice: true,
            supports_video: false,
            max_message_length: Some(4096),
            supports_editing: false,
            supports_deletion: false,
        };
        assert!(caps.supports_threads);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn test_streaming_mode_variants() {
        let polling = StreamingMode::Polling { interval_ms: 1000 };
        assert_eq!(polling, StreamingMode::Polling { interval_ms: 1000 });

        let ws = StreamingMode::WebSocket;
        assert_eq!(ws, StreamingMode::WebSocket);

        let sse = StreamingMode::ServerSentEvents;
        assert_eq!(sse, StreamingMode::ServerSentEvents);

        let lp = StreamingMode::LongPolling;
        assert_eq!(lp, StreamingMode::LongPolling);

        assert_eq!(
            StreamingMode::default(),
            StreamingMode::Polling { interval_ms: 5000 }
        );
    }

    #[test]
    fn test_thread_id_display_and_eq() {
        let t1 = ThreadId::new("thread-abc");
        let t2 = ThreadId::new("thread-abc");
        let t3 = ThreadId::new("thread-xyz");
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        assert_eq!(t1.to_string(), "thread-abc");
    }

    #[test]
    fn test_channel_message_with_thread() {
        let sender = ChannelUser::new("user1", ChannelType::Slack);
        let msg = ChannelMessage::text(ChannelType::Slack, "general", sender, "threaded reply")
            .with_thread(ThreadId::new("t-123"));
        assert_eq!(msg.thread_id, Some(ThreadId::new("t-123")));
    }

    #[test]
    fn test_message_content_media() {
        let content = MessageContent::Media {
            url: "https://example.com/audio.mp3".into(),
            mime_type: "audio/mpeg".into(),
            caption: Some("My song".into()),
        };
        if let MessageContent::Media {
            url,
            mime_type,
            caption,
        } = &content
        {
            assert_eq!(url, "https://example.com/audio.mp3");
            assert_eq!(mime_type, "audio/mpeg");
            assert_eq!(caption.as_deref(), Some("My song"));
        } else {
            panic!("Expected Media");
        }
    }

    #[test]
    fn test_message_content_location() {
        let content = MessageContent::Location {
            latitude: 37.7749,
            longitude: -122.4194,
            label: Some("San Francisco".into()),
        };
        if let MessageContent::Location {
            latitude,
            longitude,
            label,
        } = &content
        {
            assert!((latitude - 37.7749).abs() < f64::EPSILON);
            assert!((longitude - (-122.4194)).abs() < f64::EPSILON);
            assert_eq!(label.as_deref(), Some("San Francisco"));
        } else {
            panic!("Expected Location");
        }
    }

    #[test]
    fn test_message_content_contact_reaction() {
        let contact = MessageContent::Contact {
            name: "Alice".into(),
            phone: Some("+1234567890".into()),
            email: None,
        };
        if let MessageContent::Contact { name, phone, email } = &contact {
            assert_eq!(name, "Alice");
            assert_eq!(phone.as_deref(), Some("+1234567890"));
            assert!(email.is_none());
        } else {
            panic!("Expected Contact");
        }

        let reaction = MessageContent::Reaction {
            emoji: "👍".into(),
            target_message_id: MessageId::new("msg-42"),
        };
        if let MessageContent::Reaction {
            emoji,
            target_message_id,
        } = &reaction
        {
            assert_eq!(emoji, "👍");
            assert_eq!(target_message_id, &MessageId::new("msg-42"));
        } else {
            panic!("Expected Reaction");
        }
    }

    #[test]
    fn test_new_channel_type_display() {
        assert_eq!(ChannelType::Irc.to_string(), "irc");
        assert_eq!(ChannelType::Webhook.to_string(), "webhook");
        assert_eq!(ChannelType::Sms.to_string(), "sms");
        assert_eq!(ChannelType::Teams.to_string(), "teams");
        assert_eq!(ChannelType::IMessage.to_string(), "imessage");
    }
}
