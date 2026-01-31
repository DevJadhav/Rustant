//! Subset of Language Server Protocol (LSP) types for Rustant's LSP client integration.
//!
//! These types are self-contained and do not depend on any external LSP crate.
//! They implement the portions of the LSP specification needed for hover, completion,
//! diagnostics, references, rename, formatting, and initialization requests.
//!
//! All types use `serde` for JSON serialization/deserialization, with `camelCase`
//! field renaming applied where the LSP specification requires it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Basic location types
// ---------------------------------------------------------------------------

/// A zero-based position in a text document (line and character offset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based character offset (UTF-16 code units).
    pub character: u32,
}

/// A range within a text document expressed as start and end [`Position`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// Represents a location inside a resource, such as a line inside a text file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

// ---------------------------------------------------------------------------
// Text document identifiers
// ---------------------------------------------------------------------------

/// Identifies a text document by its URI.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

/// An item to transfer a text document from the client to the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentItem {
    pub uri: String,
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

/// Parameters for requests that operate on a text document at a given position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

/// The result of a hover request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoverResult {
    pub contents: HoverContents,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

/// The contents of a hover result.  Three shapes are accepted by the LSP spec.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    /// A single `MarkedString`.
    Scalar(MarkedString),
    /// A `MarkupContent` value.
    Markup(MarkupContent),
    /// An array of `MarkedString` values.
    Array(Vec<MarkedString>),
}

/// A marked string is either a plain string or a code block with a language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    /// A plain string.
    String(String),
    /// A code block with a language identifier.
    LanguageString { language: String, value: String },
}

/// Human-readable text with a rendering format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkupContent {
    /// The format of the content (e.g. `"plaintext"` or `"markdown"`).
    pub kind: String,
    pub value: String,
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

/// A completion item represents a text snippet that is proposed to complete
/// text that is being typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    /// The label of this completion item (shown in the UI).
    pub label: String,
    /// The kind of this completion item (maps to `CompletionItemKind`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<u32>,
    /// A human-readable string with additional information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Documentation for this completion item (can be string or MarkupContent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<Value>,
    /// A string that should be inserted into a document when selecting this item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
}

/// The response to a completion request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CompletionResponse {
    /// A simple array of completion items.
    Array(Vec<CompletionItem>),
    /// A completion list that can indicate whether the list is incomplete.
    List(CompletionList),
}

/// A collection of completion items along with an `is_incomplete` flag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionList {
    /// If `true`, further typing should re-request completions.
    pub is_incomplete: bool,
    pub items: Vec<CompletionItem>,
}

// ---------------------------------------------------------------------------
// Diagnostics
// ---------------------------------------------------------------------------

/// Diagnostic severity levels as defined by LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl Serialize for DiagnosticSeverity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> Deserialize<'de> for DiagnosticSeverity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        match value {
            1 => Ok(DiagnosticSeverity::Error),
            2 => Ok(DiagnosticSeverity::Warning),
            3 => Ok(DiagnosticSeverity::Information),
            4 => Ok(DiagnosticSeverity::Hint),
            other => Err(serde::de::Error::custom(format!(
                "invalid DiagnosticSeverity value: {other}"
            ))),
        }
    }
}

/// Represents a diagnostic, such as a compiler error or warning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Value>,
}

/// Parameters for the `textDocument/publishDiagnostics` notification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// Edits
// ---------------------------------------------------------------------------

/// A text edit applicable to a text document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// A workspace edit represents changes to many resources managed in the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<HashMap<String, Vec<TextEdit>>>,
}

// ---------------------------------------------------------------------------
// Rename
// ---------------------------------------------------------------------------

/// Parameters for the `textDocument/rename` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    pub new_name: String,
}

// ---------------------------------------------------------------------------
// Formatting
// ---------------------------------------------------------------------------

/// Value-object describing what options formatting should use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormattingOptions {
    pub tab_size: u32,
    pub insert_spaces: bool,
}

/// Parameters for the `textDocument/formatting` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentFormattingParams {
    pub text_document: TextDocumentIdentifier,
    pub options: FormattingOptions,
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

