//! Integration tests for real channel implementations.
//!
//! These tests are `#[ignore]`d by default and require real credentials
//! via environment variables. Run with:
//!
//! ```sh
//! RUSTANT_TEST_SLACK_TOKEN="xoxb-..." cargo test --test integration_channels -- --ignored
//! ```

use rustant_core::channels::*;

// ── Slack ────────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_SLACK_TOKEN"]
async fn test_slack_real_auth() {
    let token = std::env::var("RUSTANT_TEST_SLACK_TOKEN").expect("RUSTANT_TEST_SLACK_TOKEN not set");
    let config = rustant_core::channels::slack::SlackConfig {
        bot_token: token,
        ..Default::default()
    };
    let mut ch = rustant_core::channels::slack::create_slack_channel(config);
    let result = ch.connect().await;
    assert!(result.is_ok(), "Slack connect failed: {:?}", result.err());
    assert!(ch.is_connected());
    ch.disconnect().await.unwrap();
}

// ── Telegram ─────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_TELEGRAM_TOKEN"]
async fn test_telegram_real_auth() {
    let token =
        std::env::var("RUSTANT_TEST_TELEGRAM_TOKEN").expect("RUSTANT_TEST_TELEGRAM_TOKEN not set");
    let config = rustant_core::channels::telegram::TelegramConfig {
        bot_token: token,
        ..Default::default()
    };
    let mut ch = rustant_core::channels::telegram::create_telegram_channel(config);
    let result = ch.connect().await;
    assert!(result.is_ok(), "Telegram connect failed: {:?}", result.err());
    assert!(ch.is_connected());
    ch.disconnect().await.unwrap();
}

// ── Discord ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_DISCORD_TOKEN"]
async fn test_discord_real_auth() {
    let token =
        std::env::var("RUSTANT_TEST_DISCORD_TOKEN").expect("RUSTANT_TEST_DISCORD_TOKEN not set");
    let config = rustant_core::channels::discord::DiscordConfig {
        bot_token: token,
        ..Default::default()
    };
    let mut ch = rustant_core::channels::discord::create_discord_channel(config);
    let result = ch.connect().await;
    assert!(result.is_ok(), "Discord connect failed: {:?}", result.err());
    assert!(ch.is_connected());
    ch.disconnect().await.unwrap();
}

// ── Matrix ───────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_MATRIX_HOMESERVER and RUSTANT_TEST_MATRIX_TOKEN"]
async fn test_matrix_real_auth() {
    let homeserver =
        std::env::var("RUSTANT_TEST_MATRIX_HOMESERVER").expect("RUSTANT_TEST_MATRIX_HOMESERVER not set");
    let token =
        std::env::var("RUSTANT_TEST_MATRIX_TOKEN").expect("RUSTANT_TEST_MATRIX_TOKEN not set");
    let config = rustant_core::channels::matrix::MatrixConfig {
        homeserver_url: homeserver,
        access_token: token,
        ..Default::default()
    };
    let mut ch = rustant_core::channels::matrix::create_matrix_channel(config);
    let result = ch.connect().await;
    assert!(result.is_ok(), "Matrix connect failed: {:?}", result.err());
    assert!(ch.is_connected());
    ch.disconnect().await.unwrap();
}

// ── Webhook ──────────────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_WEBHOOK_URL"]
async fn test_webhook_real_send() {
    let url = std::env::var("RUSTANT_TEST_WEBHOOK_URL").expect("RUSTANT_TEST_WEBHOOK_URL not set");
    let config = rustant_core::channels::webhook::WebhookConfig {
        enabled: true,
        outbound_url: url,
        ..Default::default()
    };
    let mut ch = rustant_core::channels::webhook::create_webhook_channel(config);
    ch.connect().await.unwrap();
    assert!(ch.is_connected());

    let sender = ChannelUser::new("test-bot", ChannelType::Webhook);
    let msg = ChannelMessage::text(ChannelType::Webhook, "test", sender, "integration test ping");
    let result = ch.send_message(msg).await;
    assert!(result.is_ok(), "Webhook send failed: {:?}", result.err());
    ch.disconnect().await.unwrap();
}

