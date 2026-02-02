//! Slack Web API channel implementation.
//!
//! Uses the Slack Web API via reqwest for `chat.postMessage` and
//! `conversations.history`. In tests, uses mock HTTP client.

use super::{Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, MessageId, StreamingMode};
use crate::error::{ChannelError, RustantError};
use crate::oauth::AuthMethod;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Slack channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackConfig {
    /// Bot token (xoxb-...) or OAuth access token.
    pub bot_token: String,
    pub app_token: Option<String>,
    pub default_channel: Option<String>,
    pub allowed_channels: Vec<String>,
    /// Authentication method. When `OAuth`, the `bot_token` field holds
    /// the OAuth 2.0 access token obtained via `slack_oauth_config()`.
    #[serde(default)]
    pub auth_method: AuthMethod,
}

/// Trait for Slack API interactions.
#[async_trait]
pub trait SlackHttpClient: Send + Sync {
    async fn post_message(&self, channel: &str, text: &str) -> Result<String, String>;
    async fn conversations_history(&self, channel: &str, limit: usize) -> Result<Vec<SlackMessage>, String>;
    async fn auth_test(&self) -> Result<String, String>;
}

/// A Slack message from the API.
#[derive(Debug, Clone)]
pub struct SlackMessage {
    pub ts: String,
    pub channel: String,
    pub user: String,
    pub text: String,
}

/// Slack channel.
pub struct SlackChannel {
    config: SlackConfig,
    status: ChannelStatus,
    http_client: Box<dyn SlackHttpClient>,
    name: String,
}

impl SlackChannel {
    pub fn new(config: SlackConfig, http_client: Box<dyn SlackHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "slack".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for SlackChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Slack
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.bot_token.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.http_client.auth_test().await.map_err(|_e| {
            RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            })
        })?;
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    async fn send_message(&self, msg: ChannelMessage) -> Result<MessageId, RustantError> {
        let text = msg.content.as_text().unwrap_or("");
        let channel = if msg.channel_id.is_empty() {
            self.config
                .default_channel
                .as_deref()
                .unwrap_or("general")
        } else {
            &msg.channel_id
        };

        self.http_client
            .post_message(channel, text)
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
        let channels = if self.config.allowed_channels.is_empty() {
            vec![self
                .config
                .default_channel
                .clone()
                .unwrap_or_else(|| "general".to_string())]
        } else {
            self.config.allowed_channels.clone()
        };

        let mut all = Vec::new();
        for ch in &channels {
            let slack_msgs = self
                .http_client
                .conversations_history(ch, 25)
                .await
                .map_err(|e| {
                    RustantError::Channel(ChannelError::ConnectionFailed {
                        name: self.name.clone(),
                        message: e,
                    })
                })?;

            for sm in slack_msgs {
                let sender = ChannelUser::new(&sm.user, ChannelType::Slack);
                let msg = ChannelMessage::text(ChannelType::Slack, &sm.channel, sender, &sm.text);
                all.push(msg);
            }
        }
        Ok(all)
    }

    fn status(&self) -> ChannelStatus {
        self.status
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities {
            supports_threads: true,
            supports_reactions: true,
            supports_files: true,
            supports_voice: false,
            supports_video: false,
            max_message_length: Some(40000),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::WebSocket
    }
}

/// Real Slack HTTP client using the Slack Web API via reqwest.
pub struct RealSlackHttp {
    client: reqwest::Client,
    bot_token: String,
}

impl RealSlackHttp {
    pub fn new(bot_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            bot_token,
        }
    }

    fn auth_header(&self) -> String {
        format!("Bearer {}", self.bot_token)
    }
}

#[async_trait]
impl SlackHttpClient for RealSlackHttp {
    async fn post_message(&self, channel: &str, text: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "channel": channel,
            "text": text,
        });

        let resp = self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .header("Authorization", self.auth_header())
            .header("Content-Type", "application/json; charset=utf-8")
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Slack chat.postMessage request failed");
                format!("HTTP request failed: {}", e)
            })?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            tracing::warn!(status = %status, body = %body_text, "Slack API HTTP error");
            return Err(format!("HTTP {}: {}", status, body_text));
        }

        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Invalid JSON response: {}", e))?;

        if json.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            tracing::warn!(error = %error, "Slack API returned error");
            return Err(format!("Slack API error: {}", error));
        }

        json.get("ts")
            .and_then(|ts| ts.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'ts' in Slack response".to_string())
    }

    async fn conversations_history(
        &self,
        channel: &str,
        limit: usize,
    ) -> Result<Vec<SlackMessage>, String> {
        let url = format!(
            "https://slack.com/api/conversations.history?channel={}&limit={}",
            channel, limit
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Slack conversations.history request failed");
                format!("HTTP request failed: {}", e)
            })?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            tracing::warn!(status = %status, body = %body_text, "Slack API HTTP error");
            return Err(format!("HTTP {}: {}", status, body_text));
        }

        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Invalid JSON response: {}", e))?;

        if json.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            return Err(format!("Slack API error: {}", error));
        }

        let messages = json
            .get("messages")
            .and_then(|m| m.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|msg| {
                        let ts = msg.get("ts")?.as_str()?.to_string();
                        let user = msg
                            .get("user")
                            .and_then(|u| u.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let text = msg
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("")
                            .to_string();
                        Some(SlackMessage {
                            ts,
                            channel: channel.to_string(),
                            user,
                            text,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(messages)
    }

    async fn auth_test(&self) -> Result<String, String> {
        let resp = self
            .client
            .post("https://slack.com/api/auth.test")
            .header("Authorization", self.auth_header())
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "Slack auth.test request failed");
                format!("HTTP request failed: {}", e)
            })?;

        let status = resp.status();
        let body_text = resp
            .text()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status, body_text));
        }

        let json: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| format!("Invalid JSON response: {}", e))?;

        if json.get("ok") != Some(&serde_json::Value::Bool(true)) {
            let error = json
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown_error");
            return Err(format!("Slack auth error: {}", error));
        }

        json.get("user_id")
            .and_then(|u| u.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| "Missing 'user_id' in auth.test response".to_string())
    }
}

