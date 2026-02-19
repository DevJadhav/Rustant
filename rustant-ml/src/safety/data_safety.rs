//! Data safety — leakage detection, training data auditing.

use serde::{Deserialize, Serialize};

/// Data leakage detection result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataLeakageResult {
    pub leakage_detected: bool,
    pub leaked_fields: Vec<String>,
    pub severity: String,
    pub recommendation: String,
}

/// Training data audit result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingDataAudit {
    pub total_samples: usize,
    pub pii_count: usize,
    pub toxic_count: usize,
    pub duplicate_count: usize,
    pub quality_score: f64,
    pub recommendations: Vec<String>,
}

/// Detects data leakage patterns between training and evaluation data.
pub struct DataLeakageDetector {
    /// Column names or feature names to check for leakage.
    pub target_columns: Vec<String>,
}

impl Default for DataLeakageDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DataLeakageDetector {
    pub fn new() -> Self {
        Self {
            target_columns: Vec::new(),
        }
    }

    /// Detect data leakage patterns and return a result.
    pub fn detect(&self, train_features: &[String], eval_features: &[String]) -> DataLeakageResult {
        let mut leaked_fields = Vec::new();
        for col in &self.target_columns {
            if train_features.contains(col) && eval_features.contains(col) {
                leaked_fields.push(col.clone());
            }
        }
        let leakage_detected = !leaked_fields.is_empty();
        let severity = if leaked_fields.len() > 3 {
            "high".to_string()
        } else if leakage_detected {
            "medium".to_string()
        } else {
            "none".to_string()
        };
        let recommendation = if leakage_detected {
            format!(
                "Remove leaked columns from training data: {}",
                leaked_fields.join(", ")
            )
        } else {
            "No data leakage detected.".to_string()
        };
        DataLeakageResult {
            leakage_detected,
            leaked_fields,
            severity,
            recommendation,
        }
    }
}
