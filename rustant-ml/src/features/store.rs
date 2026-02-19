//! Feature store with online (SQLite) and offline (file) storage.

use crate::error::MlError;
use std::collections::HashMap;
use std::path::PathBuf;

/// Feature store with online and offline components.
pub struct FeatureStore {
    base_dir: PathBuf,
}

impl FeatureStore {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Get feature value for an entity.
    pub fn get(
        &self,
        feature_group: &str,
        entity_key: &str,
    ) -> Result<HashMap<String, serde_json::Value>, MlError> {
        let path = self
            .base_dir
            .join(feature_group)
            .join(format!("{entity_key}.json"));
        if !path.exists() {
            return Ok(HashMap::new());
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// Store feature values for an entity.
    pub fn put(
        &self,
        feature_group: &str,
        entity_key: &str,
        values: &HashMap<String, serde_json::Value>,
    ) -> Result<(), MlError> {
        let dir = self.base_dir.join(feature_group);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{entity_key}.json"));
        let content = serde_json::to_string_pretty(values)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, &content)?;
        std::fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// List all feature groups.
    pub fn list_groups(&self) -> Result<Vec<String>, MlError> {
        let mut groups = Vec::new();
        if self.base_dir.exists() {
            for entry in std::fs::read_dir(&self.base_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        groups.push(name.to_string());
                    }
                }
            }
        }
        Ok(groups)
    }
}
