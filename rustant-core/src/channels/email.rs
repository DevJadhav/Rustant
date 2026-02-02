//! Email channel via IMAP + SMTP.
//!
//! Uses trait abstractions for IMAP reading and SMTP sending.
//! In tests, mock implementations avoid network calls.

use super::{Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser, MessageId, StreamingMode};
use crate::error::{ChannelError, RustantError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Authentication method for the email channel.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailAuthMethod {
    /// Traditional username/password authentication.
    #[default]
    Password,
    /// OAuth 2.0 XOAUTH2 SASL authentication (for Gmail, Outlook, etc.).
    /// When using this method, the `password` field in `EmailConfig` holds
    /// the OAuth access token, and `username` is the email address.
    /// Use `gmail_oauth_config()` and `build_xoauth2_token()` from the
    /// oauth module to obtain and format the token.
    #[serde(rename = "xoauth2")]
    XOAuth2,
}

/// Configuration for an Email channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailConfig {
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
    /// Email address (used as the IMAP/SMTP username).
    pub username: String,
    /// Password or OAuth access token (when `auth_method` is `XOAuth2`).
    pub password: String,
    pub from_address: String,
    pub allowed_senders: Vec<String>,
    /// Authentication method for IMAP/SMTP connections.
    #[serde(default)]
    pub auth_method: EmailAuthMethod,
}

/// Trait for SMTP sending.
#[async_trait]
pub trait SmtpSender: Send + Sync {
    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<String, String>;
}

/// Trait for IMAP receiving.
#[async_trait]
pub trait ImapReader: Send + Sync {
    async fn fetch_unseen(&self) -> Result<Vec<IncomingEmail>, String>;
    async fn connect(&self) -> Result<(), String>;
}

/// An incoming email message.
#[derive(Debug, Clone)]
pub struct IncomingEmail {
    pub message_id: String,
    pub from: String,
    pub subject: String,
    pub body: String,
}

/// Email channel.
pub struct EmailChannel {
    config: EmailConfig,
    status: ChannelStatus,
    smtp: Box<dyn SmtpSender>,
    imap: Box<dyn ImapReader>,
    name: String,
}

