//! RAG retriever wrapping existing HybridSearchEngine.

use serde::{Deserialize, Serialize};

/// Retriever configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrieverConfig {
    pub top_k: usize,
    pub min_score: f32,
    pub collection: Option<String>,
    pub use_reranker: bool,
}

impl Default for RetrieverConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            min_score: 0.1,
            collection: None,
            use_reranker: false,
        }
    }
}

/// A retrieved result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedChunk {
    pub chunk_id: String,
    pub document_id: String,
    pub text: String,
    pub score: f32,
    pub metadata: std::collections::HashMap<String, String>,
}

/// RAG retriever.
pub struct RagRetriever {
    config: RetrieverConfig,
    /// HybridSearchEngine config (cross-crate boundary — stored as serialized string).
    search_config: Option<String>,
    /// Reranker type identifier (cross-crate boundary — stored as string).
    reranker_type: Option<String>,
}

impl RagRetriever {
    pub fn new(config: RetrieverConfig) -> Self {
        Self {
            config,
            search_config: None,
            reranker_type: None,
        }
    }

    /// Create a retriever with search engine and reranker configuration.
    pub fn with_search_config(mut self, search_config: String) -> Self {
        self.search_config = Some(search_config);
        self
    }

    /// Set the reranker type.
    pub fn with_reranker_type(mut self, reranker_type: String) -> Self {
        self.reranker_type = Some(reranker_type);
        self
    }

    /// Get the search config reference.
    pub fn search_config(&self) -> Option<&str> {
        self.search_config.as_deref()
    }

    /// Get the reranker type reference.
    pub fn reranker_type(&self) -> Option<&str> {
        self.reranker_type.as_deref()
    }

    pub fn retrieve(&self, _query: &str, top_k: Option<usize>) -> Vec<RetrievedChunk> {
        let _k = top_k.unwrap_or(self.config.top_k);
        // In real implementation, this would call HybridSearchEngine
        Vec::new()
    }
}
