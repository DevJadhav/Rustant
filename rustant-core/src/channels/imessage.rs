//! iMessage channel via AppleScript bridge.
//!
//! Uses `osascript` to send messages via Messages.app and reads from the
//! Messages SQLite database. macOS-only.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, MessageId,
    StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for an iMessage channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IMessageConfig {
    pub enabled: bool,
    pub polling_interval_ms: u64,
}

impl Default for IMessageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            polling_interval_ms: 5000,
        }
    }
}

/// A resolved macOS contact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedContact {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
}

/// Trait for iMessage interactions, allowing test mocking.
#[async_trait]
pub trait IMessageBridge: Send + Sync {
    async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String>;
    async fn receive_messages(&self) -> Result<Vec<IMessageIncoming>, String>;
    async fn is_available(&self) -> Result<bool, String>;
    /// Search macOS Contacts for a name, returning matching contacts with phone/email.
    async fn resolve_contact(&self, query: &str) -> Result<Vec<ResolvedContact>, String>;
}

/// An incoming iMessage.
#[derive(Debug, Clone)]
pub struct IMessageIncoming {
    pub sender: String,
    pub text: String,
    pub timestamp: u64,
}

/// iMessage channel.
pub struct IMessageChannel {
    config: IMessageConfig,
    status: ChannelStatus,
    bridge: Box<dyn IMessageBridge>,
    name: String,
}

