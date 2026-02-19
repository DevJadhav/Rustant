//! IRC channel via raw TCP/TLS.
//!
//! Connects to an IRC server using a persistent TCP connection.
//! In tests, a trait abstraction provides mock implementations.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Configuration for an IRC channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrcConfig {
    pub enabled: bool,
    pub server: String,
    pub port: u16,
    pub nick: String,
    pub channels: Vec<String>,
    pub use_tls: bool,
}

impl Default for IrcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            server: String::new(),
            port: 6667,
            nick: "rustant".into(),
            channels: Vec::new(),
            use_tls: false,
        }
    }
}

/// Trait for IRC connection interactions.
#[async_trait]
pub trait IrcConnection: Send + Sync {
    async fn connect(&self) -> Result<(), String>;
    async fn disconnect(&self) -> Result<(), String>;
    async fn send_privmsg(&self, target: &str, text: &str) -> Result<(), String>;
    async fn receive(&self) -> Result<Vec<IrcMessage>, String>;
}

/// An incoming IRC message.
#[derive(Debug, Clone)]
pub struct IrcMessage {
    pub nick: String,
    pub channel: String,
    pub text: String,
}

/// IRC channel.
pub struct IrcChannel {
    config: IrcConfig,
    status: ChannelStatus,
    connection: Box<dyn IrcConnection>,
    name: String,
}

impl IrcChannel {
    pub fn new(config: IrcConfig, connection: Box<dyn IrcConnection>) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            connection,
            name: "irc".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for IrcChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Irc
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.server.is_empty() {
            return Err(RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: "No server configured".into(),
            }));
        }
        self.connection.connect().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;
        self.status = ChannelStatus::Connected;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), RustantError> {
        let _ = self.connection.disconnect().await;
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
        self.connection
            .send_privmsg(&msg.channel_id, text)
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
        let incoming = self.connection.receive().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;

        let messages = incoming
            .into_iter()
            .map(|m| {
                let sender = ChannelUser::new(&m.nick, ChannelType::Irc);
                ChannelMessage::text(ChannelType::Irc, &m.channel, sender, &m.text)
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
            max_message_length: Some(512),
            supports_editing: false,
            supports_deletion: false,
        }
    }

    fn streaming_mode(&self) -> StreamingMode {
        StreamingMode::WebSocket
    }
}

/// Real IRC connection using tokio TCP.
pub struct RealIrcConnection {
    server: String,
    port: u16,
    nick: String,
    channels: Vec<String>,
    writer: tokio::sync::Mutex<Option<tokio::io::WriteHalf<tokio::net::TcpStream>>>,
}

impl RealIrcConnection {
    pub fn new(server: String, port: u16, nick: String, channels: Vec<String>) -> Self {
        Self {
            server,
            port,
            nick,
            channels,
            writer: tokio::sync::Mutex::new(None),
        }
    }
}

#[async_trait]
impl IrcConnection for RealIrcConnection {
    async fn connect(&self) -> Result<(), String> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

        let addr = format!("{}:{}", self.server, self.port);
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .map_err(|e| format!("TCP connect error: {e}"))?;

        let (reader, mut writer) = tokio::io::split(stream);

        // Send NICK and USER registration
        let nick_cmd = format!("NICK {}\r\n", self.nick);
        let user_cmd = format!("USER {} 0 * :{}\r\n", self.nick, self.nick);
        writer
            .write_all(nick_cmd.as_bytes())
            .await
            .map_err(|e| format!("Write error: {e}"))?;
        writer
            .write_all(user_cmd.as_bytes())
            .await
            .map_err(|e| format!("Write error: {e}"))?;

        // Join channels
        for ch in &self.channels {
            let join_cmd = format!("JOIN {ch}\r\n");
            writer
                .write_all(join_cmd.as_bytes())
                .await
                .map_err(|e| format!("Write error: {e}"))?;
        }

        // Store writer for later use
        *self.writer.lock().await = Some(writer);

        // Spawn a background task to handle incoming lines
        let mut buf_reader = tokio::io::BufReader::new(reader);
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match buf_reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
        });

        Ok(())
    }

    async fn disconnect(&self) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;

        if let Some(ref mut writer) = *self.writer.lock().await {
            let _ = writer.write_all(b"QUIT :Leaving\r\n").await;
        }
        *self.writer.lock().await = None;
        Ok(())
    }

    async fn send_privmsg(&self, target: &str, text: &str) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;

        let mut guard = self.writer.lock().await;
        let writer = guard.as_mut().ok_or_else(|| "Not connected".to_string())?;

        let cmd = format!("PRIVMSG {target} :{text}\r\n");
        writer
            .write_all(cmd.as_bytes())
            .await
            .map_err(|e| format!("Write error: {e}"))?;

        Ok(())
    }

    async fn receive(&self) -> Result<Vec<IrcMessage>, String> {
        // Messages are received asynchronously through the reader task.
        // A full implementation would use a shared message buffer.
        Ok(vec![])
    }
}

/// Create an IRC channel with a real TCP connection.
pub fn create_irc_channel(config: IrcConfig) -> IrcChannel {
    let conn = RealIrcConnection::new(
        config.server.clone(),
        config.port,
        config.nick.clone(),
        config.channels.clone(),
    );
    IrcChannel::new(config, Box::new(conn))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockIrcConnection;

    #[async_trait]
    impl IrcConnection for MockIrcConnection {
        async fn connect(&self) -> Result<(), String> {
            Ok(())
        }
        async fn disconnect(&self) -> Result<(), String> {
            Ok(())
        }
        async fn send_privmsg(&self, _target: &str, _text: &str) -> Result<(), String> {
            Ok(())
        }
        async fn receive(&self) -> Result<Vec<IrcMessage>, String> {
            Ok(vec![])
        }
    }

    #[test]
    fn test_irc_channel_creation() {
        let ch = IrcChannel::new(IrcConfig::default(), Box::new(MockIrcConnection));
        assert_eq!(ch.name(), "irc");
        assert_eq!(ch.channel_type(), ChannelType::Irc);
    }

    #[test]
    fn test_irc_capabilities() {
        let ch = IrcChannel::new(IrcConfig::default(), Box::new(MockIrcConnection));
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(!caps.supports_files);
        assert_eq!(caps.max_message_length, Some(512));
    }

    #[test]
    fn test_irc_streaming_mode() {
        let ch = IrcChannel::new(IrcConfig::default(), Box::new(MockIrcConnection));
        assert_eq!(ch.streaming_mode(), StreamingMode::WebSocket);
    }

    #[test]
    fn test_irc_status_disconnected() {
        let ch = IrcChannel::new(IrcConfig::default(), Box::new(MockIrcConnection));
        assert_eq!(ch.status(), ChannelStatus::Disconnected);
    }

    #[tokio::test]
    async fn test_irc_send_without_connect() {
        let ch = IrcChannel::new(IrcConfig::default(), Box::new(MockIrcConnection));
        let sender = ChannelUser::new("nick", ChannelType::Irc);
        let msg = ChannelMessage::text(ChannelType::Irc, "#test", sender, "hi");
        assert!(ch.send_message(msg).await.is_err());
    }
}
