//! Telegram Bot API channel implementation.
//!
//! Uses the Telegram Bot API via reqwest for `getUpdates` / `sendMessage`.
//! In tests, an `HttpClient` trait abstraction allows mocking.

use super::{Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, MessageId, StreamingMode};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Telegram channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub allowed_chat_ids: Vec<i64>,
    pub polling_timeout_secs: u64,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            bot_token: String::new(),
            allowed_chat_ids: Vec::new(),
            polling_timeout_secs: 30,
        }
    }
}

/// Trait for HTTP interactions, allowing test mocking.
#[async_trait]
pub trait TelegramHttpClient: Send + Sync {
    async fn send_message(&self, chat_id: i64, text: &str) -> Result<String, String>;
    async fn get_updates(&self, offset: i64) -> Result<Vec<TelegramUpdate>, String>;
}

/// A Telegram update from the Bot API.
#[derive(Debug, Clone)]
pub struct TelegramUpdate {
    pub update_id: i64,
    pub chat_id: i64,
    pub from_id: i64,
    pub from_name: String,
    pub text: String,
}

/// Telegram channel using the Bot API.
pub struct TelegramChannel {
    config: TelegramConfig,
    status: ChannelStatus,
    http_client: Box<dyn TelegramHttpClient>,
    last_update_id: i64,
    name: String,
}

impl TelegramChannel {
    pub fn new(config: TelegramConfig, http_client: Box<dyn TelegramHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            last_update_id: 0,
            name: "telegram".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for TelegramChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Telegram
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.bot_token.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError> {
        let text = msg.content.as_text().unwrap_or("");
        let chat_id: i64 = msg.channel_id.parse().unwrap_or(0);

        self.http_client
            .send_message(chat_id, text)
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
        let updates = self
            .http_client
            .get_updates(self.last_update_id + 1)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })?;

        let messages: Vec<ChannelMessage> = updates
            .into_iter()
            .filter(|u| {
                self.config.allowed_chat_ids.is_empty()
                    || self.config.allowed_chat_ids.contains(&u.chat_id)
            })
            .map(|u| {
                let sender = ChannelUser::new(u.from_id.to_string(), ChannelType::Telegram)
                    .with_name(u.from_name);
                ChannelMessage::text(ChannelType::Telegram, u.chat_id.to_string(), sender, u.text)
            })
            .collect();

        Ok(messages)
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
            supports_video: false,
            max_message_length: Some(4096),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling {
            interval_ms: self.config.polling_timeout_secs * 1000,
        }
    }
}

/// Real Telegram Bot API HTTP client using reqwest.
pub struct RealTelegramHttp {
    client: reqwest::Client,
    base_url: String,
}

impl RealTelegramHttp {
    pub fn new(bot_token: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: format!("https://api.telegram.org/bot{}", bot_token),
        }
    }
}

#[async_trait]
impl TelegramHttpClient for RealTelegramHttp {
    async fn send_message(&self, chat_id: i64, text: &str) -> Result<String, String> {
        let url = format!("{}/sendMessage", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(&serde_json::json!({
                "chat_id": chat_id,
                "text": text,
            }))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;

        if !body["ok"].as_bool().unwrap_or(false) {
            let desc = body["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram API error ({}): {}", status, desc));
        }

        let message_id = body["result"]["message_id"]
            .as_i64()
            .unwrap_or(0)
            .to_string();
        Ok(message_id)
    }

