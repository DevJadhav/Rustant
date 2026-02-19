//! Model provenance tracking for security.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Provenance record for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProvenance {
    pub training_experiment_id: Option<String>,
    pub parent_model_id: Option<String>,
    pub dataset_ids: Vec<String>,
    pub hash_chain: Vec<ProvenanceHash>,
    pub signed_by: Option<String>,
    pub verified: bool,
    pub created_at: DateTime<Utc>,
}

impl Default for ModelProvenance {
    fn default() -> Self {
        Self {
            training_experiment_id: None,
            parent_model_id: None,
            dataset_ids: Vec::new(),
            hash_chain: Vec::new(),
            signed_by: None,
            verified: false,
            created_at: Utc::now(),
        }
    }
}

/// A hash in the provenance chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceHash {
    pub hash: String,
    pub description: String,
    pub timestamp: DateTime<Utc>,
}

impl ModelProvenance {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_hash(&mut self, data: &[u8], description: &str) {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());
        self.hash_chain.push(ProvenanceHash {
            hash,
            description: description.to_string(),
            timestamp: Utc::now(),
        });
    }

    pub fn verify_chain(&self) -> bool {
        !self.hash_chain.is_empty()
    }
}
