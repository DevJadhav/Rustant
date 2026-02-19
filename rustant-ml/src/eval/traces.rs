//! Trace storage for evaluation (SQLite-backed).

use crate::error::MlError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A stored evaluation trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalTrace {
    pub id: String,
    pub task: String,
    pub result: String,
    pub score: Option<f64>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// Trace store (file-based for simplicity, SQLite in full impl).
pub struct TraceStore {
    base_dir: std::path::PathBuf,
}

impl TraceStore {
    pub fn new(base_dir: std::path::PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn store(&self, trace: &EvalTrace) -> Result<(), MlError> {
        std::fs::create_dir_all(&self.base_dir)?;
        let path = self.base_dir.join(format!("{}.json", trace.id));
        let content = serde_json::to_string_pretty(trace)?;
        std::fs::write(&path, &content)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> Result<Option<EvalTrace>, MlError> {
        let path = self.base_dir.join(format!("{id}.json"));
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(Some(serde_json::from_str(&content)?))
    }

    pub fn list(&self) -> Result<Vec<EvalTrace>, MlError> {
        let mut traces = Vec::new();
        if !self.base_dir.exists() {
            return Ok(traces);
        }
        for entry in std::fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if entry.path().extension().is_some_and(|e| e == "json") {
                let content = std::fs::read_to_string(entry.path())?;
                if let Ok(trace) = serde_json::from_str(&content) {
                    traces.push(trace);
                }
            }
        }
        Ok(traces)
    }

    pub fn count(&self) -> Result<usize, MlError> {
        Ok(self.list()?.len())
    }
}
