//! Channel manager — registers, connects, polls, and broadcasts across channels.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use std::collections::HashMap;

/// Manages a set of registered channels.
pub struct ChannelManager {
    channels: HashMap<String, Box<dyn Channel>>,
}

impl ChannelManager {
    pub fn new() -> Self {
        Self {
            channels: HashMap::new(),
        }
    }

    /// Register a channel by name.
    pub fn register(&mut self, channel: Box<dyn Channel>) {
        let name = channel.name().to_string();
        self.channels.insert(name, channel);
    }

    /// Number of registered channels.
    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    /// List all registered channel names.
    pub fn channel_names(&self) -> Vec<&str> {
        self.channels.keys().map(|k| k.as_str()).collect()
    }

    /// Get the status of a channel by name.
    pub fn channel_status(&self, name: &str) -> Option<ChannelStatus> {
        self.channels.get(name).map(|c| c.status())
    }

    /// Connect all registered channels.
    pub async fn connect_all(&mut self) -> Vec<(String, Result<(), RustantError>)> {
        let mut results = Vec::new();
        for (name, channel) in &mut self.channels {
            let result = channel.connect().await;
            results.push((name.clone(), result));
        }
        results
    }

    /// Disconnect all registered channels.
    pub async fn disconnect_all(&mut self) -> Vec<(String, Result<(), RustantError>)> {
        let mut results = Vec::new();
        for (name, channel) in &mut self.channels {
            let result = channel.disconnect().await;
            results.push((name.clone(), result));
        }
        results
    }

    /// Broadcast a message to all connected channels.
    pub async fn broadcast(
        &self,
        msg: ChannelMessage,
    ) -> Vec<(String, Result<MessageId, RustantError>)> {
        let mut results = Vec::new();
        for (name, channel) in &self.channels {
            if channel.is_connected() {
                let result = channel.send_message(msg.clone()).await;
                results.push((name.clone(), result));
            }
        }
        results
    }

    /// Poll all connected channels for new messages.
    pub async fn poll_all(&self) -> Vec<(String, Result<Vec<ChannelMessage>, RustantError>)> {
        let mut results = Vec::new();
        for (name, channel) in &self.channels {
            if channel.is_connected() {
                let result = channel.receive_messages().await;
                results.push((name.clone(), result));
            }
        }
        results
    }

    /// Send a message to a specific channel by name.
    pub async fn send_to(
        &self,
        channel_name: &str,
        msg: ChannelMessage,
    ) -> Result<MessageId, RustantError> {
        let channel = self.channels.get(channel_name).ok_or_else(|| {
            RustantError::Channel(ChannelError::NotConnected {
                name: channel_name.to_string(),
            })
        })?;
        if !channel.is_connected() {
            return Err(RustantError::Channel(ChannelError::NotConnected {
                name: channel_name.to_string(),
            }));
        }
        channel.send_message(msg).await
    }

    /// Get number of connected channels.
    pub fn connected_count(&self) -> usize {
        self.channels.values().filter(|c| c.is_connected()).count()
    }

    /// Get the capabilities of a channel by name.
    pub fn get_capabilities(&self, channel_name: &str) -> Option<ChannelCapabilities> {
        self.channels.get(channel_name).map(|c| c.capabilities())
    }

