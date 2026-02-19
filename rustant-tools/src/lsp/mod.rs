//! LSP Client Integration for Rustant.
//!
//! Provides language server protocol (LSP) client capabilities, enabling the
//! agent to leverage existing language servers for code intelligence such as
//! hover information, go-to-definition, find references, diagnostics,
//! completions, rename, and formatting.
//!
//! ## Architecture
//!
//! ```text
//! LspTool ──> LspManager (LspBackend) ──> LspClient ──> Language Server Process
//!                  │                           │
//!                  └── ServerRegistry           └── Content-Length framing (JSON-RPC 2.0)
//! ```

pub mod client;
pub mod discovery;
pub mod tools;
pub mod types;

use async_trait::async_trait;
use client::{LspClient, LspError};
use discovery::ServerRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};
use types::{CompletionItem, Diagnostic, Location, TextEdit, WorkspaceEdit};

/// Trait abstracting LSP operations for testability.
///
/// The [`LspManager`] implements this trait using real language server processes.
/// Tests can provide mock implementations to exercise the tools without
/// requiring actual language servers.
#[async_trait]
pub trait LspBackend: Send + Sync {
    /// Get hover information at a position.
    async fn hover(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<String>, LspError>;

    /// Go to definition of the symbol at a position.
    async fn definition(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError>;

    /// Find all references to the symbol at a position.
    async fn references(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError>;

    /// Get diagnostics for a file.
    async fn diagnostics(&self, file: &Path) -> Result<Vec<Diagnostic>, LspError>;

    /// Get completion suggestions at a position.
    async fn completions(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>, LspError>;

    /// Rename a symbol across the project.
    async fn rename(
        &self,
        file: &Path,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<WorkspaceEdit, LspError>;

    /// Format a file.
    async fn format(&self, file: &Path) -> Result<Vec<TextEdit>, LspError>;
}

/// Manages language server processes and routes LSP requests.
///
/// The `LspManager` lazily starts language server processes on demand based
/// on file extension. It maintains a cache of running clients and routes
/// requests to the appropriate server.
pub struct LspManager {
    workspace: PathBuf,
    registry: ServerRegistry,
    clients: Mutex<HashMap<String, LspClient>>,
}

impl LspManager {
    /// Create a new `LspManager` for the given workspace directory.
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            registry: ServerRegistry::with_defaults(),
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Create an `LspManager` with a custom server registry.
    pub fn with_registry(workspace: PathBuf, registry: ServerRegistry) -> Self {
        Self {
            workspace,
            registry,
            clients: Mutex::new(HashMap::new()),
        }
    }

    /// Get or start the LSP client for the given file.
    ///
    /// Detects the language from the file extension, looks up the server
    /// configuration, and starts the server if not already running.
    async fn get_client_for_file(&self, file: &Path) -> Result<String, LspError> {
        let language = discovery::ServerRegistry::detect_language(file).ok_or_else(|| {
            let ext = file
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown");
            LspError::UnsupportedLanguage {
                language: ext.to_string(),
            }
        })?;

        let mut clients = self.clients.lock().await;

        if clients.contains_key(&language) {
            return Ok(language);
        }

        let config = self
            .registry
            .get(&language)
            .ok_or_else(|| LspError::UnsupportedLanguage {
                language: language.clone(),
            })?;

        info!(
            language = %language,
            command = %config.command,
            "Starting language server"
        );

        match LspClient::start(&config.command, &config.args, &self.workspace).await {
            Ok(client) => {
                clients.insert(language.clone(), client);
                Ok(language)
            }
            Err(e) => {
                warn!(
                    language = %language,
                    error = %e,
                    "Failed to start language server"
                );
                Err(e)
            }
        }
    }

    /// Extract hover text from the raw hover result.
    fn extract_hover_text(hover: &types::HoverResult) -> String {
        match &hover.contents {
            types::HoverContents::Scalar(marked) => match marked {
                types::MarkedString::String(s) => s.clone(),
                types::MarkedString::LanguageString { language, value } => {
                    format!("```{language}\n{value}\n```")
                }
            },
            types::HoverContents::Markup(markup) => markup.value.clone(),
            types::HoverContents::Array(items) => items
                .iter()
                .map(|m| match m {
                    types::MarkedString::String(s) => s.clone(),
                    types::MarkedString::LanguageString { language, value } => {
                        format!("```{language}\n{value}\n```")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        }
    }
}

#[async_trait]
impl LspBackend for LspManager {
    async fn hover(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<String>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;

        let result = client.hover(file, line, character).await?;
        Ok(result.map(|h| Self::extract_hover_text(&h)))
    }

    async fn definition(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.definition(file, line, character).await
    }

    async fn references(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.references(file, line, character).await
    }

    async fn diagnostics(&self, file: &Path) -> Result<Vec<Diagnostic>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let clients = self.clients.lock().await;
        let client = clients
            .get(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.diagnostics(file)
    }

    async fn completions(
        &self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.completions(file, line, character).await
    }

    async fn rename(
        &self,
        file: &Path,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<WorkspaceEdit, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.rename(file, line, character, new_name).await
    }

    async fn format(&self, file: &Path) -> Result<Vec<TextEdit>, LspError> {
        let language = self.get_client_for_file(file).await?;
        let mut clients = self.clients.lock().await;
        let client = clients
            .get_mut(&language)
            .ok_or_else(|| LspError::ServerNotRunning {
                language: language.clone(),
            })?;
        client.format(file).await
    }
}

/// Create all LSP tools backed by the given `LspBackend`.
///
/// Returns a vector of tool instances ready for registration.
pub fn create_lsp_tools(backend: Arc<dyn LspBackend>) -> Vec<Arc<dyn crate::registry::Tool>> {
    vec![
        Arc::new(tools::LspHoverTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspDefinitionTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspReferencesTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspDiagnosticsTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspCompletionsTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspRenameTool::new(Arc::clone(&backend))),
        Arc::new(tools::LspFormatTool::new(Arc::clone(&backend))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_manager_creation() {
        let manager = LspManager::new(PathBuf::from("/tmp/workspace"));
        assert_eq!(manager.workspace, PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn test_extract_hover_text_scalar_string() {
        let hover = types::HoverResult {
            contents: types::HoverContents::Scalar(types::MarkedString::String(
                "fn main()".to_string(),
            )),
            range: None,
        };
        assert_eq!(LspManager::extract_hover_text(&hover), "fn main()");
    }

    #[test]
    fn test_extract_hover_text_scalar_language_string() {
        let hover = types::HoverResult {
            contents: types::HoverContents::Scalar(types::MarkedString::LanguageString {
                language: "rust".to_string(),
                value: "fn main()".to_string(),
            }),
            range: None,
        };
        assert_eq!(
            LspManager::extract_hover_text(&hover),
            "```rust\nfn main()\n```"
        );
    }

    #[test]
    fn test_extract_hover_text_markup() {
        let hover = types::HoverResult {
            contents: types::HoverContents::Markup(types::MarkupContent {
                kind: "markdown".to_string(),
                value: "# Hello\nWorld".to_string(),
            }),
            range: None,
        };
        assert_eq!(LspManager::extract_hover_text(&hover), "# Hello\nWorld");
    }

    #[test]
    fn test_extract_hover_text_array() {
        let hover = types::HoverResult {
            contents: types::HoverContents::Array(vec![
                types::MarkedString::String("Type: i32".to_string()),
                types::MarkedString::LanguageString {
                    language: "rust".to_string(),
                    value: "let x: i32 = 42;".to_string(),
                },
            ]),
            range: None,
        };
        let text = LspManager::extract_hover_text(&hover);
        assert!(text.contains("Type: i32"));
        assert!(text.contains("```rust\nlet x: i32 = 42;\n```"));
    }

    #[test]
    fn test_create_lsp_tools_returns_seven() {
        use crate::lsp::types::*;

        struct DummyBackend;

        #[async_trait]
        impl LspBackend for DummyBackend {
            async fn hover(&self, _: &Path, _: u32, _: u32) -> Result<Option<String>, LspError> {
                Ok(None)
            }
            async fn definition(
                &self,
                _: &Path,
                _: u32,
                _: u32,
            ) -> Result<Vec<Location>, LspError> {
                Ok(vec![])
            }
            async fn references(
                &self,
                _: &Path,
                _: u32,
                _: u32,
            ) -> Result<Vec<Location>, LspError> {
                Ok(vec![])
            }
            async fn diagnostics(&self, _: &Path) -> Result<Vec<Diagnostic>, LspError> {
                Ok(vec![])
            }
            async fn completions(
                &self,
                _: &Path,
                _: u32,
                _: u32,
            ) -> Result<Vec<CompletionItem>, LspError> {
                Ok(vec![])
            }
            async fn rename(
                &self,
                _: &Path,
                _: u32,
                _: u32,
                _: &str,
            ) -> Result<WorkspaceEdit, LspError> {
                Ok(WorkspaceEdit { changes: None })
            }
            async fn format(&self, _: &Path) -> Result<Vec<TextEdit>, LspError> {
                Ok(vec![])
            }
        }

        let backend: Arc<dyn LspBackend> = Arc::new(DummyBackend);
        let tools = create_lsp_tools(backend);
        assert_eq!(tools.len(), 7);

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"lsp_hover"));
        assert!(names.contains(&"lsp_definition"));
        assert!(names.contains(&"lsp_references"));
        assert!(names.contains(&"lsp_diagnostics"));
        assert!(names.contains(&"lsp_completions"));
        assert!(names.contains(&"lsp_rename"));
        assert!(names.contains(&"lsp_format"));
    }

    #[test]
    fn test_lsp_manager_with_custom_registry() {
        let registry = ServerRegistry::with_defaults();
        let manager = LspManager::with_registry(PathBuf::from("/tmp"), registry);
        assert_eq!(manager.workspace, PathBuf::from("/tmp"));
    }
}
