//! # Rustant Tools
//!
//! Built-in tool implementations for the Rustant agent.
//! Provides file operations, search, git integration, and shell execution.

pub mod browser;
pub mod canvas;
pub mod checkpoint;
pub mod codebase_search;
pub mod file;
pub mod git;
pub mod imessage;
pub mod lsp;
pub mod registry;
pub mod sandbox;
pub mod shell;
pub mod smart_edit;
pub mod utils;
pub mod web;

use registry::{Tool, ToolRegistry};
use rustant_core::types::ProgressUpdate;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Register all built-in tools with the given workspace path.
pub fn register_builtin_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    register_builtin_tools_with_progress(registry, workspace, None);
}

/// Register all built-in tools, optionally with a progress channel for streaming output.
pub fn register_builtin_tools_with_progress(
    registry: &mut ToolRegistry,
    workspace: PathBuf,
    progress_tx: Option<mpsc::UnboundedSender<ProgressUpdate>>,
) {
    let shell_tool: Arc<dyn Tool> = if let Some(tx) = progress_tx {
        Arc::new(shell::ShellExecTool::with_progress(workspace.clone(), tx))
    } else {
        Arc::new(shell::ShellExecTool::new(workspace.clone()))
    };

    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(file::FileReadTool::new(workspace.clone())),
        Arc::new(file::FileListTool::new(workspace.clone())),
        Arc::new(file::FileSearchTool::new(workspace.clone())),
        Arc::new(file::FileWriteTool::new(workspace.clone())),
        Arc::new(file::FilePatchTool::new(workspace.clone())),
        Arc::new(git::GitStatusTool::new(workspace.clone())),
        Arc::new(git::GitDiffTool::new(workspace.clone())),
        Arc::new(git::GitCommitTool::new(workspace.clone())),
        shell_tool,
        Arc::new(utils::EchoTool),
        Arc::new(utils::DateTimeTool),
        Arc::new(utils::CalculatorTool),
        // Web tools — search, fetch, and document reading
        Arc::new(web::WebSearchTool::new()),
        Arc::new(web::WebFetchTool::new()),
        Arc::new(web::DocumentReadTool::new(workspace.clone())),
        // Smart editing with fuzzy matching and auto-checkpoint
        Arc::new(smart_edit::SmartEditTool::new(workspace.clone())),
        // Codebase search with auto-indexing
        Arc::new(codebase_search::CodebaseSearchTool::new(workspace.clone())),
    ];

    // iMessage tools — macOS only
    #[cfg(target_os = "macos")]
    {
        tools.push(Arc::new(imessage::IMessageContactsTool));
        tools.push(Arc::new(imessage::IMessageSendTool));
        tools.push(Arc::new(imessage::IMessageReadTool));
    }

    for tool in tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register tool: {}", e);
        }
    }
}

/// Register all LSP tools backed by a shared [`lsp::LspManager`].
///
/// The LSP tools provide code intelligence capabilities (hover, definition,
/// references, diagnostics, completions, rename, format) by connecting to
/// language servers installed on the system.
pub fn register_lsp_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    let manager = Arc::new(lsp::LspManager::new(workspace));
    let lsp_tools = lsp::create_lsp_tools(manager);

    for tool in lsp_tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register LSP tool: {}", e);
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

        // 17 base tools (12 original + 3 web + 1 smart_edit + 1 codebase_search) + 3 iMessage on macOS
        #[cfg(target_os = "macos")]
        assert_eq!(registry.len(), 20);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(registry.len(), 17);

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
        assert!(names.contains(&"echo".to_string()));
        assert!(names.contains(&"datetime".to_string()));
        assert!(names.contains(&"calculator".to_string()));

        // iMessage tools on macOS
        #[cfg(target_os = "macos")]
        {
            assert!(names.contains(&"imessage_contacts".to_string()));
            assert!(names.contains(&"imessage_send".to_string()));
            assert!(names.contains(&"imessage_read".to_string()));
        }
    }

    #[test]
    fn test_tool_definitions_are_valid_json() {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, dir.path().to_path_buf());

        let definitions = registry.list_definitions();
        for def in &definitions {
            assert!(!def.name.is_empty(), "Tool name should not be empty");
            assert!(
                !def.description.is_empty(),
                "Tool description should not be empty"
            );
            // Parameters should be a valid JSON object
            assert!(
                def.parameters.is_object(),
                "Parameters should be a JSON object for tool '{}'",
                def.name
            );
        }
    }
}