/// Create a SlackChannel with a real HTTP client.
pub fn create_slack_channel(config: SlackConfig) -> SlackChannel {
    let http = RealSlackHttp::new(config.bot_token.clone());
    SlackChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::ChannelUser;
    use std::sync::{Arc, Mutex};

    struct MockSlackHttp {
        sent: Arc<Mutex<Vec<(String, String)>>>,
        messages: Vec<SlackMessage>,
        auth_ok: bool,
    }

    impl MockSlackHttp {
        fn new() -> Self {
            Self {
                sent: Arc::new(Mutex::new(Vec::new())),
                messages: Vec::new(),
                auth_ok: true,
            }
        }

        fn with_messages(mut self, messages: Vec<SlackMessage>) -> Self {
            self.messages = messages;
            self
        }
    }

    #[async_trait]
    impl SlackHttpClient for MockSlackHttp {
        async fn post_message(&self, channel: &str, text: &str) -> Result<String, String> {
            self.sent
                .lock()
                .unwrap()
                .push((channel.to_string(), text.to_string()));
            Ok("1234567890.123456".to_string())
        }

        async fn conversations_history(
            &self,
            _channel: &str,
            _limit: usize,
        ) -> Result<Vec<SlackMessage>, String> {
            Ok(self.messages.clone())
        }

        async fn auth_test(&self) -> Result<String, String> {
            if self.auth_ok {
                Ok("bot-user-id".to_string())
            } else {
                Err("invalid_auth".to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_slack_connect() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            ..Default::default()
        };
        let mut ch = SlackChannel::new(config, Box::new(MockSlackHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_slack_send_message() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            default_channel: Some("general".into()),
            ..Default::default()
        };
        let http = MockSlackHttp::new();
        let sent = http.sent.clone();
        let mut ch = SlackChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot", ChannelType::Slack);
        let msg = ChannelMessage::text(ChannelType::Slack, "random", sender, "Hello Slack!");
        ch.send_message(msg).await.unwrap();

        let sent = sent.lock().unwrap();
        assert_eq!(sent[0].0, "random");
        assert_eq!(sent[0].1, "Hello Slack!");
    }

    #[tokio::test]
    async fn test_slack_receive_messages() {
        let config = SlackConfig {
            bot_token: "xoxb-123".into(),
            allowed_channels: vec!["general".into()],
            ..Default::default()
        };
        let http = MockSlackHttp::new().with_messages(vec![SlackMessage {
            ts: "123.456".into(),
            channel: "general".into(),
            user: "U123".into(),
            text: "hey".into(),
        }]);
        let mut ch = SlackChannel::new(config, Box::new(http));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hey"));
    }

    #[test]
    fn test_slack_capabilities() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(!caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(40000));
    }

    #[test]
    fn test_slack_streaming_mode() {
        let ch = SlackChannel::new(SlackConfig::default(), Box::new(MockSlackHttp::new()));
        assert_eq!(ch.streaming_mode(), StreamingMode::WebSocket);
    }

    #[tokio::test]
    async fn test_slack_oauth_config_connect() {
        // SlackConfig with OAuth auth method — token is an OAuth access token
        let config = SlackConfig {
            bot_token: "xoxb-oauth-token-from-oauth-flow".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let mut ch = SlackChannel::new(config, Box::new(MockSlackHttp::new()));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_slack_config_auth_method_default() {
        let config = SlackConfig::default();
        assert_eq!(config.auth_method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_slack_config_auth_method_serde() {
        let config = SlackConfig {
            bot_token: "xoxb-test".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"oauth\""));
        let parsed: SlackConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, AuthMethod::OAuth);
    }
}
