//! User consent framework for data handling transparency.
//!
//! Manages consent records per scope (provider, storage, memory, tools,
//! channels). Supports explicit grant, revoke, TTL-based expiry, and
//! persistence. Part of the Transparency pillar.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Scope for which consent is tracked.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConsentScope {
    /// Consent to send data to a specific LLM provider.
    Provider { provider: String },
    /// Consent to store data locally.
    LocalStorage,
    /// Consent to retain data in memory across sessions.
    MemoryRetention,
    /// Consent to use a specific tool.
    ToolAccess { tool: String },
    /// Consent to interact with a specific channel.
    ChannelAccess { channel: String },
    /// Global consent (applies to all operations).
    Global,
}

impl std::fmt::Display for ConsentScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsentScope::Provider { provider } => write!(f, "provider:{provider}"),
            ConsentScope::LocalStorage => write!(f, "local_storage"),
            ConsentScope::MemoryRetention => write!(f, "memory_retention"),
            ConsentScope::ToolAccess { tool } => write!(f, "tool:{tool}"),
            ConsentScope::ChannelAccess { channel } => write!(f, "channel:{channel}"),
            ConsentScope::Global => write!(f, "global"),
        }
    }
}

impl ConsentScope {
    /// Parse a scope from a string like "provider:anthropic" or "global".
    pub fn parse(s: &str) -> Option<Self> {
        if s == "global" {
            return Some(ConsentScope::Global);
        }
        if s == "local_storage" {
            return Some(ConsentScope::LocalStorage);
        }
        if s == "memory_retention" {
            return Some(ConsentScope::MemoryRetention);
        }
        if let Some(provider) = s.strip_prefix("provider:") {
            return Some(ConsentScope::Provider {
                provider: provider.to_string(),
            });
        }
        if let Some(tool) = s.strip_prefix("tool:") {
            return Some(ConsentScope::ToolAccess {
                tool: tool.to_string(),
            });
        }
        if let Some(channel) = s.strip_prefix("channel:") {
            return Some(ConsentScope::ChannelAccess {
                channel: channel.to_string(),
            });
        }
        None
    }
}

/// A recorded consent decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// The scope this consent applies to.
    pub scope: ConsentScope,
    /// Whether consent was granted.
    pub granted: bool,
    /// When consent was granted or denied.
    pub granted_at: DateTime<Utc>,
    /// When this consent expires (None = indefinite).
    pub expires_at: Option<DateTime<Utc>>,
    /// Reason for the consent decision.
    pub reason: String,
}

impl ConsentRecord {
    /// Check if this consent is still valid (not expired).
    pub fn is_valid(&self) -> bool {
        self.granted && self.expires_at.map(|exp| Utc::now() < exp).unwrap_or(true)
    }
}

/// Default behavior when no consent record exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DefaultConsentPolicy {
    /// Return false when no record exists (privacy-first, opt-in).
    #[default]
    RequireExplicit,
    /// Return true when no record exists (backward compatible).
    ImpliedGrant,
}

/// Manages consent records for all scopes.
pub struct ConsentManager {
    records: HashMap<ConsentScope, ConsentRecord>,
    persist_path: Option<PathBuf>,
    /// What to return when no consent record exists for a scope.
    pub default_policy: DefaultConsentPolicy,
}

impl ConsentManager {
    /// Create a new consent manager with RequireExplicit default (privacy-first).
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            persist_path: None,
            default_policy: DefaultConsentPolicy::RequireExplicit,
        }
    }

    /// Create a consent manager with implied grant policy (backward compatible).
    pub fn with_implied_grant() -> Self {
        Self {
            records: HashMap::new(),
            persist_path: None,
            default_policy: DefaultConsentPolicy::ImpliedGrant,
        }
    }

    /// Create a consent manager with persistence.
    pub fn with_persistence(path: PathBuf) -> Self {
        let mut mgr = Self::new();
        mgr.persist_path = Some(path);
        mgr
    }

    /// Load persisted consent records from disk.
    pub fn load(&mut self) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.persist_path {
            if path.exists() {
                let data = std::fs::read_to_string(path)?;
                let records: Vec<ConsentRecord> = serde_json::from_str(&data)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                for record in records {
                    self.records.insert(record.scope.clone(), record);
                }
            }
        }
        Ok(())
    }

    /// Check if consent is granted for a scope.
    ///
    /// When no consent record exists, returns based on `default_policy`:
    /// - `RequireExplicit` → false (privacy-first, opt-in)
    /// - `ImpliedGrant` → true (backward compatible)
    pub fn check(&self, scope: &ConsentScope) -> bool {
        // Check specific scope first
        if let Some(record) = self.records.get(scope) {
            return record.is_valid();
        }
        // Fall back to global consent
        if let Some(global) = self.records.get(&ConsentScope::Global) {
            return global.is_valid();
        }
        // No consent recorded — apply default policy
        matches!(self.default_policy, DefaultConsentPolicy::ImpliedGrant)
    }

    /// Grant consent for a scope.
    pub fn grant(
        &mut self,
        scope: ConsentScope,
        reason: impl Into<String>,
        ttl_hours: Option<u64>,
    ) {
        let expires_at = ttl_hours.and_then(|h| {
            if h > 0 {
                Some(Utc::now() + Duration::hours(h as i64))
            } else {
                None
            }
        });

        let record = ConsentRecord {
            scope: scope.clone(),
            granted: true,
            granted_at: Utc::now(),
            expires_at,
            reason: reason.into(),
        };

        self.records.insert(scope, record);
    }

    /// Revoke consent for a scope.
    pub fn revoke(&mut self, scope: &ConsentScope, reason: impl Into<String>) {
        let record = ConsentRecord {
            scope: scope.clone(),
            granted: false,
            granted_at: Utc::now(),
            expires_at: None,
            reason: reason.into(),
        };
        self.records.insert(scope.clone(), record);
    }

    /// List all active (granted and valid) consent records.
    pub fn list_active(&self) -> Vec<&ConsentRecord> {
        self.records.values().filter(|r| r.is_valid()).collect()
    }

    /// List all consent records (including expired/revoked).
    pub fn list_all(&self) -> Vec<&ConsentRecord> {
        self.records.values().collect()
    }

    /// Persist consent records to disk.
    pub fn persist(&self) -> Result<(), std::io::Error> {
        if let Some(ref path) = self.persist_path {
            let records: Vec<&ConsentRecord> = self.records.values().collect();
            let data = serde_json::to_string_pretty(&records).map_err(std::io::Error::other)?;
            crate::persistence::atomic_write(path, data.as_bytes())?;
        }
        Ok(())
    }

    /// Format consent status for `/consent status`.
    pub fn format_status(&self) -> String {
        let active = self.list_active();
        if active.is_empty() {
            return "No active consent records.\n\nUse `/consent grant <scope>` to grant consent.\nScopes: global, provider:<name>, local_storage, memory_retention, tool:<name>, channel:<name>".to_string();
        }

        let mut output = format!("Active consent records ({}):\n\n", active.len());
        for record in &active {
            let expiry = match record.expires_at {
                Some(exp) => format!("expires {}", exp.format("%Y-%m-%d %H:%M")),
                None => "indefinite".to_string(),
            };
            output.push_str(&format!(
                "  {} — granted {} ({})\n    Reason: {}\n",
                record.scope,
                record.granted_at.format("%Y-%m-%d %H:%M"),
                expiry,
                record.reason,
            ));
        }
        output
    }
}