impl IMessageChannel {
    pub fn new(config: IMessageConfig, bridge: Box<dyn IMessageBridge>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            bridge,
            name: "imessage".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Search macOS Contacts by name and return matching entries.
    pub async fn resolve_contact(&self, query: &str) -> Result<Vec<ResolvedContact>, String> {
        self.bridge.resolve_contact(query).await
    }

    /// Send an iMessage to a recipient (phone number or email) via the bridge.
    /// This can be used directly without going through the Channel trait.
    pub async fn send_imessage(&self, recipient: &str, text: &str) -> Result<(), RustantError> {
        if self.status != ChannelStatus::Connected {
            return Err(RustantError::Channel(ChannelError::NotConnected {
                name: self.name.clone(),
            }));
        }
        self.bridge
            .send_message(recipient, text)
            .await
            .map_err(|e| {
                RustantError::Channel(ChannelError::SendFailed {
                    name: self.name.clone(),
                    message: e,
                })
            })
    }
}

#[async_trait]
impl Channel for IMessageChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::IMessage
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if !self.config.enabled {
            return Err(RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: "iMessage channel is not enabled".into(),
            }));
        }
        let available = self.bridge.is_available().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;
        if !available {
            return Err(RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: "Messages.app not available".into(),
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
            .map(|m| {
                let sender = super::ChannelUser::new(&m.sender, ChannelType::IMessage);
                ChannelMessage::text(ChannelType::IMessage, &m.sender, sender, &m.text)
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
            supports_reactions: true,
            supports_files: true,
            supports_voice: false,
            supports_video: false,
            max_message_length: None,
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::Polling {
            interval_ms: self.config.polling_interval_ms,
        }
    }
}

/// Real iMessage bridge using osascript (macOS only).
#[cfg(target_os = "macos")]
pub struct RealIMessageBridge;

#[cfg(target_os = "macos")]
impl Default for RealIMessageBridge {
    fn default() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl RealIMessageBridge {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
#[async_trait]
impl IMessageBridge for RealIMessageBridge {
    async fn send_message(&self, recipient: &str, text: &str) -> Result<(), String> {
        let escaped_recipient = recipient.replace('"', "\\\"");
        let escaped_text = text.replace('"', "\\\"");
        let script = format!(
            "tell application \"Messages\"\n\
             \tset targetService to 1st service whose service type = iMessage\n\
             \tset targetBuddy to buddy \"{escaped_recipient}\" of targetService\n\
             \tsend \"{escaped_text}\" to targetBuddy\n\
             end tell",
        );

        let output = tokio::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .await
            .map_err(|e| format!("Failed to run osascript: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("osascript failed: {stderr}"));
        }

        Ok(())
    }

    async fn receive_messages(&self) -> Result<Vec<IMessageIncoming>, String> {
        let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
        let db_path = format!("{home}/Library/Messages/chat.db");

        let output = tokio::process::Command::new("sqlite3")
            .args([
                &db_path,
                "-json",
                "SELECT m.ROWID, m.text, h.id as sender, m.date \
                 FROM message m \
                 JOIN handle h ON m.handle_id = h.ROWID \
                 WHERE m.is_from_me = 0 \
                 AND m.date > strftime('%s', 'now', '-60 seconds') * 1000000000 \
                 ORDER BY m.date DESC \
                 LIMIT 20;",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to read Messages DB: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim().is_empty() {
            return Ok(vec![]);
        }

        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&stdout).map_err(|e| format!("JSON parse error: {e}"))?;

        let messages = rows
            .iter()
            .filter_map(|r| {
                Some(IMessageIncoming {
                    sender: r["sender"].as_str()?.to_string(),
                    text: r["text"].as_str().unwrap_or("").to_string(),
                    timestamp: r["date"].as_u64().unwrap_or(0),
                })
            })
            .collect();

        Ok(messages)
    }

    async fn is_available(&self) -> Result<bool, String> {
        let output = tokio::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to (name of processes) contains \"Messages\"",
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to check Messages.app: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout.trim() == "true")
    }

    async fn resolve_contact(&self, query: &str) -> Result<Vec<ResolvedContact>, String> {
        let escaped_query = query.replace('"', "\\\"");
        let script = format!(
            r#"tell application "Contacts"
    set matchingPeople to every person whose name contains "{escaped_query}"
    set output to ""
    repeat with p in matchingPeople
        set pName to name of p
        set pPhone to ""
        set pEmail to ""
        try
            set pPhone to value of phone 1 of p
        end try
        try
            set pEmail to value of email 1 of p
        end try
        set output to output & pName & "||" & pPhone & "||" & pEmail & "%%"
    end repeat
    return output
end tell"#
        );

        let output = tokio::process::Command::new("osascript")
            .args(["-e", &script])
            .output()
            .await
            .map_err(|e| format!("Failed to run osascript: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Contacts lookup failed: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let contacts = stdout
            .trim()
            .split("%%")
            .filter(|s| !s.is_empty())
            .filter_map(|entry| {
                let parts: Vec<&str> = entry.split("||").collect();
                if parts.is_empty() {
                    return None;
                }
                let name = parts[0].trim().to_string();
                if name.is_empty() {
                    return None;
                }
                let phone = parts.get(1).and_then(|p| {
                    let p = p.trim();
                    if p.is_empty() {
                        None
                    } else {
                        Some(p.to_string())
                    }
                });
                let email = parts.get(2).and_then(|e| {
                    let e = e.trim();
                    if e.is_empty() {
                        None
                    } else {
                        Some(e.to_string())
                    }
                });
                Some(ResolvedContact { name, phone, email })
            })
            .collect();

        Ok(contacts)
    }
}

/// Create an iMessage channel with the real osascript bridge.
#[cfg(target_os = "macos")]
pub fn create_imessage_channel(config: IMessageConfig) -> IMessageChannel {
    IMessageChannel::new(config, Box::new(RealIMessageBridge::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockIMessageBridge {
        available: bool,
    }

    impl MockIMessageBridge {
        fn new(available: bool) -> Self {
            Self { available }
        }
    }

    #[async_trait]
    impl IMessageBridge for MockIMessageBridge {
        async fn send_message(&self, _recipient: &str, _text: &str) -> Result<(), String> {
            Ok(())
        }
        async fn receive_messages(&self) -> Result<Vec<IMessageIncoming>, String> {
            Ok(vec![])
        }
        async fn is_available(&self) -> Result<bool, String> {
            Ok(self.available)
        }
        async fn resolve_contact(&self, query: &str) -> Result<Vec<ResolvedContact>, String> {
            // Return a fake contact for testing
            Ok(vec![ResolvedContact {
                name: format!("Mock {query}"),
                phone: Some("+1234567890".to_string()),
                email: Some("mock@example.com".to_string()),
            }])
        }
    }

    #[test]
    fn test_imessage_channel_creation() {
        let ch = IMessageChannel::new(
            IMessageConfig::default(),
            Box::new(MockIMessageBridge::new(true)),
        );
        assert_eq!(ch.name(), "imessage");
        assert_eq!(ch.channel_type(), ChannelType::IMessage);
    }

    #[test]
    fn test_imessage_capabilities() {
        let ch = IMessageChannel::new(
            IMessageConfig::default(),
            Box::new(MockIMessageBridge::new(true)),
        );
        let caps = ch.capabilities();
        assert!(caps.supports_reactions);
        assert!(caps.supports_files);
        assert!(!caps.supports_threads);
    }

    #[test]
    fn test_imessage_streaming_mode() {
        let ch = IMessageChannel::new(
            IMessageConfig::default(),
            Box::new(MockIMessageBridge::new(true)),
        );
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 5000 }
        );
    }

    #[test]
    fn test_imessage_status_disconnected() {
        let ch = IMessageChannel::new(
            IMessageConfig::default(),
            Box::new(MockIMessageBridge::new(true)),
        );
        assert_eq!(ch.status(), ChannelStatus::Disconnected);
        assert!(!ch.is_connected());
    }

    #[tokio::test]
    async fn test_imessage_send_without_connect() {
        let ch = IMessageChannel::new(
            IMessageConfig::default(),
            Box::new(MockIMessageBridge::new(true)),
        );
        let sender = super::super::ChannelUser::new("me", ChannelType::IMessage);
        let msg = ChannelMessage::text(ChannelType::IMessage, "+1234", sender, "hi");
        let result = ch.send_message(msg).await;
        assert!(result.is_err());
    }
}
