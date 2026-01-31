//! Language server discovery and configuration.
//!
//! Provides automatic detection of language servers based on file extensions,
//! a registry of known server configurations, and utilities for checking
//! server availability on the system PATH.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use tracing::debug;

/// Configuration for a single language server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// The programming language this server handles (e.g., "rust", "python").
    pub language_id: String,
    /// The command to start the language server.
    pub command: String,
    /// Command-line arguments.
    pub args: Vec<String>,
    /// File extensions this server handles.
    pub file_extensions: Vec<String>,
}

/// Registry of language server configurations.
///
/// Holds a mapping from language ID to [`ServerConfig`], with built-in defaults
/// for common languages and the ability to register custom or override configs.
pub struct ServerRegistry {
    configs: HashMap<String, ServerConfig>,
}

impl ServerRegistry {
    /// Create an empty registry with no built-in configurations.
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Create a registry pre-populated with built-in language server configurations.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        let defaults = vec![
            ServerConfig {
                language_id: "rust".into(),
                command: "rust-analyzer".into(),
                args: vec![],
                file_extensions: vec!["rs".into()],
            },
            ServerConfig {
                language_id: "python".into(),
                command: "pyright-langserver".into(),
                args: vec!["--stdio".into()],
                file_extensions: vec!["py".into()],
            },
            ServerConfig {
                language_id: "typescript".into(),
                command: "typescript-language-server".into(),
                args: vec!["--stdio".into()],
                file_extensions: vec!["ts".into(), "tsx".into()],
            },
            ServerConfig {
                language_id: "javascript".into(),
                command: "typescript-language-server".into(),
                args: vec!["--stdio".into()],
                file_extensions: vec!["js".into(), "jsx".into()],
            },
            ServerConfig {
                language_id: "go".into(),
                command: "gopls".into(),
                args: vec!["serve".into()],
                file_extensions: vec!["go".into()],
            },
            ServerConfig {
                language_id: "cpp".into(),
                command: "clangd".into(),
                args: vec![],
                file_extensions: vec![
                    "c".into(),
                    "cpp".into(),
                    "h".into(),
                    "hpp".into(),
                    "cc".into(),
                ],
            },
            ServerConfig {
                language_id: "java".into(),
                command: "jdtls".into(),
                args: vec![],
                file_extensions: vec!["java".into()],
            },
            ServerConfig {
                language_id: "yaml".into(),
                command: "yaml-language-server".into(),
                args: vec!["--stdio".into()],
                file_extensions: vec!["yaml".into(), "yml".into()],
            },
            ServerConfig {
                language_id: "json".into(),
                command: "vscode-json-language-server".into(),
                args: vec!["--stdio".into()],
                file_extensions: vec!["json".into()],
            },
            ServerConfig {
                language_id: "toml".into(),
                command: "taplo".into(),
                args: vec!["lsp".into(), "stdio".into()],
                file_extensions: vec!["toml".into()],
            },
        ];

        for config in defaults {
            registry.configs.insert(config.language_id.clone(), config);
        }

        registry
    }

    /// Register or override a server configuration.
    ///
    /// If a config for the same `language_id` already exists, it will be replaced.
    pub fn register(&mut self, config: ServerConfig) {
        debug!(language_id = %config.language_id, command = %config.command, "Registering language server config");
        self.configs.insert(config.language_id.clone(), config);
    }

    /// Get the server configuration for a given language ID.
    pub fn get(&self, language_id: &str) -> Option<&ServerConfig> {
        self.configs.get(language_id)
    }

    /// Detect the language from a file path's extension.
    ///
    /// Returns the canonical language identifier (e.g., `"rust"`, `"python"`) or
    /// `None` if the extension is unrecognized or absent.
    pub fn detect_language(file_path: &Path) -> Option<String> {
        let ext = file_path.extension()?.to_str()?;
        let language = match ext {
            "rs" => "rust",
            "py" => "python",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "hpp" | "cc" | "cxx" => "cpp",
            "java" => "java",
            "yaml" | "yml" => "yaml",
            "json" => "json",
            "toml" => "toml",
            "rb" => "ruby",
            "sh" | "bash" => "bash",
            _ => return None,
        };
        Some(language.to_string())
    }

    /// Find the appropriate server configuration for a file path.
    ///
    /// Detects the language from the file extension, then looks up the
    /// corresponding config in the registry.
    pub fn find_config_for_file(&self, file_path: &Path) -> Option<&ServerConfig> {
        let language = Self::detect_language(file_path)?;
        self.get(&language)
    }

    /// Check whether the server command is available on the system PATH.
    pub fn is_server_available(config: &ServerConfig) -> bool {
        Command::new("which")
            .arg(&config.command)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    /// Return a sorted list of all registered language IDs.
    pub fn list_languages(&self) -> Vec<String> {
        let mut languages: Vec<String> = self.configs.keys().cloned().collect();
        languages.sort();
        languages
    }

    /// Return configurations for which the server binary is available on PATH.
    pub fn list_available_servers(&self) -> Vec<&ServerConfig> {
        self.configs
            .values()
            .filter(|config| Self::is_server_available(config))
            .collect()
    }
}

