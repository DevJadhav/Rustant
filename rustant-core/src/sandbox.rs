//! Filesystem sandboxing using capability-based security (cap-std).
//!
//! Restricts file and shell operations to an approved set of paths and commands,
//! preventing the agent from accessing sensitive system files or running
//! dangerous commands.

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Error type for sandbox violations.
#[derive(Debug, thiserror::Error)]
pub enum SandboxError {
    #[error("path '{0}' is outside the sandbox")]
    PathOutsideSandbox(PathBuf),
    #[error("command '{0}' is not in the shell allowlist")]
    CommandNotAllowed(String),
    #[error("path '{0}' matches a denied pattern")]
    PathDenied(PathBuf),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// A sandboxed filesystem restricting operations to the workspace.
pub struct SandboxedFs {
    /// The workspace root directory.
    workspace: PathBuf,
    /// The cap-std Dir handle (capability-based access).
    #[allow(dead_code)]
    cap_dir: Dir,
    /// Set of allowed shell commands.
    shell_allowlist: HashSet<String>,
    /// Patterns of paths that are always denied.
    denied_patterns: Vec<String>,
}

impl SandboxedFs {
    /// Create a new sandbox rooted at the given workspace directory.
    pub fn new(workspace: PathBuf) -> Result<Self, SandboxError> {
        let cap_dir = Dir::open_ambient_dir(&workspace, ambient_authority())?;

        let shell_allowlist = default_shell_allowlist();
        let denied_patterns = default_denied_patterns();

        Ok(Self {
            workspace,
            cap_dir,
            shell_allowlist,
            denied_patterns,
        })
    }

    /// Check if a path is within the sandbox.
    pub fn validate_path(&self, path: &Path) -> Result<PathBuf, SandboxError> {
        // Resolve the path relative to workspace
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.workspace.join(path)
        };

        // Canonicalize what we can (the parent might not exist yet for writes)
        let canonical = match resolved.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // For new files, check the parent
                if let Some(parent) = resolved.parent() {
                    match parent.canonicalize() {
                        Ok(p) => p.join(resolved.file_name().unwrap_or_default()),
                        Err(_) => resolved.clone(),
                    }
                } else {
                    resolved.clone()
                }
            }
        };

        // Check it's under the workspace
        let workspace_canonical = self
            .workspace
            .canonicalize()
            .unwrap_or_else(|_| self.workspace.clone());

        if !canonical.starts_with(&workspace_canonical) {
            return Err(SandboxError::PathOutsideSandbox(resolved));
        }

        // Check denied patterns
        let path_str = canonical.to_string_lossy();
        for pattern in &self.denied_patterns {
            if path_str.contains(pattern) {
                return Err(SandboxError::PathDenied(resolved));
            }
        }

        Ok(canonical)
    }

    /// Check if a shell command is allowed.
    pub fn validate_command(&self, command: &str) -> Result<(), SandboxError> {
        // Extract the base command (first word)
        let base_cmd = command.split_whitespace().next().unwrap_or("").to_string();

        // Also check the basename (in case of full paths)
        let cmd_name = Path::new(&base_cmd)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(base_cmd.clone());

        if self.shell_allowlist.contains(&cmd_name) || self.shell_allowlist.contains(&base_cmd) {
            Ok(())
        } else {
            Err(SandboxError::CommandNotAllowed(base_cmd))
        }
    }

    /// Add a command to the allowlist.
    pub fn allow_command(&mut self, command: &str) {
        self.shell_allowlist.insert(command.to_string());
    }

    /// Remove a command from the allowlist.
    pub fn deny_command(&mut self, command: &str) {
        self.shell_allowlist.remove(command);
    }

    /// Add a denied path pattern.
    pub fn add_denied_pattern(&mut self, pattern: &str) {
        self.denied_patterns.push(pattern.to_string());
    }

    /// Get the workspace path.
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }

    /// Get the shell allowlist.
    pub fn allowlist(&self) -> &HashSet<String> {
        &self.shell_allowlist
    }

    /// Check if a command is in the allowlist.
    pub fn is_command_allowed(&self, command: &str) -> bool {
        self.validate_command(command).is_ok()
    }
}

