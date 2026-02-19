//! LSP-based tools for code intelligence.
//!
//! Provides 7 tools that leverage language server capabilities:
//! hover, go-to-definition, find-references, diagnostics,
//! completions, rename, and format.

use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

use super::LspBackend;
use super::client::LspError;
use super::types::{CompletionItem, Diagnostic, DiagnosticSeverity, Location};

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Extract `file`, `line`, and `character` from JSON arguments.
fn extract_position_args(args: &Value) -> Result<(PathBuf, u32, u32), ToolError> {
    let file =
        args.get("file")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "lsp".to_string(),
                reason: "missing required parameter 'file'".to_string(),
            })?;

    let line =
        args.get("line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "lsp".to_string(),
                reason: "missing required parameter 'line'".to_string(),
            })? as u32;

    let character = args
        .get("character")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ToolError::InvalidArguments {
            name: "lsp".to_string(),
            reason: "missing required parameter 'character'".to_string(),
        })? as u32;

    Ok((PathBuf::from(file), line, character))
}

/// Format a single `Location` as `"uri:line:col"`.
fn format_location(loc: &Location) -> String {
    format!(
        "{}:{}:{}",
        loc.uri, loc.range.start.line, loc.range.start.character
    )
}

/// Format a `Diagnostic` as a human-readable string.
fn format_diagnostic(diag: &Diagnostic) -> String {
    let severity = match diag.severity {
        Some(DiagnosticSeverity::Error) => "error",
        Some(DiagnosticSeverity::Warning) => "warning",
        Some(DiagnosticSeverity::Information) => "info",
        Some(DiagnosticSeverity::Hint) => "hint",
        None => "unknown",
    };
    format!(
        "[{}] line {}: {}",
        severity, diag.range.start.line, diag.message
    )
}

/// Convert an `LspError` into a `ToolError`.
fn lsp_err(tool_name: &str, err: LspError) -> ToolError {
    ToolError::ExecutionFailed {
        name: tool_name.to_string(),
        message: err.to_string(),
    }
}

// ---------------------------------------------------------------------------
// 1. LspHoverTool
// ---------------------------------------------------------------------------

/// Provides hover information (type info, documentation) via the language server.
pub struct LspHoverTool {
    backend: Arc<dyn LspBackend>,
}

impl LspHoverTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspHoverTool {
    fn name(&self) -> &str {
        "lsp_hover"
    }

