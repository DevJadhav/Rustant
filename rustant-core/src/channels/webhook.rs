//! Webhook channel — generic HTTP webhook (inbound + outbound).
//!
//! Inbound messages arrive via an HTTP endpoint; outbound messages are
//! sent via POST to a configured URL. In tests, traits abstract HTTP calls.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Webhook channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub enabled: bool,
    pub listen_path: String,
    pub outbound_url: String,
    pub secret: String,
}

/// Trait for webhook HTTP interactions.
#[async_trait]
pub trait WebhookHttpClient: Send + Sync {
    async fn post_outbound(&self, url: &str, payload: &str) -> Result<String, String>;
    async fn get_inbound(&self) -> Result<Vec<WebhookIncoming>, String>;
}

/// An incoming webhook payload.
#[derive(Debug, Clone)]
pub struct WebhookIncoming {
    pub id: String,
    pub source: String,
    pub body: String,
}

/// Webhook channel.
pub struct WebhookChannel {
    config: WebhookConfig,
    status: ChannelStatus,
    http_client: Box<dyn WebhookHttpClient>,
    name: String,
}

impl WebhookChannel {
    pub fn new(config: WebhookConfig, http_client: Box<dyn WebhookHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "webhook".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for WebhookChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Webhook
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.outbound_url.is_empty() && self.config.listen_path.is_empty() {
            return Err(RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: "No outbound URL or listen path configured".into(),
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
        if self.status != ChannelStatus::Connected {
            return Err(RustantError::Channel(ChannelError::NotConnected {
                name: self.name.clone(),
            }));
        }
        let text = msg.content.as_text().unwrap_or("");
        self.http_client
            .post_outbound(&self.config.outbound_url, text)
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
        let incoming = self.http_client.get_inbound().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;

        let messages = incoming
            .into_iter()
            .map(|m| {
                let sender = ChannelUser::new(&m.source, ChannelType::Webhook);
                ChannelMessage::text(ChannelType::Webhook, &m.source, sender, &m.body)
            })
            .collect();

        Ok(messages)
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
        StreamingMode::ServerSentEvents
    }
}

/// Real Webhook HTTP client using reqwest.
pub struct RealWebhookHttp {
    client: reqwest::Client,
}

impl Default for RealWebhookHttp {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl RealWebhookHttp {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl WebhookHttpClient for RealWebhookHttp {
    async fn post_outbound(&self, url: &str, payload: &str) -> Result<String, String> {
        let resp = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .body(payload.to_string())
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body = resp.text().await.map_err(|e| format!("Read error: {e}"))?;

        if !status.is_success() {
            return Err(format!("Webhook POST failed ({status}): {body}"));
        }

        Ok(body)
    }

    async fn get_inbound(&self) -> Result<Vec<WebhookIncoming>, String> {
        // Inbound webhooks are received via HTTP server, not polled.
        Ok(vec![])
    }
}

/// Create a Webhook channel with a real HTTP client.
pub fn create_webhook_channel(config: WebhookConfig) -> WebhookChannel {
    WebhookChannel::new(config, Box::new(RealWebhookHttp::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWebhookHttp;

    #[async_trait]
    impl WebhookHttpClient for MockWebhookHttp {
        async fn post_outbound(&self, _url: &str, _payload: &str) -> Result<String, String> {
            Ok("wh-msg-1".into())
        }
        async fn get_inbound(&self) -> Result<Vec<WebhookIncoming>, String> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_webhook_channel_creation() {
        let ch = WebhookChannel::new(WebhookConfig::default(), Box::new(MockWebhookHttp));
        assert_eq!(ch.name(), "webhook");
        assert_eq!(ch.channel_type(), ChannelType::Webhook);
    }

    #[test]
    fn test_webhook_capabilities() {
        let ch = WebhookChannel::new(WebhookConfig::default(), Box::new(MockWebhookHttp));
        let caps = ch.capabilities();
        assert!(caps.supports_files);
        assert!(!caps.supports_threads);
        assert!(caps.max_message_length.is_none());
    }

    #[test]
    fn test_webhook_streaming_mode() {
        let ch = WebhookChannel::new(WebhookConfig::default(), Box::new(MockWebhookHttp));
        assert_eq!(ch.streaming_mode(), StreamingMode::ServerSentEvents);
    }

    #[test]
    fn test_webhook_status_disconnected() {
        let ch = WebhookChannel::new(WebhookConfig::default(), Box::new(MockWebhookHttp));
        assert_eq!(ch.status(), ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_webhook_send_without_connect() {
        let ch = WebhookChannel::new(WebhookConfig::default(), Box::new(MockWebhookHttp));
        let sender = ChannelUser::new("ext", ChannelType::Webhook);
        let msg = ChannelMessage::text(ChannelType::Webhook, "ext-sys", sender, "data");
        assert!(ch.send_message(msg).await.is_err());
    }
}
