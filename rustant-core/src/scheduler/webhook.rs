//! Webhook endpoint — receives HTTP callbacks, verifies HMAC signatures,
//! and dispatches handlers.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::error::SchedulerError;

type HmacSha256 = Hmac<Sha256>;

/// Configuration for a webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEndpoint {
    /// URL path to listen on (e.g., "/webhooks/github").
    pub path: String,
    /// Optional HMAC-SHA256 secret for signature verification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Handlers that can process incoming webhook events.
    pub handlers: Vec<WebhookHandler>,
}

/// A handler that maps event types to actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookHandler {
    /// The event type to match (e.g., "push", "pull_request").
    /// If None, matches all events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    /// The action to execute when matched.
    pub action: String,
}

/// An incoming webhook request.
#[derive(Debug, Clone)]
pub struct WebhookRequest {
    /// The HTTP path.
    pub path: String,
    /// The raw body bytes.
    pub body: Vec<u8>,
    /// The signature header value (e.g., "sha256=abc123...").
    pub signature: Option<String>,
    /// The event type header value.
    pub event_type: Option<String>,
}

/// Result of processing a webhook.
#[derive(Debug, Clone)]
pub struct WebhookResult {
    /// Actions to execute.
    pub actions: Vec<String>,
    /// Whether the request was verified.
    pub verified: bool,
}

impl WebhookEndpoint {
    /// Create a new webhook endpoint.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            secret: None,
            handlers: Vec::new(),
        }
    }

    /// Set the HMAC secret.
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        self.secret = Some(secret.into());
        self
    }

    /// Add a handler.
    pub fn with_handler(mut self, handler: WebhookHandler) -> Self {
        self.handlers.push(handler);
        self
    }

    /// Verify the HMAC-SHA256 signature of a request.
    pub fn verify_signature(
        &self,
        body: &[u8],
        signature: Option<&str>,
    ) -> Result<bool, SchedulerError> {
        match &self.secret {
            None => Ok(true), // No secret configured, skip verification
            Some(secret) => {
                let sig = signature.ok_or_else(|| SchedulerError::WebhookVerificationFailed {
                    message: "Missing signature header".to_string(),
                })?;

                // Parse "sha256=<hex>" format
                let hex_sig = sig.strip_prefix("sha256=").unwrap_or(sig);

                let expected_bytes = hex::decode(hex_sig).map_err(|e| {
                    SchedulerError::WebhookVerificationFailed {
                        message: format!("Invalid hex signature: {}", e),
                    }
                })?;

                let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|e| {
                    SchedulerError::WebhookVerificationFailed {
                        message: format!("HMAC error: {}", e),
                    }
                })?;
                mac.update(body);

                match mac.verify_slice(&expected_bytes) {
                    Ok(_) => Ok(true),
                    Err(_) => Ok(false),
                }
            }
        }
    }

    /// Process a webhook request: verify + match handlers.
    pub fn process(&self, request: &WebhookRequest) -> Result<WebhookResult, SchedulerError> {
        let verified = self.verify_signature(&request.body, request.signature.as_deref())?;

        if !verified {
            return Err(SchedulerError::WebhookVerificationFailed {
                message: "Signature mismatch".to_string(),
            });
        }

        let actions: Vec<String> = self
            .handlers
            .iter()
            .filter(|h| match (&h.event_type, &request.event_type) {
                (None, _) => true, // Handler matches all events
                (Some(expected), Some(actual)) => expected == actual,
                (Some(_), None) => false, // Handler expects event type but none given
            })
            .map(|h| h.action.clone())
            .collect();

        Ok(WebhookResult { actions, verified })
    }
}

/// Compute HMAC-SHA256 signature for a body, returning hex-encoded string.
pub fn compute_hmac_signature(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key length");
    mac.update(body);
    let result = mac.finalize();
    hex::encode(&result.into_bytes())
}

