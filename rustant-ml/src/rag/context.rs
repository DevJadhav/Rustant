//! Context assembly for RAG responses.

use crate::rag::retriever::RetrievedChunk;
use serde::{Deserialize, Serialize};

/// Assembled context for LLM generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    pub context_text: String,
    pub chunks_used: Vec<String>,
    pub total_tokens_estimate: usize,
    pub truncated: bool,
}

/// Context assembler with token-aware construction.
pub struct ContextAssembler {
    max_tokens: usize,
    avg_chars_per_token: f64,
}

impl ContextAssembler {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            avg_chars_per_token: 4.0,
        }
    }

    pub fn assemble(&self, chunks: &[RetrievedChunk]) -> AssembledContext {
        let max_chars = (self.max_tokens as f64 * self.avg_chars_per_token) as usize;
        let mut text = String::new();
        let mut used = Vec::new();
        let mut truncated = false;

        for chunk in chunks {
            if text.len() + chunk.text.len() > max_chars {
                truncated = true;
                break;
            }
            if !text.is_empty() {
                text.push_str("\n\n---\n\n");
            }
            text.push_str(&chunk.text);
            used.push(chunk.chunk_id.clone());
        }

        let token_est = (text.len() as f64 / self.avg_chars_per_token) as usize;
        AssembledContext {
            context_text: text,
            chunks_used: used,
            total_tokens_estimate: token_est,
            truncated,
        }
    }
}
