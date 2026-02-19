//! Explanation methods — chain of thought, counterfactual, contrastive.

use serde::{Deserialize, Serialize};

/// Explanation method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExplanationMethod {
    ChainOfThought {
        steps: Vec<String>,
    },
    Counterfactual {
        original: String,
        modified: String,
        outcome_change: String,
    },
    Contrastive {
        chosen: String,
        rejected: String,
        differentiators: Vec<String>,
    },
    ExampleBased {
        similar_examples: Vec<SimilarExample>,
        distances: Vec<f64>,
    },
}

/// A similar example for example-based explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarExample {
    pub input: String,
    pub output: String,
    pub similarity: f64,
}

/// Full interpretability report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterpretabilityReport {
    pub explanations: Vec<ExplanationMethod>,
    pub feature_importance: std::collections::HashMap<String, f64>,
    pub confidence: f64,
    pub summary: String,
    /// Summary of tools used during analysis.
    pub tool_usage_summary: Vec<String>,
    /// Safety-related observations.
    pub safety_notes: Vec<String>,
}
