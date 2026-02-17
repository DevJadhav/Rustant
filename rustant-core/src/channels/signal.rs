//! Signal channel via signal-cli subprocess bridge.
//!
//! Communicates with the Signal network through the signal-cli tool
//! using a subprocess bridge. In tests, a trait abstraction mocks the CLI.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for a Signal channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SignalConfig {
    pub phone_number: String,
    pub signal_cli_path: String,
    pub allowed_contacts: Vec<String>,
}

/// Trait for signal-cli interactions.
#[async_trait]
pub trait SignalCliBridge: Send + Sync {
    async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String>;
    async fn receive_messages(&self) -> Result<Vec<SignalIncoming>, String>;
    async fn is_registered(&self) -> Result<bool, String>;
}

/// An incoming Signal message.
#[derive(Debug, Clone)]
pub struct SignalIncoming {
    pub sender: String,
    pub sender_name: Option<String>,
    pub text: String,
    pub timestamp: u64,
}

/// Signal channel.
pub struct SignalChannel {
    config: SignalConfig,
    status: ChannelStatus,
    bridge: Box<dyn SignalCliBridge>,
    name: String,
}

impl SignalChannel {
    pub fn new(config: SignalConfig, bridge: Box<dyn SignalCliBridge>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            bridge,
            name: "signal".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for SignalChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Signal
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.phone_number.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        let registered = self.bridge.is_registered().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;
        if !registered {
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
        self.bridge
            .send_message(&msg.channel_id, text)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })?;
        Ok(MessageId::random())
    }

    async fn receive_messages(&self) -> Result<Vec<ChannelMessage>, RustantError> {
        let incoming = self.bridge.receive_messages().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;

        let messages = incoming
            .into_iter()
            .filter(|m| {
                self.config.allowed_contacts.is_empty()
                    || self.config.allowed_contacts.contains(&m.sender)
            })
            .map(|m| {
                let mut user = ChannelUser::new(&m.sender, ChannelType::Signal);
                if let Some(name) = m.sender_name {
                    user = user.with_name(name);
                }
                ChannelMessage::text(ChannelType::Signal, &m.sender, user, &m.text)
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
            max_message_length: None,
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling { interval_ms: 5000 }
    }
}

/// Real Signal CLI bridge using tokio subprocess.
pub struct RealSignalCliBridge {
    cli_path: String,
    phone_number: String,
}

impl RealSignalCliBridge {
    pub fn new(cli_path: String, phone_number: String) -> Self {
        Self {
            cli_path,
            phone_number,
        }
    }
}

#[async_trait]
impl SignalCliBridge for RealSignalCliBridge {
    async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String> {
        let output = tokio::process::Command::new(&self.cli_path)
            .args(["-u", &self.phone_number, "send", "-m", text, recipient])
            .output()
            .await
            .map_err(|e| format!("Failed to run signal-cli: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("signal-cli send failed: {}", stderr));
        }

        Ok(())
    }

    async fn receive_messages(&self) -> Result<Vec<SignalIncoming>, String> {
        let output = tokio::process::Command::new(&self.cli_path)
            .args([
                "-u",
                &self.phone_number,
                "receive",
                "--json",
                "--timeout",
                "5",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to run signal-cli: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut messages = Vec::new();

        for line in stdout.lines() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(data_msg) = v["envelope"]["dataMessage"].as_object()
                    && let Some(text) = data_msg.get("message").and_then(|m| m.as_str()) {
                        messages.push(SignalIncoming {
                            sender: v["envelope"]["source"].as_str().unwrap_or("").to_string(),
                            sender_name: v["envelope"]["sourceName"]
                                .as_str()
                                .map(|s| s.to_string()),
                            text: text.to_string(),
                            timestamp: v["envelope"]["timestamp"].as_u64().unwrap_or(0),
                        });
                    }
        }

        Ok(messages)
    }

    async fn is_registered(&self) -> Result<bool, String> {
        let output = tokio::process::Command::new(&self.cli_path)
            .args(["-u", &self.phone_number, "listAccounts"])
            .output()
            .await
            .map_err(|e| format!("Failed to run signal-cli: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.contains(&self.phone_number))
    }
}

/// Create a Signal channel with a real signal-cli bridge.
pub fn create_signal_channel(config: SignalConfig) -> SignalChannel {
    let bridge =
        RealSignalCliBridge::new(config.signal_cli_path.clone(), config.phone_number.clone());
    SignalChannel::new(config, Box::new(bridge))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSignalBridge {
        registered: bool,
    }

    impl MockSignalBridge {
        fn new(registered: bool) -> Self {
            Self { registered }
        }
    }

    #[async_trait]
    impl SignalCliBridge for MockSignalBridge {
        async fn send_message(&self, _recipient: &str, _text: &str) -> Result<(), String> {
            Ok(())
        }
        async fn receive_messages(&self) -> Result<Vec<SignalIncoming>, String> {
            Ok(vec![SignalIncoming {
                sender: "+1234567890".into(),
                sender_name: Some("Alice".into()),
                text: "hello signal".into(),
                timestamp: 1000,
            }])
        }
        async fn is_registered(&self) -> Result<bool, String> {
            Ok(self.registered)
        }
    }

    #[tokio::test]
    async fn test_signal_connect() {
        let config = SignalConfig {
            phone_number: "+1234567890".into(),
            signal_cli_path: "/usr/bin/signal-cli".into(),
            ..Default::default()
        };
        let mut ch = SignalChannel::new(config, Box::new(MockSignalBridge::new(true)));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_signal_connect_not_registered() {
        let config = SignalConfig {
            phone_number: "+1234567890".into(),
            ..Default::default()
        };
        let mut ch = SignalChannel::new(config, Box::new(MockSignalBridge::new(false)));
        assert!(ch.connect().await.is_err());
    }

    #[tokio::test]
    async fn test_signal_receive() {
        let config = SignalConfig {
            phone_number: "+1234567890".into(),
            ..Default::default()
        };
        let mut ch = SignalChannel::new(config, Box::new(MockSignalBridge::new(true)));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hello signal"));
    }

    #[test]
    fn test_signal_capabilities() {
        let ch = SignalChannel::new(
            SignalConfig {
                phone_number: "+1".into(),
                ..Default::default()
            },
            Box::new(MockSignalBridge::new(true)),
        );
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(caps.supports_files);
        assert!(caps.supports_voice);
    }

    #[test]
    fn test_signal_streaming_mode() {
        let ch = SignalChannel::new(
            SignalConfig::default(),
            Box::new(MockSignalBridge::new(true)),
        );
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 5000 }
        );
    }
}
