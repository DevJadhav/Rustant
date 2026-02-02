//! File operation tools: read, list, write, and patch.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{Artifact, RiskLevel, ToolOutput};
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// Validate that a path stays inside the workspace.
///
/// For existing paths, canonicalizes both path and workspace to handle symlinks.
/// For non-existent paths (e.g., new files to create), checks that the
/// normalized path doesn't contain `..` components that escape the workspace.
fn validate_workspace_path(
    workspace: &Path,
    path_str: &str,
    tool_name: &str,
) -> Result<PathBuf, ToolError> {
    // Canonicalize workspace to handle symlinks (e.g., /var -> /private/var on macOS)
    let workspace_canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    let resolved = if Path::new(path_str).is_absolute() {
        PathBuf::from(path_str)
    } else {
        workspace_canonical.join(path_str)
    };

    // For existing paths, use canonicalize for accurate resolution
    if resolved.exists() {
        let canonical = resolved
            .canonicalize()
            .map_err(|e| ToolError::ExecutionFailed {
                name: tool_name.into(),
                message: format!("Path resolution failed: {}", e),
            })?;

        if !canonical.starts_with(&workspace_canonical) {
            return Err(ToolError::PermissionDenied {
                name: tool_name.into(),
                reason: format!("Path '{}' is outside the workspace", path_str),
            });
        }
        return Ok(canonical);
    }

    // For non-existent paths, normalize away ".." components and check
    let mut normalized = Vec::new();
    for component in resolved.components() {
        match component {
            std::path::Component::ParentDir => {
                if normalized.pop().is_none() {
                    return Err(ToolError::PermissionDenied {
                        name: tool_name.into(),
                        reason: format!("Path '{}' escapes the workspace", path_str),
                    });
                }
            }
            std::path::Component::CurDir => {} // skip "."
            other => normalized.push(other),
        }
    }
    let normalized_path: PathBuf = normalized.iter().collect();

    if !normalized_path.starts_with(&workspace_canonical) {
        return Err(ToolError::PermissionDenied {
            name: tool_name.into(),
            reason: format!("Path '{}' is outside the workspace", path_str),
        });
    }

    Ok(resolved)
}

/// Read a file's contents.
pub struct FileReadTool {
    workspace: PathBuf,
}

impl FileReadTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn resolve_path(&self, path: &str) -> Result<PathBuf, ToolError> {
        let resolved = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.workspace.join(path)
        };

        // Ensure the path doesn't escape the workspace
        let canonical = resolved
            .canonicalize()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "file_read".into(),
                message: format!("Path resolution failed: {}", e),
            })?;

        // Canonicalize workspace too, to handle symlinks (e.g., /var -> /private/var on macOS)
        let workspace_canonical = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());

        if !canonical.starts_with(&workspace_canonical) {
            return Err(ToolError::PermissionDenied {
                name: "file_read".into(),
                reason: format!("Path '{}' is outside the workspace", path),
            });
        }

        Ok(canonical)
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Supports optional line range with start_line and end_line parameters."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative to workspace or absolute)"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Starting line number (1-based, inclusive). Optional."
                },
                "end_line": {
                    "type": "integer",
                    "description": "Ending line number (1-based, inclusive). Optional."
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_read".into(),
                reason: "'path' parameter is required and must be a string".into(),
            })?;

        let path = self.resolve_path(path_str)?;

        debug!(path = %path.display(), "Reading file");

        let content =
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "file_read".into(),
                    message: format!("Failed to read '{}': {}", path_str, e),
                })?;

        let start_line = args["start_line"].as_u64().map(|n| n as usize);
        let end_line = args["end_line"].as_u64().map(|n| n as usize);

        let output = if start_line.is_some() || end_line.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let start = start_line.unwrap_or(1).saturating_sub(1);
            let end = end_line.unwrap_or(lines.len()).min(lines.len());

            if start >= lines.len() {
                return Ok(ToolOutput::text(format!(
                    "File has {} lines, start_line {} is out of range",
                    lines.len(),
                    start + 1
                )));
            }

            let selected: Vec<String> = lines[start..end]
                .iter()
                .enumerate()
                .map(|(i, line)| format!("{:>4} | {}", start + i + 1, line))
                .collect();
            selected.join("\n")
        } else {
            // Add line numbers
            content
                .lines()
                .enumerate()
                .map(|(i, line)| format!("{:>4} | {}", i + 1, line))
                .collect::<Vec<_>>()
                .join("\n")
        };

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

