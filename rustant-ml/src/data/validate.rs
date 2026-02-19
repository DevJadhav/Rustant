//! Data quality validation and PII scanning.

use crate::data::source::DataBatch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A data quality report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataQualityReport {
    pub total_rows: usize,
    pub total_columns: usize,
    pub null_percentage: HashMap<String, f64>,
    pub type_mismatches: Vec<TypeMismatch>,
    pub duplicate_rows: usize,
    pub pii_detected: Vec<PiiMatch>,
    pub outliers: HashMap<String, Vec<usize>>,
    pub overall_score: f64,
    pub passed_gate: bool,
}

/// A type mismatch detected in a column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeMismatch {
    pub column: String,
    pub row_index: usize,
    pub expected_type: String,
    pub actual_type: String,
}

/// A PII match detected in data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMatch {
    pub column: String,
    pub row_index: usize,
    pub pii_type: PiiType,
    pub preview: String,
}

/// Types of PII that can be detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiType {
    Email,
    PhoneNumber,
    SocialSecurity,
    CreditCard,
    IpAddress,
    Name,
    Address,
    DateOfBirth,
    ApiKey,
    Other(String),
}

/// PII detection patterns.
static PII_PATTERNS: &[(&str, &str)] = &[
    ("email", r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}"),
    ("phone", r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b"),
    ("ssn", r"\b\d{3}-\d{2}-\d{4}\b"),
    ("credit_card", r"\b\d{4}[-\s]?\d{4}[-\s]?\d{4}[-\s]?\d{4}\b"),
    ("ip_address", r"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b"),
    (
        "api_key",
        r"\b(?:sk|pk|key|token|secret|password)[_-]?[a-zA-Z0-9]{20,}\b",
    ),
];

/// Validate a data batch and produce a quality report.
pub fn validate_batch(batch: &DataBatch, quality_threshold: f64) -> DataQualityReport {
    let total_rows = batch.rows.len();
    let total_columns = batch.columns.len();

    // Calculate null percentages
    let mut null_percentage = HashMap::new();
    for (i, col) in batch.columns.iter().enumerate() {
        let nulls = batch
            .rows
            .iter()
            .filter(|row| row.get(i).is_none_or(|v| v.is_null()))
            .count();
        let pct = if total_rows > 0 {
            nulls as f64 / total_rows as f64 * 100.0
        } else {
            0.0
        };
        null_percentage.insert(col.clone(), pct);
    }

    // Detect duplicates
    let mut seen = std::collections::HashSet::new();
    let mut duplicate_rows = 0;
    for row in &batch.rows {
        let key = serde_json::to_string(row).unwrap_or_default();
        if !seen.insert(key) {
            duplicate_rows += 1;
        }
    }

    // PII scanning
    let pii_detected = scan_pii(batch);

    // Calculate overall score
    let avg_null = if null_percentage.is_empty() {
        0.0
    } else {
        null_percentage.values().sum::<f64>() / null_percentage.len() as f64
    };
    let dup_penalty = if total_rows > 0 {
        duplicate_rows as f64 / total_rows as f64
    } else {
        0.0
    };
    let pii_penalty = if pii_detected.is_empty() { 0.0 } else { 0.1 };
    let overall_score = (1.0 - avg_null / 100.0 - dup_penalty - pii_penalty).clamp(0.0, 1.0);
    let passed_gate = overall_score >= quality_threshold;

    DataQualityReport {
        total_rows,
        total_columns,
        null_percentage,
        type_mismatches: Vec::new(),
        duplicate_rows,
        pii_detected,
        outliers: HashMap::new(),
        overall_score,
        passed_gate,
    }
}

/// Scan a data batch for PII.
fn scan_pii(batch: &DataBatch) -> Vec<PiiMatch> {
    let mut matches = Vec::new();
    let patterns: Vec<(PiiType, regex::Regex)> = PII_PATTERNS
        .iter()
        .filter_map(|(name, pattern)| {
            let pii_type = match *name {
                "email" => PiiType::Email,
                "phone" => PiiType::PhoneNumber,
                "ssn" => PiiType::SocialSecurity,
                "credit_card" => PiiType::CreditCard,
                "ip_address" => PiiType::IpAddress,
                "api_key" => PiiType::ApiKey,
                _ => PiiType::Other(name.to_string()),
            };
            regex::Regex::new(pattern).ok().map(|re| (pii_type, re))
        })
        .collect();

    for (row_idx, row) in batch.rows.iter().enumerate() {
        for (col_idx, val) in row.iter().enumerate() {
            if let serde_json::Value::String(text) = val {
                for (pii_type, re) in &patterns {
                    if re.is_match(text) {
                        let col = batch
                            .columns
                            .get(col_idx)
                            .cloned()
                            .unwrap_or_else(|| format!("col_{col_idx}"));
                        let preview = if text.len() > 20 {
                            format!("{}...", &text[..20])
                        } else {
                            text.clone()
                        };
                        matches.push(PiiMatch {
                            column: col,
                            row_index: row_idx,
                            pii_type: pii_type.clone(),
                            preview,
                        });
                    }
                }
            }
        }
    }

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_clean_data() {
        let batch = DataBatch {
            columns: vec!["name".into(), "age".into()],
            rows: vec![
                vec![serde_json::json!("Alice"), serde_json::json!(30)],
                vec![serde_json::json!("Bob"), serde_json::json!(25)],
            ],
            total_rows: 2,
        };
        let report = validate_batch(&batch, 0.7);
        assert!(report.passed_gate);
        assert_eq!(report.total_rows, 2);
        assert_eq!(report.duplicate_rows, 0);
    }

    #[test]
    fn test_pii_detection() {
        let batch = DataBatch {
            columns: vec!["email".into()],
            rows: vec![vec![serde_json::json!("user@example.com")]],
            total_rows: 1,
        };
        let report = validate_batch(&batch, 0.7);
        assert!(!report.pii_detected.is_empty());
    }
}
