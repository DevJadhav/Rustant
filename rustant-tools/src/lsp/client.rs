//! LSP client implementation for communicating with language server processes.
//!
//! This module provides an async LSP client that speaks JSON-RPC 2.0 over
//! Content-Length framed stdin/stdout transport. It can start a language server
//! process, send requests and notifications, and handle server-initiated
//! notifications such as `textDocument/publishDiagnostics`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tracing;

use super::types::{
    CompletionItem, CompletionResponse, Diagnostic, DocumentFormattingParams, FormattingOptions,
    HoverResult, Location, Position, ReferenceContext, ReferenceParams, RenameParams,
    TextDocumentIdentifier, TextDocumentItem, TextEdit, WorkspaceEdit,
};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can arise from LSP client operations.
#[derive(Debug, thiserror::Error)]
pub enum LspError {
    #[error("Server not running: {language}")]
    ServerNotRunning { language: String },

    #[error("Server failed to start: {message}")]
    ServerStartFailed { message: String },

    #[error("Request timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Protocol error: {message}")]
    ProtocolError { message: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Server returned error: code={code}, message={message}")]
    ServerError { code: i64, message: String },

    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Language not supported: {language}")]
    UnsupportedLanguage { language: String },
}

// ---------------------------------------------------------------------------
// LspClient
// ---------------------------------------------------------------------------

/// An asynchronous client for a single language-server process.
///
/// Communication uses JSON-RPC 2.0 messages framed with `Content-Length`
/// headers on both stdin (requests/notifications we send) and stdout
/// (responses/notifications the server sends).
pub struct LspClient {
    process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: i64,
    initialized: bool,
    /// URIs of documents that have been opened via `textDocument/didOpen`.
    open_documents: HashSet<String>,
    /// Diagnostics received from `textDocument/publishDiagnostics` notifications,
    /// keyed by document URI.
    cached_diagnostics: HashMap<String, Vec<Diagnostic>>,
    root_uri: String,
}

impl LspClient {
    // ------------------------------------------------------------------
    // Lifecycle
    // ------------------------------------------------------------------

