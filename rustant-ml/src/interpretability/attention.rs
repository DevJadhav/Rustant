//! Attention extraction and analysis.

use serde::{Deserialize, Serialize};

/// Attention map for a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttentionMap {
    pub model: String,
    pub input_tokens: Vec<String>,
    pub layers: usize,
    pub heads: usize,
    pub attention_weights: Vec<Vec<Vec<f32>>>, // [layer][head][token_pair]
}

/// Head importance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeadImportance {
    pub layer: usize,
    pub head: usize,
    pub importance_score: f64,
    pub pattern_type: String,
}

/// Attention analyzer.
pub struct AttentionAnalyzer;

impl Default for AttentionAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl AttentionAnalyzer {
    pub fn new() -> Self {
        Self
    }

    /// Extract attention weights for a given model and input text.
    pub fn extract_attention(&self, model: &str, input: &str) -> AttentionMap {
        let tokens: Vec<String> = input.split_whitespace().map(String::from).collect();
        let num_tokens = tokens.len();
        // Stub: produce a uniform attention map with 1 layer and 1 head
        let weights = if num_tokens > 0 {
            let uniform = 1.0 / num_tokens as f32;
            vec![vec![vec![uniform; num_tokens * num_tokens]]]
        } else {
            vec![vec![vec![]]]
        };
        AttentionMap {
            model: model.to_string(),
            input_tokens: tokens,
            layers: 1,
            heads: 1,
            attention_weights: weights,
        }
    }

    pub fn identify_important_heads(&self, map: &AttentionMap) -> Vec<HeadImportance> {
        let mut heads = Vec::new();
        for layer in 0..map.layers {
            for head in 0..map.heads {
                heads.push(HeadImportance {
                    layer,
                    head,
                    importance_score: 0.5,
                    pattern_type: "general".into(),
                });
            }
        }
        heads.sort_by(|a, b| {
            b.importance_score
                .partial_cmp(&a.importance_score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        heads
    }
}
