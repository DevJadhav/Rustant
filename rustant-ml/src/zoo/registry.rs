//! Model registry — catalog of local models.

use crate::error::MlError;
use crate::zoo::card::ModelCard;
use crate::zoo::provenance::ModelProvenance;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Source of a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModelSource {
    HuggingFace {
        repo_id: String,
        revision: Option<String>,
    },
    Ollama {
        model_name: String,
    },
    Url {
        url: String,
    },
    Local {
        path: PathBuf,
    },
    Trained {
        experiment_id: String,
    },
}

/// Framework the model was built with.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    PyTorch,
    TensorFlow,
    Onnx,
    CoreMl,
    Gguf,
    SafeTensors,
    Sklearn,
    XGBoost,
    Other(String),
}

/// Task type for the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    TextGeneration,
    TextClassification,
    TokenClassification,
    QuestionAnswering,
    Summarization,
    Translation,
    ImageClassification,
    ObjectDetection,
    Embeddings,
    Regression,
    Clustering,
    Other(String),
}

/// A registered model entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: ModelSource,
    pub framework: Framework,
    pub task_type: TaskType,
    pub card: ModelCard,
    pub provenance: ModelProvenance,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

/// Registry of all managed models.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelRegistry {
    pub models: Vec<ModelEntry>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self { models: Vec::new() }
    }

    pub fn add(&mut self, entry: ModelEntry) {
        self.models.push(entry);
    }

    pub fn find(&self, id: &str) -> Option<&ModelEntry> {
        self.models.iter().find(|m| m.id == id)
    }

    pub fn find_by_name(&self, name: &str) -> Option<&ModelEntry> {
        self.models.iter().filter(|m| m.name == name).next_back()
    }

    pub fn list(&self) -> &[ModelEntry] {
        &self.models
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let len = self.models.len();
        self.models.retain(|m| m.id != id);
        self.models.len() < len
    }

    pub fn search(&self, query: &str) -> Vec<&ModelEntry> {
        let q = query.to_lowercase();
        self.models
            .iter()
            .filter(|m| {
                m.name.to_lowercase().contains(&q)
                    || m.card.description.to_lowercase().contains(&q)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn load(path: &Path) -> Result<Self, MlError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), MlError> {
        rustant_core::persistence::atomic_write_json(path, self)?;
        Ok(())
    }
}
