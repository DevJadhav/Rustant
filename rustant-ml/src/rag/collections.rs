//! RAG collection management.

use crate::error::MlError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A RAG collection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    pub name: String,
    pub description: Option<String>,
    pub document_count: usize,
    pub chunk_count: usize,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Collection manager.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionManager {
    pub collections: Vec<Collection>,
}

impl CollectionManager {
    pub fn new() -> Self {
        Self {
            collections: Vec::new(),
        }
    }

    pub fn create(&mut self, name: &str, description: Option<&str>) -> &Collection {
        let now = Utc::now();
        self.collections.push(Collection {
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
            document_count: 0,
            chunk_count: 0,
            created_at: now,
            updated_at: now,
        });
        self.collections.last().unwrap()
    }

    pub fn find(&self, name: &str) -> Option<&Collection> {
        self.collections.iter().find(|c| c.name == name)
    }

    pub fn delete(&mut self, name: &str) -> bool {
        let len = self.collections.len();
        self.collections.retain(|c| c.name != name);
        self.collections.len() < len
    }

    pub fn list(&self) -> &[Collection] {
        &self.collections
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
