//! End-to-end test wiring a channel message through the agent bridge,
//! multi-agent orchestrator, task handler, and back to a channel response.
//!
//! This validates the full message flow:
//! Channel → Bridge → Orchestrator → TaskHandler → Orchestrator → Bridge → Channel

use async_trait::async_trait;
use rustant_core::channels::agent_bridge::ChannelAgentBridge;
use rustant_core::channels::{ChannelMessage, ChannelType, ChannelUser};
use rustant_core::multi::messaging::{AgentEnvelope, AgentPayload, MessageBus};
use rustant_core::multi::routing::{AgentRoute, AgentRouter, RouteCondition};
use rustant_core::multi::spawner::AgentSpawner;
use rustant_core::multi::{AgentOrchestrator, TaskHandler};
use std::collections::HashMap;

/// A handler that echoes the task description back with a prefix.
struct EchoTaskHandler;

#[async_trait]
impl TaskHandler for EchoTaskHandler {
    async fn handle_task(
        &self,
        description: &str,
        _args: &HashMap<String, String>,
    ) -> Result<String, String> {
        Ok(format!("Agent reply: {description}"))
    }
}

/// Full round-trip: Slack message → route to agent → handle → response → channel message
#[tokio::test]
async fn test_e2e_slack_message_through_agent_and_back() {
    // 1. Set up multi-agent system
    let mut spawner = AgentSpawner::default();
    let agent_id = spawner.spawn("slack-agent").unwrap();
    let bridge_id = spawner.spawn("bridge").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(agent_id);
    bus.register(bridge_id);

    // 2. Set up routing: Slack messages → slack-agent
    let mut orch_router = AgentRouter::new();
    orch_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: agent_id,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Slack)],
    });

    let mut orch = AgentOrchestrator::new(spawner, bus, orch_router);
    orch.register_handler(agent_id, Box::new(EchoTaskHandler));

    // 3. Create bridge with its own router and incoming channel message
    let mut bridge_router = AgentRouter::new();
    bridge_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: agent_id,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Slack)],
    });
    let bridge = ChannelAgentBridge::new(bridge_router);
    let sender = ChannelUser::new("alice", ChannelType::Slack).with_name("Alice");
    let incoming = ChannelMessage::text(
        ChannelType::Slack,
        "general",
        sender,
        "What is the weather today?",
    );

    // 4. Route message through bridge
    let target = bridge.route_channel_message(&incoming, bridge_id);
    assert_eq!(target, agent_id, "Message should route to slack-agent");

    // 5. Convert to envelope and send to orchestrator
    let envelope = ChannelAgentBridge::channel_message_to_envelope(&incoming, bridge_id, target);
    orch.bus_mut().send(envelope).unwrap();

    // 6. Process the task
    orch.process_pending().await;

    // 7. Receive the response
    let response = orch.bus_mut().receive(&bridge_id).unwrap();
    match &response.payload {
        AgentPayload::TaskResult { output, success } => {
            assert!(success);
            assert_eq!(output, "Agent reply: What is the weather today?");
        }
        other => panic!("Expected TaskResult, got {other:?}"),
    }

    // 8. Convert response back to channel message
    let reply =
        ChannelAgentBridge::envelope_to_channel_message(&response, ChannelType::Slack).unwrap();
    assert_eq!(reply.channel_type, ChannelType::Slack);
    assert_eq!(
        reply.content.as_text(),
        Some("Agent reply: What is the weather today?")
    );
}

/// Verify that different channels route to different agents.
#[tokio::test]
async fn test_e2e_multi_channel_routing() {
    let mut spawner = AgentSpawner::default();
    let slack_agent = spawner.spawn("slack-agent").unwrap();
    let telegram_agent = spawner.spawn("telegram-agent").unwrap();
    let default_agent = spawner.spawn("default").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(slack_agent);
    bus.register(telegram_agent);
    bus.register(default_agent);

    let mut router = AgentRouter::new();
    router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: slack_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Slack)],
    });
    router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: telegram_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Telegram)],
    });

    let bridge = ChannelAgentBridge::new(router);

    // Slack message routes to slack-agent
    let slack_msg = ChannelMessage::text(
        ChannelType::Slack,
        "general",
        ChannelUser::new("bob", ChannelType::Slack),
        "hello from slack",
    );
    assert_eq!(
        bridge.route_channel_message(&slack_msg, default_agent),
        slack_agent
    );

    // Telegram message routes to telegram-agent
    let tg_msg = ChannelMessage::text(
        ChannelType::Telegram,
        "chat123",
        ChannelUser::new("carol", ChannelType::Telegram),
        "hello from telegram",
    );
    assert_eq!(
        bridge.route_channel_message(&tg_msg, default_agent),
        telegram_agent
    );

    // Discord message (no route) falls back to default
    let discord_msg = ChannelMessage::text(
        ChannelType::Discord,
        "server1",
        ChannelUser::new("dave", ChannelType::Discord),
        "hello from discord",
    );
    assert_eq!(
        bridge.route_channel_message(&discord_msg, default_agent),
        default_agent
    );
}