impl EmailChannel {
    pub fn new(
        config: EmailConfig,
        smtp: Box<dyn SmtpSender>,
        imap: Box<dyn ImapReader>,
    ) -> Self {
        Self {
            config,
            status: ChannelStatus::Disconnected,
            smtp,
            imap,
            name: "email".to_string(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }
}

#[async_trait]
impl Channel for EmailChannel {
    fn name(&self) -> &str {
        &self.name
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Email
    }

    async fn connect(&mut self) -> Result<(), RustantError> {
        if self.config.username.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        // Both Password and XOAuth2 require a non-empty password/token
        if self.config.password.is_empty() {
            return Err(RustantError::Channel(ChannelError::AuthFailed {
                name: self.name.clone(),
            }));
        }
        self.imap.connect().await.map_err(|e| {
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
        let subject = msg
            .metadata
            .get("subject")
            .map(|s| s.as_str())
            .unwrap_or("Message from Rustant");

        self.smtp
            .send_email(&msg.channel_id, subject, text)
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
        let emails = self.imap.fetch_unseen().await.map_err(|e| {
            RustantError::Channel(ChannelError::ConnectionFailed {
                name: self.name.clone(),
                message: e,
            })
        })?;

        let messages = emails
            .into_iter()
            .filter(|e| {
                self.config.allowed_senders.is_empty()
                    || self.config.allowed_senders.contains(&e.from)
            })
            .map(|e| {
                let sender = ChannelUser::new(&e.from, ChannelType::Email);
                ChannelMessage::text(ChannelType::Email, &e.from, sender, &e.body)
                    .with_metadata("subject", &e.subject)
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
        StreamingMode::Polling { interval_ms: 30000 }
    }
}

/// Real SMTP sender using lettre.
pub struct RealSmtp {
    host: String,
    port: u16,
    username: String,
    password: String,
    from_address: String,
}

impl RealSmtp {
    pub fn new(host: String, port: u16, username: String, password: String, from_address: String) -> Self {
        Self {
            host,
            port,
            username,
            password,
            from_address,
        }
    }
}

#[async_trait]
impl SmtpSender for RealSmtp {
    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<String, String> {
        let email = lettre::Message::builder()
            .from(self.from_address.parse().map_err(|e| format!("Invalid from address: {e}"))?)
            .to(to.parse().map_err(|e| format!("Invalid to address: {e}"))?)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| format!("Failed to build email: {e}"))?;

        let creds = lettre::transport::smtp::authentication::Credentials::new(
            self.username.clone(),
            self.password.clone(),
        );

        let mailer = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&self.host)
            .map_err(|e| format!("SMTP relay error: {e}"))?
            .port(self.port)
            .credentials(creds)
            .build();

        use lettre::AsyncTransport;
        let response = mailer.send(email).await.map_err(|e| format!("SMTP send error: {e}"))?;

        Ok(format!("{}", response.code()))
    }
}

/// Real IMAP reader using async-imap.
pub struct RealImap {
    host: String,
    port: u16,
    username: String,
    password: String,
}

impl RealImap {
    pub fn new(host: String, port: u16, username: String, password: String) -> Self {
        Self {
            host,
            port,
            username,
            password,
        }
    }
}

#[async_trait]
impl ImapReader for RealImap {
    async fn fetch_unseen(&self) -> Result<Vec<IncomingEmail>, String> {
        let tcp = tokio::net::TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| format!("TCP connect error: {e}"))?;

        let native_tls_connector = native_tls::TlsConnector::new()
            .map_err(|e| format!("TLS connector error: {e}"))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_tls_connector);
        let tls_stream = tls_connector
            .connect(&self.host, tcp)
            .await
            .map_err(|e| format!("TLS connect error: {e}"))?;

        let client = async_imap::Client::new(tls_stream);

        let mut session = client
            .login(&self.username, &self.password)
            .await
            .map_err(|e| format!("IMAP login error: {}", e.0))?;

        session
            .select("INBOX")
            .await
            .map_err(|e| format!("IMAP select error: {e}"))?;

        let unseen = session
            .search("UNSEEN")
            .await
            .map_err(|e| format!("IMAP search error: {e}"))?;

        let mut emails = Vec::new();
        if !unseen.is_empty() {
            let seq_set: String = unseen
                .iter()
                .map(|s: &u32| s.to_string())
                .collect::<Vec<_>>()
                .join(",");

            let fetch_stream = session
                .fetch(&seq_set, "RFC822")
                .await
                .map_err(|e| format!("IMAP fetch error: {e}"))?;

            use futures::TryStreamExt;
            let messages: Vec<_> = fetch_stream
                .try_collect()
                .await
                .map_err(|e| format!("IMAP stream error: {e}"))?;

            for msg in &messages {
                if let Some(body_bytes) = msg.body() {
                    let raw = String::from_utf8_lossy(body_bytes).to_string();
                    let from = raw
                        .lines()
                        .find(|l| l.starts_with("From:"))
                        .map(|l| l.trim_start_matches("From:").trim().to_string())
                        .unwrap_or_default();
                    let subject = raw
                        .lines()
                        .find(|l| l.starts_with("Subject:"))
                        .map(|l| l.trim_start_matches("Subject:").trim().to_string())
                        .unwrap_or_default();
                    let body_text = raw
                        .split("\r\n\r\n")
                        .nth(1)
                        .unwrap_or("")
                        .to_string();

                    emails.push(IncomingEmail {
                        message_id: format!("imap-{}", msg.message),
                        from,
                        subject,
                        body: body_text,
                    });
                }
            }
        }

        let _ = session.logout().await;
        Ok(emails)
    }

    async fn connect(&self) -> Result<(), String> {
        let tcp = tokio::net::TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| format!("TCP connect error: {e}"))?;

        let native_tls_connector = native_tls::TlsConnector::new()
            .map_err(|e| format!("TLS connector error: {e}"))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_tls_connector);
        let tls_stream = tls_connector
            .connect(&self.host, tcp)
            .await
            .map_err(|e| format!("TLS connect error: {e}"))?;

        let client = async_imap::Client::new(tls_stream);

        let mut session = client
            .login(&self.username, &self.password)
            .await
            .map_err(|e| format!("IMAP login error: {}", e.0))?;

        let _ = session.logout().await;
        Ok(())
    }
}