/// List files in a directory, respecting .gitignore patterns.
pub struct FileListTool {
    workspace: PathBuf,
}

impl FileListTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileListTool {
    fn name(&self) -> &str {
        "file_list"
    }

    fn description(&self) -> &str {
        "List files and directories at the given path. Respects .gitignore patterns."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list (relative to workspace). Defaults to workspace root."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Whether to list files recursively. Default: false."
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum depth for recursive listing. Default: 3."
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args["path"].as_str().unwrap_or(".");
        let recursive = args["recursive"].as_bool().unwrap_or(false);
        let max_depth = args["max_depth"].as_u64().unwrap_or(3) as usize;

        let target_dir = if path_str == "." {
            self.workspace.clone()
        } else if Path::new(path_str).is_absolute() {
            PathBuf::from(path_str)
        } else {
            self.workspace.join(path_str)
        };

        if !target_dir.exists() {
            return Err(ToolError::ExecutionFailed {
                name: "file_list".into(),
                message: format!("Directory '{}' does not exist", path_str),
            });
        }

        if !target_dir.is_dir() {
            return Err(ToolError::ExecutionFailed {
                name: "file_list".into(),
                message: format!("'{}' is not a directory", path_str),
            });
        }

        debug!(path = %target_dir.display(), recursive, max_depth, "Listing directory");

        let mut entries = Vec::new();

        if recursive {
            // Use ignore crate for .gitignore-aware walking
            let walker = ignore::WalkBuilder::new(&target_dir)
                .max_depth(Some(max_depth))
                .hidden(false)
                .git_ignore(true)
                .build();

            for entry in walker {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path == target_dir {
                            continue;
                        }
                        let relative = path.strip_prefix(&target_dir).unwrap_or(path);
                        let type_indicator = if path.is_dir() { "/" } else { "" };
                        entries.push(format!("{}{}", relative.display(), type_indicator));
                    }
                    Err(e) => {
                        warn!("Error walking directory: {}", e);
                    }
                }
            }
        } else {
            let mut read_dir =
                tokio::fs::read_dir(&target_dir)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "file_list".into(),
                        message: format!("Failed to read directory '{}': {}", path_str, e),
                    })?;

            while let Some(entry) =
                read_dir
                    .next_entry()
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "file_list".into(),
                        message: format!("Error reading entry: {}", e),
                    })?
            {
                let file_type =
                    entry
                        .file_type()
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "file_list".into(),
                            message: format!("Error reading file type: {}", e),
                        })?;

                let name = entry.file_name().to_string_lossy().to_string();
                let type_indicator = if file_type.is_dir() { "/" } else { "" };
                entries.push(format!("{}{}", name, type_indicator));
            }
        }

        entries.sort();
        let output = if entries.is_empty() {
            format!("Directory '{}' is empty", path_str)
        } else {
            format!("Contents of '{}':\n{}", path_str, entries.join("\n"))
        };

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

/// Search for text patterns within files.
pub struct FileSearchTool {
    workspace: PathBuf,
}

impl FileSearchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Search for a text pattern within files in the workspace. Returns matching lines with file paths and line numbers."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text pattern to search for (supports regex)"
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search in (relative to workspace). Defaults to workspace root."
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Glob pattern for filtering files (e.g., '*.rs', '*.py')"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return. Default: 50."
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let pattern = args["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_search".into(),
                reason: "'pattern' parameter is required".into(),
            })?;

        let search_path = args["path"].as_str().unwrap_or(".");
        let file_pattern = args["file_pattern"].as_str();
        let max_results = args["max_results"].as_u64().unwrap_or(50) as usize;

        let target_dir = if search_path == "." {
            self.workspace.clone()
        } else {
            self.workspace.join(search_path)
        };

        debug!(
            pattern = pattern,
            path = %target_dir.display(),
            "Searching files"
        );

        let mut results = Vec::new();
        let pattern_lower = pattern.to_lowercase();

        // Walk files respecting .gitignore
        let walker = ignore::WalkBuilder::new(&target_dir)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker {
            if results.len() >= max_results {
                break;
            }

            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Filter by file pattern if specified
            if let Some(fp) = file_pattern {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !Self::simple_glob_match(fp, file_name) {
                    continue;
                }
            }

            // Read and search the file
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue, // Skip binary or unreadable files
            };

            let relative = path.strip_prefix(&self.workspace).unwrap_or(path);

            for (line_num, line) in content.lines().enumerate() {
                if results.len() >= max_results {
                    break;
                }
                if line.to_lowercase().contains(&pattern_lower) {
                    results.push(format!(
                        "{}:{}: {}",
                        relative.display(),
                        line_num + 1,
                        line.trim()
                    ));
                }
            }
        }

        let output = if results.is_empty() {
            format!("No matches found for pattern '{}'", pattern)
        } else {
            format!(
                "Found {} match{}:\n{}",
                results.len(),
                if results.len() == 1 { "" } else { "es" },
                results.join("\n")
            )
        };

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

