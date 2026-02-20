//! Checkpoint management for training runs.

use crate::error::MlError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// A training checkpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub experiment_id: String,
    pub epoch: usize,
    pub loss: f64,
    pub path: PathBuf,
    pub hash: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
}

/// Checkpoint manager.
pub struct CheckpointManager {
    base_dir: PathBuf,
    max_checkpoints: usize,
}

impl CheckpointManager {
    pub fn new(base_dir: PathBuf, max_checkpoints: usize) -> Self {
        Self {
            base_dir,
            max_checkpoints,
        }
    }

    /// List checkpoints for an experiment.
    pub fn list(&self, experiment_id: &str) -> Result<Vec<Checkpoint>, MlError> {
        let dir = self.base_dir.join(experiment_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let manifest_path = dir.join("checkpoints.json");
        if !manifest_path.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&manifest_path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Get the best checkpoint (lowest loss).
    pub fn best(&self, experiment_id: &str) -> Result<Option<Checkpoint>, MlError> {
        let checkpoints = self.list(experiment_id)?;
        Ok(checkpoints.into_iter().min_by(|a, b| {
            a.loss
                .partial_cmp(&b.loss)
                .unwrap_or(std::cmp::Ordering::Equal)
        }))
    }

    /// Save a new checkpoint entry with a computed hash.
    pub fn save(
        &self,
        experiment_id: &str,
        epoch: usize,
        loss: f64,
        path: &Path,
    ) -> Result<Checkpoint, MlError> {
        let dir = self.base_dir.join(experiment_id);
        std::fs::create_dir_all(&dir)?;

        // Compute a content-based hash from experiment_id, epoch, and loss.
        let mut hasher = Sha256::new();
        hasher.update(experiment_id.as_bytes());
        hasher.update(epoch.to_le_bytes());
        hasher.update(loss.to_le_bytes());
        let hash = format!("{:x}", hasher.finalize());

        let size_bytes = if path.exists() {
            std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };

        let checkpoint = Checkpoint {
            id: uuid::Uuid::new_v4().to_string(),
            experiment_id: experiment_id.to_string(),
            epoch,
            loss,
            path: path.to_path_buf(),
            hash,
            size_bytes,
            created_at: Utc::now(),
        };

        // Append to manifest.
        let manifest_path = dir.join("checkpoints.json");
        let mut checkpoints = if manifest_path.exists() {
            let content = std::fs::read_to_string(&manifest_path)?;
            serde_json::from_str::<Vec<Checkpoint>>(&content)?
        } else {
            Vec::new()
        };
        checkpoints.push(checkpoint.clone());

        // Enforce max_checkpoints by removing oldest entries.
        while checkpoints.len() > self.max_checkpoints {
            checkpoints.remove(0);
        }

        rustant_core::persistence::atomic_write_json(&manifest_path, &checkpoints)?;

        Ok(checkpoint)
    }

    /// Compare two checkpoints by id, returning a human-readable diff.
    pub fn compare(&self, experiment_id: &str, id_a: &str, id_b: &str) -> Result<String, MlError> {
        let checkpoints = self.list(experiment_id)?;
        let a = checkpoints
            .iter()
            .find(|c| c.id == id_a)
            .ok_or_else(|| MlError::NotFound(format!("checkpoint {id_a}")))?;
        let b = checkpoints
            .iter()
            .find(|c| c.id == id_b)
            .ok_or_else(|| MlError::NotFound(format!("checkpoint {id_b}")))?;

        let epoch_diff = b.epoch as i64 - a.epoch as i64;
        let loss_diff = b.loss - a.loss;
        let size_diff = b.size_bytes as i64 - a.size_bytes as i64;

        Ok(format!(
            "Checkpoint comparison ({id_a} vs {id_b}):\n  Epoch: {} -> {} (diff: {:+})\n  Loss:  {:.6} -> {:.6} (diff: {:+.6})\n  Size:  {} -> {} bytes (diff: {:+})",
            a.epoch,
            b.epoch,
            epoch_diff,
            a.loss,
            b.loss,
            loss_diff,
            a.size_bytes,
            b.size_bytes,
            size_diff,
        ))
    }
}
