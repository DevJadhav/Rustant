//! Email channel via IMAP + SMTP.
//!
//! Uses trait abstractions for IMAP reading and SMTP sending.
//! In tests, mock implementations avoid network calls.

use super::{
    Channel, ChannelCapabilities, ChannelMessage, ChannelStatus, ChannelType, ChannelUser,
    MessageId, StreamingMode,
};
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
    pub fn new(config: EmailConfig, smtp: Box<dyn SmtpSender>, imap: Box<dyn ImapReader>) -> Self {
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
    /// Authentication method — when `XOAuth2`, uses SASL XOAUTH2 mechanism.
    pub auth_method: EmailAuthMethod,
}

impl RealSmtp {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        password: String,
        from_address: String,
        auth_method: EmailAuthMethod,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password,
            from_address,
            auth_method,
        }
    }
}

#[async_trait]
impl SmtpSender for RealSmtp {
    async fn send_email(&self, to: &str, subject: &str, body: &str) -> Result<String, String> {
        let email = lettre::Message::builder()
            .from(
                self.from_address
                    .parse()
                    .map_err(|e| format!("Invalid from address: {e}"))?,
            )
            .to(to.parse().map_err(|e| format!("Invalid to address: {e}"))?)
            .subject(subject)
            .body(body.to_string())
            .map_err(|e| format!("Failed to build email: {e}"))?;

        let creds = lettre::transport::smtp::authentication::Credentials::new(
            self.username.clone(),
            self.password.clone(),
        );

        let mut builder =
            lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::starttls_relay(&self.host)
                .map_err(|e| format!("SMTP relay error: {e}"))?
                .port(self.port)
                .credentials(creds);

        // Force XOAUTH2 SASL mechanism when using OAuth tokens.
        // With Password auth, lettre auto-negotiates the mechanism.
        if self.auth_method == EmailAuthMethod::XOAuth2 {
            use lettre::transport::smtp::authentication::Mechanism;
            builder = builder.authentication(vec![Mechanism::Xoauth2]);
        }

        let mailer = builder.build();

        use lettre::AsyncTransport;
        let response = mailer
            .send(email)
            .await
            .map_err(|e| format!("SMTP send error: {e}"))?;

        Ok(format!("{}", response.code()))
    }
}

/// SASL XOAUTH2 authenticator for `async-imap`.
///
/// Implements the [XOAUTH2 protocol](https://developers.google.com/gmail/imap/xoauth2-protocol)
/// used by Gmail (and other providers) for IMAP authentication with OAuth tokens.
pub struct XOAuth2Authenticator {
    user: String,
    access_token: String,
}

impl XOAuth2Authenticator {
    pub fn new(user: &str, access_token: &str) -> Self {
        Self {
            user: user.to_string(),
            access_token: access_token.to_string(),
        }
    }

    /// Build the SASL XOAUTH2 response string.
    pub fn response(&self) -> String {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

impl async_imap::Authenticator for XOAuth2Authenticator {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        self.response()
    }
}

/// Real IMAP reader using async-imap.
pub struct RealImap {
    host: String,
    port: u16,
    username: String,
    password: String,
    /// Authentication method — when `XOAuth2`, uses SASL XOAUTH2 instead of plain login.
    pub auth_method: EmailAuthMethod,
}

impl RealImap {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        password: String,
        auth_method: EmailAuthMethod,
    ) -> Self {
        Self {
            host,
            port,
            username,
            password,
            auth_method,
        }
    }
}

#[async_trait]
impl ImapReader for RealImap {
    async fn fetch_unseen(&self) -> Result<Vec<IncomingEmail>, String> {
        let tcp = tokio::net::TcpStream::connect((self.host.as_str(), self.port))
            .await
            .map_err(|e| format!("TCP connect error: {e}"))?;

        let native_tls_connector =
            native_tls::TlsConnector::new().map_err(|e| format!("TLS connector error: {e}"))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_tls_connector);
        let tls_stream = tls_connector
            .connect(&self.host, tcp)
            .await
            .map_err(|e| format!("TLS connect error: {e}"))?;

        let mut client = async_imap::Client::new(tls_stream);

        // Read and discard the server greeting before authentication.
        // async-imap's Client::new() does NOT consume the greeting, and
        // login() handles this internally, but authenticate() does not —
        // the unread greeting causes do_auth_handshake() to deadlock.
        client
            .read_response()
            .await
            .map_err(|e| format!("IMAP greeting read error: {e}"))?
            .ok_or_else(|| "IMAP server closed connection before greeting".to_string())?;

        let mut session = match self.auth_method {
            EmailAuthMethod::XOAuth2 => {
                let auth = XOAuth2Authenticator::new(&self.username, &self.password);
                client
                    .authenticate("XOAUTH2", auth)
                    .await
                    .map_err(|e| format!("IMAP XOAUTH2 auth error: {}", e.0))?
            }
            EmailAuthMethod::Password => client
                .login(&self.username, &self.password)
                .await
                .map_err(|e| format!("IMAP login error: {}", e.0))?,
        };

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
                    let body_text = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();

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

        let native_tls_connector =
            native_tls::TlsConnector::new().map_err(|e| format!("TLS connector error: {e}"))?;
        let tls_connector = tokio_native_tls::TlsConnector::from(native_tls_connector);
        let tls_stream = tls_connector
            .connect(&self.host, tcp)
            .await
            .map_err(|e| format!("TLS connect error: {e}"))?;

        let mut client = async_imap::Client::new(tls_stream);

        // Read the server greeting — required before authenticate().
        client
            .read_response()
            .await
            .map_err(|e| format!("IMAP greeting read error: {e}"))?
            .ok_or_else(|| "IMAP server closed connection before greeting".to_string())?;

