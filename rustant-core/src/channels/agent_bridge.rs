//! Channels ↔ Multi-Agent bridge — routes channel messages to the multi-agent system.
//!
//! Optionally integrates with [`PairingManager`] to enforce device pairing for DM channels.
//! When a `PairingManager` is attached, only messages from paired device IDs are routed;
//! unpaired senders receive the `default_agent` fallback.

use crate::channels::{ChannelMessage, ChannelType, ChannelUser};
use crate::multi::messaging::{AgentEnvelope, AgentPayload};
use crate::multi::routing::{AgentRouter, RouteRequest};
use crate::pairing::PairingManager;
use std::collections::HashMap;
use uuid::Uuid;

/// Bridge routing channel messages to agents and back.
///
/// When `pairing` is set, the bridge will only route messages from senders
/// whose `sender.id` matches a paired device name. Messages from unpaired
/// senders are routed to the `default_agent`.
pub struct ChannelAgentBridge {
    router: AgentRouter,
    pairing: Option<PairingManager>,
}

impl ChannelAgentBridge {
    pub fn new(router: AgentRouter) -> Self {
        Self {
            router,
            pairing: None,
        }
    }

    /// Attach a pairing manager to enforce device-pairing for DM routing.
    pub fn with_pairing(mut self, pairing: PairingManager) -> Self {
        self.pairing = Some(pairing);
        self
    }

    /// Returns `true` if the sender is paired (or no pairing manager is set).
    pub fn is_sender_paired(&self, sender_id: &str) -> bool {
        match &self.pairing {
            None => true,
            Some(pm) => pm
                .paired_devices()
                .iter()
                .any(|d| d.device_name == sender_id || d.device_id.to_string() == sender_id),
        }
    }

    /// Access the pairing manager, if present.
    pub fn pairing(&self) -> Option<&PairingManager> {
        self.pairing.as_ref()
    }

    /// Mutable access to the pairing manager, if present.
    pub fn pairing_mut(&mut self) -> Option<&mut PairingManager> {
        self.pairing.as_mut()
    }

    /// Route a channel message to the appropriate agent.
    /// Returns the target agent ID.
    ///
    /// If a pairing manager is attached, unpaired senders are routed to
    /// `default_agent` regardless of router rules.
    pub fn route_channel_message(&self, msg: &ChannelMessage, default_agent: Uuid) -> Uuid {
        // When pairing is enabled, reject unpaired senders
        if !self.is_sender_paired(&msg.sender.id) {
            return default_agent;
        }

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

    // -- Pairing integration --------------------------------------------------

    #[test]
    fn test_bridge_without_pairing_allows_all() {
        let router = AgentRouter::new();
        let bridge = ChannelAgentBridge::new(router);
        assert!(bridge.is_sender_paired("anyone"));
        assert!(bridge.pairing().is_none());
    }

    #[test]
    fn test_bridge_with_pairing_rejects_unpaired_sender() {
        let mut router = AgentRouter::new();
        let agent_id = Uuid::new_v4();
        let default = Uuid::new_v4();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
        });

        let pm = PairingManager::new(b"secret");
        let bridge = ChannelAgentBridge::new(router).with_pairing(pm);

        // No paired devices → unpaired sender goes to default
        let sender = ChannelUser::new("stranger", ChannelType::IMessage);
        let msg = ChannelMessage::text(ChannelType::IMessage, "dm", sender, "hello");
        assert_eq!(bridge.route_channel_message(&msg, default), default);
    }

    #[test]
    fn test_bridge_with_pairing_routes_paired_device() {
        use crate::pairing::PairingResponse;

        let secret = b"shared-secret-key-for-tests-32b!";
        let mut router = AgentRouter::new();
        let agent_id = Uuid::new_v4();
        let default = Uuid::new_v4();
        router.add_route(AgentRoute {
            priority: 1,
            target_agent_id: agent_id,
            conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
        });

        let mut pm = PairingManager::new(secret);
        let challenge = pm.create_challenge();

        // Simulate the device computing the correct HMAC
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(challenge.nonce.as_bytes());
        let hmac_result = mac.finalize().into_bytes();
        let response_hmac: String = hmac_result.iter().map(|b| format!("{b:02x}")).collect();

        let device_id = Uuid::new_v4();
        let pair_resp = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id,
            device_name: "my-phone".into(),
            public_key: "pk-abc".into(),
            response_hmac,
        };
        pm.verify_response(&pair_resp);

        let bridge = ChannelAgentBridge::new(router).with_pairing(pm);

        // Paired device name matches sender.id → routed to agent
        assert!(bridge.is_sender_paired("my-phone"));
        let sender = ChannelUser::new("my-phone", ChannelType::IMessage);
        let msg = ChannelMessage::text(ChannelType::IMessage, "dm", sender, "hello");
        assert_eq!(bridge.route_channel_message(&msg, default), agent_id);

        // Unknown sender → falls back to default
        assert!(!bridge.is_sender_paired("stranger"));
        let sender2 = ChannelUser::new("stranger", ChannelType::IMessage);
        let msg2 = ChannelMessage::text(ChannelType::IMessage, "dm", sender2, "hello");
        assert_eq!(bridge.route_channel_message(&msg2, default), default);
    }

    #[test]
    fn test_bridge_pairing_revoke_device() {
        use crate::pairing::PairingResponse;

        let secret = b"shared-secret-key-for-tests-32b!";
        let mut pm = PairingManager::new(secret);
        let challenge = pm.create_challenge();

        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(challenge.nonce.as_bytes());
        let hmac_result = mac.finalize().into_bytes();
        let response_hmac: String = hmac_result.iter().map(|b| format!("{b:02x}")).collect();

        let device_id = Uuid::new_v4();
        let pair_resp = PairingResponse {
            challenge_id: challenge.challenge_id,
            device_id,
            device_name: "laptop".into(),
            public_key: "pk".into(),
            response_hmac,
        };
        pm.verify_response(&pair_resp);

        let router = AgentRouter::new();
        let mut bridge = ChannelAgentBridge::new(router).with_pairing(pm);

        assert!(bridge.is_sender_paired("laptop"));

        // Revoke
        bridge.pairing_mut().unwrap().revoke_device(&device_id);
        assert!(!bridge.is_sender_paired("laptop"));
    }
}
