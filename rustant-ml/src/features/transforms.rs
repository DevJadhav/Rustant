//! Feature transforms.

use serde::{Deserialize, Serialize};

/// A feature transform definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureTransform {
    pub name: String,
    pub transform_type: FeatureTransformType,
    pub input_columns: Vec<String>,
    pub output_column: String,
}

/// Types of feature transformations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureTransformType {
    Numerical { method: NumericalMethod },
    Text { method: TextMethod },
    Temporal { method: TemporalMethod },
    Categorical { method: CategoricalMethod },
    Custom { expression: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericalMethod {
    Log,
    Sqrt,
    Square,
    Standardize,
    MinMaxScale,
    Bucketize { boundaries: Vec<f64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextMethod {
    Length,
    WordCount,
    TokenCount,
    Lowercase,
    Hash { num_buckets: usize },
    Embedding { model: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalMethod {
    DayOfWeek,
    Month,
    Year,
    Hour,
    TimeSinceEpoch,
    IsWeekend,
    CyclicalEncode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoricalMethod {
    OneHot,
    Label,
    Frequency,
    Target { smoothing: f64 },
}
