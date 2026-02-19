//! Reproducibility tracking.

use crate::training::reproducibility::EnvironmentSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Reproducibility status.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproducibilityStatus {
    NotAttempted,
    InProgress,
    Reproduced,
    PartiallyReproduced,
    FailedToReproduce,
}

/// Record of a reproduction attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproducibilityRecord {
    pub paper_id: String,
    pub attempts: Vec<ReproductionAttempt>,
    pub status: ReproducibilityStatus,
    /// Aggregated deviation analysis across attempts.
    pub deviation_analysis: Vec<String>,
}

/// A single reproduction attempt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReproductionAttempt {
    pub attempt_number: usize,
    pub environment: EnvironmentSnapshot,
    pub claimed_metric: f64,
    pub achieved_metric: Option<f64>,
    pub deviation: Option<f64>,
    pub notes: String,
    pub timestamp: DateTime<Utc>,
}
