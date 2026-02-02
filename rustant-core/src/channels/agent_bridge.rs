//! Channels ↔ Multi-Agent bridge — routes channel messages to the multi-agent system.

use crate::channels::{ChannelMessage, ChannelType, ChannelUser};
use crate::multi::messaging::{AgentEnvelope, AgentPayload};
use crate::multi::routing::{AgentRouter, RouteRequest};
use std::collections::HashMap;
use uuid::Uuid;

/// Bridge routing channel messages to agents and back.
pub struct ChannelAgentBridge {
    router: AgentRouter,
}

impl ChannelAgentBridge {
    pub fn new(router: AgentRouter) -> Self {
        Self { router }
    }

    /// Route a channel message to the appropriate agent.
    /// Returns the target agent ID.
    pub fn route_channel_message(&self, msg: &ChannelMessage, default_agent: Uuid) -> Uuid {
        let text = msg.content.as_text().unwrap_or("").to_string();
        let request = RouteRequest::new()
            .with_channel(msg.channel_type)
            .with_user(&msg.sender.id)
            .with_message(text);
        self.router.route(&request).unwrap_or(default_agent)
    }

    /// Wrap a channel message into an AgentEnvelope (TaskRequest payload).
    pub fn channel_message_to_envelope(
        msg: &ChannelMessage,
        from: Uuid,
        to: Uuid,
    ) -> AgentEnvelope {
        let text = msg.content.as_text().unwrap_or("").to_string();
        let mut args = HashMap::new();
        args.insert("channel_type".into(), format!("{:?}", msg.channel_type));
        args.insert("channel_id".into(), msg.channel_id.clone());
        args.insert("sender".into(), msg.sender.id.clone());

        AgentEnvelope::new(
            from,
            to,
            AgentPayload::TaskRequest {
                description: text,
                args,
            },
        )
    }

    /// Extract a ChannelMessage from an AgentEnvelope (if it contains a TaskResult).
    pub fn envelope_to_channel_message(
        envelope: &AgentEnvelope,
        channel_type: ChannelType,
    ) -> Option<ChannelMessage> {
        match &envelope.payload {
            AgentPayload::TaskResult { output, .. } => {
                let sender = ChannelUser::new("agent", channel_type);
                Some(ChannelMessage::text(
                    channel_type,
                    "agent-response",
                    sender,
                    output,
                ))
            }
            AgentPayload::Response { answer, .. } => {
                let sender = ChannelUser::new("agent", channel_type);
                Some(ChannelMessage::text(
                    channel_type,
                    "agent-response",
                    sender,
                    answer,
                ))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multi::routing::{AgentRoute, RouteCondition};

    #[test]
    fn test_agent_bridge_route_by_channel_type() {
        let mut router = AgentRouter::new();
        let agent_id = Uuid::new_v4();
        let default = Uuid::new_v4();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![RouteCondition::ChannelType(ChannelType::Telegram)],
        });

        let bridge = ChannelAgentBridge::new(router);
        let sender = ChannelUser::new("user1", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "telegram-chat", sender, "hi");

        assert_eq!(bridge.route_channel_message(&msg, default), agent_id);
    }

    #[test]
    fn test_agent_bridge_route_by_user() {
        let mut router = AgentRouter::new();
        let agent_id = Uuid::new_v4();
        let default = Uuid::new_v4();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![RouteCondition::UserId("user-42".into())],
        });

        let bridge = ChannelAgentBridge::new(router);
        let sender = ChannelUser::new("user-42", ChannelType::Slack);
        let msg = ChannelMessage::text(ChannelType::Slack, "unknown-channel", sender, "hi");

        assert_eq!(bridge.route_channel_message(&msg, default), agent_id);
    }

    #[test]
    fn test_agent_bridge_fallback_to_default() {
        let router = AgentRouter::new();
        let default = Uuid::new_v4();

        let bridge = ChannelAgentBridge::new(router);
        let sender = ChannelUser::new("nobody", ChannelType::Discord);
        let msg = ChannelMessage::text(ChannelType::Discord, "random", sender, "hi");

        assert_eq!(bridge.route_channel_message(&msg, default), default);
    }

    #[test]
    fn test_agent_bridge_channel_to_envelope() {
        let sender = ChannelUser::new("user1", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "chat1", sender, "build project");
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();

        let envelope = ChannelAgentBridge::channel_message_to_envelope(&msg, from, to);
        assert_eq!(envelope.from, from);
        assert_eq!(envelope.to, to);
        match &envelope.payload {
            AgentPayload::TaskRequest { description, args } => {
                assert_eq!(description, "build project");
                assert_eq!(args.get("sender").unwrap(), "user1");
            }
            _ => panic!("Expected TaskRequest"),
        }
    }

    #[test]
    fn test_agent_bridge_envelope_to_channel() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let envelope = AgentEnvelope::new(
            from,
            to,
            AgentPayload::TaskResult {
                success: true,
                output: "Done!".into(),
            },
        );

        let msg = ChannelAgentBridge::envelope_to_channel_message(&envelope, ChannelType::Telegram);
        assert!(msg.is_some());
        let msg = msg.unwrap();
        assert_eq!(msg.content.as_text(), Some("Done!"));
        assert_eq!(msg.channel_type, ChannelType::Telegram);
    }

    #[test]
    fn test_agent_bridge_non_response_returns_none() {
        let from = Uuid::new_v4();
        let to = Uuid::new_v4();
        let envelope = AgentEnvelope::new(from, to, AgentPayload::StatusQuery);

        let msg = ChannelAgentBridge::envelope_to_channel_message(&envelope, ChannelType::Slack);
        assert!(msg.is_none());
    }
}
