//! Discord channel implementation.
//!
//! Uses the Discord Gateway (WebSocket) for real-time events and
//! the REST API for sending messages. In tests, a trait abstraction
//! provides mock implementations.

use super::{Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, MessageId, StreamingMode};
use crate::error::{ChannelError, RustantError};
use crate::oauth::AuthMethod;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Discord channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscordConfig {
    /// Bot token or OAuth access token.
    pub bot_token: String,
    pub guild_id: Option<String>,
    pub allowed_channel_ids: Vec<String>,
    /// Authentication method. When `OAuth`, the `bot_token` field holds
    /// the OAuth 2.0 access token obtained via `discord_oauth_config()`.
    #[serde(default)]
    pub auth_method: AuthMethod,
}

/// Trait for Discord API interactions.
#[async_trait]
pub trait DiscordHttpClient: Send + Sync {
    async fn send_message(&self, channel_id: &str, text: &str) -> Result<String, String>;
    async fn get_messages(&self, channel_id: &str, limit: usize) -> Result<Vec<DiscordMessage>, String>;
    async fn connect_gateway(&self) -> Result<(), String>;
    async fn disconnect_gateway(&self) -> Result<(), String>;
}

/// A Discord message from the API.
#[derive(Debug, Clone)]
pub struct DiscordMessage {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub author_name: String,
    pub content: String,
}

/// Discord channel.
pub struct DiscordChannel {
    config: DiscordConfig,
    status: ChannelStatus,
    http_client: Box<dyn DiscordHttpClient>,
    name: String,
}

impl DiscordChannel {
    pub fn new(config: DiscordConfig, http_client: Box<dyn DiscordHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "discord".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for DiscordChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Discord
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.bot_token.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.http_client
            .connect_gateway()
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })?;
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        let _ = self.http_client.disconnect_gateway().await;
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError> {
        let text = msg.content.as_text().unwrap_or("");
        self.http_client
            .send_message(&msg.channel_id, text)
            .await
            .map(MessageId::new)
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }

    async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
        let mut all_messages = Vec::new();
        let channels = if self.config.allowed_channel_ids.is_empty() {
            vec!["general".to_string()]
        } else {
            self.config.allowed_channel_ids.clone()
        };

        for channel_id in &channels {
            let discord_msgs = self
                .http_client
                .get_messages(channel_id, 25)
                .await
                .map_err(|e| {
                    RustantError::Channel(ChannelError::ConnectionFailed {
                        name: self.name.clone(),
                        message: e,
                    })
                })?;

            for dm in discord_msgs {
                let sender = ChannelUser::new(&dm.author_id, ChannelType::Discord)
                    .with_name(&dm.author_name);
                let msg = ChannelMessage::text(
                    ChannelType::Discord,
                    &dm.channel_id,
                    sender,
                    &dm.content,
                );
                all_messages.push(msg);
            }
        }

        Ok(all_messages)
    }

    fn status(&self) -> ChannelStatus {
        self.status
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_voice: true,
            supports_video: true,
            max_message_length: Some(2000),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::WebSocket
    }
}

/// Real Discord HTTP client using reqwest.
pub struct RealDiscordHttp {
    client: reqwest::Client,
    bot_token: String,
}

impl RealDiscordHttp {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    fn auth_header(&self) -> String {
        // OAuth tokens use "Bearer" prefix; bot tokens use "Bot" prefix.
        // Since the RealDiscordHttp doesn't know the auth method, callers
        // should set the bot_token field to the appropriate token.
        // For backwards compat, we use "Bot" for raw tokens.
        format!("Bot {}", self.bot_token)
    }
}

