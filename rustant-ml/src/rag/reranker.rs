//! Reranking strategies for retrieved documents.

use crate::rag::retriever::RetrievedChunk;
use async_trait::async_trait;

/// Reranker trait.
#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(
        &self,
        query: &str,
        chunks: Vec<RetrievedChunk>,
        top_k: usize,
    ) -> Vec<RetrievedChunk>;
}

/// Maximal Marginal Relevance reranker.
pub struct MmrReranker {
    pub lambda: f32,
}

impl Default for MmrReranker {
    fn default() -> Self {
        Self { lambda: 0.5 }
    }
}

#[async_trait]
impl Reranker for MmrReranker {
    async fn rerank(
        &self,
        _query: &str,
        mut chunks: Vec<RetrievedChunk>,
        top_k: usize,
    ) -> Vec<RetrievedChunk> {
        chunks.truncate(top_k);
        chunks
    }
}

/// Score-based reranker (simple threshold).
pub struct ScoreReranker {
    pub min_score: f32,
}

#[async_trait]
impl Reranker for ScoreReranker {
    async fn rerank(
        &self,
        _query: &str,
        chunks: Vec<RetrievedChunk>,
        top_k: usize,
    ) -> Vec<RetrievedChunk> {
        chunks
            .into_iter()
            .filter(|c| c.score >= self.min_score)
            .take(top_k)
            .collect()
    }
}

/// Cross-encoder reranker (dummy implementation — sorts by existing score).
pub struct CrossEncoderReranker {
    pub model_name: String,
}

impl CrossEncoderReranker {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
        }
    }
}

#[async_trait]
impl Reranker for CrossEncoderReranker {
    async fn rerank(
        &self,
        _query: &str,
        mut chunks: Vec<RetrievedChunk>,
        top_k: usize,
    ) -> Vec<RetrievedChunk> {
        // Dummy: sort by existing score descending, then truncate.
        // A real implementation would compute cross-encoder scores between query and each chunk.
        chunks.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        chunks.truncate(top_k);
        chunks
    }
}

/// LLM-based reranker (dummy implementation — sorts by existing score).
pub struct LlmBasedReranker {
    pub model_name: String,
}

impl LlmBasedReranker {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
        }
    }
}

#[async_trait]
impl Reranker for LlmBasedReranker {
    async fn rerank(
        &self,
        _query: &str,
        mut chunks: Vec<RetrievedChunk>,
        top_k: usize,
    ) -> Vec<RetrievedChunk> {
        // Dummy: sort by existing score descending, then truncate.
        // A real implementation would use an LLM to score relevance of each chunk to the query.
        chunks.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        chunks.truncate(top_k);
        chunks
    }
}
