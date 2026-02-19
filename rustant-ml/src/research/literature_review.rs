//! Automated literature review synthesis.

use serde::{Deserialize, Serialize};

/// An automated literature review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiteratureReview {
    pub topic: String,
    pub papers_analyzed: usize,
    pub key_themes: Vec<Theme>,
    pub research_gaps: Vec<ResearchGap>,
    pub trend_analysis: Vec<Trend>,
    pub synthesis: String,
    /// List of citation references for transparency.
    pub citations: Vec<String>,
    /// Summary of methodology comparison.
    pub methodology_comparison: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub description: String,
    pub paper_count: usize,
    pub key_papers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchGap {
    pub area: String,
    pub description: String,
    pub potential_impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trend {
    pub name: String,
    pub direction: String,
    pub evidence: Vec<String>,
}
