//! Error types for the rustant-ml crate.

use thiserror::Error;

/// Top-level error type for ML operations.
#[derive(Debug, Error)]
pub enum MlError {
    #[error("Dataset error: {0}")]
    Dataset(String),

    #[error("Training error: {0}")]
    Training(String),

    #[error("Model error: {0}")]
    Model(String),

    #[error("RAG error: {0}")]
    Rag(String),

    #[error("Evaluation error: {0}")]
    Evaluation(String),

    #[error("Inference error: {0}")]
    Inference(String),

    #[error("Research error: {0}")]
    Research(String),

    #[error("Feature store error: {0}")]
    FeatureStore(String),

    #[error("Python runtime error: {0}")]
    Python(String),

    #[error("Safety violation: {0}")]
    SafetyViolation(String),

    #[error("Security violation: {0}")]
    SecurityViolation(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimit(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Already exists: {0}")]
    AlreadyExists(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Timeout: {0}")]
    Timeout(String),
}

impl MlError {
    pub fn dataset(msg: impl Into<String>) -> Self {
        Self::Dataset(msg.into())
    }

    pub fn training(msg: impl Into<String>) -> Self {
        Self::Training(msg.into())
    }

    pub fn model(msg: impl Into<String>) -> Self {
        Self::Model(msg.into())
    }

    pub fn rag(msg: impl Into<String>) -> Self {
        Self::Rag(msg.into())
    }

    pub fn inference(msg: impl Into<String>) -> Self {
        Self::Inference(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    pub fn invalid_input(msg: impl Into<String>) -> Self {
        Self::InvalidInput(msg.into())
    }
}
