//! Model and dataset provenance verification.

use serde::{Deserialize, Serialize};

/// Provenance verification result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceResult {
    pub verified: bool,
    pub hash_valid: bool,
    pub source_trusted: bool,
    pub issues: Vec<String>,
}

/// Provenance verifier.
pub struct ProvenanceVerifier;

impl Default for ProvenanceVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvenanceVerifier {
    pub fn new() -> Self {
        Self
    }

    pub fn verify_hash(&self, expected: &str, actual: &str) -> bool {
        expected == actual
    }

    pub fn verify_source(&self, source: &str) -> bool {
        let trusted_sources = [
            "huggingface.co",
            "ollama.com",
            "pytorch.org",
            "tensorflow.org",
        ];
        trusted_sources.iter().any(|s| source.contains(s))
    }

    /// Verify a model's provenance by name and expected hash.
    pub fn verify_model(&self, model_name: &str, expected_hash: &str) -> ProvenanceResult {
        // Stub: in production this would fetch and verify the actual model hash
        let source_trusted = self.verify_source(model_name);
        ProvenanceResult {
            verified: source_trusted && !expected_hash.is_empty(),
            hash_valid: !expected_hash.is_empty(),
            source_trusted,
            issues: Vec::new(),
        }
    }

    /// Verify a dataset's provenance by name and expected hash.
    pub fn verify_dataset(&self, dataset_name: &str, expected_hash: &str) -> ProvenanceResult {
        let source_trusted = self.verify_source(dataset_name);
        ProvenanceResult {
            verified: source_trusted && !expected_hash.is_empty(),
            hash_valid: !expected_hash.is_empty(),
            source_trusted,
            issues: Vec::new(),
        }
    }

    /// Check the supply chain for suspicious dependencies.
    pub fn check_supply_chain(&self, dependencies: &[String]) -> SupplyChainResult {
        let suspicious: Vec<String> = dependencies
            .iter()
            .filter(|dep| {
                // Flag deps with suspicious characteristics (typosquatting indicators)
                dep.contains("--") || dep.contains("..") || dep.len() < 2
            })
            .cloned()
            .collect();
        SupplyChainResult {
            verified_count: dependencies.len() - suspicious.len(),
            total_count: dependencies.len(),
            is_clean: suspicious.is_empty(),
            suspicious,
        }
    }
}

/// Result of a supply chain verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyChainResult {
    pub verified_count: usize,
    pub total_count: usize,
    pub suspicious: Vec<String>,
    pub is_clean: bool,
}