// ── Channel Manager ──────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires RUSTANT_TEST_SLACK_TOKEN"]
async fn test_build_channel_manager_with_slack() {
    let token = std::env::var("RUSTANT_TEST_SLACK_TOKEN").expect("RUSTANT_TEST_SLACK_TOKEN not set");
    let channels_config = rustant_core::config::ChannelsConfig {
        slack: Some(rustant_core::channels::slack::SlackConfig {
            bot_token: token,
            ..Default::default()
        }),
        ..Default::default()
    };

    let mut mgr = build_channel_manager(&channels_config);
    assert_eq!(mgr.channel_count(), 1);
    assert!(mgr.channel_names().contains(&"slack"));

    let results = mgr.connect_all().await;
    assert_eq!(results.len(), 1);
    assert!(results[0].1.is_ok(), "Slack connect failed: {:?}", results[0].1);
    assert_eq!(mgr.connected_count(), 1);

    mgr.disconnect_all().await;
    assert_eq!(mgr.connected_count(), 0);
}

// ── OAuth Config Roundtrip (no credentials needed) ─────────────────────────

#[test]
fn test_oauth_config_factories_all_providers() {
    use rustant_core::oauth;

    // Slack
    let slack = oauth::slack_oauth_config("test-client-id", Some("test-secret".into()));
    assert_eq!(slack.provider_name, "slack");
    assert!(slack.authorization_url.contains("slack.com"));
    assert!(slack.scopes.contains(&"chat:write".to_string()));
    assert_eq!(slack.client_secret.as_deref(), Some("test-secret"));

    // Discord
    let discord = oauth::discord_oauth_config("test-client-id", Some("disc-secret".into()));
    assert_eq!(discord.provider_name, "discord");
    assert!(discord.authorization_url.contains("discord.com"));

    // Teams
    let teams = oauth::teams_oauth_config("test-client-id", "test-tenant", Some("teams-secret".into()));
    assert_eq!(teams.provider_name, "teams");
    assert!(teams.authorization_url.contains("test-tenant"));
    assert!(teams.token_url.contains("test-tenant"));
    assert!(teams.supports_device_code);

    // WhatsApp
    let whatsapp = oauth::whatsapp_oauth_config("test-app-id", None);
    assert_eq!(whatsapp.provider_name, "whatsapp");
    assert!(whatsapp.authorization_url.contains("facebook.com"));
    assert!(whatsapp.client_secret.is_none());

    // Gmail
    let gmail = oauth::gmail_oauth_config("test-client-id", Some("gmail-secret".into()));
    assert_eq!(gmail.provider_name, "gmail");
    assert!(gmail.scopes.contains(&"https://mail.google.com/".to_string()));
    assert!(!gmail.extra_auth_params.is_empty());
}

#[test]
fn test_xoauth2_token_format() {
    use rustant_core::oauth;

    let raw = oauth::build_xoauth2_token("user@gmail.com", "ya29.token");
    assert!(raw.starts_with("user=user@gmail.com\x01"));
    assert!(raw.contains("auth=Bearer ya29.token"));
    assert!(raw.ends_with("\x01\x01"));

    let b64 = oauth::build_xoauth2_token_base64("user@gmail.com", "ya29.token");
    // Decode and verify
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
    assert_eq!(String::from_utf8(decoded).unwrap(), raw);
}

// ── Gateway + StatusProvider ────────────────────────────────────────────────