    fn description(&self) -> &str {
        "Get hover information (type info, documentation) for a symbol at a given position in a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file", "line", "character"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let (file, line, character) = extract_position_args(&args)?;
        let result = self
            .backend
            .hover(&file, line, character)
            .await
            .map_err(|e| lsp_err("lsp_hover", e))?;

        match result {
            Some(text) => Ok(ToolOutput::text(text)),
            None => Ok(ToolOutput::text(
                "No hover information available at this position.",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// 2. LspDefinitionTool
// ---------------------------------------------------------------------------

/// Navigates to the definition of a symbol via the language server.
pub struct LspDefinitionTool {
    backend: Arc<dyn LspBackend>,
}

impl LspDefinitionTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspDefinitionTool {
    fn name(&self) -> &str {
        "lsp_definition"
    }

    fn description(&self) -> &str {
        "Go to the definition of a symbol at a given position in a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file", "line", "character"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let (file, line, character) = extract_position_args(&args)?;
        let locations = self
            .backend
            .definition(&file, line, character)
            .await
            .map_err(|e| lsp_err("lsp_definition", e))?;

        if locations.is_empty() {
            return Ok(ToolOutput::text("No definition found at this position."));
        }

        let formatted: Vec<String> = locations.iter().map(format_location).collect();
        Ok(ToolOutput::text(format!(
            "Definition location(s):\n{}",
            formatted.join("\n")
        )))
    }
}

// ---------------------------------------------------------------------------
// 3. LspReferencesTool
// ---------------------------------------------------------------------------

/// Finds all references to a symbol via the language server.
pub struct LspReferencesTool {
    backend: Arc<dyn LspBackend>,
}

impl LspReferencesTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspReferencesTool {
    fn name(&self) -> &str {
        "lsp_references"
    }

    fn description(&self) -> &str {
        "Find all references to a symbol at a given position in a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                }
            },
            "required": ["file", "line", "character"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let (file, line, character) = extract_position_args(&args)?;
        let locations = self
            .backend
            .references(&file, line, character)
            .await
            .map_err(|e| lsp_err("lsp_references", e))?;

        if locations.is_empty() {
            return Ok(ToolOutput::text("No references found at this position."));
        }

        let formatted: Vec<String> = locations.iter().map(format_location).collect();
        Ok(ToolOutput::text(format!(
            "Found {} reference(s):\n{}",
            locations.len(),
            formatted.join("\n")
        )))
    }
}

// ---------------------------------------------------------------------------
// 4. LspDiagnosticsTool
// ---------------------------------------------------------------------------

/// Retrieves diagnostics (errors, warnings) for a file from the language server.
pub struct LspDiagnosticsTool {
    backend: Arc<dyn LspBackend>,
}

impl LspDiagnosticsTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspDiagnosticsTool {
    fn name(&self) -> &str {
        "lsp_diagnostics"
    }

    fn description(&self) -> &str {
        "Get diagnostic messages (errors, warnings) for a file from the language server"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                }
            },
            "required": ["file"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let file = args.get("file").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "lsp_diagnostics".to_string(),
                reason: "missing required parameter 'file'".to_string(),
            }
        })?;

        let diagnostics = self
            .backend
            .diagnostics(std::path::Path::new(file))
            .await
            .map_err(|e| lsp_err("lsp_diagnostics", e))?;

        if diagnostics.is_empty() {
            return Ok(ToolOutput::text("No diagnostics found for this file."));
        }

        let formatted: Vec<String> = diagnostics.iter().map(format_diagnostic).collect();
        Ok(ToolOutput::text(format!(
            "Found {} diagnostic(s):\n{}",
            diagnostics.len(),
            formatted.join("\n")
        )))
    }
}

// ---------------------------------------------------------------------------
// 5. LspCompletionsTool
// ---------------------------------------------------------------------------

/// Provides code completion suggestions from the language server.
pub struct LspCompletionsTool {
    backend: Arc<dyn LspBackend>,
}

impl LspCompletionsTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspCompletionsTool {
    fn name(&self) -> &str {
        "lsp_completions"
    }

    fn description(&self) -> &str {
        "Get code completion suggestions at a given position in a file"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum completions to return (default 20)"
                }
            },
            "required": ["file", "line", "character"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let (file, line, character) = extract_position_args(&args)?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

        let completions = self
            .backend
            .completions(&file, line, character)
            .await
            .map_err(|e| lsp_err("lsp_completions", e))?;

        if completions.is_empty() {
            return Ok(ToolOutput::text(
                "No completions available at this position.",
            ));
        }

        let limited: Vec<&CompletionItem> = completions.iter().take(limit).collect();
        let formatted: Vec<String> = limited
            .iter()
            .map(|item| {
                let detail = item
                    .detail
                    .as_deref()
                    .map(|d| format!(" - {d}"))
                    .unwrap_or_default();
                format!("  {}{}", item.label, detail)
            })
            .collect();

        Ok(ToolOutput::text(format!(
            "Completions ({} of {}):\n{}",
            limited.len(),
            completions.len(),
            formatted.join("\n")
        )))
    }
}

// ---------------------------------------------------------------------------
// 6. LspRenameTool
// ---------------------------------------------------------------------------

/// Renames a symbol across the project using the language server.
pub struct LspRenameTool {
    backend: Arc<dyn LspBackend>,
}

impl LspRenameTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspRenameTool {
    fn name(&self) -> &str {
        "lsp_rename"
    }

