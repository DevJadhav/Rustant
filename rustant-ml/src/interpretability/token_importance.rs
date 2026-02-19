//! Token-level importance scoring.

use serde::{Deserialize, Serialize};

/// Token importance scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenImportance {
    pub tokens: Vec<String>,
    pub scores: Vec<f64>,
    pub method: String,
}

impl TokenImportance {
    pub fn top_tokens(&self, n: usize) -> Vec<(&str, f64)> {
        let mut pairs: Vec<_> = self
            .tokens
            .iter()
            .zip(self.scores.iter())
            .map(|(t, s)| (t.as_str(), *s))
            .collect();
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.into_iter().take(n).collect()
    }
}