impl Default for ConsentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_and_check() {
        let mut mgr = ConsentManager::new();

        let scope = ConsentScope::Provider {
            provider: "anthropic".into(),
        };
        mgr.grant(scope.clone(), "User approved", None);

        assert!(mgr.check(&scope));
    }

    #[test]
    fn test_revoke() {
        let mut mgr = ConsentManager::new();

        let scope = ConsentScope::Global;
        mgr.grant(scope.clone(), "Initial setup", None);
        assert!(mgr.check(&scope));

        mgr.revoke(&scope, "User requested removal");
        assert!(!mgr.check(&scope));
    }

    #[test]
    fn test_ttl_expiry() {
        let mut mgr = ConsentManager::new();

        let scope = ConsentScope::LocalStorage;
        // Grant with 0-hour TTL (effectively indefinite)
        mgr.grant(scope.clone(), "Test", Some(0));
        assert!(mgr.check(&scope));

        // Grant with far-future TTL
        mgr.grant(scope.clone(), "Test", Some(8760));
        assert!(mgr.check(&scope));
    }

    #[test]
    fn test_global_fallback() {
        let mut mgr = ConsentManager::with_implied_grant();

        // No specific consent — ImpliedGrant policy returns true
        let scope = ConsentScope::Provider {
            provider: "openai".into(),
        };
        assert!(mgr.check(&scope));

        // Grant global consent
        mgr.grant(ConsentScope::Global, "Blanket approval", None);
        assert!(mgr.check(&scope));

        // Revoke global — specific scope falls through to denied global
        mgr.revoke(&ConsentScope::Global, "Revoked");
        assert!(!mgr.check(&scope));
    }

    #[test]
    fn test_require_explicit_policy() {
        let mgr = ConsentManager::new(); // default = RequireExplicit

        // No consent recorded — should return false
        let scope = ConsentScope::Provider {
            provider: "openai".into(),
        };
        assert!(!mgr.check(&scope));
    }

    #[test]
    fn test_implied_grant_policy() {
        let mgr = ConsentManager::with_implied_grant();

        // No consent recorded — ImpliedGrant returns true
        let scope = ConsentScope::Provider {
            provider: "openai".into(),
        };
        assert!(mgr.check(&scope));
    }

    #[test]
    fn test_scope_parse() {
        assert_eq!(ConsentScope::parse("global"), Some(ConsentScope::Global));
        assert_eq!(
            ConsentScope::parse("provider:anthropic"),
            Some(ConsentScope::Provider {
                provider: "anthropic".into()
            })
        );
        assert_eq!(
            ConsentScope::parse("tool:file_read"),
            Some(ConsentScope::ToolAccess {
                tool: "file_read".into()
            })
        );
        assert!(ConsentScope::parse("invalid").is_none());
    }

    #[test]
    fn test_scope_display() {
        assert_eq!(ConsentScope::Global.to_string(), "global");
        assert_eq!(
            ConsentScope::Provider {
                provider: "anthropic".into()
            }
            .to_string(),
            "provider:anthropic"
        );
    }

    #[test]
    fn test_list_active() {
        let mut mgr = ConsentManager::new();
        mgr.grant(ConsentScope::Global, "Test", None);
        mgr.grant(
            ConsentScope::Provider {
                provider: "openai".into(),
            },
            "Test",
            None,
        );
        mgr.revoke(&ConsentScope::LocalStorage, "Denied");

        let active = mgr.list_active();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_format_status() {
        let mut mgr = ConsentManager::new();
        let status = mgr.format_status();
        assert!(status.contains("No active consent"));

        mgr.grant(ConsentScope::Global, "User consented", None);
        let status = mgr.format_status();
        assert!(status.contains("global"));
        assert!(status.contains("User consented"));
    }
}