    fn description(&self) -> &str {
        "Rename a symbol across the project using the language server"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "line": {
                    "type": "integer",
                    "description": "Line number (0-indexed)"
                },
                "character": {
                    "type": "integer",
                    "description": "Character position (0-indexed)"
                },
                "new_name": {
                    "type": "string",
                    "description": "The new name for the symbol"
                }
            },
            "required": ["file", "line", "character", "new_name"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let (file, line, character) = extract_position_args(&args)?;
        let new_name = args
            .get("new_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "lsp_rename".to_string(),
                reason: "missing required parameter 'new_name'".to_string(),
            })?;

        let edit = self
            .backend
            .rename(&file, line, character, new_name)
            .await
            .map_err(|e| lsp_err("lsp_rename", e))?;

        let changes = match &edit.changes {
            Some(c) if !c.is_empty() => c,
            _ => {
                return Ok(ToolOutput::text("No changes produced by rename operation."));
            }
        };

        let mut lines = Vec::new();
        for (uri, edits) in changes {
            lines.push(format!("{uri}:"));
            for te in edits {
                lines.push(format!(
                    "  line {}:{}-{}:{}: \"{}\"",
                    te.range.start.line,
                    te.range.start.character,
                    te.range.end.line,
                    te.range.end.character,
                    te.new_text
                ));
            }
        }

        Ok(ToolOutput::text(format!(
            "Rename applied across {} file(s):\n{}",
            changes.len(),
            lines.join("\n")
        )))
    }
}

// ---------------------------------------------------------------------------
// 7. LspFormatTool
// ---------------------------------------------------------------------------

/// Formats a source file using the language server.
pub struct LspFormatTool {
    backend: Arc<dyn LspBackend>,
}