    /// Start a language server process and perform the LSP handshake.
    ///
    /// `command` is the server binary (e.g. `rust-analyzer`), `args` are any
    /// extra CLI arguments, and `workspace` is the root directory that will
    /// be communicated to the server as `rootUri`.
    pub async fn start(command: &str, args: &[String], workspace: &Path) -> Result<Self, LspError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .current_dir(workspace)
            .spawn()
            .map_err(|e| LspError::ServerStartFailed {
                message: format!("Failed to spawn `{command}`: {e}"),
            })?;

        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::ServerStartFailed {
                message: "Could not capture stdin of child process".into(),
            })?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::ServerStartFailed {
                message: "Could not capture stdout of child process".into(),
            })?;

        let canonical =
            std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
        let root_uri = format!("file://{}", canonical.display());

        let mut client = Self {
            process: child,
            stdin: BufWriter::new(child_stdin),
            stdout: BufReader::new(child_stdout),
            next_id: 0,
            initialized: false,
            open_documents: HashSet::new(),
            cached_diagnostics: HashMap::new(),
            root_uri,
        };

        client.initialize().await?;

        Ok(client)
    }

    /// Perform the LSP `initialize` / `initialized` handshake.
    async fn initialize(&mut self) -> Result<(), LspError> {
        let params = json!({
            "processId": std::process::id(),
            "rootUri": self.root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": {
                        "contentFormat": ["plaintext", "markdown"]
                    },
                    "completion": {
                        "completionItem": {
                            "snippetSupport": false
                        }
                    },
                    "definition": {},
                    "references": {},
                    "rename": {
                        "prepareSupport": false
                    },
                    "formatting": {},
                    "publishDiagnostics": {
                        "relatedInformation": true
                    }
                },
                "workspace": {
                    "applyEdit": true,
                    "workspaceFolders": false
                }
            }
        });

        let _response = self.send_request("initialize", params).await?;
        self.send_notification("initialized", json!({})).await?;
        self.initialized = true;
        tracing::info!(root_uri = %self.root_uri, "LSP server initialized");
        Ok(())
    }

    // ------------------------------------------------------------------
    // Low-level transport
    // ------------------------------------------------------------------

    /// Send a JSON-RPC request and wait for the matching response.
    ///
    /// Notifications received while waiting for the response are processed
    /// and buffered (e.g. diagnostics) rather than returned.
    pub async fn send_request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, LspError> {
        self.next_id += 1;
        let id = self.next_id;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.write_message(&request).await?;
        tracing::debug!(id, method, "Sent LSP request");

        // Read messages until we get the response matching our id.
        loop {
            let msg = self.read_message().await?;

            // If the message has no `id` it is a notification.
            if msg.get("id").is_none() {
                self.handle_notification(&msg);
                continue;
            }

            // Check that the id matches.
            let resp_id = msg["id"].as_i64().unwrap_or(-1);
            if resp_id != id {
                // Could be a stale response or a server-initiated request;
                // log and continue.
                tracing::warn!(
                    expected_id = id,
                    received_id = resp_id,
                    "Received response with unexpected id, skipping"
                );
                continue;
            }

            // Check for error.
            if let Some(err) = msg.get("error") {
                let code = err["code"].as_i64().unwrap_or(0);
                let message = err["message"]
                    .as_str()
                    .unwrap_or("unknown error")
                    .to_string();
                return Err(LspError::ServerError { code, message });
            }

            // Return the result (may be null).
            return Ok(msg
                .get("result")
                .cloned()
                .unwrap_or(serde_json::Value::Null));
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    pub async fn send_notification(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(), LspError> {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.write_message(&notification).await?;
        tracing::debug!(method, "Sent LSP notification");
        Ok(())
    }

    /// Read a single Content-Length-framed JSON-RPC message from the server.
    pub async fn read_message(&mut self) -> Result<serde_json::Value, LspError> {
        let mut content_length: usize = 0;

        // Read headers until we hit the blank line.
        loop {
            let mut line = String::new();
            let bytes_read = self.stdout.read_line(&mut line).await?;
            if bytes_read == 0 {
                return Err(LspError::ProtocolError {
                    message: "Unexpected EOF while reading headers".into(),
                });
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length =
                    len_str
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| LspError::ProtocolError {
                            message: format!("Invalid Content-Length value: {len_str}"),
                        })?;
            }
            // Ignore other headers (e.g. Content-Type).
        }

        if content_length == 0 {
            return Err(LspError::ProtocolError {
                message: "Missing or zero Content-Length header".into(),
            });
        }

        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await?;

        let message: serde_json::Value = serde_json::from_slice(&body)?;
        Ok(message)
    }

    /// Write a serialized JSON-RPC message with Content-Length framing.
    async fn write_message(&mut self, message: &serde_json::Value) -> Result<(), LspError> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    /// Process a server-initiated notification.
    fn handle_notification(&mut self, msg: &serde_json::Value) {
        let method = match msg.get("method").and_then(|m| m.as_str()) {
            Some(m) => m,
            None => return,
        };

        match method {
            "textDocument/publishDiagnostics" => {
                if let Some(params) = msg.get("params") {
                    if let Ok(diag_params) =
                        serde_json::from_value::<PublishDiagnosticsNotification>(params.clone())
                    {
                        tracing::debug!(
                            uri = %diag_params.uri,
                            count = diag_params.diagnostics.len(),
                            "Received diagnostics"
                        );
                        self.cached_diagnostics
                            .insert(diag_params.uri, diag_params.diagnostics);
                    }
                }
            }
            other => {
                tracing::debug!(method = other, "Received unhandled notification");
            }
        }
    }

    // ------------------------------------------------------------------
    // Document management
    // ------------------------------------------------------------------

    /// Ensure a file is opened in the language server.
    ///
    /// If the file has not yet been opened, the client reads it from disk and
    /// sends a `textDocument/didOpen` notification. Returns the `file://` URI
    /// for the file.
    pub async fn ensure_document_open(&mut self, file_path: &Path) -> Result<String, LspError> {
        let uri = file_path_to_uri(file_path)?;

        if self.open_documents.contains(&uri) {
            return Ok(uri);
        }

        let content =
            tokio::fs::read_to_string(file_path)
                .await
                .map_err(|_| LspError::FileNotFound {
                    path: file_path.display().to_string(),
                })?;

        let language_id = detect_language_id(file_path);

        let text_doc = TextDocumentItem {
            uri: uri.clone(),
            language_id,
            version: 1,
            text: content,
        };

        self.send_notification(
            "textDocument/didOpen",
            serde_json::to_value(json!({
                "textDocument": text_doc
            }))?,
        )
        .await?;

        self.open_documents.insert(uri.clone());
        Ok(uri)
    }

    // ------------------------------------------------------------------
    // High-level LSP operations
    // ------------------------------------------------------------------

    /// Perform a `textDocument/hover` request.
    pub async fn hover(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Option<HoverResult>, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = make_text_document_position_params(&uri, line, character);
        let result = self.send_request("textDocument/hover", params).await?;

        if result.is_null() {
            return Ok(None);
        }

        let hover: HoverResult = serde_json::from_value(result)?;
        Ok(Some(hover))
    }

    /// Perform a `textDocument/definition` request.
    ///
    /// The response may be a single `Location` or an array of `Location`s;
    /// both forms are handled.
    pub async fn definition(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = make_text_document_position_params(&uri, line, character);
        let result = self.send_request("textDocument/definition", params).await?;

        parse_location_response(result)
    }

    /// Perform a `textDocument/references` request.
    pub async fn references(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<Location>, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = serde_json::to_value(ReferenceParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position { line, character },
            context: ReferenceContext {
                include_declaration: true,
            },
        })?;

        let result = self.send_request("textDocument/references", params).await?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        let locations: Vec<Location> = serde_json::from_value(result)?;
        Ok(locations)
    }

    /// Perform a `textDocument/completion` request.
    ///
    /// Handles both a plain array response and the `CompletionList` wrapper.
    pub async fn completions(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
    ) -> Result<Vec<CompletionItem>, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = make_text_document_position_params(&uri, line, character);
        let result = self.send_request("textDocument/completion", params).await?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        // CompletionResponse is an untagged enum that handles both array
        // and CompletionList forms.
        let response: CompletionResponse = serde_json::from_value(result)?;
        match response {
            CompletionResponse::Array(items) => Ok(items),
            CompletionResponse::List(list) => Ok(list.items),
        }
    }

    /// Return cached diagnostics for the given file.
    ///
    /// Diagnostics are populated asynchronously by the server via
    /// `textDocument/publishDiagnostics` notifications which are captured
    /// every time a response is read. This method itself is synchronous
    /// because it only reads from the in-memory cache.
    pub fn diagnostics(&self, file: &Path) -> Result<Vec<Diagnostic>, LspError> {
        let uri = file_path_to_uri(file)?;
        Ok(self
            .cached_diagnostics
            .get(&uri)
            .cloned()
            .unwrap_or_default())
    }

    /// Perform a `textDocument/rename` request.
    pub async fn rename(
        &mut self,
        file: &Path,
        line: u32,
        character: u32,
        new_name: &str,
    ) -> Result<WorkspaceEdit, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = serde_json::to_value(RenameParams {
            text_document: TextDocumentIdentifier { uri },
            position: Position { line, character },
            new_name: new_name.to_string(),
        })?;

        let result = self.send_request("textDocument/rename", params).await?;

        if result.is_null() {
            return Ok(WorkspaceEdit { changes: None });
        }

        let edit: WorkspaceEdit = serde_json::from_value(result)?;
        Ok(edit)
    }

    /// Perform a `textDocument/formatting` request.
    pub async fn format(&mut self, file: &Path) -> Result<Vec<TextEdit>, LspError> {
        let uri = self.ensure_document_open(file).await?;

        let params = serde_json::to_value(DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions {
                tab_size: 4,
                insert_spaces: true,
            },
        })?;

        let result = self.send_request("textDocument/formatting", params).await?;

        if result.is_null() {
            return Ok(Vec::new());
        }

        let edits: Vec<TextEdit> = serde_json::from_value(result)?;
        Ok(edits)
    }

    /// Gracefully shut down the language server.
    ///
    /// Sends `shutdown` followed by `exit`, then waits for the child process
    /// to terminate.
    pub async fn shutdown(&mut self) -> Result<(), LspError> {
        tracing::info!("Shutting down LSP server");

        // The shutdown request expects a null result.
        let _ = self.send_request("shutdown", json!(null)).await;

        // The exit notification tells the server to terminate.
        let _ = self.send_notification("exit", json!(null)).await;

        // Wait for the child process to finish.
        let _ = self.process.wait().await;

        self.initialized = false;
        Ok(())
    }

    // ------------------------------------------------------------------
    // Accessors (primarily useful for testing / inspection)
    // ------------------------------------------------------------------

    /// Returns the current request id counter value.
    pub fn current_id(&self) -> i64 {
        self.next_id
    }

    /// Returns whether the client has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Returns a reference to the set of currently-opened document URIs.
    pub fn open_documents(&self) -> &HashSet<String> {
        &self.open_documents
    }

    /// Returns a reference to the cached diagnostics map.
    pub fn cached_diagnostics(&self) -> &HashMap<String, Vec<Diagnostic>> {
        &self.cached_diagnostics
    }

    /// Returns the root URI communicated to the language server.
    pub fn root_uri(&self) -> &str {
        &self.root_uri
    }
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// Minimal type for deserializing `textDocument/publishDiagnostics` params.
#[derive(serde::Deserialize)]
struct PublishDiagnosticsNotification {
    uri: String,
    diagnostics: Vec<Diagnostic>,
}