#[test]
fn test_gateway_list_channels_nodes_integration() {
    use rustant_core::gateway::{ClientMessage, GatewayConfig, GatewayServer, ServerMessage, StatusProvider};

    struct TestStatusProvider;
    impl StatusProvider for TestStatusProvider {
        fn channel_statuses(&self) -> Vec<(String, String)> {
            vec![
                ("slack".into(), "Connected".into()),
                ("discord".into(), "Connected".into()),
                ("telegram".into(), "Disconnected".into()),
            ]
        }
        fn node_statuses(&self) -> Vec<(String, String)> {
            vec![("macos-local".into(), "Healthy".into())]
        }
    }

    let mut server = GatewayServer::new(GatewayConfig::default());
    server.set_status_provider(Box::new(TestStatusProvider));
    let conn_id = server.connections_mut().add_connection().unwrap();

    // List channels
    let resp = server.handle_client_message(ClientMessage::ListChannels, conn_id);
    match resp {
        ServerMessage::ChannelStatus { channels } => {
            assert_eq!(channels.len(), 3);
            let names: Vec<&str> = channels.iter().map(|(n, _)| n.as_str()).collect();
            assert!(names.contains(&"slack"));
            assert!(names.contains(&"discord"));
            assert!(names.contains(&"telegram"));
        }
        other => panic!("Expected ChannelStatus, got {:?}", other),
    }

    // List nodes
    let resp = server.handle_client_message(ClientMessage::ListNodes, conn_id);
    match resp {
        ServerMessage::NodeStatus { nodes } => {
            assert_eq!(nodes.len(), 1);
            assert_eq!(nodes[0].0, "macos-local");
            assert_eq!(nodes[0].1, "Healthy");
        }
        other => panic!("Expected NodeStatus, got {:?}", other),
    }
}

// ── Multi-Agent Orchestrator Integration ────────────────────────────────────

#[tokio::test]
async fn test_orchestrator_full_lifecycle() {
    use rustant_core::multi::{AgentOrchestrator, TaskHandler};
    use rustant_core::multi::spawner::AgentSpawner;
    use rustant_core::multi::messaging::{AgentEnvelope, AgentPayload, MessageBus};
    use rustant_core::multi::routing::AgentRouter;
    use async_trait::async_trait;
    use std::collections::HashMap;

    struct EchoHandler;

    #[async_trait]
    impl TaskHandler for EchoHandler {
        async fn handle_task(
            &self,
            description: &str,
            _args: &HashMap<String, String>,
        ) -> Result<String, String> {
            Ok(format!("Echoed: {}", description))
        }
    }

    let mut spawner = AgentSpawner::default();
    let agent_id = spawner.spawn("echo-agent").unwrap();
    let sender_id = spawner.spawn("sender").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(agent_id);
    bus.register(sender_id);

    let router = AgentRouter::new();
    let mut orch = AgentOrchestrator::new(spawner, bus, router);
    orch.register_handler(agent_id, Box::new(EchoHandler));

    // Send a task to the agent
    let task = AgentEnvelope::new(
        sender_id,
        agent_id,
        AgentPayload::TaskRequest {
            description: "Hello from integration test".to_string(),
            args: HashMap::new(),
        },
    );
    orch.bus_mut().send(task).unwrap();

    // Process
    orch.process_pending().await;

    // Check result in the sender's mailbox
    let result = orch.bus_mut().receive(&sender_id).unwrap();
    match &result.payload {
        AgentPayload::TaskResult { output, success } => {
            assert!(success);
            assert!(output.contains("Echoed: Hello from integration test"));
        }
        other => panic!("Expected TaskResult, got {:?}", other),
    }
}

