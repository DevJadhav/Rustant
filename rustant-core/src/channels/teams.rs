//! Microsoft Teams channel via Graph API.
//!
//! Uses the Microsoft Graph API via reqwest for sending and receiving messages.
//! In tests, a trait abstraction provides mock implementations.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use crate::oauth::AuthMethod;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Microsoft Teams channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TeamsConfig {
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
    pub tenant_id: String,
    pub polling_interval_ms: u64,
    /// Authentication method. When `OAuth`, the channel uses the OAuth 2.0
    /// client credentials flow via `teams_oauth_config()` and
    /// `authorize_client_credentials_flow()` from the oauth module.
    #[serde(default)]
    pub auth_method: AuthMethod,
}

/// Trait for Teams API interactions.
#[async_trait]
pub trait TeamsHttpClient: Send + Sync {
    async fn send_message(&self, channel_id: &str, text: &str) -> Result<String, String>;
    async fn get_messages(&self, channel_id: &str) -> Result<Vec<TeamsMessage>, String>;
    async fn authenticate(&self) -> Result<String, String>;
}

/// A Teams message from the API.
#[derive(Debug, Clone)]
pub struct TeamsMessage {
    pub id: String,
    pub channel_id: String,
    pub from_id: String,
    pub from_name: String,
    pub content: String,
}

/// Microsoft Teams channel.
pub struct TeamsChannel {
    config: TeamsConfig,
    status: ChannelStatus,
    http_client: Box<dyn TeamsHttpClient>,
    name: String,
}

