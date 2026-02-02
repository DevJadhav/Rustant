//! Consent store — per-capability consent management per node.
//!
//! Supports permanent, time-limited, and one-time consent entries.

use super::types::{Capability, NodeId};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// A single consent entry for a capability.
#[derive(Debug, Clone)]
pub struct ConsentEntry {
    pub capability: Capability,
    pub granted_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub one_time: bool,
    pub used: bool,
}

impl ConsentEntry {
    /// Whether this entry is currently valid (not expired and not consumed).
    pub fn is_valid(&self) -> bool {
        if self.one_time && self.used {
            return false;
        }
        if let Some(expires) = self.expires_at {
            Utc::now() < expires
        } else {
            true
        }
    }
}

/// Stores granted/revoked consent per-node per-capability.
#[derive(Debug, Clone, Default)]
pub struct ConsentStore {
    entries: HashMap<NodeId, Vec<ConsentEntry>>,
}

impl ConsentStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant permanent consent for a capability on a node.
    pub fn grant(&mut self, node_id: &NodeId, capability: Capability) {
        let entry = ConsentEntry {
            capability,
            granted_at: Utc::now(),
            expires_at: None,
            one_time: false,
            used: false,
        };
        self.entries.entry(node_id.clone()).or_default().push(entry);
    }

    /// Grant consent with a time-based expiry.
    pub fn grant_with_expiry(
        &mut self,
        node_id: &NodeId,
        capability: Capability,
        duration: Duration,
    ) -> ConsentEntry {
        let now = Utc::now();
        let entry = ConsentEntry {
            capability,
            granted_at: now,
            expires_at: Some(now + duration),
            one_time: false,
            used: false,
        };
        self.entries
            .entry(node_id.clone())
            .or_default()
            .push(entry.clone());
        entry
    }

    /// Grant a one-time consent that can only be used once.
    pub fn grant_one_time(&mut self, node_id: &NodeId, capability: Capability) -> ConsentEntry {
        let entry = ConsentEntry {
            capability,
            granted_at: Utc::now(),
            expires_at: None,
            one_time: true,
            used: false,
        };
        self.entries
            .entry(node_id.clone())
            .or_default()
            .push(entry.clone());
        entry
    }

    /// Revoke consent for a capability on a node (removes all matching entries).
    pub fn revoke(&mut self, node_id: &NodeId, capability: &Capability) {
        if let Some(entries) = self.entries.get_mut(node_id) {
            entries.retain(|e| &e.capability != capability);
            if entries.is_empty() {
                self.entries.remove(node_id);
            }
        }
    }

    /// Check whether a capability is consented for a node.
    /// Returns true if any valid entry exists for this capability.
    pub fn is_granted(&self, node_id: &NodeId, capability: &Capability) -> bool {
        self.entries.get(node_id).is_some_and(|entries| {
            entries
                .iter()
                .any(|e| &e.capability == capability && e.is_valid())
        })
    }

    /// Consume a one-time consent. Returns true if successfully consumed,
    /// false if no one-time entry was available.
    pub fn consume_one_time(&mut self, node_id: &NodeId, capability: &Capability) -> bool {
        if let Some(entries) = self.entries.get_mut(node_id) {
            for entry in entries.iter_mut() {
                if &entry.capability == capability
                    && entry.one_time
                    && !entry.used
                    && entry.is_valid()
                {
                    entry.used = true;
                    return true;
                }
            }
        }
        false
    }

    /// List all granted capabilities for a node (only valid entries).
    pub fn granted_capabilities(&self, node_id: &NodeId) -> Vec<&Capability> {
        self.entries
            .get(node_id)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|e| e.is_valid())
                    .map(|e| &e.capability)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// List all consent entries for a node (including expired/consumed).
    pub fn list_grants(&self, node_id: &NodeId) -> Vec<&ConsentEntry> {
        self.entries
            .get(node_id)
            .map(|entries| entries.iter().collect())
            .unwrap_or_default()
    }

    /// Revoke all consent for a node.
    pub fn revoke_all(&mut self, node_id: &NodeId) {
        self.entries.remove(node_id);
    }

    /// Remove all expired and consumed one-time entries. Returns count removed.
    pub fn cleanup_expired(&mut self) -> usize {
        let mut removed = 0;
        for entries in self.entries.values_mut() {
            let before = entries.len();
            entries.retain(|e| e.is_valid());
            removed += before - entries.len();
        }
        // Remove empty node entries
        self.entries.retain(|_, v| !v.is_empty());
        removed
    }

    /// Number of nodes with any consent granted.
    pub fn node_count(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_grant_and_check() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        assert!(!store.is_granted(&node, &Capability::Shell));

        store.grant(&node, Capability::Shell);
        assert!(store.is_granted(&node, &Capability::Shell));
        assert!(!store.is_granted(&node, &Capability::FileSystem));
    }

    #[test]
    fn test_consent_revoke() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant(&node, Capability::Shell);
        store.grant(&node, Capability::FileSystem);
        assert!(store.is_granted(&node, &Capability::Shell));

        store.revoke(&node, &Capability::Shell);
        assert!(!store.is_granted(&node, &Capability::Shell));
        assert!(store.is_granted(&node, &Capability::FileSystem));
    }

    #[test]
    fn test_consent_revoke_all() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant(&node, Capability::Shell);
        store.grant(&node, Capability::Screenshot);
        assert_eq!(store.node_count(), 1);

        store.revoke_all(&node);
        assert!(!store.is_granted(&node, &Capability::Shell));
        assert_eq!(store.node_count(), 0);
    }

    #[test]
    fn test_consent_granted_capabilities() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant(&node, Capability::Shell);
        store.grant(&node, Capability::Clipboard);
        let caps = store.granted_capabilities(&node);
        assert_eq!(caps.len(), 2);
    }

    #[test]
    fn test_consent_multiple_nodes() {
        let mut store = ConsentStore::new();
        let n1 = NodeId::new("node-1");
        let n2 = NodeId::new("node-2");

        store.grant(&n1, Capability::Shell);
        store.grant(&n2, Capability::FileSystem);

        assert!(store.is_granted(&n1, &Capability::Shell));
        assert!(!store.is_granted(&n1, &Capability::FileSystem));
        assert!(store.is_granted(&n2, &Capability::FileSystem));
        assert_eq!(store.node_count(), 2);
    }

    // --- New enrichment tests ---

    #[test]
    fn test_consent_grant_with_expiry() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        let entry = store.grant_with_expiry(&node, Capability::Shell, Duration::hours(1));
        assert!(entry.expires_at.is_some());
        assert!(!entry.one_time);
        assert!(store.is_granted(&node, &Capability::Shell));
    }

    #[test]
    fn test_consent_expired_denied() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        // Grant with negative duration (already expired)
        store.grant_with_expiry(&node, Capability::Shell, Duration::seconds(-1));
        assert!(!store.is_granted(&node, &Capability::Shell));
    }

    #[test]
    fn test_consent_one_time_grant() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        let entry = store.grant_one_time(&node, Capability::Shell);
        assert!(entry.one_time);
        assert!(!entry.used);
        assert!(store.is_granted(&node, &Capability::Shell));
    }

    #[test]
    fn test_consent_one_time_consumed() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant_one_time(&node, Capability::Shell);
        assert!(store.consume_one_time(&node, &Capability::Shell));
    }

    #[test]
    fn test_consent_one_time_reuse_denied() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant_one_time(&node, Capability::Shell);
        assert!(store.consume_one_time(&node, &Capability::Shell));
        // After consumption, it should no longer be granted
        assert!(!store.is_granted(&node, &Capability::Shell));
        // Second consume attempt fails
        assert!(!store.consume_one_time(&node, &Capability::Shell));
    }

    #[test]
    fn test_consent_cleanup_expired() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        // Add an already-expired entry
        store.grant_with_expiry(&node, Capability::Shell, Duration::seconds(-1));
        // Add a valid entry
        store.grant(&node, Capability::FileSystem);

        let removed = store.cleanup_expired();
        assert_eq!(removed, 1);
        assert!(!store.is_granted(&node, &Capability::Shell));
        assert!(store.is_granted(&node, &Capability::FileSystem));
    }

    #[test]
    fn test_consent_list_grants() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant(&node, Capability::Shell);
        store.grant_one_time(&node, Capability::FileSystem);
        store.grant_with_expiry(&node, Capability::Screenshot, Duration::hours(1));

        let grants = store.list_grants(&node);
        assert_eq!(grants.len(), 3);
    }

    #[test]
    fn test_consent_revoke_still_works() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        store.grant(&node, Capability::Shell);
        store.grant_one_time(&node, Capability::Shell);
        assert!(store.is_granted(&node, &Capability::Shell));

        // Revoke removes ALL entries for that capability
        store.revoke(&node, &Capability::Shell);
        assert!(!store.is_granted(&node, &Capability::Shell));
        assert!(store.list_grants(&node).is_empty());
    }

    #[test]
    fn test_consent_multiple_nodes_isolation() {
        let mut store = ConsentStore::new();
        let n1 = NodeId::new("node-1");
        let n2 = NodeId::new("node-2");

        store.grant_one_time(&n1, Capability::Shell);
        store.grant(&n2, Capability::Shell);

        // Consuming n1's one-time doesn't affect n2
        store.consume_one_time(&n1, &Capability::Shell);
        assert!(!store.is_granted(&n1, &Capability::Shell));
        assert!(store.is_granted(&n2, &Capability::Shell));
    }

    #[test]
    fn test_consent_mixed_entries() {
        let mut store = ConsentStore::new();
        let node = NodeId::new("node-1");

        // One permanent, one one-time, one expired
        store.grant(&node, Capability::Shell);
        store.grant_one_time(&node, Capability::FileSystem);
        store.grant_with_expiry(&node, Capability::Screenshot, Duration::seconds(-1));

        // Shell: valid (permanent), FileSystem: valid (one-time unused), Screenshot: invalid (expired)
        let valid = store.granted_capabilities(&node);
        assert_eq!(valid.len(), 2);

        // All entries still stored (including expired)
        let all = store.list_grants(&node);
        assert_eq!(all.len(), 3);

        // Cleanup removes expired
        let removed = store.cleanup_expired();
        assert_eq!(removed, 1);
        assert_eq!(store.list_grants(&node).len(), 2);
    }
}