    async fn get_updates(&self, offset: i64) -> Result<Vec<TelegramUpdate>, String> {
        let url = format!(
            "{}/getUpdates?offset={}&timeout=30",
            self.base_url, offset
        );
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: serde_json::Value = resp.json().await.map_err(|e| format!("JSON parse error: {e}"))?;

        if !body["ok"].as_bool().unwrap_or(false) {
            let desc = body["description"].as_str().unwrap_or("unknown error");
            return Err(format!("Telegram API error: {}", desc));
        }

        let updates = body["result"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|u| {
                let msg = &u["message"];
                Some(TelegramUpdate {
                    update_id: u["update_id"].as_i64()?,
                    chat_id: msg["chat"]["id"].as_i64()?,
                    from_id: msg["from"]["id"].as_i64().unwrap_or(0),
                    from_name: msg["from"]["first_name"]
                        .as_str()
                        .unwrap_or("Unknown")
                        .to_string(),
                    text: msg["text"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();

        Ok(updates)
    }
}

/// Create a Telegram channel with a real HTTP client.
pub fn create_telegram_channel(config: TelegramConfig) -> TelegramChannel {
    let http = RealTelegramHttp::new(&config.bot_token);
    TelegramChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;
    use std::sync::{Arc, Mutex};

    struct MockTelegramHttp {
        sent: Arc<Mutex<Vec<(i64, String)>>>,
        updates: Vec<TelegramUpdate>,
    }

    impl MockTelegramHttp {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                updates: Vec::new(),
            }
        }

        fn with_updates(mut self, updates: Vec<TelegramUpdate>) -> Self {
            self.updates = updates;
            self
        }
    }

    #[async_trait]
    impl TelegramHttpClient for MockTelegramHttp {
        async fn send_message(&self, chat_id: i64, text: &str) -> Result<String, String> {
            self.sent.lock().unwrap().push((chat_id, text.to_string()));
            Ok("msg-123".to_string())
        }

        async fn get_updates(&self, _offset: i64) -> Result<Vec<TelegramUpdate>, String> {
            Ok(self.updates.clone())
        }
    }

    #[tokio::test]
    async fn test_telegram_connect_no_token() {
        let mut ch = TelegramChannel::new(TelegramConfig::default(), Box::new(MockTelegramHttp::new()));
        let result = ch.connect().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_telegram_connect_with_token() {
        let config = TelegramConfig {
            bot_token: "123:ABC".into(),
            ..Default::default()
        };
        let mut ch = TelegramChannel::new(config, Box::new(MockTelegramHttp::new()));
        ch.connect().await.unwrap();
        assert_eq!(ch.status(), ChannelStatus::Connected);
    }

    #[tokio::test]
    async fn test_telegram_send_message() {
        let config = TelegramConfig {
            bot_token: "123:ABC".into(),
            ..Default::default()
        };
        let http = MockTelegramHttp::new();
        let sent = http.sent.clone();
        let mut ch = TelegramChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot", ChannelType::Telegram);
        let msg = ChannelMessage::text(ChannelType::Telegram, "12345", sender, "Hello Telegram!");
        let id = ch.send_message(msg).await.unwrap();
        assert_eq!(id.0, "msg-123");

        let sent = sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].0, 12345);
        assert_eq!(sent[0].1, "Hello Telegram!");
    }

    #[tokio::test]
    async fn test_telegram_receive_messages() {
        let config = TelegramConfig {
            bot_token: "123:ABC".into(),
            allowed_chat_ids: vec![100],
            ..Default::default()
        };
        let http = MockTelegramHttp::new().with_updates(vec![
            TelegramUpdate {
                update_id: 1,
                chat_id: 100,
                from_id: 42,
                from_name: "Alice".into(),
                text: "hello".into(),
            },
            TelegramUpdate {
                update_id: 2,
                chat_id: 999, // not allowed
                from_id: 99,
                from_name: "Eve".into(),
                text: "spam".into(),
            },
        ]);
        let mut ch = TelegramChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hello"));
    }

    #[test]
    fn test_telegram_capabilities() {
        let ch = TelegramChannel::new(TelegramConfig::default(), Box::new(MockTelegramHttp::new()));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(caps.supports_voice);
        assert!(!caps.supports_video);
        assert_eq!(caps.max_message_length, Some(4096));
    }

    #[test]
    fn test_telegram_streaming_mode() {
        let ch = TelegramChannel::new(TelegramConfig::default(), Box::new(MockTelegramHttp::new()));
        assert_eq!(ch.streaming_mode(), StreamingMode::Polling { interval_ms: 30000 });
    }
}