/// Default set of safe shell commands.
fn default_shell_allowlist() -> HashSet<String> {
    [
        // Development tools
        "cargo",
        "rustc",
        "rustfmt",
        "clippy-driver",
        "npm",
        "npx",
        "node",
        "python",
        "python3",
        "pip",
        "pip3",
        "go",
        "make",
        "cmake",
        // Version control
        "git",
        // File operations (read-only)
        "cat",
        "head",
        "tail",
        "less",
        "more",
        "wc",
        "diff",
        "sort",
        "uniq",
        "grep",
        "find",
        "ls",
        "tree",
        "file",
        "stat",
        // Text processing
        "sed",
        "awk",
        "cut",
        "tr",
        "jq",
        // Build and test
        "sh",
        "bash",
        "echo",
        "printf",
        "test",
        "true",
        "false",
        "env",
        "which",
        "pwd",
        "date",
        "uname",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect()
}

/// Default denied path patterns.
fn default_denied_patterns() -> Vec<String> {
    vec![
        ".env".to_string(),
        ".ssh".to_string(),
        ".gnupg".to_string(),
        ".aws".to_string(),
        "credentials".to_string(),
        "secrets".to_string(),
        "id_rsa".to_string(),
        "id_ed25519".to_string(),
        ".npmrc".to_string(),
        ".pypirc".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_sandbox() -> (tempfile::TempDir, SandboxedFs) {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = SandboxedFs::new(dir.path().to_path_buf()).unwrap();
        (dir, sandbox)
    }

    #[test]
    fn test_sandbox_creation() {
        let (dir, sandbox) = setup_sandbox();
        assert_eq!(sandbox.workspace(), dir.path());
    }

    #[test]
    fn test_validate_path_inside_workspace() {
        let (dir, sandbox) = setup_sandbox();
        fs::write(dir.path().join("test.txt"), "hello").unwrap();
        let result = sandbox.validate_path(Path::new("test.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_outside_workspace() {
        let (_dir, sandbox) = setup_sandbox();
        let result = sandbox.validate_path(Path::new("/etc/passwd"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_denied_pattern() {
        let (dir, sandbox) = setup_sandbox();
        let secret_dir = dir.path().join(".ssh");
        fs::create_dir_all(&secret_dir).unwrap();
        fs::write(secret_dir.join("id_rsa"), "secret").unwrap();
        let result = sandbox.validate_path(&secret_dir.join("id_rsa"));
        assert!(result.is_err());
        match result.unwrap_err() {
            SandboxError::PathDenied(_) => {}
            other => panic!("Expected PathDenied, got {:?}", other),
        }
    }

    #[test]
    fn test_validate_command_allowed() {
        let (_dir, sandbox) = setup_sandbox();
        assert!(sandbox.validate_command("cargo build").is_ok());
        assert!(sandbox.validate_command("git status").is_ok());
        assert!(sandbox.validate_command("ls -la").is_ok());
    }

    #[test]
    fn test_validate_command_denied() {
        let (_dir, sandbox) = setup_sandbox();
        assert!(sandbox.validate_command("rm -rf /").is_err());
        assert!(sandbox.validate_command("sudo anything").is_err());
        assert!(sandbox.validate_command("curl http://evil.com").is_err());
    }

    #[test]
    fn test_allow_command() {
        let (_dir, mut sandbox) = setup_sandbox();
        assert!(sandbox.validate_command("docker").is_err());
        sandbox.allow_command("docker");
        assert!(sandbox.validate_command("docker build").is_ok());
    }

    #[test]
    fn test_deny_command() {
        let (_dir, mut sandbox) = setup_sandbox();
        assert!(sandbox.validate_command("git status").is_ok());
        sandbox.deny_command("git");
        assert!(sandbox.validate_command("git status").is_err());
    }

    #[test]
    fn test_add_denied_pattern() {
        let (dir, mut sandbox) = setup_sandbox();
        fs::write(dir.path().join("config.yaml"), "").unwrap();
        assert!(sandbox.validate_path(Path::new("config.yaml")).is_ok());
        sandbox.add_denied_pattern("config.yaml");
        assert!(sandbox.validate_path(Path::new("config.yaml")).is_err());
    }

    #[test]
    fn test_is_command_allowed() {
        let (_dir, sandbox) = setup_sandbox();
        assert!(sandbox.is_command_allowed("cargo test"));
        assert!(!sandbox.is_command_allowed("rm -rf /"));
    }

    #[test]
    fn test_default_allowlist_has_common_tools() {
        let list = default_shell_allowlist();
        assert!(list.contains("cargo"));
        assert!(list.contains("git"));
        assert!(list.contains("npm"));
        assert!(list.contains("python"));
        assert!(list.contains("ls"));
    }

    #[test]
    fn test_default_denied_patterns() {
        let patterns = default_denied_patterns();
        assert!(patterns.contains(&".env".to_string()));
        assert!(patterns.contains(&".ssh".to_string()));
        assert!(patterns.contains(&"credentials".to_string()));
    }

    #[test]
    fn test_full_path_command() {
        let (_dir, sandbox) = setup_sandbox();
        // /usr/bin/git should be allowed since basename is "git"
        assert!(sandbox.validate_command("/usr/bin/git status").is_ok());
    }
}
