//! Research synthesis — cross-paper insights and trend analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Research synthesis across multiple papers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchSynthesis {
    pub topic: String,
    pub papers_count: usize,
    pub key_insights: Vec<Insight>,
    pub methodology_trends: Vec<String>,
    pub performance_trends: HashMap<String, Vec<f64>>,
    pub open_questions: Vec<String>,
}

/// A key insight from synthesis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub description: String,
    pub supporting_papers: Vec<String>,
    pub confidence: f64,
    pub category: String,
}