/// Test that channel metadata is preserved through the bridge round-trip.
#[test]
fn test_e2e_channel_metadata_preservation() {
    let sender = ChannelUser::new("alice", ChannelType::Email).with_name("Alice Smith");
    let msg = ChannelMessage::text(ChannelType::Email, "inbox", sender, "Check my schedule")
        .with_metadata("subject", "Schedule Request")
        .with_metadata("from_addr", "alice@example.com");

    let from = uuid::Uuid::new_v4();
    let to = uuid::Uuid::new_v4();
    let envelope = ChannelAgentBridge::channel_message_to_envelope(&msg, from, to);

    match &envelope.payload {
        AgentPayload::TaskRequest { description, args } => {
            assert_eq!(description, "Check my schedule");
            assert_eq!(args.get("channel_type").unwrap(), "Email");
            assert_eq!(args.get("sender").unwrap(), "alice");
        }
        _ => panic!("Expected TaskRequest"),
    }
}

/// Full round-trip for Email: message → bridge → orchestrator → handler → bridge → channel
#[tokio::test]
async fn test_e2e_email_message_through_agent_and_back() {
    let mut spawner = AgentSpawner::default();
    let email_agent = spawner.spawn("email-agent").unwrap();
    let bridge_id = spawner.spawn("bridge").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(email_agent);
    bus.register(bridge_id);

    let mut orch_router = AgentRouter::new();
    orch_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: email_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Email)],
    });

    let mut orch = AgentOrchestrator::new(spawner, bus, orch_router);
    orch.register_handler(email_agent, Box::new(EchoTaskHandler));

    let mut bridge_router = AgentRouter::new();
    bridge_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: email_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Email)],
    });
    let bridge = ChannelAgentBridge::new(bridge_router);
    let sender = ChannelUser::new("alice@example.com", ChannelType::Email).with_name("Alice");
    let incoming = ChannelMessage::text(
        ChannelType::Email,
        "alice@example.com",
        sender,
        "Please schedule a meeting",
    )
    .with_metadata("subject", "Meeting Request");

    // Route + convert + send
    let target = bridge.route_channel_message(&incoming, bridge_id);
    assert_eq!(target, email_agent, "Email should route to email-agent");

    let envelope = ChannelAgentBridge::channel_message_to_envelope(&incoming, bridge_id, target);
    orch.bus_mut().send(envelope).unwrap();
    orch.process_pending().await;

    // Receive and convert back
    let response = orch.bus_mut().receive(&bridge_id).unwrap();
    match &response.payload {
        AgentPayload::TaskResult { output, success } => {
            assert!(success);
            assert_eq!(output, "Agent reply: Please schedule a meeting");
        }
        other => panic!("Expected TaskResult, got {other:?}"),
    }

    let reply =
        ChannelAgentBridge::envelope_to_channel_message(&response, ChannelType::Email).unwrap();
    assert_eq!(reply.channel_type, ChannelType::Email);
    assert_eq!(
        reply.content.as_text(),
        Some("Agent reply: Please schedule a meeting")
    );
}

/// Full round-trip for iMessage channel through the bridge.
#[tokio::test]
async fn test_e2e_imessage_message_through_agent_and_back() {
    let mut spawner = AgentSpawner::default();
    let imessage_agent = spawner.spawn("imessage-agent").unwrap();
    let bridge_id = spawner.spawn("bridge").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(imessage_agent);
    bus.register(bridge_id);

    let mut orch_router = AgentRouter::new();
    orch_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: imessage_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
    });

    let mut orch = AgentOrchestrator::new(spawner, bus, orch_router);
    orch.register_handler(imessage_agent, Box::new(EchoTaskHandler));

    let mut bridge_router = AgentRouter::new();
    bridge_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: imessage_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
    });
    let bridge = ChannelAgentBridge::new(bridge_router);
    let sender = ChannelUser::new("+31644709979", ChannelType::IMessage).with_name("Chaitu");
    let incoming = ChannelMessage::text(
        ChannelType::IMessage,
        "+31644709979",
        sender,
        "Hi! What can you do?",
    );

    let target = bridge.route_channel_message(&incoming, bridge_id);
    assert_eq!(target, imessage_agent);

    let envelope = ChannelAgentBridge::channel_message_to_envelope(&incoming, bridge_id, target);
    orch.bus_mut().send(envelope).unwrap();
    orch.process_pending().await;

    let response = orch.bus_mut().receive(&bridge_id).unwrap();
    match &response.payload {
        AgentPayload::TaskResult { output, success } => {
            assert!(success);
            assert_eq!(output, "Agent reply: Hi! What can you do?");
        }
        other => panic!("Expected TaskResult, got {other:?}"),
    }

    let reply =
        ChannelAgentBridge::envelope_to_channel_message(&response, ChannelType::IMessage).unwrap();
    assert_eq!(reply.channel_type, ChannelType::IMessage);
}

