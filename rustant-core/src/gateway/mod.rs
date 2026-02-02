//! # WebSocket Gateway
//!
//! Provides a WebSocket-based server for real-time communication between
//! external clients and the Rustant agent. Supports authentication,
//! connection management, session lifecycle, and a structured event protocol.

mod auth;
pub mod channel_bridge;
mod connection;
mod events;
pub mod node_bridge;
mod server;
mod session;

pub use auth::GatewayAuth;
pub use channel_bridge::ChannelBridge;
pub use connection::ConnectionManager;
pub use events::{ClientMessage, GatewayEvent, ServerMessage};
pub use node_bridge::NodeBridge;
pub use server::{
    router as gateway_router, run as run_gateway, GatewayServer, SharedGateway, StatusProvider,
};
pub use session::{GatewaySession, SessionManager, SessionState};

use serde::{Deserialize, Serialize};

/// Configuration for the WebSocket gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Whether the gateway is enabled.
    pub enabled: bool,
    /// Host to bind to.
    pub host: String,
    /// Port to listen on.
    pub port: u16,
    /// Valid authentication tokens.
    pub auth_tokens: Vec<String>,
    /// Maximum concurrent WebSocket connections.
    pub max_connections: usize,
    /// Session timeout in seconds (0 = no timeout).
    pub session_timeout_secs: u64,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "127.0.0.1".to_string(),
            port: 8080,
            auth_tokens: Vec::new(),
            max_connections: 10,
            session_timeout_secs: 3600,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gateway_config_default() {
        let config = GatewayConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert!(config.auth_tokens.is_empty());
        assert_eq!(config.max_connections, 10);
        assert_eq!(config.session_timeout_secs, 3600);
    }

    #[test]
    fn test_gateway_config_serialization() {
        let config = GatewayConfig {
            enabled: true,
            host: "0.0.0.0".into(),
            port: 9090,
            auth_tokens: vec!["token1".into()],
            max_connections: 50,
            session_timeout_secs: 7200,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: GatewayConfig = serde_json::from_str(&json).unwrap();
        assert!(restored.enabled);
        assert_eq!(restored.port, 9090);
        assert_eq!(restored.auth_tokens.len(), 1);
    }
}
