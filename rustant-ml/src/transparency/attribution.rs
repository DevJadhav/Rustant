//! Source attribution for claims.

use serde::{Deserialize, Serialize};

/// An attribution linking a claim to a source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attribution {
    pub claim: String,
    pub source_id: String,
    pub source_text: String,
    pub confidence: f64,
    pub source_type: String,
}

/// Source attributor.
pub struct SourceAttributor;

impl Default for SourceAttributor {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceAttributor {
    pub fn new() -> Self {
        Self
    }

    pub fn attribute(&self, claim: &str, sources: &[(String, String)]) -> Vec<Attribution> {
        sources
            .iter()
            .map(|(id, text)| {
                let words_in_common = claim
                    .split_whitespace()
                    .filter(|w| text.to_lowercase().contains(&w.to_lowercase()))
                    .count();
                let total_words = claim.split_whitespace().count().max(1);
                let confidence = words_in_common as f64 / total_words as f64;

                Attribution {
                    claim: claim.to_string(),
                    source_id: id.clone(),
                    source_text: text.chars().take(200).collect(),
                    confidence,
                    source_type: "document".to_string(),
                }
            })
            .collect()
    }
}