/// Verify all 13 channel types can be routed through the bridge without panics.
#[test]
fn test_e2e_all_channel_types_route_through_bridge() {
    let mut router = AgentRouter::new();
    let default_agent = uuid::Uuid::new_v4();

    // Add routes for a few channels, rest fall back to default
    let email_agent = uuid::Uuid::new_v4();
    let imessage_agent = uuid::Uuid::new_v4();
    router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: email_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Email)],
    });
    router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: imessage_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
    });

    let bridge = ChannelAgentBridge::new(router);

    let all_types = vec![
        ChannelType::Slack,
        ChannelType::Discord,
        ChannelType::Telegram,
        ChannelType::Email,
        ChannelType::Matrix,
        ChannelType::Signal,
        ChannelType::WhatsApp,
        ChannelType::Sms,
        ChannelType::Irc,
        ChannelType::Teams,
        ChannelType::IMessage,
        ChannelType::WebChat,
        ChannelType::Webhook,
    ];

    for ch_type in &all_types {
        let sender = ChannelUser::new("test-user", *ch_type);
        let msg = ChannelMessage::text(*ch_type, "test-channel", sender, "hello");
        let target = bridge.route_channel_message(&msg, default_agent);

        match ch_type {
            ChannelType::Email => assert_eq!(target, email_agent),
            ChannelType::IMessage => assert_eq!(target, imessage_agent),
            _ => assert_eq!(target, default_agent),
        }
    }
}

// ── DM Pairing Integration ──────────────────────────────────────────────

/// Paired device messages are routed through the bridge; unpaired are rejected.
#[tokio::test]
async fn test_e2e_pairing_gates_bridge_routing() {
    use rustant_core::pairing::{PairingManager, PairingResponse};

    let secret = b"e2e-test-shared-secret-32bytes!!";
    let mut spawner = AgentSpawner::default();
    let imessage_agent = spawner.spawn("imessage-agent").unwrap();
    let bridge_id = spawner.spawn("bridge").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(imessage_agent);
    bus.register(bridge_id);

    let mut orch_router = AgentRouter::new();
    orch_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: imessage_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
    });

    let mut orch = AgentOrchestrator::new(spawner, bus, orch_router);
    orch.register_handler(imessage_agent, Box::new(EchoTaskHandler));

    // Set up bridge with pairing
    let mut pm = PairingManager::new(secret);
    let challenge = pm.create_challenge();

    // Pair the device "chaitu-phone"
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).unwrap();
    mac.update(challenge.nonce.as_bytes());
    let hmac_bytes = mac.finalize().into_bytes();
    let response_hmac: String = hmac_bytes.iter().map(|b| format!("{b:02x}")).collect();

    let device_id = uuid::Uuid::new_v4();
    let pair_resp = PairingResponse {
        challenge_id: challenge.challenge_id,
        device_id,
        device_name: "chaitu-phone".into(),
        public_key: "pk-chaitu".into(),
        response_hmac,
    };
    pm.verify_response(&pair_resp);

    let mut bridge_router = AgentRouter::new();
    bridge_router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: imessage_agent,
        conditions: vec![RouteCondition::ChannelType(ChannelType::IMessage)],
    });
    let bridge = ChannelAgentBridge::new(bridge_router).with_pairing(pm);

    // Paired sender → routes to imessage_agent
    let paired_sender = ChannelUser::new("chaitu-phone", ChannelType::IMessage);
    let paired_msg = ChannelMessage::text(
        ChannelType::IMessage,
        "+31644709979",
        paired_sender,
        "Hello from paired device!",
    );
    let target = bridge.route_channel_message(&paired_msg, bridge_id);
    assert_eq!(
        target, imessage_agent,
        "Paired device should route to agent"
    );

    // Unpaired sender → falls back to bridge_id (default)
    let unpaired_sender = ChannelUser::new("stranger", ChannelType::IMessage);
    let unpaired_msg = ChannelMessage::text(
        ChannelType::IMessage,
        "+0000000000",
        unpaired_sender,
        "Hello from stranger!",
    );
    let target2 = bridge.route_channel_message(&unpaired_msg, bridge_id);
    assert_eq!(target2, bridge_id, "Unpaired sender should get default");

    // Paired message flows through orchestrator
    let envelope =
        ChannelAgentBridge::channel_message_to_envelope(&paired_msg, bridge_id, imessage_agent);
    orch.bus_mut().send(envelope).unwrap();
    orch.process_pending().await;

    let response = orch.bus_mut().receive(&bridge_id).unwrap();
    match &response.payload {
        AgentPayload::TaskResult { output, success } => {
            assert!(success);
            assert_eq!(output, "Agent reply: Hello from paired device!");
        }
        other => panic!("Expected TaskResult, got {other:?}"),
    }
}