impl FileSearchTool {
    fn simple_glob_match(pattern: &str, name: &str) -> bool {
        if pattern.starts_with("*.") {
            let ext = &pattern[1..];
            name.ends_with(ext)
        } else if let Some(prefix) = pattern.strip_suffix("*") {
            name.starts_with(prefix)
        } else {
            name == pattern
        }
    }
}

/// Write contents to a file (create or overwrite).
pub struct FileWriteTool {
    workspace: PathBuf,
}

impl FileWriteTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Creates parent directories as needed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative to workspace)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_write".into(),
                reason: "'path' parameter is required".into(),
            })?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_write".into(),
                reason: "'content' parameter is required".into(),
            })?;

        // Validate the path stays inside the workspace
        let _ = validate_workspace_path(&self.workspace, path_str, "file_write")?;
        let path = self.workspace.join(path_str);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "file_write".into(),
                    message: format!("Failed to create directories: {}", e),
                })?;
        }

        let existed = path.exists();
        let bytes = content.len();

        debug!(path = %path.display(), bytes, existed, "Writing file");

        tokio::fs::write(&path, content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "file_write".into(),
                message: format!("Failed to write '{}': {}", path_str, e),
            })?;

        let action = if existed { "Updated" } else { "Created" };
        let artifact = if existed {
            Artifact::FileModified {
                path: PathBuf::from(path_str),
                diff: String::new(), // Diff computed separately if needed
            }
        } else {
            Artifact::FileCreated {
                path: PathBuf::from(path_str),
            }
        };

        Ok(
            ToolOutput::text(format!("{} '{}' ({} bytes)", action, path_str, bytes))
                .with_artifact(artifact),
        )
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

/// Apply a unified diff patch to a file.
pub struct FilePatchTool {
    workspace: PathBuf,
}

impl FilePatchTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for FilePatchTool {
    fn name(&self) -> &str {
        "file_patch"
    }

