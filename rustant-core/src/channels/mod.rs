//! # Channel System
//!
//! Multi-platform communication channels for the Rustant agent.
//! Each channel implements the `Channel` trait to provide a uniform
//! interface for sending and receiving messages across platforms.

pub mod types;
pub mod manager;
pub mod telegram;
pub mod discord;
pub mod slack;
pub mod webchat;
pub mod matrix;
pub mod signal;
pub mod whatsapp;
pub mod email;
pub mod routing;
pub mod normalize;
pub mod imessage;
pub mod teams;
pub mod sms;
pub mod irc;
pub mod webhook;
pub mod agent_bridge;

pub use agent_bridge::ChannelAgentBridge;
pub use manager::{build_channel_manager, ChannelManager};
pub use normalize::MessageNormalizer;
pub use routing::{ChannelRouter, RoutingCondition, RoutingRule};
pub use types::{
    ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, MessageContent,
    MessageId, StreamingMode, ThreadId,
};
pub use imessage::{IMessageChannel, IMessageConfig};
pub use teams::{TeamsChannel, TeamsConfig};
pub use sms::{SmsChannel, SmsConfig};
pub use irc::{IrcChannel, IrcConfig};
pub use webhook::{WebhookChannel, WebhookConfig};

use async_trait::async_trait;
use crate::error::RustantError;

/// Core trait that all channel implementations must satisfy.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Human-readable name of this channel instance.
    fn name(&self) -> &str;

    /// The platform type.
    fn channel_type(&self) -> ChannelType;

    /// Connect to the channel's platform.
    async fn connect(&mut self) -> Result<(), RustantError>;

    /// Disconnect from the channel's platform.
    async fn disconnect(&mut self) -> Result<(), RustantError>;

    /// Send a message through this channel. Returns the platform message ID.
    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError>;

    /// Poll for new incoming messages.
    async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError>;

    /// Current connection status.
    fn status(&self) -> ChannelStatus;

    /// Convenience: whether the channel is connected.
    fn is_connected(&self) -> bool {
        self.status() == ChannelStatus::Connected
    }

    /// The capabilities that this channel supports.
    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::default()
    }

    /// How this channel receives incoming messages.
    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_type_reexported() {
        let _ = ChannelType::Telegram;
        let _ = ChannelStatus::Connected;
    }

    /// A minimal mock channel to test default trait methods.
    struct DefaultTestChannel;

    #[async_trait]
    impl Channel for DefaultTestChannel {
        fn name(&self) -> &str {
            "default-test"
        }
        fn channel_type(&self) -> ChannelType {
            ChannelType::WebChat
        }
        async fn connect(&mut self) -> Result<(), RustantError> {
            Ok(())
        }
        async fn disconnect(&mut self) -> Result<(), RustantError> {
            Ok(())
        }
        async fn send_message(&self, _msg: ChannelMessage) -> Result<MessageId, RustantError> {
            Ok(MessageId::new("test"))
        }
        async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
            Ok(Vec::new())
        }
        fn status(&self) -> ChannelStatus {
            ChannelStatus::Disconnected
        }
    }

    #[test]
    fn test_default_capabilities() {
        let ch = DefaultTestChannel;
        let caps = ch.capabilities();
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
    fn test_default_streaming_mode() {
        let ch = DefaultTestChannel;
        assert_eq!(ch.streaming_mode(), StreamingMode::Polling { interval_ms: 5000 });
    }
}
