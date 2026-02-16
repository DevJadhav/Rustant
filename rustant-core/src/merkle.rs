//! # Merkle Tree Audit Logging
//!
//! Provides a tamper-evident hash chain for audit events.  Each [`AuditNode`]
//! contains a SHA-256 hash of its own event data chained with the hash of the
//! preceding node, forming a verifiable append-only log.
//!
//! Use [`MerkleChain`] to append events and verify the integrity of the full
//! chain (or individual nodes).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single node in the Merkle audit chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditNode {
    /// 0-based sequence number in the chain.
    pub sequence: u64,
    /// SHA-256 hash of the event payload.
    pub event_hash: String,
    /// Hash of the previous node (all zeros for the genesis node).
    pub previous_hash: String,
    /// Combined chain hash: SHA-256(sequence || event_hash || previous_hash).
    pub chain_hash: String,
    /// When this node was appended.
    pub timestamp: DateTime<Utc>,
}

/// Result of verifying the integrity of a [`MerkleChain`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the entire checked range is valid.
    pub is_valid: bool,
    /// How many nodes were checked.
    pub checked_nodes: usize,
    /// Index of the first invalid node, if any.
    pub first_invalid: Option<u64>,
}

// ---------------------------------------------------------------------------
// Chain
// ---------------------------------------------------------------------------

/// An append-only Merkle hash chain for tamper-evident audit logging.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MerkleChain {
    nodes: Vec<AuditNode>,
    /// Periodic checkpoints: (sequence_number, root_hash_at_that_point).
    #[serde(default)]
    checkpoints: Vec<(u64, String)>,
    /// How often to create a checkpoint (every N appends). 0 = disabled.
    #[serde(default)]
    checkpoint_interval: u64,
}

