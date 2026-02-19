//! Feature registry for tracking feature definitions and versions.

use crate::error::MlError;
use crate::features::definition::{DistributionStats, FeatureGroup};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Registry of all feature definitions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureRegistry {
    pub groups: Vec<FeatureGroup>,
}

impl FeatureRegistry {
    pub fn new() -> Self {
        Self { groups: Vec::new() }
    }

    pub fn add_group(&mut self, group: FeatureGroup) {
        self.groups.push(group);
    }

    pub fn find_group(&self, name: &str) -> Option<&FeatureGroup> {
        self.groups.iter().find(|g| g.name == name)
    }

    pub fn remove_group(&mut self, name: &str) -> bool {
        let len = self.groups.len();
        self.groups.retain(|g| g.name != name);
        self.groups.len() < len
    }

    /// Load from JSON file.
    pub fn load(path: &Path) -> Result<Self, MlError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save to JSON file (atomic write).
    pub fn save(&self, path: &Path) -> Result<(), MlError> {
        let content = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &content)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Check drift for a named feature against its baseline using KL divergence approximation.
    ///
    /// Returns a drift score between 0.0 (no drift) and 1.0 (extreme drift),
    /// or `None` if the feature has no drift baseline configured.
    pub fn check_drift(
        &self,
        feature_name: &str,
        current_stats: &DistributionStats,
    ) -> Option<f64> {
        // Find the feature across all groups
        for group in &self.groups {
            for feature in &group.features {
                if feature.name == feature_name {
                    if let Some(baseline) = &feature.drift_baseline {
                        return Some(compute_drift_score(baseline, current_stats));
                    }
                }
            }
        }
        None
    }
}

/// Compute drift score using a KL divergence approximation based on Gaussian assumptions.
///
/// Uses the closed-form KL divergence for two Gaussians:
///   KL(P || Q) = ln(sigma_q / sigma_p) + (sigma_p^2 + (mu_p - mu_q)^2) / (2 * sigma_q^2) - 0.5
///
/// The raw KL divergence is then clamped to [0.0, 1.0] via a sigmoid-like transform.
fn compute_drift_score(baseline: &DistributionStats, current: &DistributionStats) -> f64 {
    let sigma_b = baseline.std_dev.max(1e-10);
    let sigma_c = current.std_dev.max(1e-10);

    let kl = (sigma_c / sigma_b).ln()
        + (sigma_b * sigma_b + (baseline.mean - current.mean).powi(2)) / (2.0 * sigma_c * sigma_c)
        - 0.5;

    let kl = kl.max(0.0);

    // Also consider range shift as a secondary signal
    let range_b = (baseline.max - baseline.min).max(1e-10);
    let mean_shift = ((current.mean - baseline.mean) / range_b).abs();

    // Combine KL divergence and mean shift, then map to [0, 1] via sigmoid
    let combined = kl + mean_shift;
    1.0 - 1.0 / (1.0 + combined)
}
