//! Retrieval diagnostics — explain why documents were/weren't retrieved.

use serde::{Deserialize, Serialize};

/// Analysis of retrieval coverage across query aspects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageAnalysis {
    pub coverage_score: f64,
    pub covered_aspects: Vec<String>,
    pub uncovered_aspects: Vec<String>,
}

/// Retrieval diagnostics report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalDiagnostics {
    pub query_analysis: QueryAnalysis,
    pub retrieval_explanations: Vec<RetrievalExplanation>,
    pub coverage_analysis: CoverageAnalysis,
    pub improvement_suggestions: Vec<String>,
}

/// Analysis of the query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryAnalysis {
    pub original_query: String,
    pub key_terms: Vec<String>,
    pub query_type: String,
    pub estimated_specificity: f64,
}

/// Explanation for why a document was or wasn't retrieved.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalExplanation {
    pub document_id: String,
    pub retrieved: bool,
    pub score: f32,
    pub reason: String,
    pub matching_terms: Vec<String>,
}
