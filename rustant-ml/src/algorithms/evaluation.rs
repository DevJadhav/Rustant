//! Algorithm evaluation metrics.

use serde::{Deserialize, Serialize};

/// Cross-validation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossValidation {
    pub n_folds: usize,
    pub stratified: bool,
    pub shuffle: bool,
    pub random_state: Option<u64>,
}

impl Default for CrossValidation {
    fn default() -> Self {
        Self {
            n_folds: 5,
            stratified: true,
            shuffle: true,
            random_state: Some(42),
        }
    }
}

/// Cross-validation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossValidationResult {
    pub fold_scores: Vec<f64>,
    pub mean_score: f64,
    pub std_score: f64,
    pub metric_name: String,
}

impl CrossValidationResult {
    pub fn from_scores(scores: Vec<f64>, metric_name: &str) -> Self {
        let mean = scores.iter().sum::<f64>() / scores.len() as f64;
        let variance = scores.iter().map(|s| (s - mean).powi(2)).sum::<f64>() / scores.len() as f64;
        Self {
            fold_scores: scores,
            mean_score: mean,
            std_score: variance.sqrt(),
            metric_name: metric_name.to_string(),
        }
    }
}