#[tokio::test]
async fn test_orchestrator_resource_limit_enforcement() {
    use rustant_core::multi::{AgentOrchestrator, TaskHandler};
    use rustant_core::multi::spawner::AgentSpawner;
    use rustant_core::multi::messaging::{AgentEnvelope, AgentPayload, MessageBus};
    use rustant_core::multi::routing::AgentRouter;
    use async_trait::async_trait;
    use std::collections::HashMap;

    struct CountHandler;

    #[async_trait]
    impl TaskHandler for CountHandler {
        async fn handle_task(
            &self,
            _description: &str,
            _args: &HashMap<String, String>,
        ) -> Result<String, String> {
            Ok("done".to_string())
        }
    }

    let mut spawner = AgentSpawner::default();
    let agent_id = spawner.spawn("limited-agent").unwrap();

    // Set resource limits on the agent (max 2 tool calls)
    if let Some(ctx) = spawner.get_mut(&agent_id) {
        ctx.resource_limits.max_tool_calls = Some(2);
    }

    let sender_id = spawner.spawn("sender").unwrap();

    let mut bus = MessageBus::new(100);
    bus.register(agent_id);
    bus.register(sender_id);

    let router = AgentRouter::new();
    let mut orch = AgentOrchestrator::new(spawner, bus, router);
    orch.register_handler(agent_id, Box::new(CountHandler));

    // Send 3 tasks sequentially, processing after each
    for i in 0..3 {
        let task = AgentEnvelope::new(
            sender_id,
            agent_id,
            AgentPayload::TaskRequest {
                description: format!("task {}", i),
                args: HashMap::new(),
            },
        );
        orch.bus_mut().send(task).unwrap();
        orch.process_pending().await;
    }

    // Collect all results
    let mut successes = 0;
    let mut failures = 0;
    while let Some(r) = orch.bus_mut().receive(&sender_id) {
        match &r.payload {
            AgentPayload::TaskResult { success, .. } if *success => successes += 1,
            AgentPayload::TaskResult { .. } => failures += 1,
            AgentPayload::Error { .. } => failures += 1,
            _ => {}
        }
    }
    assert_eq!(successes, 2);
    assert_eq!(failures, 1);
}

// ── Channel Auth Method Configs ─────────────────────────────────────────────

#[test]
fn test_channel_oauth_config_serialization_roundtrip() {
    use rustant_core::oauth::AuthMethod;

    // Slack
    let slack = rustant_core::channels::slack::SlackConfig {
        bot_token: "xoxb-test".into(),
        auth_method: AuthMethod::OAuth,
        ..Default::default()
    };
    let json = serde_json::to_string(&slack).unwrap();
    let restored: rustant_core::channels::slack::SlackConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.auth_method, AuthMethod::OAuth);

    // Discord
    let discord = rustant_core::channels::discord::DiscordConfig {
        bot_token: "token".into(),
        auth_method: AuthMethod::OAuth,
        ..Default::default()
    };
    let json = serde_json::to_string(&discord).unwrap();
    let restored: rustant_core::channels::discord::DiscordConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.auth_method, AuthMethod::OAuth);

    // Teams
    let teams = rustant_core::channels::teams::TeamsConfig {
        client_id: "id".into(),
        client_secret: "secret".into(),
        auth_method: AuthMethod::OAuth,
        ..Default::default()
    };
    let json = serde_json::to_string(&teams).unwrap();
    let restored: rustant_core::channels::teams::TeamsConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.auth_method, AuthMethod::OAuth);

    // WhatsApp
    let whatsapp = rustant_core::channels::whatsapp::WhatsAppConfig {
        phone_number_id: "12345".into(),
        access_token: "token".into(),
        auth_method: AuthMethod::OAuth,
        ..Default::default()
    };
    let json = serde_json::to_string(&whatsapp).unwrap();
    let restored: rustant_core::channels::whatsapp::WhatsAppConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.auth_method, AuthMethod::OAuth);
}

#[test]
fn test_email_auth_method_xoauth2_config() {
    use rustant_core::channels::email::{EmailAuthMethod, EmailConfig};

    let config = EmailConfig {
        imap_host: "imap.gmail.com".into(),
        imap_port: 993,
        smtp_host: "smtp.gmail.com".into(),
        smtp_port: 587,
        username: "user@gmail.com".into(),
        password: String::new(),
        auth_method: EmailAuthMethod::XOAuth2,
        ..Default::default()
    };
    let json = serde_json::to_string(&config).unwrap();
    let restored: EmailConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.auth_method, EmailAuthMethod::XOAuth2);
    assert!(restored.imap_host.contains("gmail"));
}
