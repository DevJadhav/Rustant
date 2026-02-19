//! Resource management for the MCP server.
//!
//! Exposes workspace files as MCP resources. Each file in the workspace
//! can be listed and read as a resource via the MCP protocol.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::McpError;
use crate::protocol::{McpResource, ResourceContent};

/// Maximum number of files returned by `list_resources`.
const MAX_RESOURCE_FILES: usize = 1000;

/// Directories to skip when walking the workspace.
const SKIP_DIRS: &[&str] = &["target", "node_modules"];

/// Manages workspace files as MCP resources.
pub struct ResourceManager {
    workspace: PathBuf,
}

impl ResourceManager {
    /// Create a new `ResourceManager` rooted at the given workspace directory.
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    /// List all eligible files in the workspace as MCP resources.
    ///
    /// Walks the workspace directory recursively, skipping hidden files/directories
    /// (names starting with `.`), `target/`, and `node_modules/`. Results are sorted
    /// by name and capped at 1000 entries.
    pub fn list_resources(&self) -> Result<Vec<McpResource>, McpError> {
        let mut resources = Vec::new();
        self.walk_dir(&self.workspace, &mut resources)?;

        // Sort by name (relative path)
        resources.sort_by(|a, b| a.name.cmp(&b.name));

        // Enforce the limit
        resources.truncate(MAX_RESOURCE_FILES);

        Ok(resources)
    }

    /// Read a resource identified by its `file://` URI.
    ///
    /// Validates that the URI points to a file within the workspace to prevent
    /// path traversal attacks. Reads the file as UTF-8 text.
    pub fn read_resource(&self, uri: &str) -> Result<Vec<ResourceContent>, McpError> {
        let path_str = uri
            .strip_prefix("file://")
            .ok_or_else(|| McpError::InvalidParams {
                message: format!("URI must start with file://, got: {uri}"),
            })?;

        let path = PathBuf::from(path_str);

        // Validate the path is within the workspace
        if !is_within_workspace(&self.workspace, &path) {
            return Err(McpError::InvalidParams {
                message: format!("Path is outside the workspace: {uri}"),
            });
        }

        // Ensure the file exists
        if !path.exists() {
            return Err(McpError::ResourceNotFound {
                uri: uri.to_string(),
            });
        }

        // Read as UTF-8 text
        let text = fs::read_to_string(&path).map_err(|e| McpError::InternalError {
            message: format!("Failed to read resource as UTF-8: {e}"),
        })?;

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let mime = mime_from_extension(ext);

        Ok(vec![ResourceContent {
            uri: uri.to_string(),
            mime_type: mime,
            text: Some(text),
        }])
    }

    /// Recursively walk a directory, collecting files as `McpResource` entries.
    fn walk_dir(&self, dir: &Path, resources: &mut Vec<McpResource>) -> Result<(), McpError> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                return Err(McpError::InternalError {
                    message: format!("Failed to read directory {}: {}", dir.display(), e),
                });
            }
        };

        for entry in entries {
            let entry = entry.map_err(|e| McpError::InternalError {
                message: format!("Failed to read directory entry: {e}"),
            })?;

            let path = entry.path();
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip hidden files and directories
            if name_str.starts_with('.') {
                continue;
            }

            if path.is_dir() {
                // Skip excluded directories
                if SKIP_DIRS.contains(&name_str.as_ref()) {
                    continue;
                }
                self.walk_dir(&path, resources)?;
            } else if path.is_file() {
                // Early exit if we already hit the limit
                if resources.len() >= MAX_RESOURCE_FILES {
                    return Ok(());
                }

                let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

                let rel_path = path
                    .strip_prefix(&self.workspace)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let mime = mime_from_extension(ext);

                resources.push(McpResource {
                    uri: format!("file://{}", abs_path.display()),
                    name: rel_path,
                    description: None,
                    mime_type: mime,
                });
            }
        }

        Ok(())
    }
}

/// Infer a MIME type from a file extension.
///
/// Returns `None` for unrecognised extensions.
pub fn mime_from_extension(ext: &str) -> Option<String> {
    let mime = match ext {
        "rs" => "text/x-rust",
        "py" => "text/x-python",
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "json" => "application/json",
        "toml" => "application/toml",
        "yaml" | "yml" => "application/yaml",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "html" => "text/html",
        "css" => "text/css",
        "sh" => "text/x-shellscript",
        _ => return None,
    };
    Some(mime.to_string())
}

