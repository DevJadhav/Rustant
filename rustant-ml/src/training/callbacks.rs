//! Training callbacks — early stopping, anomaly detection, checkpointing.

use crate::training::metrics::TrainingMetrics;
use serde::{Deserialize, Serialize};

/// Action a callback can request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackAction {
    Continue,
    Stop,
    Checkpoint,
}

/// Trait for training callbacks.
pub trait TrainingCallback: Send + Sync {
    /// Called at the end of each epoch with epoch number and current metrics.
    fn on_epoch_end(&mut self, epoch: usize, metrics: &TrainingMetrics) -> CallbackAction;
}

/// Early stopping callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EarlyStoppingCallback {
    pub patience: usize,
    pub min_delta: f64,
    #[serde(skip)]
    counter: usize,
    #[serde(skip)]
    best_loss: Option<f64>,
}

impl EarlyStoppingCallback {
    pub fn new(patience: usize, min_delta: f64) -> Self {
        Self {
            patience,
            min_delta,
            counter: 0,
            best_loss: None,
        }
    }

    pub fn on_epoch_end(&mut self, _epoch: usize, loss: f64) -> CallbackAction {
        match self.best_loss {
            None => {
                self.best_loss = Some(loss);
                CallbackAction::Continue
            }
            Some(best) => {
                if loss < best - self.min_delta {
                    self.best_loss = Some(loss);
                    self.counter = 0;
                    CallbackAction::Continue
                } else {
                    self.counter += 1;
                    if self.counter >= self.patience {
                        CallbackAction::Stop
                    } else {
                        CallbackAction::Continue
                    }
                }
            }
        }
    }
}

impl TrainingCallback for EarlyStoppingCallback {
    fn on_epoch_end(&mut self, epoch: usize, metrics: &TrainingMetrics) -> CallbackAction {
        let loss = metrics.loss_history.last().copied().unwrap_or(f64::MAX);
        self.on_epoch_end(epoch, loss)
    }
}

/// Anomaly detection callback for training metrics.
#[derive(Debug, Clone)]
pub struct AnomalyDetectionCallback {
    pub threshold: f64,
    window: Vec<f64>,
    window_size: usize,
}

impl AnomalyDetectionCallback {
    pub fn new(threshold: f64, window_size: usize) -> Self {
        Self {
            threshold,
            window: Vec::new(),
            window_size,
        }
    }

    pub fn on_epoch_end(&mut self, _epoch: usize, loss: f64) -> CallbackAction {
        // Detect NaN/Inf
        if loss.is_nan() || loss.is_infinite() {
            return CallbackAction::Stop;
        }

        self.window.push(loss);
        if self.window.len() > self.window_size {
            self.window.remove(0);
        }

        // Detect loss spike (> threshold * mean)
        if self.window.len() >= 3 {
            let mean = self.window.iter().sum::<f64>() / self.window.len() as f64;
            if loss > mean * self.threshold {
                return CallbackAction::Stop;
            }
        }

        CallbackAction::Continue
    }
}

impl TrainingCallback for AnomalyDetectionCallback {
    fn on_epoch_end(&mut self, epoch: usize, metrics: &TrainingMetrics) -> CallbackAction {
        let loss = metrics.loss_history.last().copied().unwrap_or(0.0);
        self.on_epoch_end(epoch, loss)
    }
}

/// Checkpoint callback — triggers a checkpoint every N epochs and tracks the best ones.
#[derive(Debug, Clone)]
pub struct CheckpointCallback {
    /// Checkpoint every `frequency` epochs.
    pub frequency: usize,
    /// How many best checkpoints to keep.
    pub keep_best: usize,
    epoch_counter: usize,
}

impl CheckpointCallback {
    pub fn new(frequency: usize, keep_best: usize) -> Self {
        Self {
            frequency,
            keep_best,
            epoch_counter: 0,
        }
    }

    pub fn on_epoch_end(&mut self, _epoch: usize, _loss: f64) -> CallbackAction {
        self.epoch_counter += 1;
        if self.epoch_counter >= self.frequency {
            self.epoch_counter = 0;
            CallbackAction::Checkpoint
        } else {
            CallbackAction::Continue
        }
    }
}

impl TrainingCallback for CheckpointCallback {
    fn on_epoch_end(&mut self, epoch: usize, metrics: &TrainingMetrics) -> CallbackAction {
        let loss = metrics.loss_history.last().copied().unwrap_or(0.0);
        self.on_epoch_end(epoch, loss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_early_stopping() {
        let mut cb = EarlyStoppingCallback::new(3, 0.01);
        assert_eq!(cb.on_epoch_end(1, 0.5), CallbackAction::Continue); // first: sets best=0.5
        assert_eq!(cb.on_epoch_end(2, 0.4), CallbackAction::Continue); // improves: best=0.4, counter=0
        assert_eq!(cb.on_epoch_end(3, 0.4), CallbackAction::Continue); // no improve: counter=1
        assert_eq!(cb.on_epoch_end(4, 0.4), CallbackAction::Continue); // no improve: counter=2
        assert_eq!(cb.on_epoch_end(5, 0.4), CallbackAction::Stop); // no improve: counter=3 >= patience
    }

    #[test]
    fn test_anomaly_nan() {
        let mut cb = AnomalyDetectionCallback::new(3.0, 5);
        assert_eq!(cb.on_epoch_end(1, f64::NAN), CallbackAction::Stop);
    }
}
