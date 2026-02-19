//! Feature definitions and groups.

use crate::data::lineage::DataLineage;
use crate::data::schema::ColumnType;
use crate::data::transform::TransformPipeline;
use serde::{Deserialize, Serialize};

/// Distribution statistics for drift detection baselines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionStats {
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub quartiles: [f64; 3],
}

/// A feature definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDefinition {
    pub name: String,
    pub dtype: ColumnType,
    pub description: Option<String>,
    pub transform: Option<TransformPipeline>,
    pub version: u32,
    pub source_dataset: Option<String>,
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<DataLineage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift_baseline: Option<DistributionStats>,
}

/// A group of related features.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureGroup {
    pub name: String,
    pub description: Option<String>,
    pub features: Vec<FeatureDefinition>,
    pub entity_key: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl FeatureGroup {
    pub fn new(name: &str, entity_key: &str) -> Self {
        Self {
            name: name.to_string(),
            description: None,
            features: Vec::new(),
            entity_key: entity_key.to_string(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn add_feature(&mut self, feature: FeatureDefinition) {
        self.features.push(feature);
    }
}
