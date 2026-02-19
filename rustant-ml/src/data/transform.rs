//! Data transformation pipeline.

use crate::data::source::DataBatch;
use crate::error::MlError;
use serde::{Deserialize, Serialize};

/// A transformation step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformStep {
    DropColumn {
        column: String,
    },
    RenameColumn {
        from: String,
        to: String,
    },
    FillNull {
        column: String,
        value: serde_json::Value,
    },
    FilterRows {
        condition: String,
    },
    Normalize {
        column: String,
        method: NormMethod,
    },
    Encode {
        column: String,
        method: EncodeMethod,
    },
    Deduplicate {
        columns: Option<Vec<String>>,
    },
    SortBy {
        column: String,
        ascending: bool,
    },
    Sample {
        fraction: f64,
    },
}

/// Normalization method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormMethod {
    MinMax,
    ZScore,
    RobustScaler,
}

/// Encoding method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncodeMethod {
    OneHot,
    Label,
    Ordinal,
}

/// A pipeline of transformation steps.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransformPipeline {
    pub steps: Vec<TransformStep>,
}

impl TransformPipeline {
    pub fn new() -> Self {
        Self { steps: Vec::new() }
    }

    pub fn add_step(mut self, step: TransformStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Apply the pipeline to a data batch.
    pub fn apply(&self, mut batch: DataBatch) -> Result<DataBatch, MlError> {
        for step in &self.steps {
            batch = apply_step(batch, step)?;
        }
        Ok(batch)
    }
}

fn apply_step(mut batch: DataBatch, step: &TransformStep) -> Result<DataBatch, MlError> {
    match step {
        TransformStep::DropColumn { column } => {
            if let Some(idx) = batch.columns.iter().position(|c| c == column) {
                batch.columns.remove(idx);
                for row in &mut batch.rows {
                    if idx < row.len() {
                        row.remove(idx);
                    }
                }
            }
            Ok(batch)
        }
        TransformStep::RenameColumn { from, to } => {
            if let Some(col) = batch
                .columns
                .iter_mut()
                .find(|c| c.as_str() == from.as_str())
            {
                *col = to.clone();
            }
            Ok(batch)
        }
        TransformStep::FillNull { column, value } => {
            if let Some(idx) = batch.columns.iter().position(|c| c == column) {
                for row in &mut batch.rows {
                    if let Some(cell) = row.get_mut(idx) {
                        if cell.is_null() {
                            *cell = value.clone();
                        }
                    }
                }
            }
            Ok(batch)
        }
        TransformStep::Deduplicate { columns: _ } => {
            let mut seen = std::collections::HashSet::new();
            batch.rows.retain(|row| {
                let key = serde_json::to_string(row).unwrap_or_default();
                seen.insert(key)
            });
            batch.total_rows = batch.rows.len();
            Ok(batch)
        }
        TransformStep::Sample { fraction } => {
            use rand::seq::SliceRandom;
            let count = (batch.rows.len() as f64 * fraction).ceil() as usize;
            let mut rng = rand::thread_rng();
            batch.rows.shuffle(&mut rng);
            batch.rows.truncate(count);
            batch.total_rows = batch.rows.len();
            Ok(batch)
        }
        _ => Ok(batch), // Other transforms return unchanged for now
    }
}

/// Record of a transform applied (for lineage tracking).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformRecord {
    pub step: TransformStep,
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub rows_before: usize,
    pub rows_after: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_column() {
        let batch = DataBatch {
            columns: vec!["a".into(), "b".into(), "c".into()],
            rows: vec![vec![
                serde_json::json!(1),
                serde_json::json!(2),
                serde_json::json!(3),
            ]],
            total_rows: 1,
        };
        let pipeline =
            TransformPipeline::new().add_step(TransformStep::DropColumn { column: "b".into() });
        let result = pipeline.apply(batch).unwrap();
        assert_eq!(result.columns, vec!["a", "c"]);
        assert_eq!(result.rows[0].len(), 2);
    }

    #[test]
    fn test_fill_null() {
        let batch = DataBatch {
            columns: vec!["x".into()],
            rows: vec![vec![serde_json::Value::Null], vec![serde_json::json!(5)]],
            total_rows: 2,
        };
        let pipeline = TransformPipeline::new().add_step(TransformStep::FillNull {
            column: "x".into(),
            value: serde_json::json!(0),
        });
        let result = pipeline.apply(batch).unwrap();
        assert_eq!(result.rows[0][0], serde_json::json!(0));
        assert_eq!(result.rows[1][0], serde_json::json!(5));
    }
}
