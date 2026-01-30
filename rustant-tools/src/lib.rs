//! # Rustant Tools
//!
//! Built-in tool implementations for the Rustant agent.
//! Provides file operations, search, git integration, and shell execution.

pub mod file;
pub mod git;
pub mod registry;
pub mod shell;

use registry::{Tool, ToolRegistry};
use std::path::PathBuf;
use std::sync::Arc;

/// Register all built-in tools with the given workspace path.
pub fn register_builtin_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(file::FileReadTool::new(workspace.clone())),
        Arc::new(file::FileListTool::new(workspace.clone())),
        Arc::new(file::FileSearchTool::new(workspace.clone())),
        Arc::new(file::FileWriteTool::new(workspace.clone())),
        Arc::new(file::FilePatchTool::new(workspace.clone())),
        Arc::new(git::GitStatusTool::new(workspace.clone())),
        Arc::new(git::GitDiffTool::new(workspace.clone())),
        Arc::new(git::GitCommitTool::new(workspace.clone())),
        Arc::new(shell::ShellExecTool::new(workspace)),
    ];

    for tool in tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register tool: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_register_all_builtin_tools() {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, dir.path().to_path_buf());

        assert_eq!(registry.len(), 9);

        // Verify all expected tools are registered
        let names = registry.list_names();
        assert!(names.contains(&"file_read".to_string()));
        assert!(names.contains(&"file_list".to_string()));
        assert!(names.contains(&"file_search".to_string()));
        assert!(names.contains(&"file_write".to_string()));
        assert!(names.contains(&"file_patch".to_string()));
        assert!(names.contains(&"git_status".to_string()));
        assert!(names.contains(&"git_diff".to_string()));
        assert!(names.contains(&"git_commit".to_string()));
        assert!(names.contains(&"shell_exec".to_string()));
    }

    #[test]
    fn test_tool_definitions_are_valid_json() {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, dir.path().to_path_buf());

        let definitions = registry.list_definitions();
        for def in &definitions {
            assert!(!def.name.is_empty(), "Tool name should not be empty");
            assert!(!def.description.is_empty(), "Tool description should not be empty");
            // Parameters should be a valid JSON object
            assert!(def.parameters.is_object(), "Parameters should be a JSON object for tool '{}'", def.name);
        }
    }
}
