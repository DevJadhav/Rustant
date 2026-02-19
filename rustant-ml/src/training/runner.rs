//! Training runner — subprocess orchestration.

use crate::error::MlError;
use crate::training::experiment::{TrainingExperiment, TrainingStatus};
use crate::training::metrics::TrainingMetrics;
use std::path::PathBuf;
use std::time::Duration;

/// Training runner that orchestrates Python subprocess training.
pub struct TrainingRunner {
    #[allow(dead_code)]
    workspace: PathBuf,
    /// Maximum duration before a training run is forcibly stopped.
    pub timeout: Duration,
    /// Threshold for detecting loss anomalies (e.g., loss spikes). Replaces anomaly_detector Arc.
    pub anomaly_threshold: f64,
    /// Whether pillar enforcement checks are enabled during training.
    pub pillar_enforcement_enabled: bool,
}

impl TrainingRunner {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            timeout: Duration::from_secs(3600),
            anomaly_threshold: 3.0,
            pillar_enforcement_enabled: false,
        }
    }

    /// Start a training run.
    pub async fn start(
        &self,
        experiment: &mut TrainingExperiment,
    ) -> Result<TrainingMetrics, MlError> {
        experiment.status = TrainingStatus::Running;
        experiment.updated_at = chrono::Utc::now();

        // In a real implementation, this would launch a Python subprocess.
        // For now, return a placeholder.
        let metrics = TrainingMetrics::default();
        experiment.status = TrainingStatus::Completed;
        experiment.updated_at = chrono::Utc::now();
        experiment.metrics = Some(metrics.clone());

        Ok(metrics)
    }

    /// Get training status.
    pub fn status<'a>(&self, experiment: &'a TrainingExperiment) -> &'a TrainingStatus {
        &experiment.status
    }

    /// Stop a running training.
    pub async fn stop(&self, experiment: &mut TrainingExperiment) -> Result<(), MlError> {
        if experiment.status == TrainingStatus::Running {
            experiment.status = TrainingStatus::Cancelled;
            experiment.updated_at = chrono::Utc::now();
        }
        Ok(())
    }
}