    /// List names of channels that support threads.
    pub fn channels_supporting_threads(&self) -> Vec<&str> {
        self.channels
            .iter()
            .filter(|(_, c)| c.capabilities().supports_threads)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Map of channel names to their streaming modes.
    pub fn channels_by_streaming_mode(&self) -> HashMap<&str, StreamingMode> {
        self.channels
            .iter()
            .map(|(name, c)| (name.as_str(), c.streaming_mode()))
            .collect()
    }
}

impl Default for ChannelManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a `ChannelManager` from configuration, registering real channel implementations
/// for each enabled/present channel config.
pub fn build_channel_manager(config: &crate::config::ChannelsConfig) -> ChannelManager {
    let mut mgr = ChannelManager::new();

    if let Some(ref cfg) = config.slack {
        mgr.register(Box::new(super::slack::create_slack_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.telegram {
        mgr.register(Box::new(super::telegram::create_telegram_channel(
            cfg.clone(),
        )));
    }

    if let Some(ref cfg) = config.discord {
        mgr.register(Box::new(super::discord::create_discord_channel(
            cfg.clone(),
        )));
    }

    if let Some(ref cfg) = config.webhook {
        mgr.register(Box::new(super::webhook::create_webhook_channel(
            cfg.clone(),
        )));
    }

    if let Some(ref cfg) = config.whatsapp {
        mgr.register(Box::new(super::whatsapp::create_whatsapp_channel(
            cfg.clone(),
        )));
    }

    if let Some(ref cfg) = config.sms {
        mgr.register(Box::new(super::sms::create_sms_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.matrix {
        mgr.register(Box::new(super::matrix::create_matrix_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.teams {
        mgr.register(Box::new(super::teams::create_teams_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.email {
        mgr.register(Box::new(super::email::create_email_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.irc {
        mgr.register(Box::new(super::irc::create_irc_channel(cfg.clone())));
    }

    if let Some(ref cfg) = config.signal {
        mgr.register(Box::new(super::signal::create_signal_channel(cfg.clone())));
    }

    #[cfg(target_os = "macos")]
    if let Some(ref cfg) = config.imessage {
        mgr.register(Box::new(super::imessage::create_imessage_channel(
            cfg.clone(),
        )));
    }

    mgr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::types::{ChannelCapabilities, ChannelType, ChannelUser, StreamingMode};

    /// A mock channel for testing.
    struct MockChannel {
        name: String,
        channel_type: ChannelType,
        status: ChannelStatus,
        sent: std::sync::Arc<std::sync::Mutex<Vec<ChannelMessage>>>,
        inbox: Vec<ChannelMessage>,
        caps: ChannelCapabilities,
        mode: StreamingMode,
    }

    impl MockChannel {
        fn new(name: &str, channel_type: ChannelType) -> Self {
            Self {
                name: name.to_string(),
                channel_type,
                status: ChannelStatus::Disconnected,
                sent: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
                inbox: Vec::new(),
                caps: ChannelCapabilities::default(),
                mode: StreamingMode::default(),
            }
        }

        fn with_inbox(mut self, messages: Vec<ChannelMessage>) -> Self {
            self.inbox = messages;
            self
        }

        fn with_capabilities(mut self, caps: ChannelCapabilities) -> Self {
            self.caps = caps;
            self
        }

        fn with_streaming_mode(mut self, mode: StreamingMode) -> Self {
            self.mode = mode;
            self
        }
    }

    #[async_trait::async_trait]
    impl Channel for MockChannel {
        fn name(&self) -> &str {
            &self.name
        }

        fn channel_type(&self) -> ChannelType {
            self.channel_type
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
            self.sent.lock().unwrap().push(msg);
            Ok(id)
        }

        async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
            Ok(self.inbox.clone())
        }

        fn status(&self) -> ChannelStatus {
            self.status
        }

        fn capabilities(&self) -> ChannelCapabilities {
            self.caps.clone()
        }

        fn streaming_mode(&self) -> StreamingMode {
            self.mode.clone()
        }
    }

    #[test]
    fn test_manager_new() {
        let mgr = ChannelManager::new();
        assert_eq!(mgr.channel_count(), 0);
        assert_eq!(mgr.connected_count(), 0);
    }

    #[test]
    fn test_manager_register() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new(
            "telegram",
            ChannelType::Telegram,
        )));
        mgr.register(Box::new(MockChannel::new("slack", ChannelType::Slack)));
        assert_eq!(mgr.channel_count(), 2);
        assert!(mgr.channel_names().contains(&"telegram"));
    }

    #[tokio::test]
    async fn test_manager_connect_all() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new("tg", ChannelType::Telegram)));
        mgr.register(Box::new(MockChannel::new("sl", ChannelType::Slack)));

        let results = mgr.connect_all().await;
        assert_eq!(results.len(), 2);
        for (_, result) in &results {
            assert!(result.is_ok());
        }
        assert_eq!(mgr.connected_count(), 2);
    }

    #[tokio::test]
    async fn test_manager_disconnect_all() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new("tg", ChannelType::Telegram)));
        mgr.connect_all().await;
        assert_eq!(mgr.connected_count(), 1);

        mgr.disconnect_all().await;
        assert_eq!(mgr.connected_count(), 0);
    }

    #[tokio::test]
    async fn test_manager_broadcast() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new("tg", ChannelType::Telegram)));
        mgr.register(Box::new(MockChannel::new("sl", ChannelType::Slack)));
        mgr.connect_all().await;

        let sender = ChannelUser::new("bot", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "broadcast", sender, "hello all");
        let results = mgr.broadcast(msg).await;
        assert_eq!(results.len(), 2);
        for (_, result) in &results {
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn test_manager_broadcast_skips_disconnected() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new("tg", ChannelType::Telegram)));
        // Don't connect — should be skipped in broadcast

        let sender = ChannelUser::new("bot", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "broadcast", sender, "hello");
        let results = mgr.broadcast(msg).await;
        assert_eq!(results.len(), 0); // skipped because disconnected
    }

    #[tokio::test]
    async fn test_manager_send_to() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(MockChannel::new("tg", ChannelType::Telegram)));
        mgr.connect_all().await;

        let sender = ChannelUser::new("bot", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "chat", sender, "specific");
        let result = mgr.send_to("tg", msg).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_manager_send_to_not_found() {
        let mgr = ChannelManager::new();
        let sender = ChannelUser::new("bot", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "chat", sender, "test");
        let result = mgr.send_to("nonexistent", msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_manager_poll_all() {
        let sender = ChannelUser::new("user1", ChannelType::Telegram);
        let inbox_msg = ChannelMessage::text(ChannelType::Telegram, "chat1", sender, "incoming");

        let mut mock = MockChannel::new("tg", ChannelType::Telegram);
        mock.status = ChannelStatus::Connected;
        let mock = mock.with_inbox(vec![inbox_msg]);

        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(mock));

        let results = mgr.poll_all().await;
        assert_eq!(results.len(), 1);
        let (name, msgs) = &results[0];
        assert_eq!(name, "tg");
        let msgs = msgs.as_ref().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("incoming"));
    }

    #[test]
    fn test_manager_get_capabilities() {
        let mut mgr = ChannelManager::new();
        let caps = ChannelCapabilities {
            supports_threads: true,
            supports_files: true,
            ..Default::default()
        };
        mgr.register(Box::new(
            MockChannel::new("tg", ChannelType::Telegram).with_capabilities(caps.clone()),
        ));
        assert_eq!(mgr.get_capabilities("tg"), Some(caps));
    }

    #[test]
    fn test_manager_channels_supporting_threads() {
        let mut mgr = ChannelManager::new();
        let threaded_caps = ChannelCapabilities {
            supports_threads: true,
            ..Default::default()
        };
        mgr.register(Box::new(
            MockChannel::new("tg", ChannelType::Telegram).with_capabilities(threaded_caps),
        ));
        mgr.register(Box::new(MockChannel::new("wc", ChannelType::WebChat)));

        let threaded = mgr.channels_supporting_threads();
        assert_eq!(threaded.len(), 1);
        assert!(threaded.contains(&"tg"));
    }

    #[test]
    fn test_manager_channels_by_streaming_mode() {
        let mut mgr = ChannelManager::new();
        mgr.register(Box::new(
            MockChannel::new("tg", ChannelType::Telegram)
                .with_streaming_mode(StreamingMode::Polling { interval_ms: 1000 }),
        ));
        mgr.register(Box::new(
            MockChannel::new("dc", ChannelType::Discord)
                .with_streaming_mode(StreamingMode::WebSocket),
        ));

        let modes = mgr.channels_by_streaming_mode();
        assert_eq!(modes.len(), 2);
        assert_eq!(modes["tg"], StreamingMode::Polling { interval_ms: 1000 });
        assert_eq!(modes["dc"], StreamingMode::WebSocket);
    }

    #[test]
    fn test_manager_capability_unknown_channel() {
        let mgr = ChannelManager::new();
        assert!(mgr.get_capabilities("nonexistent").is_none());
    }
}
