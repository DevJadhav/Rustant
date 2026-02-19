//! Model Card — structured documentation for models.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A model card documenting model properties, limitations, and biases.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelCard {
    pub description: String,
    #[serde(default)]
    pub training_data: Option<String>,
    #[serde(default)]
    pub evaluation_metrics: HashMap<String, f64>,
    #[serde(default)]
    pub limitations: Vec<String>,
    #[serde(default)]
    pub biases: Vec<BiasReport>,
    #[serde(default)]
    pub intended_use: String,
    #[serde(default)]
    pub out_of_scope_uses: Vec<String>,
    #[serde(default)]
    pub ethical_considerations: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub citation: Option<String>,
}

/// A bias report for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiasReport {
    pub dimension: String,
    pub description: String,
    pub severity: String,
    pub mitigation: Option<String>,
}