impl MerkleChain {
    /// Create a new, empty chain.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            checkpoints: Vec::new(),
            checkpoint_interval: 0,
        }
    }

    /// Create a new chain with checkpoint interval.
    ///
    /// Every `interval` appends, a checkpoint is automatically stored recording
    /// the current sequence number and root hash. Set to 0 to disable.
    pub fn with_checkpoint_interval(interval: u64) -> Self {
        Self {
            nodes: Vec::new(),
            checkpoints: Vec::new(),
            checkpoint_interval: interval,
        }
    }

    /// Number of nodes in the chain.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Return the root (latest) chain hash, or `None` if the chain is empty.
    pub fn root_hash(&self) -> Option<&str> {
        self.nodes.last().map(|n| n.chain_hash.as_str())
    }

    /// Return a reference to all nodes.
    pub fn nodes(&self) -> &[AuditNode] {
        &self.nodes
    }

    /// Append a new event to the chain.
    ///
    /// `event_data` is an arbitrary byte payload (e.g. serialised JSON of the
    /// audit event).  The function computes the event hash, chains it with the
    /// previous node, and returns a reference to the new node.
    pub fn append(&mut self, event_data: &[u8]) -> &AuditNode {
        let sequence = self.nodes.len() as u64;
        let event_hash = hex_sha256(event_data);

        let previous_hash = self
            .nodes
            .last()
            .map(|n| n.chain_hash.clone())
            .unwrap_or_else(|| "0".repeat(64)); // genesis sentinel

        let chain_hash = compute_chain_hash(sequence, &event_hash, &previous_hash);

        self.nodes.push(AuditNode {
            sequence,
            event_hash,
            previous_hash,
            chain_hash,
            timestamp: Utc::now(),
        });

        // Auto-checkpoint if interval is configured
        if self.checkpoint_interval > 0 && (sequence + 1).is_multiple_of(self.checkpoint_interval) {
            if let Some(hash) = self.root_hash() {
                self.checkpoints.push((sequence, hash.to_string()));
            }
        }

        self.nodes.last().unwrap()
    }

    /// Verify that a single node's chain hash is consistent with its fields.
    pub fn verify_node(&self, index: usize) -> bool {
        let Some(node) = self.nodes.get(index) else {
            return false;
        };

        // Recompute expected chain hash
        let expected = compute_chain_hash(node.sequence, &node.event_hash, &node.previous_hash);
        if expected != node.chain_hash {
            return false;
        }

        // Check link to previous node
        if index == 0 {
            node.previous_hash == "0".repeat(64)
        } else {
            let prev = &self.nodes[index - 1];
            node.previous_hash == prev.chain_hash
        }
    }

    /// Verify the integrity of the entire chain, including checkpoints.
    pub fn verify_chain(&self) -> VerificationResult {
        if self.nodes.is_empty() {
            return VerificationResult {
                is_valid: true,
                checked_nodes: 0,
                first_invalid: None,
            };
        }

        for i in 0..self.nodes.len() {
            if !self.verify_node(i) {
                return VerificationResult {
                    is_valid: false,
                    checked_nodes: i + 1,
                    first_invalid: Some(i as u64),
                };
            }
        }

        // Also verify checkpoints
        if !self.verify_checkpoints() {
            return VerificationResult {
                is_valid: false,
                checked_nodes: self.nodes.len(),
                first_invalid: None, // Checkpoint failure, not a specific node
            };
        }

        VerificationResult {
            is_valid: true,
            checked_nodes: self.nodes.len(),
            first_invalid: None,
        }
    }

    /// Verify that all stored checkpoints match the current chain state.
    ///
    /// Returns `true` if all checkpoints are valid (or there are none).
    pub fn verify_checkpoints(&self) -> bool {
        for (seq, expected_hash) in &self.checkpoints {
            let idx = *seq as usize;
            if idx >= self.nodes.len() {
                return false; // Checkpoint refers to a node that doesn't exist
            }
            if self.nodes[idx].chain_hash != *expected_hash {
                return false; // Chain hash at checkpoint doesn't match
            }
        }
        true
    }

    /// Get a reference to the stored checkpoints.
    pub fn checkpoints(&self) -> &[(u64, String)] {
        &self.checkpoints
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute SHA-256 of arbitrary bytes and return hex string.
fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

/// Compute the chain hash for a node: SHA-256(sequence || event_hash || previous_hash).
fn compute_chain_hash(sequence: u64, event_hash: &str, previous_hash: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sequence.to_le_bytes());
    hasher.update(event_hash.as_bytes());
    hasher.update(previous_hash.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Construction -------------------------------------------------------

    #[test]
    fn test_new_chain_is_empty() {
        let chain = MerkleChain::new();
        assert!(chain.is_empty());
        assert_eq!(chain.len(), 0);
        assert!(chain.root_hash().is_none());
    }

    #[test]
    fn test_append_single_node() {
        let mut chain = MerkleChain::new();
        chain.append(b"event-1");

        assert_eq!(chain.len(), 1);
        assert!(!chain.is_empty());
        assert!(chain.root_hash().is_some());
        assert_eq!(chain.nodes()[0].sequence, 0);
        assert_eq!(chain.nodes()[0].previous_hash, "0".repeat(64));
    }

    #[test]
    fn test_append_multiple_nodes() {
        let mut chain = MerkleChain::new();
        chain.append(b"event-1");
        chain.append(b"event-2");
        chain.append(b"event-3");

        assert_eq!(chain.len(), 3);
        assert_eq!(chain.nodes()[1].previous_hash, chain.nodes()[0].chain_hash);
        assert_eq!(chain.nodes()[2].previous_hash, chain.nodes()[1].chain_hash);
    }

    // -- Single-node verification -------------------------------------------

    #[test]
    fn test_verify_genesis_node() {
        let mut chain = MerkleChain::new();
        chain.append(b"genesis");
        assert!(chain.verify_node(0));
    }

    #[test]
    fn test_verify_subsequent_node() {
        let mut chain = MerkleChain::new();
        chain.append(b"first");
        chain.append(b"second");
        assert!(chain.verify_node(1));
    }

    #[test]
    fn test_verify_out_of_bounds() {
        let chain = MerkleChain::new();
        assert!(!chain.verify_node(0));
    }

    // -- Full chain verification --------------------------------------------

    #[test]
    fn test_verify_empty_chain() {
        let chain = MerkleChain::new();
        let result = chain.verify_chain();
        assert!(result.is_valid);
        assert_eq!(result.checked_nodes, 0);
        assert!(result.first_invalid.is_none());
    }

    #[test]
    fn test_verify_valid_chain() {
        let mut chain = MerkleChain::new();
        for i in 0..10 {
            chain.append(format!("event-{}", i).as_bytes());
        }

        let result = chain.verify_chain();
        assert!(result.is_valid);
        assert_eq!(result.checked_nodes, 10);
        assert!(result.first_invalid.is_none());
    }

    // -- Tamper detection ---------------------------------------------------

    #[test]
    fn test_tampered_event_hash_detected() {
        let mut chain = MerkleChain::new();
        chain.append(b"honest-1");
        chain.append(b"honest-2");
        chain.append(b"honest-3");

        chain.nodes[1].event_hash = "deadbeef".repeat(8);

        let result = chain.verify_chain();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid, Some(1));
    }

    #[test]
    fn test_tampered_chain_hash_detected() {
        let mut chain = MerkleChain::new();
        chain.append(b"a");
        chain.append(b"b");
        chain.append(b"c");

        chain.nodes[0].chain_hash = "badc0ffee".to_string() + &"0".repeat(55);

        let result = chain.verify_chain();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid, Some(0));
    }

    #[test]
    fn test_tampered_previous_hash_detected() {
        let mut chain = MerkleChain::new();
        chain.append(b"x");
        chain.append(b"y");

        chain.nodes[1].previous_hash = "0".repeat(64);

        let result = chain.verify_chain();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid, Some(1));
    }

    #[test]
    fn test_tampered_sequence_detected() {
        let mut chain = MerkleChain::new();
        chain.append(b"first");
        chain.append(b"second");

        chain.nodes[1].sequence = 99;

        let result = chain.verify_chain();
        assert!(!result.is_valid);
        assert_eq!(result.first_invalid, Some(1));
    }

    // -- Serialization ------------------------------------------------------

    #[test]
    fn test_serialization_roundtrip() {
        let mut chain = MerkleChain::new();
        chain.append(b"alpha");
        chain.append(b"beta");

        let json = serde_json::to_string(&chain).expect("serialize");
        let restored: MerkleChain = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.len(), 2);
        assert!(restored.verify_chain().is_valid);
        assert_eq!(chain.root_hash(), restored.root_hash());
    }

    #[test]
    fn test_node_serialization() {
        let mut chain = MerkleChain::new();
        chain.append(b"test");
        let node = &chain.nodes()[0];

        let json = serde_json::to_string(node).expect("serialize");
        let restored: AuditNode = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.sequence, node.sequence);
        assert_eq!(restored.chain_hash, node.chain_hash);
    }

    // -- Determinism --------------------------------------------------------

    #[test]
    fn test_same_data_same_event_hash() {
        let mut chain1 = MerkleChain::new();
        let mut chain2 = MerkleChain::new();

        let node1 = chain1.append(b"identical");
        let node2 = chain2.append(b"identical");

        assert_eq!(node1.event_hash, node2.event_hash);
    }

    #[test]
    fn test_different_data_different_event_hash() {
        let mut chain = MerkleChain::new();
        let n1 = chain.append(b"alpha").event_hash.clone();
        let n2 = chain.append(b"beta").event_hash.clone();
        assert_ne!(n1, n2);
    }

    // -- Root hash ----------------------------------------------------------

    #[test]
    fn test_root_hash_changes_on_append() {
        let mut chain = MerkleChain::new();
        chain.append(b"first");
        let root1 = chain.root_hash().unwrap().to_owned();
        chain.append(b"second");
        let root2 = chain.root_hash().unwrap().to_owned();
        assert_ne!(root1, root2);
    }

    // -- Checkpoint tests ---------------------------------------------------

    #[test]
    fn test_checkpoint_creation() {
        let mut chain = MerkleChain::with_checkpoint_interval(5);
        for i in 0..10 {
            chain.append(format!("event-{}", i).as_bytes());
        }
        // Checkpoints at sequence 4 and 9
        assert_eq!(chain.checkpoints().len(), 2);
        assert_eq!(chain.checkpoints()[0].0, 4);
        assert_eq!(chain.checkpoints()[1].0, 9);
    }

    #[test]
    fn test_checkpoint_verification_valid() {
        let mut chain = MerkleChain::with_checkpoint_interval(3);
        for i in 0..9 {
            chain.append(format!("event-{}", i).as_bytes());
        }
        assert!(chain.verify_checkpoints());
        assert!(chain.verify_chain().is_valid);
    }

    #[test]
    fn test_checkpoint_verification_detects_tampering() {
        let mut chain = MerkleChain::with_checkpoint_interval(3);
        for i in 0..6 {
            chain.append(format!("event-{}", i).as_bytes());
        }
        assert_eq!(chain.checkpoints().len(), 2);

        // Tamper with a node that has a checkpoint
        chain.nodes[2].chain_hash = "tampered".to_string();

        // verify_chain detects it via node verification
        let result = chain.verify_chain();
        assert!(!result.is_valid);
    }

    #[test]
    fn test_no_checkpoints_when_disabled() {
        let mut chain = MerkleChain::new();
        for i in 0..100 {
            chain.append(format!("event-{}", i).as_bytes());
        }
        assert!(chain.checkpoints().is_empty());
    }

    #[test]
    fn test_checkpoint_serialization_roundtrip() {
        let mut chain = MerkleChain::with_checkpoint_interval(2);
        for i in 0..4 {
            chain.append(format!("event-{}", i).as_bytes());
        }
        assert_eq!(chain.checkpoints().len(), 2);

        let json = serde_json::to_string(&chain).expect("serialize");
        let restored: MerkleChain = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.checkpoints().len(), 2);
        assert!(restored.verify_chain().is_valid);
    }
}
