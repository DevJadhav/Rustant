//! SMS channel via Twilio API.
//!
//! Uses the Twilio REST API via reqwest for sending and receiving SMS.
//! In tests, a trait abstraction provides mock implementations.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for an SMS channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SmsConfig {
    pub enabled: bool,
    pub account_sid: String,
    pub auth_token: String,
    pub from_number: String,
    pub polling_interval_ms: u64,
}

/// Trait for SMS API interactions.
#[async_trait]
pub trait SmsHttpClient: Send + Sync {
    async fn send_sms(&self, to: &str, body: &str) -> Result<String, String>;
    async fn get_messages(&self) -> Result<Vec<SmsIncoming>, String>;
}

/// An incoming SMS message.
#[derive(Debug, Clone)]
pub struct SmsIncoming {
    pub sid: String,
    pub from: String,
    pub body: String,
}

/// SMS channel.
pub struct SmsChannel {
    config: SmsConfig,
    status: ChannelStatus,
    http_client: Box<dyn SmsHttpClient>,
    name: String,
}

impl SmsChannel {
    pub fn new(config: SmsConfig, http_client: Box<dyn SmsHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "sms".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for SmsChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Sms
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.account_sid.is_empty() || self.config.auth_token.is_empty() {
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
        if self.status != ChannelStatus::Connected {
            return Err(RustantError::Channel(ChannelError::NotConnected {
                name: self.name.clone(),
            }));
        }
        let text = msg.content.as_text().unwrap_or("");
        self.http_client
            .send_sms(&msg.channel_id, text)
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
            .map(|m| {
                let sender = ChannelUser::new(&m.from, ChannelType::Sms);
                ChannelMessage::text(ChannelType::Sms, &m.from, sender, &m.body)
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
            supports_files: false,
            supports_voice: false,
            supports_video: false,
            max_message_length: Some(1600),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling {
            interval_ms: self.config.polling_interval_ms.max(1000),
        }
    }
}

/// Real Twilio SMS HTTP client using reqwest.
pub struct RealSmsHttp {
    client: reqwest::Client,
    account_sid: String,
    auth_token: String,
    from_number: String,
}

impl RealSmsHttp {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            account_sid,
            auth_token,
            from_number,
        }
    }
}

#[async_trait]
impl SmsHttpClient for RealSmsHttp {
    async fn send_sms(&self, to: &str, body: &str) -> Result<String, String> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );
        let auth = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", self.account_sid, self.auth_token),
        );
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Basic {auth}"))
            .form(&[("To", to), ("From", &self.from_number), ("Body", body)])
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let msg = body["message"].as_str().unwrap_or("unknown error");
            return Err(format!("Twilio API error ({status}): {msg}"));
        }

        let sid = body["sid"].as_str().unwrap_or("unknown").to_string();
        Ok(sid)
    }

    async fn get_messages(&self) -> Result<Vec<SmsIncoming>, String> {
        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json?PageSize=20",
            self.account_sid
        );
        let auth = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("{}:{}", self.account_sid, self.auth_token),
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Basic {auth}"))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        let messages = body["messages"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter(|m| m["direction"].as_str() == Some("inbound"))
            .filter_map(|m| {
                Some(SmsIncoming {
                    sid: m["sid"].as_str()?.to_string(),
                    from: m["from"].as_str()?.to_string(),
                    body: m["body"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();

        Ok(messages)
    }
}

/// Create an SMS channel with a real Twilio HTTP client.
pub fn create_sms_channel(config: SmsConfig) -> SmsChannel {
    let http = RealSmsHttp::new(
        config.account_sid.clone(),
        config.auth_token.clone(),
        config.from_number.clone(),
    );
    SmsChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSmsHttp;

    #[async_trait]
    impl SmsHttpClient for MockSmsHttp {
        async fn send_sms(&self, _to: &str, _body: &str) -> Result<String, String> {
            Ok("SM123".into())
        }
        async fn get_messages(&self) -> Result<Vec<SmsIncoming>, String> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_sms_channel_creation() {
        let ch = SmsChannel::new(SmsConfig::default(), Box::new(MockSmsHttp));
        assert_eq!(ch.name(), "sms");
        assert_eq!(ch.channel_type(), ChannelType::Sms);
    }

    #[test]
    fn test_sms_capabilities() {
        let ch = SmsChannel::new(SmsConfig::default(), Box::new(MockSmsHttp));
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(!caps.supports_files);
        assert_eq!(caps.max_message_length, Some(1600));
    }

    #[test]
    fn test_sms_streaming_mode() {
        let ch = SmsChannel::new(SmsConfig::default(), Box::new(MockSmsHttp));
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 1000 }
        );
    }

    #[test]
    fn test_sms_status_disconnected() {
        let ch = SmsChannel::new(SmsConfig::default(), Box::new(MockSmsHttp));
        assert_eq!(ch.status(), ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_sms_send_without_connect() {
        let ch = SmsChannel::new(SmsConfig::default(), Box::new(MockSmsHttp));
        let sender = ChannelUser::new("bot", ChannelType::Sms);
        let msg = ChannelMessage::text(ChannelType::Sms, "+1234", sender, "hi");
        assert!(ch.send_message(msg).await.is_err());
    }
}
