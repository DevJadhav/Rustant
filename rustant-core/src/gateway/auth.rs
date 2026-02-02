//! Gateway authentication.

use super::GatewayConfig;

/// Token-based authentication for WebSocket connections.
#[derive(Debug, Clone)]
pub struct GatewayAuth {
    valid_tokens: Vec<String>,
}

impl GatewayAuth {
    /// Create a new auth validator from the gateway config.
    pub fn from_config(config: &GatewayConfig) -> Self {
        Self {
            valid_tokens: config.auth_tokens.clone(),
        }
    }

    /// Create a new auth validator with the given tokens.
    pub fn new(tokens: Vec<String>) -> Self {
        Self {
            valid_tokens: tokens,
        }
    }

    /// Validate a token. Returns `true` if the token is valid.
    ///
    /// If no tokens are configured, all tokens are accepted (open mode).
    pub fn validate(&self, token: &str) -> bool {
        if self.valid_tokens.is_empty() {
            return true; // open mode: no auth required
        }
        self.valid_tokens.iter().any(|t| t == token)
    }

    /// Number of configured tokens.
    pub fn token_count(&self) -> usize {
        self.valid_tokens.len()
    }

    /// Whether the gateway is in open mode (no auth required).
    pub fn is_open_mode(&self) -> bool {
        self.valid_tokens.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_validate_valid_token() {
        let auth = GatewayAuth::new(vec!["token-1".into(), "token-2".into()]);
        assert!(auth.validate("token-1"));
        assert!(auth.validate("token-2"));
    }

    #[test]
    fn test_auth_validate_invalid_token() {
        let auth = GatewayAuth::new(vec!["token-1".into()]);
        assert!(!auth.validate("wrong-token"));
        assert!(!auth.validate(""));
    }

    #[test]
    fn test_auth_open_mode() {
        let auth = GatewayAuth::new(vec![]);
        assert!(auth.is_open_mode());
        assert!(auth.validate("anything"));
        assert!(auth.validate(""));
    }

    #[test]
    fn test_auth_from_config() {
        let config = GatewayConfig {
            auth_tokens: vec!["abc".into(), "def".into()],
            ..GatewayConfig::default()
        };
        let auth = GatewayAuth::from_config(&config);
        assert_eq!(auth.token_count(), 2);
        assert!(auth.validate("abc"));
        assert!(!auth.validate("xyz"));
    }
}
