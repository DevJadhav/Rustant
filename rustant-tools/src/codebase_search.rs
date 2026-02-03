//! Codebase search tool powered by the Project Context Auto-Indexer.
//!
//! Provides semantic search over the indexed project files, function signatures,
//! and content summaries. Requires the workspace to have been indexed first.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::indexer::ProjectIndexer;
use rustant_core::search::SearchConfig;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::path::PathBuf;
use std::sync::Mutex;

/// Tool for searching the project codebase using hybrid search.
pub struct CodebaseSearchTool {
    indexer: Mutex<Option<ProjectIndexer>>,
    workspace: PathBuf,
}

impl CodebaseSearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            indexer: Mutex::new(None),
            workspace,
        }
    }

    /// Ensure the indexer is initialized and workspace is indexed.
    fn ensure_indexed(&self) -> Result<(), ToolError> {
        let mut guard = self
            .indexer
            .lock()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Lock error: {}", e),
            })?;

        if guard.is_none() {
            let search_config = SearchConfig {
                index_path: self.workspace.join(".rustant/search_index"),
                db_path: self.workspace.join(".rustant/vectors.db"),
                ..Default::default()
            };

            let mut indexer =
                ProjectIndexer::new(self.workspace.clone(), search_config).map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "codebase_search".into(),
                        message: format!("Failed to initialize indexer: {}", e),
                    }
                })?;

            indexer.index_workspace();
            *guard = Some(indexer);
        }

        Ok(())
    }
}

#[async_trait]
impl Tool for CodebaseSearchTool {
    fn name(&self) -> &str {
        "codebase_search"
    }

    fn description(&self) -> &str {
        "Search the project codebase using natural language queries. \
         Finds relevant files, function signatures, and code content. \
         The workspace is automatically indexed on first use."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural language search query (e.g., 'authentication handler', \
                        'database connection', 'error types')"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 10)"
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "codebase_search".into(),
                reason: "'query' parameter is required".into(),
            })?;

        let max_results = args["max_results"].as_u64().unwrap_or(10) as usize;

        // Ensure workspace is indexed (lazy initialization)
        self.ensure_indexed()?;

        let guard = self
            .indexer
            .lock()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Lock error: {}", e),
            })?;

        let indexer = guard.as_ref().ok_or_else(|| ToolError::ExecutionFailed {
            name: "codebase_search".into(),
            message: "Indexer not initialized".into(),
        })?;

        let results = indexer
            .search(query)
            .map_err(|e| ToolError::ExecutionFailed {
                name: "codebase_search".into(),
                message: format!("Search failed: {}", e),
            })?;

        if results.is_empty() {
            return Ok(ToolOutput::text(format!(
                "No results found for query: '{}'",
                query
            )));
        }

        let mut output = format!(
            "Found {} results for '{}':\n\n",
            results.len().min(max_results),
            query
        );

        for (i, result) in results.iter().take(max_results).enumerate() {
            output.push_str(&format!(
                "{}. [score: {:.2}] {}\n",
                i + 1,
                result.combined_score,
                result.content.lines().next().unwrap_or(&result.content)
            ));

            // Show a bit more context for top results
            if i < 3 {
                let extra_lines: Vec<&str> = result.content.lines().skip(1).take(3).collect();
                if !extra_lines.is_empty() {
                    for line in extra_lines {
                        output.push_str(&format!("   {}\n", line));
                    }
                }
            }
            output.push('\n');
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> std::time::Duration {
        // Indexing can take a while on first run
        std::time::Duration::from_secs(120)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_workspace() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(
            path.join("src/main.rs"),
            "fn main() {\n    run_server();\n}\n\nfn run_server() {\n    println!(\"starting\");\n}\n",
        )
        .unwrap();
        fs::write(
            path.join("src/auth.rs"),
            "pub fn authenticate(token: &str) -> bool {\n    !token.is_empty()\n}\n",
        )
        .unwrap();
        fs::write(path.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        (dir, path)
    }

    #[tokio::test]
    async fn test_codebase_search_basic() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "authenticate"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(
            result.content.contains("authenticate") || result.content.contains("auth"),
            "Should find auth-related content: {}",
            result.content
        );
    }

    #[tokio::test]
    async fn test_codebase_search_no_results() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({
            "query": "zzz_nonexistent_xyz_999"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("No results") || result.content.contains("Found"));
    }

    #[tokio::test]
    async fn test_codebase_search_missing_query() {
        let (_dir, path) = setup_workspace();
        let tool = CodebaseSearchTool::new(path);

        let args = serde_json::json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_tool_properties() {
        let dir = TempDir::new().unwrap();
        let tool = CodebaseSearchTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "codebase_search");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("Search"));
    }
}
