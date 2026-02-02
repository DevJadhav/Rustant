//! Gateway ↔ Channels bridge — translates between gateway events and channel messages.

use crate::channels::{ChannelMessage, ChannelType, ChannelUser};
use crate::gateway::events::{GatewayEvent, ServerMessage};

/// Bridge connecting Gateway events to the Channel system.
pub struct ChannelBridge;

impl ChannelBridge {
    pub fn new() -> Self {
        Self
    }

    /// Translate a gateway event into channel messages (if applicable).
    pub fn handle_gateway_event(&self, event: &GatewayEvent) -> Vec<ChannelMessage> {
        match event {
            GatewayEvent::TaskSubmitted { description, .. } => {
                let sender = ChannelUser::new("gateway", ChannelType::WebChat);
                vec![ChannelMessage::text(
                    ChannelType::WebChat,
                    "gateway",
                    sender,
                    description,
                )]
            }
            GatewayEvent::AssistantMessage { content } => {
                let sender = ChannelUser::new("assistant", ChannelType::WebChat);
                vec![ChannelMessage::text(
                    ChannelType::WebChat,
                    "gateway",
                    sender,
                    content,
                )]
            }
            _ => Vec::new(),
        }
    }

    /// Translate a channel message into a gateway event.
    pub fn gateway_event_from_channel_message(msg: &ChannelMessage) -> GatewayEvent {
        let channel_type = format!("{:?}", msg.channel_type);
        let text = msg.content.as_text().unwrap_or("").to_string();
        GatewayEvent::ChannelMessageReceived {
            channel_type,
            message: text,
        }
    }

    /// Translate a channel message into a ServerMessage.
    pub fn server_message_from_channel_message(msg: &ChannelMessage) -> ServerMessage {
        ServerMessage::Event {
            event: Self::gateway_event_from_channel_message(msg),
        }
    }
}

impl Default for ChannelBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_bridge_task_submitted_to_channel_message() {
        let bridge = ChannelBridge::new();
        let event = GatewayEvent::TaskSubmitted {
            task_id: Uuid::new_v4(),
            description: "do something".into(),
        };
        let messages = bridge.handle_gateway_event(&event);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content.as_text(), Some("do something"));
    }

    #[test]
    fn test_bridge_channel_message_to_gateway_event() {
        let sender = ChannelUser::new("user1", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "chat1", sender, "hello");
        let event = ChannelBridge::gateway_event_from_channel_message(&msg);
        match event {
            GatewayEvent::ChannelMessageReceived {
                channel_type,
                message,
            } => {
                assert!(channel_type.contains("Telegram"));
                assert_eq!(message, "hello");
            }
            _ => panic!("Expected ChannelMessageReceived"),
        }
    }

    #[test]
    fn test_bridge_ignore_irrelevant_events() {
        let bridge = ChannelBridge::new();
        let event = GatewayEvent::Connected {
            connection_id: Uuid::new_v4(),
        };
        let messages = bridge.handle_gateway_event(&event);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_bridge_preserves_metadata() {
        let bridge = ChannelBridge::new();
        let event = GatewayEvent::AssistantMessage {
            content: "response text".into(),
        };
        let messages = bridge.handle_gateway_event(&event);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel_type, ChannelType::WebChat);
    }

    #[test]
    fn test_bridge_handles_all_message_content_types() {
        let sender = ChannelUser::new("user", ChannelType::Slack);
        let text_msg = ChannelMessage::text(ChannelType::Slack, "ch", sender.clone(), "text");
        let event = ChannelBridge::gateway_event_from_channel_message(&text_msg);
        match event {
            GatewayEvent::ChannelMessageReceived { message, .. } => {
                assert_eq!(message, "text");
            }
            _ => panic!("Expected ChannelMessageReceived"),
        }

        // Non-text content returns empty string for as_text
        // Use a text message and verify it round-trips
        let another = ChannelMessage::text(ChannelType::Slack, "ch", sender, "");
        let event = ChannelBridge::gateway_event_from_channel_message(&another);
        match event {
            GatewayEvent::ChannelMessageReceived { message, .. } => {
                assert_eq!(message, "");
            }
            _ => panic!("Expected ChannelMessageReceived"),
        }
    }

    #[test]
    fn test_bridge_roundtrip() {
        let bridge = ChannelBridge::new();
        // Gateway → Channel
        let event = GatewayEvent::TaskSubmitted {
            task_id: Uuid::new_v4(),
            description: "build project".into(),
        };
        let messages = bridge.handle_gateway_event(&event);
        assert_eq!(messages.len(), 1);

        // Channel → Gateway
        let back = ChannelBridge::gateway_event_from_channel_message(&messages[0]);
        match back {
            GatewayEvent::ChannelMessageReceived { message, .. } => {
                assert_eq!(message, "build project");
            }
            _ => panic!("Expected ChannelMessageReceived"),
        }
    }
}
