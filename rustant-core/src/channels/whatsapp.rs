//! WhatsApp Business Cloud API channel implementation.
//!
//! Uses the WhatsApp Business Cloud API via reqwest.
//! In tests, a trait abstraction provides mock implementations.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use crate::oauth::AuthMethod;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a WhatsApp channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    pub phone_number_id: String,
    /// Access token — either a permanent token or an OAuth access token.
    pub access_token: String,
    pub verify_token: String,
    pub allowed_numbers: Vec<String>,
    /// Authentication method. When `OAuth`, the `access_token` field holds
    /// the OAuth 2.0 token obtained via `whatsapp_oauth_config()`.
    #[serde(default)]
    pub auth_method: AuthMethod,
}

/// Trait for WhatsApp API interactions.
#[async_trait]
pub trait WhatsAppHttpClient: Send + Sync {
    async fn send_text(&self, to: &str, text: &str) -> Result<String, String>;
    async fn get_messages(&self) -> Result<Vec<WhatsAppIncoming>, String>;
}

/// An incoming WhatsApp message.
#[derive(Debug, Clone)]
pub struct WhatsAppIncoming {
    pub message_id: String,
    pub from: String,
    pub from_name: Option<String>,
    pub text: String,
}

/// WhatsApp channel.
pub struct WhatsAppChannel {
    config: WhatsAppConfig,
    status: ChannelStatus,
    http_client: Box<dyn WhatsAppHttpClient>,
    name: String,
}

impl WhatsAppChannel {
    pub fn new(config: WhatsAppConfig, http_client: Box<dyn WhatsAppHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "whatsapp".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::WhatsApp
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.access_token.is_empty() || self.config.phone_number_id.is_empty() {
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
        self.http_client
            .send_text(&msg.channel_id, text)
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
        let incoming = self.http_client.get_messages().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;

        let messages = incoming
            .into_iter()
            .filter(|m| {
                self.config.allowed_numbers.is_empty()
                    || self.config.allowed_numbers.contains(&m.from)
            })
            .map(|m| {
                let mut user = ChannelUser::new(&m.from, ChannelType::WhatsApp);
                if let Some(name) = m.from_name {
                    user = user.with_name(name);
                }
                ChannelMessage::text(ChannelType::WhatsApp, &m.from, user, &m.text)
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
            supports_voice: true,
            supports_video: false,
            max_message_length: Some(65536),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling { interval_ms: 5000 }
    }
}

/// Real WhatsApp Business Cloud API HTTP client using reqwest.
pub struct RealWhatsAppHttp {
    client: reqwest::Client,
    phone_number_id: String,
    access_token: String,
}

impl RealWhatsAppHttp {
    pub fn new(phone_number_id: String, access_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            phone_number_id,
            access_token,
        }
    }
}

#[async_trait]
impl WhatsAppHttpClient for RealWhatsAppHttp {
    async fn send_text(&self, to: &str, text: &str) -> Result<String, String> {
        let url = format!(
            "https://graph.facebook.com/v18.0/{}/messages",
            self.phone_number_id
        );
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&serde_json::json!({
                "messaging_product": "whatsapp",
                "to": to,
                "type": "text",
                "text": { "body": text }
            }))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let err = body["error"]["message"].as_str().unwrap_or("unknown error");
            return Err(format!("WhatsApp API error ({}): {}", status, err));
        }

        let wamid = body["messages"][0]["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        Ok(wamid)
    }

    async fn get_messages(&self) -> Result<Vec<WhatsAppIncoming>, String> {
        // WhatsApp Cloud API uses webhooks for inbound messages, not polling.
        Ok(vec![])
    }
}

/// Create a WhatsApp channel with a real HTTP client.
pub fn create_whatsapp_channel(config: WhatsAppConfig) -> WhatsAppChannel {
    let http = RealWhatsAppHttp::new(config.phone_number_id.clone(), config.access_token.clone());
    WhatsAppChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockWhatsAppHttp;

    #[async_trait]
    impl WhatsAppHttpClient for MockWhatsAppHttp {
        async fn send_text(&self, _to: &str, _text: &str) -> Result<String, String> {
            Ok("wamid.123".to_string())
        }
        async fn get_messages(&self) -> Result<Vec<WhatsAppIncoming>, String> {
            Ok(vec![WhatsAppIncoming {
                message_id: "wamid.1".into(),
                from: "+1234567890".into(),
                from_name: Some("Bob".into()),
                text: "hello whatsapp".into(),
            }])
        }
    }

    #[tokio::test]
    async fn test_whatsapp_connect() {
        let config = WhatsAppConfig {
            phone_number_id: "12345".into(),
            access_token: "token".into(),
            ..Default::default()
        };
        let mut ch = WhatsAppChannel::new(config, Box::new(MockWhatsAppHttp));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_whatsapp_send() {
        let config = WhatsAppConfig {
            phone_number_id: "12345".into(),
            access_token: "token".into(),
            ..Default::default()
        };
        let mut ch = WhatsAppChannel::new(config, Box::new(MockWhatsAppHttp));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot", ChannelType::WhatsApp);
        let msg = ChannelMessage::text(ChannelType::WhatsApp, "+9876543210", sender, "hi wa");
        let id = ch.send_message(msg).await.unwrap();
        assert_eq!(id.0, "wamid.123");
    }

    #[tokio::test]
    async fn test_whatsapp_receive() {
        let config = WhatsAppConfig {
            phone_number_id: "12345".into(),
            access_token: "token".into(),
            ..Default::default()
        };
        let mut ch = WhatsAppChannel::new(config, Box::new(MockWhatsAppHttp));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hello whatsapp"));
    }

    #[test]
    fn test_whatsapp_capabilities() {
        let ch = WhatsAppChannel::new(WhatsAppConfig::default(), Box::new(MockWhatsAppHttp));
        let caps = ch.capabilities();
        assert!(caps.supports_files);
        assert!(caps.supports_voice);
        assert_eq!(caps.max_message_length, Some(65536));
    }

    #[test]
    fn test_whatsapp_streaming_mode() {
        let ch = WhatsAppChannel::new(WhatsAppConfig::default(), Box::new(MockWhatsAppHttp));
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 5000 }
        );
    }

    #[tokio::test]
    async fn test_whatsapp_oauth_config_connect() {
        let config = WhatsAppConfig {
            phone_number_id: "12345".into(),
            access_token: "oauth-token-from-meta".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let mut ch = WhatsAppChannel::new(config, Box::new(MockWhatsAppHttp));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_whatsapp_config_auth_method_default() {
        let config = WhatsAppConfig::default();
        assert_eq!(config.auth_method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_whatsapp_config_auth_method_serde() {
        let config = WhatsAppConfig {
            phone_number_id: "12345".into(),
            access_token: "token".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"oauth\""));
        let parsed: WhatsAppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, AuthMethod::OAuth);
    }
}