/// Create an Email channel with real SMTP and IMAP clients.
pub fn create_email_channel(config: EmailConfig) -> EmailChannel {
    let smtp = RealSmtp::new(
        config.smtp_host.clone(),
        config.smtp_port,
        config.username.clone(),
        config.password.clone(),
        config.from_address.clone(),
    );
    let imap = RealImap::new(
        config.imap_host.clone(),
        config.imap_port,
        config.username.clone(),
        config.password.clone(),
    );
    EmailChannel::new(config, Box::new(smtp), Box::new(imap))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSmtp;

    #[async_trait]
    impl SmtpSender for MockSmtp {
        async fn send_email(&self, _to: &str, _subject: &str, _body: &str) -> Result<String, String> {
            Ok("email-id-1".to_string())
        }
    }

    struct MockImap;

    #[async_trait]
    impl ImapReader for MockImap {
        async fn fetch_unseen(&self) -> Result<Vec<IncomingEmail>, String> {
            Ok(vec![IncomingEmail {
                message_id: "msg1".into(),
                from: "alice@example.com".into(),
                subject: "Test".into(),
                body: "hello email".into(),
            }])
        }
        async fn connect(&self) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_email_connect() {
        let config = EmailConfig {
            username: "bot@example.com".into(),
            password: "pass".into(),
            ..Default::default()
        };
        let mut ch = EmailChannel::new(config, Box::new(MockSmtp), Box::new(MockImap));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[tokio::test]
    async fn test_email_send() {
        let config = EmailConfig {
            username: "bot@example.com".into(),
            password: "pass".into(),
            ..Default::default()
        };
        let mut ch = EmailChannel::new(config, Box::new(MockSmtp), Box::new(MockImap));
        ch.connect().await.unwrap();

        let sender = ChannelUser::new("bot@ex.com", ChannelType::Email);
        let msg = ChannelMessage::text(ChannelType::Email, "alice@ex.com", sender, "hi email")
            .with_metadata("subject", "Greetings");
        let id = ch.send_message(msg).await.unwrap();
        assert_eq!(id.0, "email-id-1");
    }

    #[tokio::test]
    async fn test_email_receive() {
        let config = EmailConfig {
            username: "bot@example.com".into(),
            password: "pass".into(),
            ..Default::default()
        };
        let mut ch = EmailChannel::new(config, Box::new(MockSmtp), Box::new(MockImap));
        ch.connect().await.unwrap();

        let msgs = ch.receive_messages().await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content.as_text(), Some("hello email"));
        assert_eq!(msgs[0].metadata.get("subject").map(|s| s.as_str()), Some("Test"));
    }

    #[test]
    fn test_email_capabilities() {
        let ch = EmailChannel::new(EmailConfig::default(), Box::new(MockSmtp), Box::new(MockImap));
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(caps.supports_files);
        assert!(caps.max_message_length.is_none());
    }

    #[test]
    fn test_email_streaming_mode() {
        let ch = EmailChannel::new(EmailConfig::default(), Box::new(MockSmtp), Box::new(MockImap));
        assert_eq!(ch.streaming_mode(), StreamingMode::Polling { interval_ms: 30000 });
    }

    #[tokio::test]
    async fn test_email_xoauth2_connect() {
        let config = EmailConfig {
            username: "user@gmail.com".into(),
            password: "ya29.oauth-access-token".into(),
            auth_method: EmailAuthMethod::XOAuth2,
            ..Default::default()
        };
        let mut ch = EmailChannel::new(config, Box::new(MockSmtp), Box::new(MockImap));
        ch.connect().await.unwrap();
        assert!(ch.is_connected());
    }

    #[test]
    fn test_email_auth_method_default() {
        let config = EmailConfig::default();
        assert_eq!(config.auth_method, EmailAuthMethod::Password);
    }

    #[test]
    fn test_email_auth_method_serde() {
        let config = EmailConfig {
            username: "user@gmail.com".into(),
            password: "token".into(),
            auth_method: EmailAuthMethod::XOAuth2,
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"xoauth2\""));
        let parsed: EmailConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.auth_method, EmailAuthMethod::XOAuth2);
    }

    #[test]
    fn test_email_xoauth2_token_format() {
        use crate::oauth::build_xoauth2_token;
        let token = build_xoauth2_token("user@gmail.com", "ya29.access-token");
        assert!(token.starts_with("user=user@gmail.com\x01"));
        assert!(token.contains("auth=Bearer ya29.access-token"));
        assert!(token.ends_with("\x01\x01"));
    }
}