impl TeamsChannel {
    pub fn new(config: TeamsConfig, http_client: Box<dyn TeamsHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "teams".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for TeamsChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Teams
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.client_id.is_empty() || self.config.client_secret.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.http_client.authenticate().await.map_err(|e| {
            RustantError::Channel(ChannelError::AuthFailed {
                name: format!("{}: {}", self.name, e),
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
        if self.status != ChannelStatus::Connected {
            return Err(RustantError::Channel(ChannelError::NotConnected {
                name: self.name.clone(),
            }));
        }
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
        let msgs = self
            .http_client
            .get_messages("default")
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })?;

        let messages = msgs
            .into_iter()
            .map(|m| {
                let sender =
                    ChannelUser::new(&m.from_id, ChannelType::Teams).with_name(&m.from_name);
                ChannelMessage::text(ChannelType::Teams, &m.channel_id, sender, &m.content)
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
            supports_voice: false,
            supports_video: false,
            max_message_length: Some(28000),
            supports_editing: true,
            supports_deletion: true,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling {
            interval_ms: self.config.polling_interval_ms.max(1000),
        }
    }
}

/// Real Microsoft Teams HTTP client using reqwest + OAuth2.
pub struct RealTeamsHttp {
    client: reqwest::Client,
    client_id: String,
    client_secret: String,
    tenant_id: String,
    access_token: std::sync::Mutex<Option<String>>,
}

impl RealTeamsHttp {
    pub fn new(client_id: String, client_secret: String, tenant_id: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            client_id,
            client_secret,
            tenant_id,
            access_token: std::sync::Mutex::new(None),
        }
    }

    fn get_token(&self) -> Result<String, String> {
        self.access_token
            .lock()
            .map_err(|e| format!("Lock error: {e}"))?
            .clone()
            .ok_or_else(|| "Not authenticated".to_string())
    }
}

#[async_trait]
impl TeamsHttpClient for RealTeamsHttp {
    async fn send_message(&self, channel_id: &str, text: &str) -> Result<String, String> {
        let token = self.get_token()?;
        let url = format!(
            "https://graph.microsoft.com/v1.0/teams/{}/channels/{}/messages",
            self.tenant_id, channel_id
        );
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&serde_json::json!({
                "body": {
                    "content": text,
                    "contentType": "text"
                }
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
            return Err(format!("Teams API error ({}): {}", status, err));
        }

        let id = body["id"].as_str().unwrap_or("").to_string();
        Ok(id)
    }

    async fn get_messages(&self, channel_id: &str) -> Result<Vec<TeamsMessage>, String> {
        let token = self.get_token()?;
        let url = format!(
            "https://graph.microsoft.com/v1.0/teams/{}/channels/{}/messages?$top=25",
            self.tenant_id, channel_id
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        let messages = body["value"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .filter_map(|m| {
                Some(TeamsMessage {
                    id: m["id"].as_str()?.to_string(),
                    channel_id: channel_id.to_string(),
                    from_id: m["from"]["user"]["id"].as_str().unwrap_or("").to_string(),
                    from_name: m["from"]["user"]["displayName"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    content: m["body"]["content"].as_str().unwrap_or("").to_string(),
                })
            })
            .collect();

        Ok(messages)
    }

    async fn authenticate(&self) -> Result<String, String> {
        let url = format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            self.tenant_id
        );
        let resp = self
            .client
            .post(&url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("scope", "https://graph.microsoft.com/.default"),
                ("grant_type", "client_credentials"),
            ])
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let err = body["error_description"].as_str().unwrap_or("auth failed");
            return Err(format!("Teams OAuth error ({}): {}", status, err));
        }

        let token = body["access_token"]
            .as_str()
            .ok_or("No access_token in response")?
            .to_string();

        *self
            .access_token
            .lock()
            .map_err(|e| format!("Lock error: {e}"))? = Some(token.clone());

        Ok(token)
    }
}

/// Create a Teams channel with a real HTTP client.
pub fn create_teams_channel(config: TeamsConfig) -> TeamsChannel {
    let http = RealTeamsHttp::new(
        config.client_id.clone(),
        config.client_secret.clone(),
        config.tenant_id.clone(),
    );
    TeamsChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockTeamsHttp;

    #[async_trait]
    impl TeamsHttpClient for MockTeamsHttp {
        async fn send_message(&self, _channel_id: &str, _text: &str) -> Result<String, String> {
            Ok("teams-msg-1".into())
        }
        async fn get_messages(&self, _channel_id: &str) -> Result<Vec<TeamsMessage>, String> {
            Ok(vec![])
        }
        async fn authenticate(&self) -> Result<String, String> {
            Ok("token".into())
        }
    }

    #[test]
    fn test_teams_channel_creation() {
        let ch = TeamsChannel::new(TeamsConfig::default(), Box::new(MockTeamsHttp));
        assert_eq!(ch.name(), "teams");
        assert_eq!(ch.channel_type(), ChannelType::Teams);
    }

    #[test]
    fn test_teams_capabilities() {
        let ch = TeamsChannel::new(TeamsConfig::default(), Box::new(MockTeamsHttp));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(caps.supports_editing);
        assert!(caps.supports_deletion);
        assert_eq!(caps.max_message_length, Some(28000));
    }

    #[test]
    fn test_teams_streaming_mode() {
        let ch = TeamsChannel::new(TeamsConfig::default(), Box::new(MockTeamsHttp));
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 1000 }
        );
    }

    #[test]
    fn test_teams_status_disconnected() {
        let ch = TeamsChannel::new(TeamsConfig::default(), Box::new(MockTeamsHttp));
        assert_eq!(ch.status(), ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_teams_send_without_connect() {
        let ch = TeamsChannel::new(TeamsConfig::default(), Box::new(MockTeamsHttp));
        let sender = ChannelUser::new("bot", ChannelType::Teams);
        let msg = ChannelMessage::text(ChannelType::Teams, "ch1", sender, "hi");
        assert!(ch.send_message(msg).await.is_err());
    }

    #[tokio::test]
    async fn test_teams_oauth_config_connect() {
        let config = TeamsConfig {
            client_id: "teams-client-id".into(),
            client_secret: "teams-client-secret".into(),
            tenant_id: "test-tenant".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let mut ch = TeamsChannel::new(config, Box::new(MockTeamsHttp));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_teams_config_auth_method_default() {
        let config = TeamsConfig::default();
        assert_eq!(config.auth_method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_teams_config_auth_method_serde() {
        let config = TeamsConfig {
            client_id: "id".into(),
            client_secret: "secret".into(),
            tenant_id: "tenant".into(),
            auth_method: AuthMethod::OAuth,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"oauth\""));
        let parsed: TeamsConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, AuthMethod::OAuth);
    }
}
