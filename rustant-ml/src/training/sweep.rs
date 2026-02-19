//! Hyperparameter sweep strategies.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Sweep strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SweepStrategy {
    Grid {
        params: HashMap<String, Vec<serde_json::Value>>,
    },
    Random {
        params: HashMap<String, ParamDistribution>,
        n_trials: usize,
    },
    Bayesian {
        params: HashMap<String, ParamRange>,
        n_trials: usize,
        surrogate: String,
    },
}

/// Parameter distribution for random search.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamDistribution {
    Uniform { min: f64, max: f64 },
    LogUniform { min: f64, max: f64 },
    Choice { values: Vec<serde_json::Value> },
    IntRange { min: i64, max: i64 },
}

/// Parameter range for Bayesian optimization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamRange {
    pub min: f64,
    pub max: f64,
    pub log_scale: bool,
}

/// A hyperparameter sweep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HyperparamSweep {
    pub id: String,
    pub experiment_name: String,
    pub strategy: SweepStrategy,
    pub trials: Vec<SweepTrial>,
    pub best_trial: Option<usize>,
}

/// A single sweep trial.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweepTrial {
    pub trial_number: usize,
    pub params: HashMap<String, serde_json::Value>,
    pub metric: Option<f64>,
    pub status: String,
}

impl HyperparamSweep {
    pub fn new(experiment_name: &str, strategy: SweepStrategy) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            experiment_name: experiment_name.to_string(),
            strategy,
            trials: Vec::new(),
            best_trial: None,
        }
    }

    /// Generate trial configurations.
    pub fn generate_trials(&mut self) -> Vec<HashMap<String, serde_json::Value>> {
        match &self.strategy {
            SweepStrategy::Grid { params } => {
                let mut configs = vec![HashMap::new()];
                for (key, values) in params {
                    let mut new_configs = Vec::new();
                    for config in &configs {
                        for value in values {
                            let mut c = config.clone();
                            c.insert(key.clone(), value.clone());
                            new_configs.push(c);
                        }
                    }
                    configs = new_configs;
                }
                configs
            }
            SweepStrategy::Random { params, n_trials } => {
                let mut configs = Vec::new();
                for _ in 0..*n_trials {
                    let mut config = HashMap::new();
                    for (key, dist) in params {
                        let value = match dist {
                            ParamDistribution::Choice { values } => {
                                values.first().cloned().unwrap_or(serde_json::Value::Null)
                            }
                            _ => serde_json::Value::Null,
                        };
                        config.insert(key.clone(), value);
                    }
                    configs.push(config);
                }
                configs
            }
            SweepStrategy::Bayesian { n_trials, .. } => {
                vec![HashMap::new(); *n_trials]
            }
        }
    }
}