/// Pairing + revocation: once revoked, messages are no longer routed.
#[test]
fn test_e2e_pairing_revoke_blocks_routing() {
    use rustant_core::pairing::{PairingManager, PairingResponse};

    let secret = b"revocation-test-secret-32bytes!!";
    let mut pm = PairingManager::new(secret);
    let challenge = pm.create_challenge();

    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).unwrap();
    mac.update(challenge.nonce.as_bytes());
    let hmac_bytes = mac.finalize().into_bytes();
    let response_hmac: String = hmac_bytes.iter().map(|b| format!("{b:02x}")).collect();

    let device_id = uuid::Uuid::new_v4();
    let pair_resp = PairingResponse {
        challenge_id: challenge.challenge_id,
        device_id,
        device_name: "revoke-me".into(),
        public_key: "pk".into(),
        response_hmac,
    };
    pm.verify_response(&pair_resp);

    let mut router = AgentRouter::new();
    let agent_id = uuid::Uuid::new_v4();
    let default = uuid::Uuid::new_v4();
    router.add_route(AgentRoute {
        priority: 1,
        target_agent_id: agent_id,
        conditions: vec![RouteCondition::ChannelType(ChannelType::Email)],
    });

    let mut bridge = ChannelAgentBridge::new(router).with_pairing(pm);

    // Before revocation: routed to agent
    let sender = ChannelUser::new("revoke-me", ChannelType::Email);
    let msg = ChannelMessage::text(ChannelType::Email, "inbox", sender.clone(), "test");
    assert_eq!(bridge.route_channel_message(&msg, default), agent_id);

    // Revoke the device
    bridge.pairing_mut().unwrap().revoke_device(&device_id);

    // After revocation: falls back to default
    let msg2 = ChannelMessage::text(ChannelType::Email, "inbox", sender, "test after revoke");
    assert_eq!(bridge.route_channel_message(&msg2, default), default);
}

/// Verify bidirectional envelope conversion works for all channel types.
#[test]
fn test_e2e_envelope_conversion_all_channel_types() {
    let all_types = vec![
        ChannelType::Slack,
        ChannelType::Discord,
        ChannelType::Telegram,
        ChannelType::Email,
        ChannelType::Matrix,
        ChannelType::Signal,
        ChannelType::WhatsApp,
        ChannelType::Sms,
        ChannelType::Irc,
        ChannelType::Teams,
        ChannelType::IMessage,
        ChannelType::WebChat,
        ChannelType::Webhook,
    ];

    for ch_type in &all_types {
        // channel → envelope
        let sender = ChannelUser::new("user1", *ch_type);
        let msg = ChannelMessage::text(*ch_type, "chan1", sender, "test message");
        let from = uuid::Uuid::new_v4();
        let to = uuid::Uuid::new_v4();
        let envelope = ChannelAgentBridge::channel_message_to_envelope(&msg, from, to);

        match &envelope.payload {
            AgentPayload::TaskRequest { description, args } => {
                assert_eq!(description, "test message");
                assert_eq!(args.get("channel_type").unwrap(), &format!("{ch_type:?}"));
            }
            _ => panic!("Expected TaskRequest for {ch_type:?}"),
        }

        // envelope → channel (TaskResult)
        let response_envelope = AgentEnvelope::new(
            to,
            from,
            AgentPayload::TaskResult {
                success: true,
                output: format!("reply for {ch_type:?}"),
            },
        );
        let reply =
            ChannelAgentBridge::envelope_to_channel_message(&response_envelope, *ch_type).unwrap();
        assert_eq!(reply.channel_type, *ch_type);
        assert_eq!(
            reply.content.as_text().unwrap(),
            &format!("reply for {ch_type:?}")
        );
    }
}
