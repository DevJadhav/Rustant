//! Training experiment tracking.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Training status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    Paused,
}

/// A training experiment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingExperiment {
    pub id: String,
    pub name: String,
    pub dataset_id: String,
    pub model_type: String,
    pub hyperparams: serde_json::Value,
    pub status: TrainingStatus,
    pub metrics: Option<super::metrics::TrainingMetrics>,
    pub checkpoint_path: Option<std::path::PathBuf>,
    pub seed: u64,
    pub environment: Option<super::reproducibility::EnvironmentSnapshot>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    /// Human-readable explanation of why this experiment configuration was chosen.
    /// Uses String (not DecisionExplanation from core) to avoid circular dependency.
    pub decision_explanation: Option<String>,
}

impl TrainingExperiment {
    pub fn new(name: &str, dataset_id: &str, model_type: &str) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            dataset_id: dataset_id.to_string(),
            model_type: model_type.to_string(),
            hyperparams: serde_json::Value::Object(serde_json::Map::new()),
            status: TrainingStatus::Pending,
            metrics: None,
            checkpoint_path: None,
            seed: 42,
            environment: None,
            created_at: now,
            updated_at: now,
            tags: Vec::new(),
            notes: None,
            decision_explanation: None,
        }
    }
}

/// Registry of training experiments.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExperimentRegistry {
    pub experiments: Vec<TrainingExperiment>,
}

impl ExperimentRegistry {
    pub fn new() -> Self {
        Self {
            experiments: Vec::new(),
        }
    }

    pub fn add(&mut self, exp: TrainingExperiment) {
        self.experiments.push(exp);
    }

    pub fn find(&self, id: &str) -> Option<&TrainingExperiment> {
        self.experiments.iter().find(|e| e.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut TrainingExperiment> {
        self.experiments.iter_mut().find(|e| e.id == id)
    }

    pub fn list_by_status(&self, status: &TrainingStatus) -> Vec<&TrainingExperiment> {
        self.experiments
            .iter()
            .filter(|e| &e.status == status)
            .collect()
    }

    pub fn load(path: &std::path::Path) -> Result<Self, crate::error::MlError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), crate::error::MlError> {
        let content = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &content)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}
