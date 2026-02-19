//! LoRA adapter management.

use crate::error::MlError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A registered adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterInfo {
    pub name: String,
    pub base_model: String,
    pub method: String,
    pub path: PathBuf,
    pub rank: Option<u32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Adapter manager.
pub struct AdapterManager {
    base_dir: PathBuf,
    active_adapter: Option<String>,
}

impl AdapterManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            active_adapter: None,
        }
    }

    pub fn list(&self) -> Result<Vec<AdapterInfo>, MlError> {
        let manifest = self.base_dir.join("adapters.json");
        if !manifest.exists() {
            return Ok(Vec::new());
        }
        let content = std::fs::read_to_string(&manifest)?;
        Ok(serde_json::from_str(&content)?)
    }

    pub fn register(&self, info: AdapterInfo) -> Result<(), MlError> {
        let mut adapters = self.list()?;
        adapters.push(info);
        self.save_adapters(&adapters)?;
        Ok(())
    }

    /// Merge an adapter's weights and mark it as merged. Returns the output path.
    pub fn merge(&self, name: &str, output_path: &Path) -> Result<PathBuf, MlError> {
        let mut adapters = self.list()?;
        let adapter = adapters
            .iter_mut()
            .find(|a| a.name == name)
            .ok_or_else(|| MlError::NotFound(format!("adapter '{name}' not found")))?;
        adapter.method = format!("{}-merged", adapter.method);
        adapter.path = output_path.to_path_buf();
        self.save_adapters(&adapters)?;
        Ok(output_path.to_path_buf())
    }

    /// Switch the active adapter.
    pub fn switch(&mut self, name: &str) -> Result<(), MlError> {
        let adapters = self.list()?;
        if !adapters.iter().any(|a| a.name == name) {
            return Err(MlError::NotFound(format!("adapter '{name}' not found")));
        }
        self.active_adapter = Some(name.to_string());
        Ok(())
    }

    /// Delete an adapter from the registry.
    pub fn delete(&self, name: &str) -> Result<(), MlError> {
        let mut adapters = self.list()?;
        let len_before = adapters.len();
        adapters.retain(|a| a.name != name);
        if adapters.len() == len_before {
            return Err(MlError::NotFound(format!("adapter '{name}' not found")));
        }
        self.save_adapters(&adapters)?;
        Ok(())
    }

    fn save_adapters(&self, adapters: &[AdapterInfo]) -> Result<(), MlError> {
        let content = serde_json::to_string_pretty(adapters)?;
        let manifest = self.base_dir.join("adapters.json");
        std::fs::create_dir_all(&self.base_dir)?;
        std::fs::write(&manifest, &content)?;
        Ok(())
    }
}
