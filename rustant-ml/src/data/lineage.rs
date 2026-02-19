//! Data lineage tracking for transparency.

use crate::data::transform::TransformRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Full lineage record for a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLineage {
    pub dataset_id: String,
    pub source_type: String,
    pub source_location: String,
    pub transforms_applied: Vec<TransformRecord>,
    pub created_at: DateTime<Utc>,
    pub hash_chain: Vec<String>,
}

impl DataLineage {
    pub fn new(dataset_id: &str, source_type: &str, source_location: &str) -> Self {
        let initial_hash = compute_hash(&format!("{dataset_id}:{source_type}:{source_location}"));
        Self {
            dataset_id: dataset_id.to_string(),
            source_type: source_type.to_string(),
            source_location: source_location.to_string(),
            transforms_applied: Vec::new(),
            created_at: Utc::now(),
            hash_chain: vec![initial_hash],
        }
    }

    /// Add a transform record and extend the hash chain.
    pub fn add_transform(&mut self, record: TransformRecord) {
        let prev_hash = self.hash_chain.last().cloned().unwrap_or_default();
        let step_json = serde_json::to_string(&record.step).unwrap_or_default();
        let new_hash = compute_hash(&format!("{prev_hash}:{step_json}"));
        self.hash_chain.push(new_hash);
        self.transforms_applied.push(record);
    }

    /// Verify the integrity of the hash chain.
    pub fn verify_integrity(&self) -> bool {
        if self.hash_chain.is_empty() {
            return false;
        }

        let expected_initial = compute_hash(&format!(
            "{}:{}:{}",
            self.dataset_id, self.source_type, self.source_location
        ));
        if self.hash_chain[0] != expected_initial {
            return false;
        }

        for (i, record) in self.transforms_applied.iter().enumerate() {
            let prev = &self.hash_chain[i];
            let step_json = serde_json::to_string(&record.step).unwrap_or_default();
            let expected = compute_hash(&format!("{prev}:{step_json}"));
            if i + 1 >= self.hash_chain.len() || self.hash_chain[i + 1] != expected {
                return false;
            }
        }
        true
    }
}

fn compute_hash(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::transform::TransformStep;

    #[test]
    fn test_lineage_creation() {
        let lineage = DataLineage::new("ds-001", "csv", "data.csv");
        assert_eq!(lineage.dataset_id, "ds-001");
        assert_eq!(lineage.hash_chain.len(), 1);
        assert!(lineage.verify_integrity());
    }

    #[test]
    fn test_lineage_with_transforms() {
        let mut lineage = DataLineage::new("ds-001", "csv", "data.csv");
        lineage.add_transform(TransformRecord {
            step: TransformStep::DropColumn {
                column: "id".into(),
            },
            applied_at: Utc::now(),
            rows_before: 100,
            rows_after: 100,
        });
        assert_eq!(lineage.hash_chain.len(), 2);
        assert!(lineage.verify_integrity());
    }
}
