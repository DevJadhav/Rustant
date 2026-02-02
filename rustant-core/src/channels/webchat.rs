//! WebChat channel — wraps the existing GatewayServer for browser-based chat.
//!
//! This adapter bridges the Channel trait to the WebSocket gateway,
//! allowing the agent to interact with web-based clients via the same
//! channel abstraction used for Telegram, Discord, etc.

use super::{Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, MessageId, StreamingMode};
use crate::error::RustantError;
use crate::gateway::{GatewayEvent, SharedGateway};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

/// Configuration for the WebChat channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebChatConfig {
    pub enabled: bool,
}

impl Default for WebChatConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// WebChat channel backed by the gateway.
pub struct WebChatChannel {
    gateway: Option<SharedGateway>,
    status: ChannelStatus,
    name: String,
    outbox: Arc<Mutex<Vec<ChannelMessage>>>,
}

impl WebChatChannel {
    pub fn new() -> Self {
        Self {
            gateway: None,
            status: ChannelStatus::Disconnected,
            name: "webchat".to_string(),
            outbox: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Attach a shared gateway for real WebSocket connectivity.
    pub fn with_gateway(mut self, gw: SharedGateway) -> Self {
        self.gateway = Some(gw);
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

impl Default for WebChatChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for WebChatChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::WebChat
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError> {
        let id = msg.id.clone();

        // If we have a gateway, broadcast the message as an event
        if let Some(ref gw) = self.gateway {
            let text = msg.content.as_text().unwrap_or("").to_string();
            let gw = gw.lock().await;
            gw.broadcast(GatewayEvent::AssistantMessage { content: text });
        }

        // Also store in outbox for testing
        self.outbox.lock().unwrap().push(msg);
        Ok(id)
    }

    async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
        // In a real implementation, this would read from the gateway's incoming queue.
        // For now, return an empty list — messages come in via the WS handler.
        Ok(Vec::new())
    }

    fn status(&self) -> ChannelStatus {
        self.status
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: false,
            supports_reactions: false,
            supports_files: true,
            supports_voice: false,
            supports_video: false,
            max_message_length: None,
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::WebSocket
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;

    #[tokio::test]
    async fn test_webchat_lifecycle() {
        let mut ch = WebChatChannel::new();
        assert_eq!(ch.status(), ChannelStatus::Disconnected);

        ch.connect().await.unwrap();
        assert!(ch.is_connected());

        ch.disconnect().await.unwrap();
        assert!(!ch.is_connected());
    }

    #[tokio::test]
    async fn test_webchat_send_message() {
        let ch = WebChatChannel::new();
        let outbox = ch.outbox.clone();

        let sender = ChannelUser::new("web-user", ChannelType::WebChat);
        let msg = ChannelMessage::text(ChannelType::WebChat, "session-1", sender, "Hello from web!");
        let id = ch.send_message(msg).await.unwrap();
        assert!(!id.0.is_empty());

        let outbox = outbox.lock().unwrap();
        assert_eq!(outbox.len(), 1);
        assert_eq!(outbox[0].content.as_text(), Some("Hello from web!"));
    }

    #[tokio::test]
    async fn test_webchat_receive_empty() {
        let ch = WebChatChannel::new();
        let msgs = ch.receive_messages().await.unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn test_webchat_capabilities() {
        let ch = WebChatChannel::new();
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(caps.supports_files);
        assert!(caps.max_message_length.is_none());
    }

    #[test]
    fn test_webchat_streaming_mode() {
        let ch = WebChatChannel::new();
        assert_eq!(ch.streaming_mode(), StreamingMode::WebSocket);
    }
}