/// Parameters for the `textDocument/references` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    pub context: ReferenceContext,
}

/// Context carried with a `textDocument/references` request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

// ---------------------------------------------------------------------------
// Initialize
// ---------------------------------------------------------------------------

/// Parameters sent with the `initialize` request from client to server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The process ID of the parent process that started the server.
    /// `None` if the process has not been started by another process.
    pub process_id: Option<u32>,
    /// The root URI of the workspace.  Preferred over the deprecated `rootPath`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_uri: Option<String>,
    /// The capabilities provided by the client (editor or tool).
    pub capabilities: ClientCapabilities,
    /// User-provided initialization options.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub initialization_options: Option<Value>,
}

/// Capabilities the client (editor) declares to the server.
///
/// This is a simplified representation; the full LSP spec is much larger.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    /// Text document specific client capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document: Option<Value>,
    /// Workspace specific client capabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<Value>,
}

/// Simplified server capabilities returned in the `initialize` response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    /// The server provides hover support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<bool>,
    /// The server provides goto-definition support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<bool>,
    /// The server provides find-references support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<bool>,
    /// The server provides completion support (provider options).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_provider: Option<Value>,
    /// The server provides rename support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_provider: Option<Value>,
    /// The server provides document formatting support.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_formatting_provider: Option<bool>,
    /// How text documents are synced (0 = None, 1 = Full, 2 = Incremental).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document_sync: Option<Value>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- Position, Range, Location round-trips --------------------------------

    #[test]
    fn test_position_round_trip() {
        let pos = Position {
            line: 10,
            character: 5,
        };
        let json = serde_json::to_string(&pos).unwrap();
        let decoded: Position = serde_json::from_str(&json).unwrap();
        assert_eq!(pos, decoded);
    }

    #[test]
    fn test_position_json_shape() {
        let pos = Position {
            line: 3,
            character: 12,
        };
        let v: Value = serde_json::to_value(pos).unwrap();
        assert_eq!(v, json!({"line": 3, "character": 12}));
    }

    #[test]
    fn test_range_round_trip() {
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 10,
            },
        };
        let json = serde_json::to_string(&range).unwrap();
        let decoded: Range = serde_json::from_str(&json).unwrap();
        assert_eq!(range, decoded);
    }

    #[test]
    fn test_location_round_trip() {
        let loc = Location {
            uri: "file:///src/main.rs".to_string(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 0,
                },
                end: Position {
                    line: 5,
                    character: 20,
                },
            },
        };
        let json = serde_json::to_string(&loc).unwrap();
        let decoded: Location = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, decoded);
    }

    // -- HoverResult with different HoverContents variants --------------------

    #[test]
    fn test_hover_result_scalar_string() {
        let hover = HoverResult {
            contents: HoverContents::Scalar(MarkedString::String("hello world".to_string())),
            range: None,
        };
        let json = serde_json::to_string(&hover).unwrap();
        let decoded: HoverResult = serde_json::from_str(&json).unwrap();
        assert_eq!(hover, decoded);
    }

    #[test]
    fn test_hover_result_scalar_language_string() {
        let hover = HoverResult {
            contents: HoverContents::Scalar(MarkedString::LanguageString {
                language: "rust".to_string(),
                value: "fn main() {}".to_string(),
            }),
            range: Some(Range {
                start: Position {
                    line: 1,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 12,
                },
            }),
        };
        let json = serde_json::to_string(&hover).unwrap();
        let decoded: HoverResult = serde_json::from_str(&json).unwrap();
        assert_eq!(hover, decoded);
    }

    #[test]
    fn test_hover_result_markup_content() {
        let hover = HoverResult {
            contents: HoverContents::Markup(MarkupContent {
                kind: "markdown".to_string(),
                value: "# Hello\nWorld".to_string(),
            }),
            range: None,
        };
        let json = serde_json::to_string(&hover).unwrap();
        let decoded: HoverResult = serde_json::from_str(&json).unwrap();
        assert_eq!(hover, decoded);
    }

    #[test]
    fn test_hover_result_array() {
        let hover = HoverResult {
            contents: HoverContents::Array(vec![
                MarkedString::String("first".to_string()),
                MarkedString::LanguageString {
                    language: "python".to_string(),
                    value: "print('hi')".to_string(),
                },
            ]),
            range: None,
        };
        let json = serde_json::to_string(&hover).unwrap();
        let decoded: HoverResult = serde_json::from_str(&json).unwrap();
        assert_eq!(hover, decoded);
    }

    // -- CompletionItem with and without optional fields ----------------------

    #[test]
    fn test_completion_item_full() {
        let item = CompletionItem {
            label: "println!".to_string(),
            kind: Some(3),
            detail: Some("macro".to_string()),
            documentation: Some(json!("Prints to stdout.")),
            insert_text: Some("println!(\"$1\")".to_string()),
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: CompletionItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, decoded);
    }

    #[test]
    fn test_completion_item_minimal() {
        let item = CompletionItem {
            label: "my_func".to_string(),
            kind: None,
            detail: None,
            documentation: None,
            insert_text: None,
        };
        let json = serde_json::to_string(&item).unwrap();

        // Ensure optional fields are omitted from the JSON output
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("kind").is_none());
        assert!(v.get("detail").is_none());
        assert!(v.get("documentation").is_none());
        assert!(v.get("insertText").is_none());

        let decoded: CompletionItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, decoded);
    }

    #[test]
    fn test_completion_item_camel_case() {
        let item = CompletionItem {
            label: "foo".to_string(),
            kind: Some(1),
            detail: None,
            documentation: None,
            insert_text: Some("foo()".to_string()),
        };
        let v: Value = serde_json::to_value(&item).unwrap();
        // `insert_text` should serialize as `insertText`
        assert!(v.get("insertText").is_some());
        assert!(v.get("insert_text").is_none());
    }

    #[test]
    fn test_completion_response_array() {
        let resp = CompletionResponse::Array(vec![CompletionItem {
            label: "a".to_string(),
            kind: None,
            detail: None,
            documentation: None,
            insert_text: None,
        }]);
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CompletionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_completion_response_list() {
        let resp = CompletionResponse::List(CompletionList {
            is_incomplete: true,
            items: vec![CompletionItem {
                label: "b".to_string(),
                kind: Some(6),
                detail: None,
                documentation: None,
                insert_text: None,
            }],
        });
        let json = serde_json::to_string(&resp).unwrap();
        let decoded: CompletionResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(resp, decoded);
    }

    #[test]
    fn test_completion_list_camel_case() {
        let list = CompletionList {
            is_incomplete: false,
            items: vec![],
        };
        let v: Value = serde_json::to_value(&list).unwrap();
        assert!(v.get("isIncomplete").is_some());
        assert!(v.get("is_incomplete").is_none());
    }

    // -- Diagnostic with severity ---------------------------------------------

    #[test]
    fn test_diagnostic_round_trip() {
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 10,
                    character: 4,
                },
                end: Position {
                    line: 10,
                    character: 15,
                },
            },
            severity: Some(DiagnosticSeverity::Error),
            message: "expected `;`".to_string(),
            source: Some("rustc".to_string()),
            code: Some(json!("E0308")),
        };
        let json = serde_json::to_string(&diag).unwrap();
        let decoded: Diagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(diag, decoded);
    }

    #[test]
    fn test_diagnostic_no_optional_fields() {
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 1,
                },
            },
            severity: None,
            message: "something".to_string(),
            source: None,
            code: None,
        };
        let json = serde_json::to_string(&diag).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("severity").is_none());
        assert!(v.get("source").is_none());
        assert!(v.get("code").is_none());

        let decoded: Diagnostic = serde_json::from_str(&json).unwrap();
        assert_eq!(diag, decoded);
    }

    // -- DiagnosticSeverity integer serialization -----------------------------

    #[test]
    fn test_diagnostic_severity_serialize_as_integer() {
        assert_eq!(
            serde_json::to_value(DiagnosticSeverity::Error).unwrap(),
            json!(1)
        );
        assert_eq!(
            serde_json::to_value(DiagnosticSeverity::Warning).unwrap(),
            json!(2)
        );
        assert_eq!(
            serde_json::to_value(DiagnosticSeverity::Information).unwrap(),
            json!(3)
        );
        assert_eq!(
            serde_json::to_value(DiagnosticSeverity::Hint).unwrap(),
            json!(4)
        );
    }

    #[test]
    fn test_diagnostic_severity_deserialize_from_integer() {
        let err: DiagnosticSeverity = serde_json::from_value(json!(1)).unwrap();
        assert_eq!(err, DiagnosticSeverity::Error);

        let warn: DiagnosticSeverity = serde_json::from_value(json!(2)).unwrap();
        assert_eq!(warn, DiagnosticSeverity::Warning);

        let info: DiagnosticSeverity = serde_json::from_value(json!(3)).unwrap();
        assert_eq!(info, DiagnosticSeverity::Information);

        let hint: DiagnosticSeverity = serde_json::from_value(json!(4)).unwrap();
        assert_eq!(hint, DiagnosticSeverity::Hint);
    }

    #[test]
    fn test_diagnostic_severity_invalid_value() {
        let result = serde_json::from_value::<DiagnosticSeverity>(json!(0));
        assert!(result.is_err());

        let result = serde_json::from_value::<DiagnosticSeverity>(json!(5));
        assert!(result.is_err());
    }

    // -- TextEdit & WorkspaceEdit ---------------------------------------------

    #[test]
    fn test_text_edit_round_trip() {
        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 2,
                    character: 0,
                },
                end: Position {
                    line: 2,
                    character: 5,
                },
            },
            new_text: "hello".to_string(),
        };
        let json = serde_json::to_string(&edit).unwrap();
        let decoded: TextEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(edit, decoded);
    }

    #[test]
    fn test_text_edit_camel_case() {
        let edit = TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            new_text: "inserted".to_string(),
        };
        let v: Value = serde_json::to_value(&edit).unwrap();
        assert!(v.get("newText").is_some());
        assert!(v.get("new_text").is_none());
    }

    #[test]
    fn test_workspace_edit_with_changes() {
        let mut changes = HashMap::new();
        changes.insert(
            "file:///src/lib.rs".to_string(),
            vec![TextEdit {
                range: Range {
                    start: Position {
                        line: 0,
                        character: 0,
                    },
                    end: Position {
                        line: 0,
                        character: 3,
                    },
                },
                new_text: "pub".to_string(),
            }],
        );
        let ws_edit = WorkspaceEdit {
            changes: Some(changes),
        };
        let json = serde_json::to_string(&ws_edit).unwrap();
        let decoded: WorkspaceEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(ws_edit, decoded);
    }

    #[test]
    fn test_workspace_edit_empty() {
        let ws_edit = WorkspaceEdit { changes: None };
        let json = serde_json::to_string(&ws_edit).unwrap();
        let v: Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("changes").is_none());

        let decoded: WorkspaceEdit = serde_json::from_str(&json).unwrap();
        assert_eq!(ws_edit, decoded);
    }

    // -- camelCase field naming ------------------------------------------------

    #[test]
    fn test_text_document_item_camel_case() {
        let item = TextDocumentItem {
            uri: "file:///test.rs".to_string(),
            language_id: "rust".to_string(),
            version: 1,
            text: "fn main() {}".to_string(),
        };
        let v: Value = serde_json::to_value(&item).unwrap();
        assert!(v.get("languageId").is_some());
        assert!(v.get("language_id").is_none());
    }

    #[test]
    fn test_text_document_position_params_camel_case() {
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position {
                line: 0,
                character: 0,
            },
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        assert!(v.get("textDocument").is_some());
        assert!(v.get("text_document").is_none());
    }

    #[test]
    fn test_rename_params_camel_case() {
        let params = RenameParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position {
                line: 5,
                character: 10,
            },
            new_name: "bar".to_string(),
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        assert!(v.get("textDocument").is_some());
        assert!(v.get("newName").is_some());
        assert!(v.get("text_document").is_none());
        assert!(v.get("new_name").is_none());
    }

    #[test]
    fn test_formatting_options_camel_case() {
        let opts = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
        };
        let v: Value = serde_json::to_value(&opts).unwrap();
        assert!(v.get("tabSize").is_some());
        assert!(v.get("insertSpaces").is_some());
        assert!(v.get("tab_size").is_none());
        assert!(v.get("insert_spaces").is_none());
    }

    #[test]
    fn test_document_formatting_params_camel_case() {
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            options: FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
            },
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        assert!(v.get("textDocument").is_some());
    }

    #[test]
    fn test_reference_params_camel_case() {
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position {
                line: 3,
                character: 7,
            },
            context: ReferenceContext {
                include_declaration: true,
            },
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        assert!(v.get("textDocument").is_some());
        let ctx = v.get("context").unwrap();
        assert!(ctx.get("includeDeclaration").is_some());
        assert!(ctx.get("include_declaration").is_none());
    }

    #[test]
    fn test_initialize_params_camel_case() {
        let params = InitializeParams {
            process_id: Some(1234),
            root_uri: Some("file:///workspace".to_string()),
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        assert!(v.get("processId").is_some());
        assert!(v.get("rootUri").is_some());
        assert!(v.get("process_id").is_none());
        assert!(v.get("root_uri").is_none());
    }

    #[test]
    fn test_server_capabilities_camel_case() {
        let caps = ServerCapabilities {
            hover_provider: Some(true),
            definition_provider: Some(true),
            references_provider: Some(false),
            completion_provider: Some(json!({"triggerCharacters": [".", ":"]})),
            rename_provider: Some(json!(true)),
            document_formatting_provider: Some(true),
            text_document_sync: Some(json!(2)),
        };
        let v: Value = serde_json::to_value(&caps).unwrap();
        assert!(v.get("hoverProvider").is_some());
        assert!(v.get("definitionProvider").is_some());
        assert!(v.get("referencesProvider").is_some());
        assert!(v.get("completionProvider").is_some());
        assert!(v.get("renameProvider").is_some());
        assert!(v.get("documentFormattingProvider").is_some());
        assert!(v.get("textDocumentSync").is_some());
        // snake_case keys must not appear
        assert!(v.get("hover_provider").is_none());
        assert!(v.get("text_document_sync").is_none());
    }

    #[test]
    fn test_server_capabilities_omits_none() {
        let caps = ServerCapabilities::default();
        let v: Value = serde_json::to_value(&caps).unwrap();
        assert!(v.get("hoverProvider").is_none());
        assert!(v.get("definitionProvider").is_none());
    }

    // -- PublishDiagnosticsParams round-trip -----------------------------------

    #[test]
    fn test_publish_diagnostics_round_trip() {
        let params = PublishDiagnosticsParams {
            uri: "file:///src/main.rs".to_string(),
            diagnostics: vec![Diagnostic {
                range: Range {
                    start: Position {
                        line: 1,
                        character: 0,
                    },
                    end: Position {
                        line: 1,
                        character: 10,
                    },
                },
                severity: Some(DiagnosticSeverity::Warning),
                message: "unused variable".to_string(),
                source: Some("rustc".to_string()),
                code: Some(json!("W0001")),
            }],
        };
        let json = serde_json::to_string(&params).unwrap();
        let decoded: PublishDiagnosticsParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, decoded);
    }

    // -- Deserialization from raw JSON (simulating server responses) -----------

    #[test]
    fn test_deserialize_hover_from_raw_json() {
        let raw = r##"{
            "contents": {"kind": "markdown", "value": "# Docs\nSome info"},
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 5}}
        }"##;
        let hover: HoverResult = serde_json::from_str(raw).unwrap();
        assert!(matches!(hover.contents, HoverContents::Markup(_)));
        assert!(hover.range.is_some());
    }

    #[test]
    fn test_deserialize_completion_list_from_raw_json() {
        let raw = r#"{
            "isIncomplete": false,
            "items": [
                {"label": "println!", "kind": 3, "insertText": "println!(\"$1\")"},
                {"label": "eprintln!", "kind": 3}
            ]
        }"#;
        let list: CompletionList = serde_json::from_str(raw).unwrap();
        assert!(!list.is_incomplete);
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].label, "println!");
        assert!(list.items[0].insert_text.is_some());
        assert!(list.items[1].insert_text.is_none());
    }

    #[test]
    fn test_deserialize_diagnostic_from_raw_json() {
        let raw = json!({
            "range": {
                "start": {"line": 3, "character": 0},
                "end": {"line": 3, "character": 10}
            },
            "severity": 1,
            "message": "unused variable"
        });
        let diag: Diagnostic = serde_json::from_value(raw).unwrap();
        assert_eq!(diag.severity, Some(DiagnosticSeverity::Error));
        assert_eq!(diag.message, "unused variable");
        assert_eq!(diag.range.start.line, 3);
    }

    #[test]
    fn test_deserialize_text_edit_from_raw_json() {
        let raw = json!({
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 10}
            },
            "newText": "fn main() {"
        });
        let edit: TextEdit = serde_json::from_value(raw).unwrap();
        assert_eq!(edit.new_text, "fn main() {");
        assert_eq!(edit.range.end.character, 10);
    }

    #[test]
    fn test_deserialize_workspace_edit_from_raw_json() {
        let raw = json!({
            "changes": {
                "file:///src/main.rs": [
                    {
                        "range": {
                            "start": {"line": 10, "character": 4},
                            "end": {"line": 10, "character": 7}
                        },
                        "newText": "new_func"
                    }
                ]
            }
        });
        let edit: WorkspaceEdit = serde_json::from_value(raw).unwrap();
        let changes = edit.changes.unwrap();
        assert_eq!(changes.len(), 1);
        let edits = changes.get("file:///src/main.rs").unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].new_text, "new_func");
    }

    // -- Additional round-trip tests ------------------------------------------

    #[test]
    fn test_text_document_item_round_trip() {
        let item = TextDocumentItem {
            uri: "file:///src/main.rs".to_string(),
            language_id: "rust".to_string(),
            version: 3,
            text: "fn main() {}".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let decoded: TextDocumentItem = serde_json::from_str(&json).unwrap();
        assert_eq!(item, decoded);
    }

    #[test]
    fn test_initialize_params_round_trip() {
        let params = InitializeParams {
            process_id: Some(42),
            root_uri: Some("file:///workspace".to_string()),
            capabilities: ClientCapabilities {
                text_document: Some(json!({"hover": {"contentFormat": ["plaintext"]}})),
                workspace: None,
            },
            initialization_options: Some(json!({"customSetting": true})),
        };
        let json = serde_json::to_string(&params).unwrap();
        let decoded: InitializeParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, decoded);
    }

    #[test]
    fn test_initialize_params_null_process_id() {
        let params = InitializeParams {
            process_id: None,
            root_uri: None,
            capabilities: ClientCapabilities::default(),
            initialization_options: None,
        };
        let v: Value = serde_json::to_value(&params).unwrap();
        // processId should be present but null (not omitted).
        assert!(v.get("processId").is_some());
        assert!(v["processId"].is_null());
        // rootUri and initializationOptions should be omitted.
        assert!(v.get("rootUri").is_none());
        assert!(v.get("initializationOptions").is_none());
    }

    #[test]
    fn test_reference_params_round_trip() {
        let params = ReferenceParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///test.rs".to_string(),
            },
            position: Position {
                line: 10,
                character: 3,
            },
            context: ReferenceContext {
                include_declaration: false,
            },
        };
        let json = serde_json::to_string(&params).unwrap();
        let decoded: ReferenceParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, decoded);
    }

    #[test]
    fn test_document_formatting_params_round_trip() {
        let params = DocumentFormattingParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///src/lib.rs".to_string(),
            },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
            },
        };
        let json = serde_json::to_string(&params).unwrap();
        let decoded: DocumentFormattingParams = serde_json::from_str(&json).unwrap();
        assert_eq!(params, decoded);
    }
}
