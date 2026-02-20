//! Dataset versioning and registry.

use crate::data::lineage::DataLineage;
use crate::data::schema::SchemaDefinition;
use crate::data::source::DataSourceType;
use crate::data::validate::DataQualityReport;
use crate::error::MlError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// A registered dataset entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetEntry {
    pub id: String,
    pub name: String,
    pub version: u32,
    pub source: DataSourceType,
    pub schema: SchemaDefinition,
    pub path: PathBuf,
    pub hash: String,
    pub row_count: usize,
    pub lineage: DataLineage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_report: Option<DataQualityReport>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Registry of all managed datasets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DatasetRegistry {
    pub datasets: Vec<DatasetEntry>,
}

impl DatasetRegistry {
    pub fn new() -> Self {
        Self {
            datasets: Vec::new(),
        }
    }

    /// Load registry from a JSON file.
    pub fn load(path: &Path) -> Result<Self, MlError> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Save registry to a JSON file (atomic write).
    pub fn save(&self, path: &Path) -> Result<(), MlError> {
        rustant_core::persistence::atomic_write_json(path, self)?;
        Ok(())
    }

    /// Add a dataset entry.
    pub fn add(&mut self, entry: DatasetEntry) {
        self.datasets.push(entry);
    }

    /// Find a dataset by ID.
    pub fn find(&self, id: &str) -> Option<&DatasetEntry> {
        self.datasets.iter().find(|d| d.id == id)
    }

    /// Find a dataset by name (returns latest version).
    pub fn find_by_name(&self, name: &str) -> Option<&DatasetEntry> {
        self.datasets
            .iter()
            .filter(|d| d.name == name)
            .max_by_key(|d| d.version)
    }

    /// List all datasets.
    pub fn list(&self) -> &[DatasetEntry] {
        &self.datasets
    }

    /// Remove a dataset by ID.
    pub fn remove(&mut self, id: &str) -> bool {
        let len = self.datasets.len();
        self.datasets.retain(|d| d.id != id);
        self.datasets.len() < len
    }
}

/// Compute SHA-256 hash of file contents.
pub fn hash_file(path: &Path) -> Result<String, MlError> {
    let content = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute SHA-256 hash of arbitrary bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_crud() {
        let mut reg = DatasetRegistry::new();
        assert!(reg.list().is_empty());

        let entry = DatasetEntry {
            id: "ds-001".into(),
            name: "test".into(),
            version: 1,
            source: DataSourceType::Csv {
                path: "data.csv".into(),
                delimiter: ',',
            },
            schema: SchemaDefinition {
                columns: Vec::new(),
            },
            path: PathBuf::from(".rustant/ml/datasets/ds-001"),
            hash: "abc123".into(),
            row_count: 100,
            lineage: DataLineage::new("ds-001", "csv", "data.csv"),
            quality_report: None,
            created_at: Utc::now(),
            tags: vec!["test".into()],
        };
        reg.add(entry);
        assert_eq!(reg.list().len(), 1);
        assert!(reg.find("ds-001").is_some());
        assert!(reg.find_by_name("test").is_some());
        assert!(reg.remove("ds-001"));
        assert!(reg.list().is_empty());
    }
}
