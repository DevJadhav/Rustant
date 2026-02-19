//! Training metrics tracking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Training metrics for an experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetrics {
    pub epochs_completed: usize,
    pub loss_history: Vec<f64>,
    pub val_loss_history: Vec<f64>,
    pub custom_metrics: HashMap<String, Vec<f64>>,
    pub best_epoch: Option<usize>,
    pub best_loss: Option<f64>,
    pub total_training_time_secs: f64,
}

impl Default for TrainingMetrics {
    fn default() -> Self {
        Self {
            epochs_completed: 0,
            loss_history: Vec::new(),
            val_loss_history: Vec::new(),
            custom_metrics: HashMap::new(),
            best_epoch: None,
            best_loss: None,
            total_training_time_secs: 0.0,
        }
    }
}

impl TrainingMetrics {
    pub fn record_epoch(&mut self, loss: f64, val_loss: Option<f64>) {
        self.loss_history.push(loss);
        if let Some(vl) = val_loss {
            self.val_loss_history.push(vl);
        }
        self.epochs_completed += 1;

        let check_loss = val_loss.unwrap_or(loss);
        if self.best_loss.is_none() || check_loss < self.best_loss.unwrap() {
            self.best_loss = Some(check_loss);
            self.best_epoch = Some(self.epochs_completed);
        }
    }

    pub fn add_custom_metric(&mut self, name: &str, value: f64) {
        self.custom_metrics
            .entry(name.to_string())
            .or_default()
            .push(value);
    }
}

/// Classification metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub confusion_matrix: Option<Vec<Vec<usize>>>,
    pub auc_roc: Option<f64>,
}

/// Regression metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionMetrics {
    pub mse: f64,
    pub rmse: f64,
    pub mae: f64,
    pub r_squared: f64,
    pub explained_variance: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_training_metrics() {
        let mut metrics = TrainingMetrics::default();
        metrics.record_epoch(0.5, Some(0.6));
        metrics.record_epoch(0.3, Some(0.4));
        assert_eq!(metrics.epochs_completed, 2);
        assert_eq!(metrics.best_epoch, Some(2));
        assert_eq!(metrics.best_loss, Some(0.4));
    }
}
