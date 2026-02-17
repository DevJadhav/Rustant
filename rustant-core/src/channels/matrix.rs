//! Matrix Client-Server API channel implementation.
//!
//! Connects to a Matrix homeserver via the Client-Server API using reqwest.
//! In tests, a trait abstraction provides mock implementations.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Matrix channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatrixConfig {
    pub homeserver_url: String,
    pub access_token: String,
    pub user_id: String,
    pub room_ids: Vec<String>,
}

/// Trait for Matrix API interactions.
#[async_trait]
pub trait MatrixHttpClient: Send + Sync {
    async fn send_message(&self, room_id: &str, text: &str) -> Result<String, String>;
    async fn sync(&self, since: Option<&str>) -> Result<Vec<MatrixEvent>, String>;
    async fn login(&self) -> Result<String, String>;
}

/// A Matrix room event.
#[derive(Debug, Clone)]
pub struct MatrixEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub body: String,
}

/// Matrix channel.
pub struct MatrixChannel {
    config: MatrixConfig,
    status: ChannelStatus,
    http_client: Box<dyn MatrixHttpClient>,
    name: String,
    next_batch: Option<String>,
}

impl MatrixChannel {
    pub fn new(config: MatrixConfig, http_client: Box<dyn MatrixHttpClient>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            http_client,
            name: "matrix".to_string(),
            next_batch: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for MatrixChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Matrix
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.homeserver_url.is_empty() || self.config.access_token.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.http_client.login().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
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
        let events = self
            .http_client
            .sync(self.next_batch.as_deref())
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::ConnectionFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })?;

        let messages = events
            .into_iter()
            .filter(|e| {
                self.config.room_ids.is_empty() || self.config.room_ids.contains(&e.room_id)
            })
            .map(|e| {
                let sender = ChannelUser::new(&e.sender, ChannelType::Matrix);
                ChannelMessage::text(ChannelType::Matrix, &e.room_id, sender, &e.body)
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
            max_message_length: None,
            supports_editing: true,
            supports_deletion: true,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::LongPolling
    }
}

/// Real Matrix Client-Server API HTTP client using reqwest.
pub struct RealMatrixHttp {
    client: reqwest::Client,
    homeserver_url: String,
    access_token: String,
    txn_counter: std::sync::atomic::AtomicU64,
}

impl RealMatrixHttp {
    pub fn new(homeserver_url: String, access_token: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            homeserver_url: homeserver_url.trim_end_matches('/').to_string(),
            access_token,
            txn_counter: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[async_trait]
impl MatrixHttpClient for RealMatrixHttp {
    async fn send_message(&self, room_id: &str, text: &str) -> Result<String, String> {
        let txn_id = self
            .txn_counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.homeserver_url, room_id, txn_id
        );
        let resp = self
            .client
            .put(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .json(&serde_json::json!({
                "msgtype": "m.text",
                "body": text,
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
            let err = body["error"].as_str().unwrap_or("unknown error");
            return Err(format!("Matrix API error ({}): {}", status, err));
        }

        let event_id = body["event_id"].as_str().unwrap_or("").to_string();
        Ok(event_id)
    }

    async fn sync(&self, since: Option<&str>) -> Result<Vec<MatrixEvent>, String> {
        let mut url = format!(
            "{}/_matrix/client/v3/sync?timeout=30000",
            self.homeserver_url
        );
        if let Some(since) = since {
            url.push_str(&format!("&since={}", since));
        }
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        let mut events = Vec::new();
        if let Some(rooms) = body["rooms"]["join"].as_object() {
            for (room_id, room_data) in rooms {
                if let Some(timeline) = room_data["timeline"]["events"].as_array() {
                    for event in timeline {
                        if event["type"].as_str() == Some("m.room.message")
                            && let Some(event_body) = event["content"]["body"].as_str() {
                                events.push(MatrixEvent {
                                    event_id: event["event_id"].as_str().unwrap_or("").to_string(),
                                    room_id: room_id.clone(),
                                    sender: event["sender"].as_str().unwrap_or("").to_string(),
                                    body: event_body.to_string(),
                                });
                            }
                    }
                }
            }
        }

        Ok(events)
    }

    async fn login(&self) -> Result<String, String> {
        // When using an access token, verify it with whoami
        let url = format!("{}/_matrix/client/v3/account/whoami", self.homeserver_url);
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.access_token))
            .send()
            .await
            .map_err(|e| format!("HTTP error: {e}"))?;

        let status = resp.status();
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {e}"))?;

        if !status.is_success() {
            let err = body["error"].as_str().unwrap_or("unauthorized");
            return Err(format!("Matrix auth failed ({}): {}", status, err));
        }

        let user_id = body["user_id"].as_str().unwrap_or("").to_string();
        Ok(user_id)
    }
}

/// Create a Matrix channel with a real HTTP client.
pub fn create_matrix_channel(config: MatrixConfig) -> MatrixChannel {
    let http = RealMatrixHttp::new(config.homeserver_url.clone(), config.access_token.clone());
    MatrixChannel::new(config, Box::new(http))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockMatrixHttp;

    #[async_trait]
    impl MatrixHttpClient for MockMatrixHttp {
        async fn send_message(&self, _room_id: &str, _text: &str) -> Result<String, String> {
            Ok("$event1".to_string())
        }
        async fn sync(&self, _since: Option<&str>) -> Result<Vec<MatrixEvent>, String> {
            Ok(vec![MatrixEvent {
                event_id: "$ev1".into(),
                room_id: "!room1:example.com".into(),
                sender: "@alice:example.com".into(),
                body: "hello matrix".into(),
            }])
        }
        async fn login(&self) -> Result<String, String> {
            Ok("token123".to_string())
        }
    }

    #[tokio::test]
    async fn test_matrix_connect() {
        let config = MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            access_token: "token".into(),
            user_id: "@bot:example.com".into(),
            room_ids: vec![],
        };
        let mut ch = MatrixChannel::new(config, Box::new(MockMatrixHttp));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_matrix_send() {
        let config = MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            access_token: "token".into(),
            ..Default::default()
        };
        let mut ch = MatrixChannel::new(config, Box::new(MockMatrixHttp));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("@bot:ex.com", ChannelType::Matrix);
        let msg = ChannelMessage::text(ChannelType::Matrix, "!room1:ex.com", sender, "hi");
        let id = ch.send_message(msg).await.unwrap();
        assert_eq!(id.0, "$event1");
    }

    #[tokio::test]
    async fn test_matrix_receive() {
        let config = MatrixConfig {
            homeserver_url: "https://matrix.example.com".into(),
            access_token: "token".into(),
            ..Default::default()
        };
        let mut ch = MatrixChannel::new(config, Box::new(MockMatrixHttp));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hello matrix"));
    }

    #[test]
    fn test_matrix_capabilities() {
        let ch = MatrixChannel::new(MatrixConfig::default(), Box::new(MockMatrixHttp));
        let caps = ch.capabilities();
        assert!(caps.supports_threads);
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(caps.supports_editing);
        assert!(caps.supports_deletion);
        assert!(caps.max_message_length.is_none());
    }

    #[test]
    fn test_matrix_streaming_mode() {
        let ch = MatrixChannel::new(MatrixConfig::default(), Box::new(MockMatrixHttp));
        assert_eq!(ch.streaming_mode(), StreamingMode::LongPolling);
    }
}
