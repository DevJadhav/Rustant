//! Automated error taxonomy and analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Error category for AI operations.
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiErrorCategory {
    ModelHallucination,
    DataLeakage,
    TokenBudgetExceeded,
    ReproducibilityFailure,
    SafetyViolation,
    BiasDetected,
    ToolMisuse,
    ContextLoss,
    RetrievalFailure,
    FormattingError,
    Other(String),
}

/// Error taxonomy derived from trace analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorTaxonomy {
    pub categories: Vec<AiErrorCategory>,
    pub distribution: HashMap<String, usize>,
    pub total_errors: usize,
    pub saturation_reached: bool,
    pub recommended_actions: Vec<String>,
}

impl Default for ErrorTaxonomy {
    fn default() -> Self {
        Self::new()
    }
}

impl ErrorTaxonomy {
    pub fn new() -> Self {
        Self {
            categories: Vec::new(),
            distribution: HashMap::new(),
            total_errors: 0,
            saturation_reached: false,
            recommended_actions: Vec::new(),
        }
    }

    pub fn add_error(&mut self, category: AiErrorCategory) {
        let key = format!("{category:?}");
        *self.distribution.entry(key).or_insert(0) += 1;
        if !self.categories.contains(&category) {
            self.categories.push(category);
        }
        self.total_errors += 1;
    }

    pub fn top_categories(&self, n: usize) -> Vec<(&String, &usize)> {
        let mut items: Vec<_> = self.distribution.iter().collect();
        items.sort_by(|a, b| b.1.cmp(a.1));
        items.into_iter().take(n).collect()
    }
}
