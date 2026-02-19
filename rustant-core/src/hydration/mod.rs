//! Context Hydration Pipeline — two-stage generation with context injection.
//!
//! Stage 1: Select relevant context from the repository map (fast model or search fallback).
//! Stage 2: Inject selected context into the LLM prompt for better code generation.

pub mod assembler;
pub mod selector;

use crate::repo_map::{ContextChunk, RepoMap};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Configuration for the hydration pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrationConfig {
    /// Whether hydration is enabled.
    pub enabled: bool,
    /// Model to use for context selection (fast model).
    pub selector_model: Option<String>,
    /// Provider for the selector model.
    pub selector_provider: Option<String>,
    /// Maximum tokens for injected context.
    pub max_context_tokens: usize,
    /// Maximum number of files to include.
    pub max_files: usize,
}

impl Default for HydrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            selector_model: None,
            selector_provider: None,
            max_context_tokens: 8000,
            max_files: 20,
        }
    }
}

/// Result of the hydration pipeline.
#[derive(Debug, Clone)]
pub struct HydrationResult {
    /// Selected context chunks.
    pub chunks: Vec<ContextChunk>,
    /// Formatted context string ready for injection.
    pub context_text: String,
    /// Total tokens used by the context.
    pub estimated_tokens: usize,
    /// Number of files represented.
    pub file_count: usize,
}

/// The hydration pipeline.
pub struct HydrationPipeline {
    config: HydrationConfig,
}

impl HydrationPipeline {
    pub fn new(config: HydrationConfig) -> Self {
        Self { config }
    }

    /// Determine whether hydration should be used for this task.
    pub fn should_hydrate(&self, workspace: &Path) -> bool {
        if !self.config.enabled {
            return false;
        }

        // Skip for small repos — less than 10 code files
        let file_count = count_code_files(workspace);
        file_count >= 10
    }

    /// Run the hydration pipeline: select context relevant to the query.
    pub fn hydrate(&self, workspace: &Path, query: &str) -> HydrationResult {
        // Build the repo map
        let repo_map = RepoMap::build(workspace);

        // Select context chunks using search + PageRank ranking
        let chunks = selector::select_context(
            &repo_map,
            query,
            self.config.max_context_tokens,
            self.config.max_files,
        );

        // Assemble the context text
        let context_text = assembler::assemble_context(&chunks, workspace);
        let estimated_tokens = context_text.len() / 4; // rough estimate

        let file_count = {
            let mut files = std::collections::HashSet::new();
            for chunk in &chunks {
                files.insert(&chunk.file);
            }
            files.len()
        };

        HydrationResult {
            chunks,
            context_text,
            estimated_tokens,
            file_count,
        }
    }
}

/// Count code files in workspace (fast estimate).
fn count_code_files(workspace: &Path) -> usize {
    let walker = ignore::WalkBuilder::new(workspace)
        .hidden(true)
        .git_ignore(true)
        .build();

    walker
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            if !path.is_file() {
                return false;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            matches!(
                ext,
                "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go" | "java" | "c" | "cpp" | "rb"
            )
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hydration_config_default() {
        let config = HydrationConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.max_context_tokens, 8000);
        assert_eq!(config.max_files, 20);
    }

    #[test]
    fn test_should_hydrate_disabled() {
        let pipeline = HydrationPipeline::new(HydrationConfig::default());
        let dir = TempDir::new().unwrap();
        assert!(!pipeline.should_hydrate(dir.path()));
    }

    #[test]
    fn test_should_hydrate_small_repo() {
        let config = HydrationConfig {
            enabled: true,
            ..Default::default()
        };
        let pipeline = HydrationPipeline::new(config);
        let dir = TempDir::new().unwrap();
        // Small repo with < 10 files
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        assert!(!pipeline.should_hydrate(dir.path()));
    }

    #[test]
    fn test_hydrate_basic() {
        let config = HydrationConfig {
            enabled: true,
            ..Default::default()
        };
        let pipeline = HydrationPipeline::new(config);
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("main.rs"),
            "pub fn hello() {}\npub fn world() {}",
        )
        .unwrap();
        let result = pipeline.hydrate(dir.path(), "hello");
        // May or may not have results depending on parsing
        assert!(result.context_text.is_empty() || !result.chunks.is_empty());
    }
}
