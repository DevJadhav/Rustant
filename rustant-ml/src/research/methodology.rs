//! Methodology extraction from papers.

use serde::{Deserialize, Serialize};

/// Extracted methodology from a paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodologyExtraction {
    pub paper_id: String,
    pub approach: String,
    pub steps: Vec<MethodStep>,
    pub datasets_used: Vec<String>,
    pub baselines: Vec<String>,
    pub metrics_reported: Vec<String>,
    pub hardware: Option<String>,
    pub reproducibility_score: f64,
}

/// A step in the methodology.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodStep {
    pub order: usize,
    pub description: String,
    pub technique: Option<String>,
    pub tools: Vec<String>,
}