impl Default for ServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_new_creates_empty() {
        let registry = ServerRegistry::new();
        assert!(registry.configs.is_empty());
        assert!(registry.list_languages().is_empty());
    }

    #[test]
    fn test_with_defaults_has_configs() {
        let registry = ServerRegistry::with_defaults();
        assert!(registry.get("rust").is_some());
        assert!(registry.get("python").is_some());
        assert!(registry.get("typescript").is_some());
        assert!(registry.get("javascript").is_some());
        assert!(registry.get("go").is_some());
        assert!(registry.get("cpp").is_some());
        assert!(registry.get("java").is_some());
        assert!(registry.get("yaml").is_some());
        assert!(registry.get("json").is_some());
        assert!(registry.get("toml").is_some());
    }

    #[test]
    fn test_register_custom() {
        let mut registry = ServerRegistry::new();
        registry.register(ServerConfig {
            language_id: "ruby".into(),
            command: "solargraph".into(),
            args: vec!["stdio".into()],
            file_extensions: vec!["rb".into()],
        });

        let config = registry.get("ruby").expect("ruby config should exist");
        assert_eq!(config.command, "solargraph");
        assert_eq!(config.args, vec!["stdio"]);
        assert_eq!(config.file_extensions, vec!["rb"]);
    }

    #[test]
    fn test_register_override() {
        let mut registry = ServerRegistry::with_defaults();
        let original = registry.get("rust").unwrap();
        assert_eq!(original.command, "rust-analyzer");

        registry.register(ServerConfig {
            language_id: "rust".into(),
            command: "custom-rust-ls".into(),
            args: vec!["--custom".into()],
            file_extensions: vec!["rs".into()],
        });

        let overridden = registry.get("rust").unwrap();
        assert_eq!(overridden.command, "custom-rust-ls");
        assert_eq!(overridden.args, vec!["--custom"]);
    }

    #[test]
    fn test_get_known_language() {
        let registry = ServerRegistry::with_defaults();
        let config = registry.get("rust").expect("should find rust config");
        assert_eq!(config.command, "rust-analyzer");
        assert_eq!(config.language_id, "rust");
        assert!(config.file_extensions.contains(&"rs".to_string()));
    }

    #[test]
    fn test_get_unknown_language() {
        let registry = ServerRegistry::with_defaults();
        assert!(registry.get("fortran").is_none());
    }

    #[test]
    fn test_detect_language_rs() {
        let path = PathBuf::from("main.rs");
        assert_eq!(ServerRegistry::detect_language(&path), Some("rust".into()));
    }

    #[test]
    fn test_detect_language_py() {
        let path = PathBuf::from("script.py");
        assert_eq!(
            ServerRegistry::detect_language(&path),
            Some("python".into())
        );
    }

    #[test]
    fn test_detect_language_ts() {
        let path = PathBuf::from("app.ts");
        assert_eq!(
            ServerRegistry::detect_language(&path),
            Some("typescript".into())
        );
    }

    #[test]
    fn test_detect_language_tsx() {
        let path = PathBuf::from("App.tsx");
        assert_eq!(
            ServerRegistry::detect_language(&path),
            Some("typescript".into())
        );
    }

    #[test]
    fn test_detect_language_go() {
        let path = PathBuf::from("main.go");
        assert_eq!(ServerRegistry::detect_language(&path), Some("go".into()));
    }

    #[test]
    fn test_detect_language_unknown() {
        let path = PathBuf::from("file.xyz");
        assert_eq!(ServerRegistry::detect_language(&path), None);
    }

    #[test]
    fn test_detect_language_no_extension() {
        let path = PathBuf::from("Makefile");
        assert_eq!(ServerRegistry::detect_language(&path), None);
    }

    #[test]
    fn test_find_config_for_file() {
        let registry = ServerRegistry::with_defaults();
        let path = PathBuf::from("main.rs");
        let config = registry
            .find_config_for_file(&path)
            .expect("should find config for .rs file");
        assert_eq!(config.language_id, "rust");
        assert_eq!(config.command, "rust-analyzer");
    }

    #[test]
    fn test_find_config_for_unknown_file() {
        let registry = ServerRegistry::with_defaults();
        let path = PathBuf::from("data.xyz");
        assert!(registry.find_config_for_file(&path).is_none());
    }

    #[test]
    fn test_list_languages() {
        let registry = ServerRegistry::with_defaults();
        let languages = registry.list_languages();

        // Should be sorted
        let mut sorted = languages.clone();
        sorted.sort();
        assert_eq!(languages, sorted);

        // Should contain all defaults
        assert!(languages.contains(&"rust".to_string()));
        assert!(languages.contains(&"python".to_string()));
        assert!(languages.contains(&"go".to_string()));
        assert!(languages.contains(&"typescript".to_string()));
        assert!(languages.contains(&"javascript".to_string()));
        assert!(languages.contains(&"cpp".to_string()));
        assert!(languages.contains(&"java".to_string()));
        assert!(languages.contains(&"yaml".to_string()));
        assert!(languages.contains(&"json".to_string()));
        assert!(languages.contains(&"toml".to_string()));
    }

    #[test]
    fn test_server_config_serde() {
        let config = ServerConfig {
            language_id: "rust".into(),
            command: "rust-analyzer".into(),
            args: vec!["--log-file".into(), "/tmp/ra.log".into()],
            file_extensions: vec!["rs".into()],
        };

        let json = serde_json::to_string(&config).expect("serialization should succeed");
        let deserialized: ServerConfig =
            serde_json::from_str(&json).expect("deserialization should succeed");

        assert_eq!(deserialized.language_id, config.language_id);
        assert_eq!(deserialized.command, config.command);
        assert_eq!(deserialized.args, config.args);
        assert_eq!(deserialized.file_extensions, config.file_extensions);
    }
}