    fn description(&self) -> &str {
        "Apply a text replacement to a file. Specify the old text to find and the new text to replace it with."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to patch (relative to workspace)"
                },
                "old_text": {
                    "type": "string",
                    "description": "The exact text to find in the file"
                },
                "new_text": {
                    "type": "string",
                    "description": "The replacement text"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_patch".into(),
                reason: "'path' parameter is required".into(),
            })?;
        let old_text = args["old_text"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_patch".into(),
                reason: "'old_text' parameter is required".into(),
            })?;
        let new_text = args["new_text"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "file_patch".into(),
                reason: "'new_text' parameter is required".into(),
            })?;

        // Validate the path stays inside the workspace
        let _ = validate_workspace_path(&self.workspace, path_str, "file_patch")?;
        let path = self.workspace.join(path_str);

        let content =
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "file_patch".into(),
                    message: format!("Failed to read '{}': {}", path_str, e),
                })?;

        if !content.contains(old_text) {
            return Err(ToolError::ExecutionFailed {
                name: "file_patch".into(),
                message: format!(
                    "Could not find the specified text in '{}'. The old_text must match exactly.",
                    path_str
                ),
            });
        }

        let count = content.matches(old_text).count();
        let new_content = content.replacen(old_text, new_text, 1);

        tokio::fs::write(&path, &new_content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "file_patch".into(),
                message: format!("Failed to write '{}': {}", path_str, e),
            })?;

        let mut output = ToolOutput::text(format!(
            "Patched '{}' ({} occurrence{} found, replaced first)",
            path_str,
            count,
            if count == 1 { "" } else { "s" }
        ));
        output.artifacts.push(Artifact::FileModified {
            path: PathBuf::from(path_str),
            diff: format!(
                "- {}\n+ {}",
                old_text.lines().next().unwrap_or(""),
                new_text.lines().next().unwrap_or("")
            ),
        });

        Ok(output)
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_workspace() -> TempDir {
        let dir = TempDir::new().unwrap();
        // Create some test files
        std::fs::write(
            dir.path().join("hello.txt"),
            "Hello, World!\nLine 2\nLine 3\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/main.rs"),
            "fn main() {\n    println!(\"Hello\");\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn add(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )
        .unwrap();
        dir
    }

    // --- FileReadTool tests ---

    #[tokio::test]
    async fn test_file_read_basic() {
        let dir = setup_workspace();
        let tool = FileReadTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": "hello.txt"}))
            .await
            .unwrap();
        assert!(result.content.contains("Hello, World!"));
        assert!(result.content.contains("1 |")); // line numbers
    }

    #[tokio::test]
    async fn test_file_read_with_line_range() {
        let dir = setup_workspace();
        let tool = FileReadTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": "hello.txt", "start_line": 2, "end_line": 3}))
            .await
            .unwrap();
        assert!(result.content.contains("Line 2"));
        assert!(result.content.contains("Line 3"));
        assert!(!result.content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_file_read_missing_file() {
        let dir = setup_workspace();
        let tool = FileReadTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": "nonexistent.txt"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_file_read_missing_path_param() {
        let dir = setup_workspace();
        let tool = FileReadTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, .. } => assert_eq!(name, "file_read"),
            e => panic!("Expected InvalidArguments, got: {:?}", e),
        }
    }

    #[test]
    fn test_file_read_properties() {
        let tool = FileReadTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "file_read");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("Read"));
    }

    // --- FileListTool tests ---

    #[tokio::test]
    async fn test_file_list_basic() {
        let dir = setup_workspace();
        let tool = FileListTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("hello.txt"));
        assert!(result.content.contains("src/"));
    }

    #[tokio::test]
    async fn test_file_list_subdirectory() {
        let dir = setup_workspace();
        let tool = FileListTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": "src"}))
            .await
            .unwrap();
        assert!(result.content.contains("main.rs"));
        assert!(result.content.contains("lib.rs"));
    }

    #[tokio::test]
    async fn test_file_list_recursive() {
        let dir = setup_workspace();
        let tool = FileListTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": ".", "recursive": true}))
            .await
            .unwrap();
        assert!(result.content.contains("src/main.rs") || result.content.contains("src\\main.rs"));
    }

    #[tokio::test]
    async fn test_file_list_nonexistent_dir() {
        let dir = setup_workspace();
        let tool = FileListTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"path": "nonexistent"}))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_list_properties() {
        let tool = FileListTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "file_list");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    // --- FileSearchTool tests ---

    #[tokio::test]
    async fn test_file_search_basic() {
        let dir = setup_workspace();
        let tool = FileSearchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"pattern": "Hello"}))
            .await
            .unwrap();
        assert!(result.content.contains("match"));
        assert!(result.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_file_search_with_file_pattern() {
        let dir = setup_workspace();
        let tool = FileSearchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"pattern": "fn", "file_pattern": "*.rs"}))
            .await
            .unwrap();
        assert!(result.content.contains("fn main"));
    }

    #[tokio::test]
    async fn test_file_search_no_results() {
        let dir = setup_workspace();
        let tool = FileSearchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"pattern": "xyznonexistent123"}))
            .await
            .unwrap();
        assert!(result.content.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_file_search_case_insensitive() {
        let dir = setup_workspace();
        let tool = FileSearchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({"pattern": "hello"}))
            .await
            .unwrap();
        // Should find "Hello" despite searching for "hello"
        assert!(result.content.contains("match"));
    }

    #[test]
    fn test_file_search_properties() {
        let tool = FileSearchTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "file_search");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_simple_glob_match() {
        assert!(FileSearchTool::simple_glob_match("*.rs", "main.rs"));
        assert!(FileSearchTool::simple_glob_match("*.rs", "lib.rs"));
        assert!(!FileSearchTool::simple_glob_match("*.rs", "main.py"));
        assert!(FileSearchTool::simple_glob_match("test*", "test_file.rs"));
        assert!(FileSearchTool::simple_glob_match("main.rs", "main.rs"));
        assert!(!FileSearchTool::simple_glob_match("main.rs", "lib.rs"));
    }

    // --- FileWriteTool tests ---

    #[tokio::test]
    async fn test_file_write_create_new() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "new_file.txt",
                "content": "New content!"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Created"));
        assert!(result.artifacts.len() == 1);
        assert!(matches!(&result.artifacts[0], Artifact::FileCreated { .. }));

        let content = std::fs::read_to_string(dir.path().join("new_file.txt")).unwrap();
        assert_eq!(content, "New content!");
    }

    #[tokio::test]
    async fn test_file_write_overwrite_existing() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "hello.txt",
                "content": "Overwritten!"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Updated"));

        let content = std::fs::read_to_string(dir.path().join("hello.txt")).unwrap();
        assert_eq!(content, "Overwritten!");
    }

    #[tokio::test]
    async fn test_file_write_creates_directories() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "deep/nested/dir/file.txt",
                "content": "Deep content"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Created"));
        assert!(dir.path().join("deep/nested/dir/file.txt").exists());
    }

    #[tokio::test]
    async fn test_file_write_missing_params() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool.execute(serde_json::json!({"path": "test.txt"})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_file_write_properties() {
        let tool = FileWriteTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "file_write");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    // --- FilePatchTool tests ---

    #[tokio::test]
    async fn test_file_patch_basic() {
        let dir = setup_workspace();
        let tool = FilePatchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "hello.txt",
                "old_text": "Hello, World!",
                "new_text": "Hi, World!"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Patched"));

        let content = std::fs::read_to_string(dir.path().join("hello.txt")).unwrap();
        assert!(content.contains("Hi, World!"));
        assert!(!content.contains("Hello, World!"));
    }

    #[tokio::test]
    async fn test_file_patch_text_not_found() {
        let dir = setup_workspace();
        let tool = FilePatchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "hello.txt",
                "old_text": "nonexistent text",
                "new_text": "replacement"
            }))
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::ExecutionFailed { name, message } => {
                assert_eq!(name, "file_patch");
                assert!(message.contains("Could not find"));
            }
            e => panic!("Expected ExecutionFailed, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_file_patch_missing_file() {
        let dir = setup_workspace();
        let tool = FilePatchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "nonexistent.txt",
                "old_text": "old",
                "new_text": "new"
            }))
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_file_patch_properties() {
        let tool = FilePatchTool::new(PathBuf::from("/tmp"));
        assert_eq!(tool.name(), "file_patch");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    // --- Workspace boundary validation tests ---

    #[tokio::test]
    async fn test_file_write_rejects_path_traversal() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "../../escape.txt",
                "content": "escaped!"
            }))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::PermissionDenied { name, .. } => assert_eq!(name, "file_write"),
            e => panic!("Expected PermissionDenied, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_file_write_rejects_absolute_path_outside_workspace() {
        let dir = setup_workspace();
        let tool = FileWriteTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "/tmp/escape.txt",
                "content": "escaped!"
            }))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::PermissionDenied { name, .. } => assert_eq!(name, "file_write"),
            e => panic!("Expected PermissionDenied, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_file_patch_rejects_path_traversal() {
        let dir = setup_workspace();
        let tool = FilePatchTool::new(dir.path().to_path_buf());

        let result = tool
            .execute(serde_json::json!({
                "path": "../../escape.txt",
                "old_text": "old",
                "new_text": "new"
            }))
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::PermissionDenied { name, .. } => assert_eq!(name, "file_patch"),
            e => panic!("Expected PermissionDenied, got: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_file_read_rejects_path_traversal() {
        let dir = setup_workspace();
        let tool = FileReadTool::new(dir.path().to_path_buf());

        // Attempt to read outside workspace using path traversal
        let result = tool
            .execute(serde_json::json!({"path": "../../etc/passwd"}))
            .await;
        assert!(result.is_err());
    }
}