#[async_trait]
impl DiscordHttpClient for RealDiscordHttp {
    async fn send_message(&self, channel_id: &str, text: &str) -> Result<String, String> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );
        let resp = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .json(&serde_json::json!({ "content": text }))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(format!("Discord API error ({}): {}", status, msg));
        }

        let id = body["id"].as_str().unwrap_or("0").to_string();
        Ok(id)
    }

    async fn get_messages(&self, channel_id: &str, limit: usize) -> Result<Vec<DiscordMessage>, String> {
        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages?limit={}",
            channel_id, limit
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(format!("Discord API error ({}): {}", status, msg));
        }

        let messages = body
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|m| {
                Some(DiscordMessage {
                    id: m["id"].as_str()?.to_string(),
                    channel_id: m["channel_id"].as_str().unwrap_or("").to_string(),
                    author_id: m["author"]["id"].as_str().unwrap_or("").to_string(),
                    author_name: m["author"]["username"].as_str().unwrap_or("").to_string(),
                    content: m["content"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();

        Ok(messages)
    }

    async fn connect_gateway(&self) -> Result<(), String> {
        // Gateway (WebSocket) connection is not implemented yet;
        // REST-only mode works for sending/receiving via polling.
        Ok(())
    }

    async fn disconnect_gateway(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Create a Discord channel with a real HTTP client.
pub fn create_discord_channel(config: DiscordConfig) -> DiscordChannel {
    let http = RealDiscordHttp::new(config.bot_token.clone());
    DiscordChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;
    use std::sync::{Arc, Mutex};

    struct MockDiscordHttp {
        sent: Arc<Mutex<Vec<(String, String)>>>,
        messages: Vec<DiscordMessage>,
    }

    impl MockDiscordHttp {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                messages: Vec::new(),
            }
        }

        fn with_messages(mut self, messages: Vec<DiscordMessage>) -> Self {
            self.messages = messages;
            self
        }
    }

    #[async_trait]
    impl DiscordHttpClient for MockDiscordHttp {
        async fn send_message(&self, channel_id: &str, text: &str) -> Result<String, String> {
            self.sent
                .lock()
                .unwrap()
                .push((channel_id.to_string(), text.to_string()));
            Ok("disc-msg-1".to_string())
        }

        async fn get_messages(&self, _channel_id: &str, _limit: usize) -> Result<Vec<DiscordMessage>, String> {
            Ok(self.messages.clone())
        }

        async fn connect_gateway(&self) -> Result<(), String> {
            Ok(())
        }

        async fn disconnect_gateway(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_discord_connect_no_token() {
        let mut ch = DiscordChannel::new(DiscordConfig::default(), Box::new(MockDiscordHttp::new()));
        assert!(ch.connect().await.is_err());
    }

    #[tokio::test]
    async fn test_discord_connect_with_token() {
        let config = DiscordConfig {
            bot_token: "Bot token123".into(),
            ..Default::default()
        };
        let mut ch = DiscordChannel::new(config, Box::new(MockDiscordHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_discord_send_message() {
        let config = DiscordConfig {
            bot_token: "token".into(),
            ..Default::default()
        };
        let http = MockDiscordHttp::new();
        let sent = http.sent.clone();
        let mut ch = DiscordChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot", ChannelType::Discord);
        let msg = ChannelMessage::text(ChannelType::Discord, "channel-1", sender, "Hello Discord!");
        let id = ch.send_message(msg).await.unwrap();
        assert_eq!(id.0, "disc-msg-1");

        let sent = sent.lock().unwrap();
        assert_eq!(sent[0].0, "channel-1");
        assert_eq!(sent[0].1, "Hello Discord!");
    }

    #[tokio::test]
    async fn test_discord_receive_messages() {
        let config = DiscordConfig {
            bot_token: "token".into(),
            allowed_channel_ids: vec!["ch1".into()],
            ..Default::default()
        };
        let http = MockDiscordHttp::new().with_messages(vec![DiscordMessage {
            id: "1".into(),
            channel_id: "ch1".into(),
            author_id: "user1".into(),
            author_name: "Bob".into(),
            content: "hey there".into(),
        }]);
        let mut ch = DiscordChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hey there"));
    }

    #[test]
    fn test_discord_capabilities() {
        let ch = DiscordChannel::new(DiscordConfig::default(), Box::new(MockDiscordHttp::new()));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(caps.supports_voice);
        assert!(caps.supports_video);
        assert_eq!(caps.max_message_length, Some(2000));
    }

    #[test]
    fn test_discord_streaming_mode() {
        let ch = DiscordChannel::new(DiscordConfig::default(), Box::new(MockDiscordHttp::new()));
        assert_eq!(ch.streaming_mode(), StreamingMode::WebSocket);
    }

    #[tokio::test]
    async fn test_discord_oauth_config_connect() {
        let config = DiscordConfig {
            bot_token: "oauth-access-token".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let mut ch = DiscordChannel::new(config, Box::new(MockDiscordHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_discord_config_auth_method_default() {
        let config = DiscordConfig::default();
        assert_eq!(config.auth_method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_discord_config_auth_method_serde() {
        let config = DiscordConfig {
            bot_token: "token".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"oauth\""));
        let parsed: DiscordConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, AuthMethod::OAuth);
    }
}
