//! Local model catalog and discovery.

use crate::inference::backends::LocalModel;
use crate::inference::formats::{ModelFormat, detect_format};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Catalog of locally available models.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalModelCatalog {
    pub models: Vec<LocalModel>,
}

impl LocalModelCatalog {
    pub fn new() -> Self {
        Self { models: Vec::new() }
    }

    pub fn scan_directory(&mut self, dir: &Path) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let format = detect_format(&path);
                if format != ModelFormat::Unknown {
                    let size = std::fs::metadata(&path).map(|m| m.len()).ok();
                    self.models.push(LocalModel {
                        name: path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default(),
                        path: Some(path),
                        format,
                        size_bytes: size,
                        parameters: None,
                    });
                    count += 1;
                }
            }
        }
        count
    }

    pub fn find(&self, name: &str) -> Option<&LocalModel> {
        self.models.iter().find(|m| m.name == name)
    }
}
