//! End-to-end RAG pipeline.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step in the reasoning trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    pub step: String,
    pub detail: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// RAG response with source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagResponse {
    pub answer: String,
    pub sources: Vec<SourceReference>,
    pub groundedness_score: f64,
    pub reasoning_trace: Vec<ReasoningStep>,
    pub retrieval_stats: RetrievalStats,
}

/// Reference to a source document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReference {
    pub document_id: String,
    pub chunk_id: String,
    pub relevance_score: f32,
    pub text_excerpt: String,
    pub metadata: HashMap<String, String>,
}

/// Retrieval statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetrievalStats {
    pub chunks_retrieved: usize,
    pub chunks_used: usize,
    pub avg_relevance_score: f32,
    pub retrieval_time_ms: u64,
}

/// RAG pipeline.
pub struct RagPipeline {
    #[allow(dead_code)]
    workspace: std::path::PathBuf,
}

impl RagPipeline {
    pub fn new(workspace: std::path::PathBuf) -> Self {
        Self { workspace }
    }

    pub fn status(&self) -> RagPipelineStatus {
        RagPipelineStatus {
            collections: 0,
            total_documents: 0,
            total_chunks: 0,
            index_healthy: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagPipelineStatus {
    pub collections: usize,
    pub total_documents: usize,
    pub total_chunks: usize,
    pub index_healthy: bool,
}