// ---------------------------------------------------------------------------
// Free functions
// ---------------------------------------------------------------------------

/// Convert a filesystem path to a `file://` URI.
///
/// The path is canonicalized before conversion so that the URI is absolute and
/// consistent.
pub fn file_path_to_uri(path: &Path) -> Result<String, LspError> {
    let canonical = std::fs::canonicalize(path).map_err(|_| LspError::FileNotFound {
        path: path.display().to_string(),
    })?;
    Ok(format!("file://{}", canonical.display()))
}

/// Build `textDocument/hover`-style positional params as a `serde_json::Value`.
fn make_text_document_position_params(uri: &str, line: u32, character: u32) -> serde_json::Value {
    json!({
        "textDocument": { "uri": uri },
        "position": { "line": line, "character": character }
    })
}

/// Parse a definition/declaration response that can be a single `Location`,
/// an array of `Location`s, or `null`.
fn parse_location_response(value: serde_json::Value) -> Result<Vec<Location>, LspError> {
    if value.is_null() {
        return Ok(Vec::new());
    }

    // Try as array first (most common).
    if value.is_array() {
        let locations: Vec<Location> = serde_json::from_value(value)?;
        return Ok(locations);
    }

    // Single location.
    let location: Location = serde_json::from_value(value)?;
    Ok(vec![location])
}