/// Simple hex encoding (no external crate needed beyond what we have).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if !s.len().is_multiple_of(2) {
            return Err("Odd-length hex string".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex at position {}: {}", i, e))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_endpoint_with_secret() -> WebhookEndpoint {
        WebhookEndpoint::new("/webhooks/test")
            .with_secret("my-secret-key")
            .with_handler(WebhookHandler {
                event_type: Some("push".to_string()),
                action: "run tests".to_string(),
            })
            .with_handler(WebhookHandler {
                event_type: Some("pull_request".to_string()),
                action: "run review".to_string(),
            })
            .with_handler(WebhookHandler {
                event_type: None,
                action: "log event".to_string(),
            })
    }

    #[test]
    fn test_webhook_hmac_verification_valid() {
        let endpoint = make_endpoint_with_secret();
        let body = b"test body content";
        let sig = compute_hmac_signature("my-secret-key", body);
        let result = endpoint
            .verify_signature(body, Some(&format!("sha256={}", sig)))
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_webhook_hmac_verification_invalid() {
        let endpoint = make_endpoint_with_secret();
        let body = b"test body content";
        let result = endpoint
            .verify_signature(
                body,
                Some("sha256=0000000000000000000000000000000000000000000000000000000000000000"),
            )
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_webhook_hmac_no_secret_passes() {
        let endpoint = WebhookEndpoint::new("/webhooks/open");
        let result = endpoint.verify_signature(b"anything", None).unwrap();
        assert!(result);
    }

    #[test]
    fn test_webhook_hmac_missing_signature_errors() {
        let endpoint = make_endpoint_with_secret();
        let result = endpoint.verify_signature(b"body", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_webhook_event_type_filter() {
        let endpoint = make_endpoint_with_secret();
        let body = b"push event";
        let sig = compute_hmac_signature("my-secret-key", body);

        let request = WebhookRequest {
            path: "/webhooks/test".to_string(),
            body: body.to_vec(),
            signature: Some(format!("sha256={}", sig)),
            event_type: Some("push".to_string()),
        };

        let result = endpoint.process(&request).unwrap();
        assert!(result.verified);
        // Should match: "push" handler + catch-all handler
        assert_eq!(result.actions.len(), 2);
        assert!(result.actions.contains(&"run tests".to_string()));
        assert!(result.actions.contains(&"log event".to_string()));
    }

    #[test]
    fn test_webhook_handler_extracts_action() {
        let endpoint = make_endpoint_with_secret();
        let body = b"pr event";
        let sig = compute_hmac_signature("my-secret-key", body);

        let request = WebhookRequest {
            path: "/webhooks/test".to_string(),
            body: body.to_vec(),
            signature: Some(format!("sha256={}", sig)),
            event_type: Some("pull_request".to_string()),
        };

        let result = endpoint.process(&request).unwrap();
        assert!(result.actions.contains(&"run review".to_string()));
        assert!(result.actions.contains(&"log event".to_string()));
        assert!(!result.actions.contains(&"run tests".to_string()));
    }

    #[test]
    fn test_webhook_endpoint_config_serde() {
        let endpoint = make_endpoint_with_secret();
        let json = serde_json::to_string(&endpoint).unwrap();
        let deserialized: WebhookEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.path, "/webhooks/test");
        assert_eq!(deserialized.secret, Some("my-secret-key".to_string()));
        assert_eq!(deserialized.handlers.len(), 3);
    }

    #[test]
    fn test_webhook_no_matching_event() {
        let endpoint = WebhookEndpoint::new("/hooks").with_handler(WebhookHandler {
            event_type: Some("push".to_string()),
            action: "deploy".to_string(),
        });

        let request = WebhookRequest {
            path: "/hooks".to_string(),
            body: b"data".to_vec(),
            signature: None,
            event_type: Some("issue".to_string()),
        };

        let result = endpoint.process(&request).unwrap();
        assert!(result.actions.is_empty());
    }

    #[test]
    fn test_hex_roundtrip() {
        let original = b"hello world";
        let encoded = hex::encode(original);
        let decoded = hex::decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }
}