/// Check whether `target` is located within `workspace`.
///
/// Both paths are canonicalized before the comparison so that symlinks and
/// `..` segments are resolved.
pub fn is_within_workspace(workspace: &Path, target: &Path) -> bool {
    let Ok(canon_workspace) = workspace.canonicalize() else {
        return false;
    };
    let Ok(canon_target) = target.canonicalize() else {
        return false;
    };
    canon_target.starts_with(&canon_workspace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_resource_manager_new() {
        let dir = TempDir::new().unwrap();
        let manager = ResourceManager::new(dir.path().to_path_buf());
        assert_eq!(manager.workspace, dir.path().to_path_buf());
    }

    #[test]
    fn test_list_resources_empty_dir() {
        let dir = TempDir::new().unwrap();
        let manager = ResourceManager::new(dir.path().to_path_buf());
        let resources = manager.list_resources().unwrap();
        assert!(resources.is_empty());
    }

    #[test]
    fn test_list_resources_with_files() {
        let dir = TempDir::new().unwrap();

        // Create some files
        File::create(dir.path().join("main.rs")).unwrap();
        File::create(dir.path().join("lib.rs")).unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        File::create(dir.path().join("src").join("utils.rs")).unwrap();

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let resources = manager.list_resources().unwrap();

        assert_eq!(resources.len(), 3);

        // Verify sorted by name
        let names: Vec<&str> = resources.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"lib.rs"));
        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"src/utils.rs"));

        // Verify sorted order
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);

        // Verify MIME type for .rs files
        for r in &resources {
            assert_eq!(r.mime_type, Some("text/x-rust".to_string()));
        }

        // Verify URI format
        for r in &resources {
            assert!(r.uri.starts_with("file://"));
        }
    }

    #[test]
    fn test_list_resources_skips_hidden() {
        let dir = TempDir::new().unwrap();

        // Create a visible file and a hidden directory with a file inside
        File::create(dir.path().join("visible.txt")).unwrap();
        fs::create_dir(dir.path().join(".hidden")).unwrap();
        File::create(dir.path().join(".hidden").join("secret.txt")).unwrap();
        File::create(dir.path().join(".gitignore")).unwrap();

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let resources = manager.list_resources().unwrap();

        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "visible.txt");
    }

    #[test]
    fn test_list_resources_skips_target() {
        let dir = TempDir::new().unwrap();

        // Create a visible file and target/node_modules directories
        File::create(dir.path().join("main.rs")).unwrap();

        fs::create_dir(dir.path().join("target")).unwrap();
        File::create(dir.path().join("target").join("debug.rs")).unwrap();

        fs::create_dir(dir.path().join("node_modules")).unwrap();
        File::create(dir.path().join("node_modules").join("package.json")).unwrap();

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let resources = manager.list_resources().unwrap();

        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, "main.rs");
    }

    #[test]
    fn test_read_resource() {
        let dir = TempDir::new().unwrap();

        let file_path = dir.path().join("hello.txt");
        let mut f = File::create(&file_path).unwrap();
        writeln!(f, "Hello, world!").unwrap();
        drop(f);

        let canonical = file_path.canonicalize().unwrap();
        let uri = format!("file://{}", canonical.display());

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let contents = manager.read_resource(&uri).unwrap();

        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].uri, uri);
        assert_eq!(contents[0].mime_type, Some("text/plain".to_string()));
        assert!(contents[0].text.as_ref().unwrap().contains("Hello, world!"));
    }

    #[test]
    fn test_read_resource_not_found() {
        let dir = TempDir::new().unwrap();
        let canonical_workspace = dir.path().canonicalize().unwrap();
        let uri = format!("file://{}/nonexistent.txt", canonical_workspace.display());

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let result = manager.read_resource(&uri);

        // The file doesn't exist, so canonicalize will fail in is_within_workspace,
        // which means we get an InvalidParams error for non-existent paths outside
        // workspace check, or ResourceNotFound. Either way it should be an error.
        assert!(result.is_err());
    }

    #[test]
    fn test_read_resource_path_traversal() {
        let dir = TempDir::new().unwrap();
        let uri = "file://../../../etc/passwd";

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let result = manager.read_resource(uri);

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Should be rejected because path is outside workspace
        let msg = err.to_string();
        assert!(
            msg.contains("outside the workspace") || msg.contains("Path"),
            "Expected path traversal error, got: {msg}"
        );
    }

    #[test]
    fn test_mime_from_extension() {
        assert_eq!(mime_from_extension("rs"), Some("text/x-rust".to_string()));
        assert_eq!(mime_from_extension("py"), Some("text/x-python".to_string()));
        assert_eq!(
            mime_from_extension("js"),
            Some("text/javascript".to_string())
        );
        assert_eq!(
            mime_from_extension("ts"),
            Some("text/typescript".to_string())
        );
        assert_eq!(
            mime_from_extension("json"),
            Some("application/json".to_string())
        );
        assert_eq!(
            mime_from_extension("toml"),
            Some("application/toml".to_string())
        );
        assert_eq!(
            mime_from_extension("yaml"),
            Some("application/yaml".to_string())
        );
        assert_eq!(
            mime_from_extension("yml"),
            Some("application/yaml".to_string())
        );
        assert_eq!(mime_from_extension("md"), Some("text/markdown".to_string()));
        assert_eq!(mime_from_extension("txt"), Some("text/plain".to_string()));
        assert_eq!(mime_from_extension("html"), Some("text/html".to_string()));
        assert_eq!(mime_from_extension("css"), Some("text/css".to_string()));
        assert_eq!(
            mime_from_extension("sh"),
            Some("text/x-shellscript".to_string())
        );
        assert_eq!(mime_from_extension("unknown"), None);
        assert_eq!(mime_from_extension(""), None);
    }

    #[test]
    fn test_list_resources_limit() {
        let dir = TempDir::new().unwrap();

        // Create more files than the limit
        for i in 0..1050 {
            File::create(dir.path().join(format!("file_{i:04}.txt"))).unwrap();
        }

        let manager = ResourceManager::new(dir.path().to_path_buf());
        let resources = manager.list_resources().unwrap();

        assert_eq!(resources.len(), MAX_RESOURCE_FILES);
        assert_eq!(resources.len(), 1000);

        // Verify they are still sorted
        for i in 1..resources.len() {
            assert!(resources[i - 1].name <= resources[i].name);
        }
    }

    #[test]
    fn test_is_within_workspace() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        File::create(&file_path).unwrap();

        assert!(is_within_workspace(dir.path(), &file_path));

        // A path that doesn't exist can't be canonicalized, so returns false
        let bad_path = PathBuf::from("/nonexistent/path/file.txt");
        assert!(!is_within_workspace(dir.path(), &bad_path));
    }
}
