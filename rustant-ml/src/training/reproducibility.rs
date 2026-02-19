//! Reproducibility tracking — environment snapshots, seed management.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Snapshot of the training environment for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    pub python_version: Option<String>,
    pub packages: HashMap<String, String>,
    pub system_info: String,
    pub git_hash: Option<String>,
    pub platform: String,
    pub timestamp: DateTime<Utc>,
    /// Hash of relevant environment variables (PATH, PYTHONPATH, CUDA_VISIBLE_DEVICES).
    pub env_vars_hash: String,
}

impl EnvironmentSnapshot {
    pub fn capture() -> Self {
        let env_vars_hash = Self::compute_env_vars_hash();
        Self {
            python_version: None,
            packages: HashMap::new(),
            system_info: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            git_hash: None,
            platform: std::env::consts::OS.to_string(),
            timestamp: Utc::now(),
            env_vars_hash,
        }
    }

    /// Compute a SHA-256 hash over relevant environment variables.
    fn compute_env_vars_hash() -> String {
        let mut hasher = Sha256::new();
        for var in &["PATH", "PYTHONPATH", "CUDA_VISIBLE_DEVICES"] {
            let value = std::env::var(var).unwrap_or_default();
            hasher.update(var.as_bytes());
            hasher.update(b"=");
            hasher.update(value.as_bytes());
            hasher.update(b"\n");
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Seed manager for reproducible training.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedManager {
    pub global_seed: u64,
    pub component_seeds: HashMap<String, u64>,
}

impl SeedManager {
    pub fn new(global_seed: u64) -> Self {
        Self {
            global_seed,
            component_seeds: HashMap::new(),
        }
    }

    pub fn get_seed(&mut self, component: &str) -> u64 {
        *self
            .component_seeds
            .entry(component.to_string())
            .or_insert_with(|| self.global_seed.wrapping_add(component.len() as u64))
    }
}