impl LspFormatTool {
    pub fn new(backend: Arc<dyn LspBackend>) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for LspFormatTool {
    fn name(&self) -> &str {
        "lsp_format"
    }

    fn description(&self) -> &str {
        "Format a source file using the language server's formatting capabilities"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "file": {
                    "type": "string",
                    "description": "Path to the file to format"
                }
            },
            "required": ["file"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let file = args.get("file").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "lsp_format".to_string(),
                reason: "missing required parameter 'file'".to_string(),
            }
        })?;

        let edits = self
            .backend
            .format(std::path::Path::new(file))
            .await
            .map_err(|e| lsp_err("lsp_format", e))?;

        if edits.is_empty() {
            return Ok(ToolOutput::text(
                "File is already formatted. No changes needed.",
            ));
        }

        let formatted: Vec<String> = edits
            .iter()
            .map(|te| {
                format!(
                    "  line {}:{}-{}:{}: \"{}\"",
                    te.range.start.line,
                    te.range.start.character,
                    te.range.end.line,
                    te.range.end.character,
                    te.new_text
                )
            })
            .collect();

        Ok(ToolOutput::text(format!(
            "Applied {} formatting edit(s):\n{}",
            edits.len(),
            formatted.join("\n")
        )))
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::super::types::{Position, Range, TextEdit, WorkspaceEdit};
    use super::*;
    use std::collections::HashMap;

    // -----------------------------------------------------------------------
    // Mock backend
    // -----------------------------------------------------------------------

    struct MockLspBackend {
        hover_result: tokio::sync::Mutex<Option<String>>,
        definition_result: tokio::sync::Mutex<Vec<Location>>,
        references_result: tokio::sync::Mutex<Vec<Location>>,
        diagnostics_result: tokio::sync::Mutex<Vec<Diagnostic>>,
        completions_result: tokio::sync::Mutex<Vec<CompletionItem>>,
        rename_result: tokio::sync::Mutex<WorkspaceEdit>,
        format_result: tokio::sync::Mutex<Vec<TextEdit>>,
    }

    impl MockLspBackend {
        fn new() -> Self {
            Self {
                hover_result: tokio::sync::Mutex::new(None),
                definition_result: tokio::sync::Mutex::new(Vec::new()),
                references_result: tokio::sync::Mutex::new(Vec::new()),
                diagnostics_result: tokio::sync::Mutex::new(Vec::new()),
                completions_result: tokio::sync::Mutex::new(Vec::new()),
                rename_result: tokio::sync::Mutex::new(WorkspaceEdit { changes: None }),
                format_result: tokio::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl LspBackend for MockLspBackend {
        async fn hover(
            &self,
            _file: &std::path::Path,
            _line: u32,
            _character: u32,
        ) -> Result<Option<String>, LspError> {
            Ok(self.hover_result.lock().await.clone())
        }

        async fn definition(
            &self,
            _file: &std::path::Path,
            _line: u32,
            _character: u32,
        ) -> Result<Vec<Location>, LspError> {
            Ok(self.definition_result.lock().await.clone())
        }

        async fn references(
            &self,
            _file: &std::path::Path,
            _line: u32,
            _character: u32,
        ) -> Result<Vec<Location>, LspError> {
            Ok(self.references_result.lock().await.clone())
        }

        async fn diagnostics(&self, _file: &std::path::Path) -> Result<Vec<Diagnostic>, LspError> {
            Ok(self.diagnostics_result.lock().await.clone())
        }

        async fn completions(
            &self,
            _file: &std::path::Path,
            _line: u32,
            _character: u32,
        ) -> Result<Vec<CompletionItem>, LspError> {
            Ok(self.completions_result.lock().await.clone())
        }

        async fn rename(
            &self,
            _file: &std::path::Path,
            _line: u32,
            _character: u32,
            _new_name: &str,
        ) -> Result<WorkspaceEdit, LspError> {
            Ok(self.rename_result.lock().await.clone())
        }

        async fn format(&self, _file: &std::path::Path) -> Result<Vec<TextEdit>, LspError> {
            Ok(self.format_result.lock().await.clone())
        }
    }

    // -----------------------------------------------------------------------
    // Helper to build a mock-backed tool
    // -----------------------------------------------------------------------

    fn mock_backend() -> Arc<MockLspBackend> {
        Arc::new(MockLspBackend::new())
    }

    // -----------------------------------------------------------------------
    // 1. test_hover_tool_name_and_schema
    // -----------------------------------------------------------------------

    #[test]
    fn test_hover_tool_name_and_schema() {
        let backend = mock_backend();
        let tool = LspHoverTool::new(backend);

        assert_eq!(tool.name(), "lsp_hover");
        assert_eq!(
            tool.description(),
            "Get hover information (type info, documentation) for a symbol at a given position in a file"
        );
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);

        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["file"].is_object());
        assert!(schema["properties"]["line"].is_object());
        assert!(schema["properties"]["character"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("file")));
        assert!(required.contains(&serde_json::json!("line")));
        assert!(required.contains(&serde_json::json!("character")));
    }

    // -----------------------------------------------------------------------
    // 2. test_hover_tool_execute_with_result
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_hover_tool_execute_with_result() {
        let backend = mock_backend();
        *backend.hover_result.lock().await = Some("fn main() -> ()".to_string());

        let tool = LspHoverTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 5
            }))
            .await
            .unwrap();

        assert_eq!(result.content, "fn main() -> ()");
    }

    // -----------------------------------------------------------------------
    // 3. test_hover_tool_execute_no_result
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_hover_tool_execute_no_result() {
        let backend = mock_backend();
        // hover_result is None by default

        let tool = LspHoverTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 0,
                "character": 0
            }))
            .await
            .unwrap();

        assert!(result.content.contains("No hover information"));
    }

    // -----------------------------------------------------------------------
    // 4. test_definition_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_definition_tool_execute() {
        let backend = mock_backend();
        *backend.definition_result.lock().await = vec![Location {
            uri: "/src/lib.rs".to_string(),
            range: Range {
                start: Position {
                    line: 42,
                    character: 4,
                },
                end: Position {
                    line: 42,
                    character: 10,
                },
            },
        }];

        let tool = LspDefinitionTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 5
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Definition location(s):"));
        assert!(result.content.contains("/src/lib.rs:42:4"));
    }

    // -----------------------------------------------------------------------
    // 5. test_definition_tool_no_results
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_definition_tool_no_results() {
        let backend = mock_backend();
        // definition_result is empty by default

        let tool = LspDefinitionTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 0,
                "character": 0
            }))
            .await
            .unwrap();

        assert!(result.content.contains("No definition found"));
    }

    // -----------------------------------------------------------------------
    // 6. test_references_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_references_tool_execute() {
        let backend = mock_backend();
        *backend.references_result.lock().await = vec![
            Location {
                uri: "/src/main.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 10,
                        character: 5,
                    },
                    end: Position {
                        line: 10,
                        character: 15,
                    },
                },
            },
            Location {
                uri: "/src/lib.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 20,
                        character: 8,
                    },
                    end: Position {
                        line: 20,
                        character: 18,
                    },
                },
            },
            Location {
                uri: "/tests/integration.rs".to_string(),
                range: Range {
                    start: Position {
                        line: 3,
                        character: 12,
                    },
                    end: Position {
                        line: 3,
                        character: 22,
                    },
                },
            },
        ];

        let tool = LspReferencesTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 5
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Found 3 reference(s):"));
        assert!(result.content.contains("/src/main.rs:10:5"));
        assert!(result.content.contains("/src/lib.rs:20:8"));
        assert!(result.content.contains("/tests/integration.rs:3:12"));
    }

    // -----------------------------------------------------------------------
    // 7. test_diagnostics_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_diagnostics_tool_execute() {
        let backend = mock_backend();
        *backend.diagnostics_result.lock().await = vec![
            Diagnostic {
                range: Range {
                    start: Position {
                        line: 5,
                        character: 0,
                    },
                    end: Position {
                        line: 5,
                        character: 1,
                    },
                },
                severity: Some(DiagnosticSeverity::Error),
                message: "expected `;`".to_string(),
                source: Some("rustc".to_string()),
                code: None,
            },
            Diagnostic {
                range: Range {
                    start: Position {
                        line: 12,
                        character: 4,
                    },
                    end: Position {
                        line: 12,
                        character: 5,
                    },
                },
                severity: Some(DiagnosticSeverity::Warning),
                message: "unused variable `x`".to_string(),
                source: Some("rustc".to_string()),
                code: None,
            },
            Diagnostic {
                range: Range {
                    start: Position {
                        line: 20,
                        character: 0,
                    },
                    end: Position {
                        line: 20,
                        character: 10,
                    },
                },
                severity: Some(DiagnosticSeverity::Information),
                message: "consider using `let` binding".to_string(),
                source: Some("clippy".to_string()),
                code: None,
            },
        ];

        let tool = LspDiagnosticsTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Found 3 diagnostic(s):"));
        assert!(result.content.contains("[error] line 5: expected `;`"));
        assert!(
            result
                .content
                .contains("[warning] line 12: unused variable `x`")
        );
        assert!(
            result
                .content
                .contains("[info] line 20: consider using `let` binding")
        );
    }

    // -----------------------------------------------------------------------
    // 8. test_completions_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_completions_tool_execute() {
        let backend = mock_backend();
        *backend.completions_result.lock().await = vec![
            CompletionItem {
                label: "println!".to_string(),
                kind: Some(15),
                detail: Some("macro".to_string()),
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "print!".to_string(),
                kind: Some(15),
                detail: Some("macro".to_string()),
                documentation: None,
                insert_text: None,
            },
        ];

        let tool = LspCompletionsTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 4
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Completions (2 of 2):"));
        assert!(result.content.contains("println! - macro"));
        assert!(result.content.contains("print! - macro"));
    }

    // -----------------------------------------------------------------------
    // 9. test_completions_tool_with_limit
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_completions_tool_with_limit() {
        let backend = mock_backend();
        *backend.completions_result.lock().await = vec![
            CompletionItem {
                label: "aaa".to_string(),
                kind: None,
                detail: None,
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "bbb".to_string(),
                kind: None,
                detail: None,
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "ccc".to_string(),
                kind: None,
                detail: None,
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "ddd".to_string(),
                kind: None,
                detail: None,
                documentation: None,
                insert_text: None,
            },
            CompletionItem {
                label: "eee".to_string(),
                kind: None,
                detail: None,
                documentation: None,
                insert_text: None,
            },
        ];

        let tool = LspCompletionsTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 4,
                "limit": 2
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Completions (2 of 5):"));
        assert!(result.content.contains("aaa"));
        assert!(result.content.contains("bbb"));
        assert!(!result.content.contains("ccc"));
    }

    // -----------------------------------------------------------------------
    // 10. test_rename_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_rename_tool_execute() {
        let backend = mock_backend();
        let mut changes = HashMap::new();
        changes.insert(
            "/src/main.rs".to_string(),
            vec![TextEdit {
                range: Range {
                    start: Position {
                        line: 10,
                        character: 4,
                    },
                    end: Position {
                        line: 10,
                        character: 7,
                    },
                },
                new_text: "new_func".to_string(),
            }],
        );
        changes.insert(
            "/src/lib.rs".to_string(),
            vec![TextEdit {
                range: Range {
                    start: Position {
                        line: 5,
                        character: 8,
                    },
                    end: Position {
                        line: 5,
                        character: 11,
                    },
                },
                new_text: "new_func".to_string(),
            }],
        );
        *backend.rename_result.lock().await = WorkspaceEdit {
            changes: Some(changes),
        };

        let tool = LspRenameTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs",
                "line": 10,
                "character": 4,
                "new_name": "new_func"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Rename applied across 2 file(s):"));
        assert!(result.content.contains("new_func"));
    }

    // -----------------------------------------------------------------------
    // 11. test_rename_tool_risk_level
    // -----------------------------------------------------------------------

    #[test]
    fn test_rename_tool_risk_level() {
        let backend = mock_backend();
        let tool = LspRenameTool::new(backend);
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    // -----------------------------------------------------------------------
    // 12. test_format_tool_execute
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_format_tool_execute() {
        let backend = mock_backend();
        *backend.format_result.lock().await = vec![
            TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 10,
                    },
                },
                new_text: "fn main() {".to_string(),
            },
            TextEdit {
                range: Range {
                    start: Position {
                        line: 1,
                        character: 0,
                    },
                    end: Position {
                        line: 1,
                        character: 5,
                    },
                },
                new_text: "    println!(\"hello\");".to_string(),
            },
        ];

        let tool = LspFormatTool::new(backend);
        let result = tool
            .execute(serde_json::json!({
                "file": "/src/main.rs"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("Applied 2 formatting edit(s):"));
        assert!(result.content.contains("fn main()"));
    }

    // -----------------------------------------------------------------------
    // 13. test_format_tool_risk_level
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_tool_risk_level() {
        let backend = mock_backend();
        let tool = LspFormatTool::new(backend);
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    // -----------------------------------------------------------------------
    // 14. test_extract_position_args_valid
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_position_args_valid() {
        let args = serde_json::json!({
            "file": "/src/main.rs",
            "line": 10,
            "character": 5
        });

        let (file, line, character) = extract_position_args(&args).unwrap();
        assert_eq!(file, PathBuf::from("/src/main.rs"));
        assert_eq!(line, 10);
        assert_eq!(character, 5);
    }

    // -----------------------------------------------------------------------
    // 15. test_extract_position_args_missing_field
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_position_args_missing_field() {
        // Missing 'character'
        let args = serde_json::json!({
            "file": "/src/main.rs",
            "line": 10
        });
        let result = extract_position_args(&args);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("character"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }

        // Missing 'file'
        let args = serde_json::json!({
            "line": 10,
            "character": 5
        });
        let result = extract_position_args(&args);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("file"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }

        // Missing 'line'
        let args = serde_json::json!({
            "file": "/src/main.rs",
            "character": 5
        });
        let result = extract_position_args(&args);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { reason, .. } => {
                assert!(reason.contains("line"));
            }
            other => panic!("Expected InvalidArguments, got: {other:?}"),
        }
    }
}