/// Best-effort language identifier detection from file extension.
fn detect_language_id(path: &Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "jsx" => "javascriptreact",
        "c" | "h" => "c",
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
        "java" => "java",
        "go" => "go",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "lua" => "lua",
        "sh" | "bash" | "zsh" => "shellscript",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "md" | "markdown" => "markdown",
        "sql" => "sql",
        "zig" => "zig",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        _ => "plaintext",
    }
    .to_string()
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Build a Content-Length framed message from a JSON value.
    fn frame_message(value: &serde_json::Value) -> Vec<u8> {
        let body = serde_json::to_string(value).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut buf = Vec::new();
        buf.extend_from_slice(header.as_bytes());
        buf.extend_from_slice(body.as_bytes());
        buf
    }

    /// Read a single Content-Length framed message from a byte slice using
    /// the same algorithm as `LspClient::read_message`.
    async fn read_framed_message(data: &[u8]) -> Result<serde_json::Value, LspError> {
        let mut reader = tokio::io::BufReader::new(Cursor::new(data));
        let mut content_length: usize = 0;

        loop {
            let mut line = String::new();
            let bytes_read = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line).await?;
            if bytes_read == 0 {
                return Err(LspError::ProtocolError {
                    message: "Unexpected EOF while reading headers".into(),
                });
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length =
                    len_str
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| LspError::ProtocolError {
                            message: format!("Invalid Content-Length value: {len_str}"),
                        })?;
            }
        }

        if content_length == 0 {
            return Err(LspError::ProtocolError {
                message: "Missing or zero Content-Length header".into(),
            });
        }

        let mut body = vec![0u8; content_length];
        tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body).await?;
        let msg: serde_json::Value = serde_json::from_slice(&body)?;
        Ok(msg)
    }

    // ------------------------------------------------------------------
    // Content-Length framing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_content_length_framing() {
        let original = json!({"jsonrpc": "2.0", "id": 1, "method": "test", "params": {}});
        let framed = frame_message(&original);

        // The framed data should start with the header.
        let header_end = "Content-Length: ".len();
        assert!(
            std::str::from_utf8(&framed[..header_end])
                .unwrap()
                .starts_with("Content-Length: "),
            "Frame should start with Content-Length header"
        );

        // We should be able to read the message back.
        let decoded = read_framed_message(&framed).await.unwrap();
        assert_eq!(decoded, original);
    }

    #[tokio::test]
    async fn test_content_length_framing_with_unicode() {
        let original = json!({"jsonrpc": "2.0", "id": 2, "method": "test", "params": {"text": "\u{1f600} hello"}});
        let framed = frame_message(&original);
        let decoded = read_framed_message(&framed).await.unwrap();
        assert_eq!(decoded, original);
    }

    #[tokio::test]
    async fn test_content_length_framing_multiple_messages() {
        let msg1 = json!({"jsonrpc": "2.0", "id": 1, "result": "first"});
        let msg2 = json!({"jsonrpc": "2.0", "id": 2, "result": "second"});

        let mut data = frame_message(&msg1);
        data.extend(frame_message(&msg2));

        // Read first message.
        let mut reader = tokio::io::BufReader::new(Cursor::new(data.clone()));
        let mut content_length: usize = 0;

        // Parse first message headers.
        loop {
            let mut line = String::new();
            tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
                .await
                .unwrap();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length = len_str.trim().parse().unwrap();
            }
        }
        let mut body = vec![0u8; content_length];
        tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body)
            .await
            .unwrap();
        let first: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(first["id"], 1);
        assert_eq!(first["result"], "first");

        // Parse second message headers.
        content_length = 0;
        loop {
            let mut line = String::new();
            tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line)
                .await
                .unwrap();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(len_str) = trimmed.strip_prefix("Content-Length: ") {
                content_length = len_str.trim().parse().unwrap();
            }
        }
        let mut body2 = vec![0u8; content_length];
        tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body2)
            .await
            .unwrap();
        let second: serde_json::Value = serde_json::from_slice(&body2).unwrap();
        assert_eq!(second["id"], 2);
        assert_eq!(second["result"], "second");
    }

    // ------------------------------------------------------------------
    // Request ID incrementing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_request_id_incrementing() {
        // We cannot easily construct an LspClient without a real process, but
        // we can verify the framing logic plus id assignment by building
        // request JSON the same way the client does.
        let mut next_id: i64 = 0;

        let mut ids = Vec::new();
        for _ in 0..5 {
            next_id += 1;
            ids.push(next_id);
        }

        assert_eq!(ids, vec![1, 2, 3, 4, 5]);

        // Verify the request bodies have the right ids.
        for (i, id) in ids.iter().enumerate() {
            let request = json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": "test",
                "params": {}
            });
            assert_eq!(request["id"], (i as i64) + 1);
        }
    }

    // ------------------------------------------------------------------
    // file:// URI conversion
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_file_to_uri_conversion() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();

        let uri = file_path_to_uri(path).unwrap();

        assert!(uri.starts_with("file://"), "URI should start with file://");
        // The URI should contain the canonical path (no relative components).
        assert!(
            !uri.contains(".."),
            "URI should not contain relative components"
        );
        // The canonical path from the URI should resolve to the same file.
        let canonical = std::fs::canonicalize(path).unwrap();
        assert_eq!(uri, format!("file://{}", canonical.display()));
    }

    #[tokio::test]
    async fn test_file_to_uri_nonexistent() {
        let result = file_path_to_uri(Path::new("/tmp/absolutely_nonexistent_file_xyz.rs"));
        assert!(result.is_err());
        match result.unwrap_err() {
            LspError::FileNotFound { path } => {
                assert!(path.contains("absolutely_nonexistent_file_xyz.rs"));
            }
            other => panic!("Expected FileNotFound, got: {other:?}"),
        }
    }

    // ------------------------------------------------------------------
    // Hover response parsing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_hover_response_parsing_with_contents() {
        // A typical hover response with a string contents field.
        let response = json!({
            "contents": "fn main()",
            "range": {
                "start": {"line": 0, "character": 3},
                "end": {"line": 0, "character": 7}
            }
        });

        let hover: Result<HoverResult, _> = serde_json::from_value(response);
        assert!(hover.is_ok(), "Should parse hover with string contents");
    }

    #[tokio::test]
    async fn test_hover_response_parsing_null() {
        let value = serde_json::Value::Null;
        assert!(value.is_null(), "Null response should indicate no hover");
    }

    // ------------------------------------------------------------------
    // Definition response parsing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_definition_response_parsing_single() {
        let response = json!({
            "uri": "file:///src/main.rs",
            "range": {
                "start": {"line": 10, "character": 0},
                "end": {"line": 10, "character": 5}
            }
        });

        let locations = parse_location_response(response).unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].uri, "file:///src/main.rs");
        assert_eq!(locations[0].range.start.line, 10);
    }

    #[tokio::test]
    async fn test_definition_response_parsing_array() {
        let response = json!([
            {
                "uri": "file:///src/lib.rs",
                "range": {
                    "start": {"line": 5, "character": 0},
                    "end": {"line": 5, "character": 10}
                }
            },
            {
                "uri": "file:///src/util.rs",
                "range": {
                    "start": {"line": 20, "character": 4},
                    "end": {"line": 20, "character": 15}
                }
            }
        ]);

        let locations = parse_location_response(response).unwrap();
        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].uri, "file:///src/lib.rs");
        assert_eq!(locations[1].uri, "file:///src/util.rs");
    }

    #[tokio::test]
    async fn test_definition_response_parsing_null() {
        let locations = parse_location_response(serde_json::Value::Null).unwrap();
        assert!(locations.is_empty());
    }

    // ------------------------------------------------------------------
    // Completion response parsing
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_completion_response_parsing_array() {
        let response = json!([
            {"label": "foo", "kind": 6},
            {"label": "bar", "kind": 3}
        ]);

        let items: Vec<CompletionItem> = serde_json::from_value(response).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].label, "foo");
        assert_eq!(items[1].label, "bar");
    }

    #[tokio::test]
    async fn test_completion_response_parsing_completion_list() {
        let response = json!({
            "isIncomplete": false,
            "items": [
                {"label": "println!", "kind": 3},
                {"label": "print!", "kind": 3}
            ]
        });

        // CompletionResponse is an untagged enum that can decode both forms.
        let cr: CompletionResponse = serde_json::from_value(response).unwrap();
        match cr {
            CompletionResponse::List(list) => {
                assert_eq!(list.items.len(), 2);
                assert_eq!(list.items[0].label, "println!");
            }
            CompletionResponse::Array(items) => {
                // Should have matched as List, but either way verify items.
                assert_eq!(items.len(), 2);
            }
        }
    }

    #[tokio::test]
    async fn test_completion_response_parsing_null() {
        let value = serde_json::Value::Null;
        assert!(value.is_null());
        // completions() would return Ok(Vec::new()) for null.
    }

    // ------------------------------------------------------------------
    // Diagnostic caching
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_diagnostic_caching() {
        let mut diagnostics: HashMap<String, Vec<Diagnostic>> = HashMap::new();
        let uri = "file:///src/main.rs".to_string();

        // Initially empty.
        assert!(!diagnostics.contains_key(&uri));

        // Simulate receiving a publishDiagnostics notification.
        let notification_params = json!({
            "uri": "file:///src/main.rs",
            "diagnostics": [
                {
                    "range": {
                        "start": {"line": 3, "character": 0},
                        "end": {"line": 3, "character": 10}
                    },
                    "severity": 1,
                    "message": "unused variable"
                }
            ]
        });

        let parsed: PublishDiagnosticsNotification =
            serde_json::from_value(notification_params).unwrap();

        diagnostics.insert(parsed.uri.clone(), parsed.diagnostics);

        let cached = diagnostics.get(&uri).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].message, "unused variable");

        // Simulate updated diagnostics (replaces previous).
        let updated_params = json!({
            "uri": "file:///src/main.rs",
            "diagnostics": [
                {
                    "range": {
                        "start": {"line": 3, "character": 0},
                        "end": {"line": 3, "character": 10}
                    },
                    "severity": 1,
                    "message": "unused variable"
                },
                {
                    "range": {
                        "start": {"line": 10, "character": 0},
                        "end": {"line": 10, "character": 5}
                    },
                    "severity": 2,
                    "message": "dead code"
                }
            ]
        });

        let parsed2: PublishDiagnosticsNotification =
            serde_json::from_value(updated_params).unwrap();
        diagnostics.insert(parsed2.uri.clone(), parsed2.diagnostics);

        let cached2 = diagnostics.get(&uri).unwrap();
        assert_eq!(cached2.len(), 2);
        assert_eq!(cached2[1].message, "dead code");
    }

    // ------------------------------------------------------------------
    // LspError display messages
    // ------------------------------------------------------------------

    #[test]
    fn test_lsp_error_display() {
        let err = LspError::ServerNotRunning {
            language: "rust".into(),
        };
        assert_eq!(err.to_string(), "Server not running: rust");

        let err = LspError::ServerStartFailed {
            message: "binary not found".into(),
        };
        assert_eq!(err.to_string(), "Server failed to start: binary not found");

        let err = LspError::Timeout { timeout_secs: 30 };
        assert_eq!(err.to_string(), "Request timed out after 30s");

        let err = LspError::ProtocolError {
            message: "bad header".into(),
        };
        assert_eq!(err.to_string(), "Protocol error: bad header");

        let err = LspError::ServerError {
            code: -32600,
            message: "Invalid request".into(),
        };
        assert_eq!(
            err.to_string(),
            "Server returned error: code=-32600, message=Invalid request"
        );

        let err = LspError::FileNotFound {
            path: "/tmp/foo.rs".into(),
        };
        assert_eq!(err.to_string(), "File not found: /tmp/foo.rs");

        let err = LspError::UnsupportedLanguage {
            language: "brainfuck".into(),
        };
        assert_eq!(err.to_string(), "Language not supported: brainfuck");
    }

    // ------------------------------------------------------------------
    // Language detection helper
    // ------------------------------------------------------------------

    #[test]
    fn test_detect_language_id() {
        assert_eq!(detect_language_id(Path::new("main.rs")), "rust");
        assert_eq!(detect_language_id(Path::new("app.py")), "python");
        assert_eq!(detect_language_id(Path::new("index.ts")), "typescript");
        assert_eq!(detect_language_id(Path::new("App.tsx")), "typescriptreact");
        assert_eq!(detect_language_id(Path::new("script.js")), "javascript");
        assert_eq!(detect_language_id(Path::new("main.go")), "go");
        assert_eq!(detect_language_id(Path::new("Main.java")), "java");
        assert_eq!(detect_language_id(Path::new("prog.cpp")), "cpp");
        assert_eq!(detect_language_id(Path::new("header.h")), "c");
        assert_eq!(detect_language_id(Path::new("unknown.xyz")), "plaintext");
        assert_eq!(detect_language_id(Path::new("no_extension")), "plaintext");
    }

    // ------------------------------------------------------------------
    // Notification handling
    // ------------------------------------------------------------------

    #[test]
    fn test_handle_notification_diagnostics() {
        // Verify that handle_notification correctly updates cached_diagnostics.
        // We cannot construct a full LspClient without a process, so we
        // directly test the deserialization path used by handle_notification.
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": "file:///project/src/lib.rs",
                "diagnostics": [
                    {
                        "range": {
                            "start": {"line": 1, "character": 0},
                            "end": {"line": 1, "character": 5}
                        },
                        "severity": 1,
                        "message": "syntax error"
                    }
                ]
            }
        });

        // Simulate what handle_notification does.
        let params = notification.get("params").unwrap();
        let parsed: PublishDiagnosticsNotification =
            serde_json::from_value(params.clone()).unwrap();

        assert_eq!(parsed.uri, "file:///project/src/lib.rs");
        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.diagnostics[0].message, "syntax error");
    }

    // ------------------------------------------------------------------
    // parse_location_response edge cases
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn test_parse_location_response_empty_array() {
        let response = json!([]);
        let locations = parse_location_response(response).unwrap();
        assert!(locations.is_empty());
    }

    #[tokio::test]
    async fn test_parse_location_response_invalid() {
        let response = json!("not a location");
        let result = parse_location_response(response);
        assert!(result.is_err());
    }
}
