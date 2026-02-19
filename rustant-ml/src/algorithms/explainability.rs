//! Model explainability stubs (SHAP/LIME integration points).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Explainability report for a model prediction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainabilityReport {
    pub feature_importance: HashMap<String, f64>,
    pub shap_values: Option<Vec<Vec<f64>>>,
    pub partial_dependence: Option<HashMap<String, Vec<(f64, f64)>>>,
    pub decision_path: Option<Vec<String>>,
    pub method_used: String,
}

impl ExplainabilityReport {
    pub fn from_feature_importance(importance: HashMap<String, f64>) -> Self {
        Self {
            feature_importance: importance,
            shap_values: None,
            partial_dependence: None,
            decision_path: None,
            method_used: "feature_importance".to_string(),
        }
    }

    /// Get the top N most important features.
    pub fn top_features(&self, n: usize) -> Vec<(&String, &f64)> {
        let mut items: Vec<_> = self.feature_importance.iter().collect();
        items.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
        items.into_iter().take(n).collect()
    }
}