        let mut session = match self.auth_method {
            EmailAuthMethod::XOAuth2 => {
                let auth = XOAuth2Authenticator::new(&self.username, &self.password);
                client
                    .authenticate("XOAUTH2", auth)
                    .await
                    .map_err(|e| format!("IMAP XOAUTH2 auth error: {}", e.0))?
            }
            EmailAuthMethod::Password => client
                .login(&self.username, &self.password)
                .await
                .map_err(|e| format!("IMAP login error: {}", e.0))?,
        };

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
        config.auth_method.clone(),
    );
    let imap = RealImap::new(
        config.imap_host.clone(),
        config.imap_port,
        config.username.clone(),
        config.password.clone(),
        config.auth_method.clone(),
    );
    EmailChannel::new(config, Box::new(smtp), Box::new(imap))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSmtp;

    #[async_trait]
    impl SmtpSender for MockSmtp {
        async fn send_email(
            &self,
            _to: &str,
            _subject: &str,
            _body: &str,
        ) -> Result<String, String> {
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
        assert_eq!(
            msgs[0].metadata.get("subject").map(|s| s.as_str()),
            Some("Test")
        );
    }

    #[test]
    fn test_email_capabilities() {
        let ch = EmailChannel::new(
            EmailConfig::default(),
            Box::new(MockSmtp),
            Box::new(MockImap),
        );
        let caps = ch.capabilities();
        assert!(!caps.supports_threads);
        assert!(caps.supports_files);
        assert!(caps.max_message_length.is_none());
    }

    #[test]
    fn test_email_streaming_mode() {
        let ch = EmailChannel::new(
            EmailConfig::default(),
            Box::new(MockSmtp),
            Box::new(MockImap),
        );
        assert_eq!(
            ch.streaming_mode(),
            StreamingMode::Polling { interval_ms: 30000 }
        );
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

    // ── XOAUTH2 Authenticator Tests ─────────────────────────────────────

    #[test]
    fn test_xoauth2_authenticator_response_format() {
        let auth = XOAuth2Authenticator::new("user@gmail.com", "ya29.test-token");
        let response = auth.response();
        assert_eq!(
            response,
            "user=user@gmail.com\x01auth=Bearer ya29.test-token\x01\x01"
        );
    }

    #[test]
    fn test_xoauth2_authenticator_process() {
        let mut auth = XOAuth2Authenticator::new("user@gmail.com", "ya29.test-token");
        // async-imap's Authenticator trait calls process() with the server challenge
        let response = async_imap::Authenticator::process(&mut auth, b"");
        assert_eq!(
            response,
            "user=user@gmail.com\x01auth=Bearer ya29.test-token\x01\x01"
        );
    }

    #[test]
    fn test_xoauth2_authenticator_ignores_challenge() {
        let mut auth = XOAuth2Authenticator::new("alice@example.com", "token123");
        // The XOAUTH2 protocol sends the same response regardless of challenge
        let r1 = async_imap::Authenticator::process(&mut auth, b"");
        let r2 = async_imap::Authenticator::process(&mut auth, b"some challenge data");
        assert_eq!(r1, r2);
    }

    // ── RealImap / RealSmtp auth_method Tests ───────────────────────────

    #[test]
    fn test_real_imap_stores_auth_method_password() {
        let imap = RealImap::new(
            "imap.gmail.com".into(),
            993,
            "user@gmail.com".into(),
            "password123".into(),
            EmailAuthMethod::Password,
        );
        assert_eq!(imap.auth_method, EmailAuthMethod::Password);
    }

    #[test]
    fn test_real_imap_stores_auth_method_xoauth2() {
        let imap = RealImap::new(
            "imap.gmail.com".into(),
            993,
            "user@gmail.com".into(),
            "ya29.token".into(),
            EmailAuthMethod::XOAuth2,
        );
        assert_eq!(imap.auth_method, EmailAuthMethod::XOAuth2);
    }

    #[test]
    fn test_real_smtp_stores_auth_method_password() {
        let smtp = RealSmtp::new(
            "smtp.gmail.com".into(),
            587,
            "user@gmail.com".into(),
            "password123".into(),
            "user@gmail.com".into(),
            EmailAuthMethod::Password,
        );
        assert_eq!(smtp.auth_method, EmailAuthMethod::Password);
    }

    #[test]
    fn test_real_smtp_stores_auth_method_xoauth2() {
        let smtp = RealSmtp::new(
            "smtp.gmail.com".into(),
            587,
            "user@gmail.com".into(),
            "ya29.token".into(),
            "user@gmail.com".into(),
            EmailAuthMethod::XOAuth2,
        );
        assert_eq!(smtp.auth_method, EmailAuthMethod::XOAuth2);
    }

    #[test]
    fn test_create_email_channel_passes_auth_method_password() {
        let config = EmailConfig {
            imap_host: "imap.gmail.com".into(),
            imap_port: 993,
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 587,
            username: "user@gmail.com".into(),
            password: "pass".into(),
            from_address: "user@gmail.com".into(),
            auth_method: EmailAuthMethod::Password,
            ..Default::default()
        };
        // Should not panic — auth_method is passed through
        let _ch = create_email_channel(config);
    }

    #[test]
    fn test_create_email_channel_passes_auth_method_xoauth2() {
        let config = EmailConfig {
            imap_host: "imap.gmail.com".into(),
            imap_port: 993,
            smtp_host: "smtp.gmail.com".into(),
            smtp_port: 587,
            username: "user@gmail.com".into(),
            password: "ya29.token".into(),
            from_address: "user@gmail.com".into(),
            auth_method: EmailAuthMethod::XOAuth2,
            ..Default::default()
        };
        // Should not panic — auth_method is passed through to RealImap/RealSmtp
        let _ch = create_email_channel(config);
    }
}
