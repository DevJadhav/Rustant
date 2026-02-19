//! RAG evaluation metrics.

use serde::{Deserialize, Serialize};

/// RAG-specific metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagMetrics {
    pub context_precision: f64,
    pub context_recall: f64,
    pub faithfulness: f64,
    pub answer_relevancy: f64,
    pub mrr: f64,
    pub ndcg_at_k: f64,
}

impl Default for RagMetrics {
    fn default() -> Self {
        Self {
            context_precision: 0.0,
            context_recall: 0.0,
            faithfulness: 0.0,
            answer_relevancy: 0.0,
            mrr: 0.0,
            ndcg_at_k: 0.0,
        }
    }
}

/// A RAG evaluation test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagTestCase {
    pub query: String,
    pub expected_answer: Option<String>,
    pub relevant_doc_ids: Vec<String>,
}

/// Result of evaluating a RAG system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagEvalResult {
    pub test_cases_run: usize,
    pub metrics: RagMetrics,
    pub per_query_results: Vec<QueryEvalResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryEvalResult {
    pub query: String,
    pub retrieved_relevant: usize,
    pub total_retrieved: usize,
    pub precision: f64,
    pub recall: f64,
}
