//! Transport layer for the MCP server.
//!
//! Provides different transport implementations for JSON-RPC 2.0 message exchange:
//! - [`StdioTransport`]: Newline-delimited JSON (NDJSON) over stdin/stdout
//! - [`ChannelTransport`]: In-process tokio mpsc channels (for testing)
//! - [`HttpTransport`]: HTTP-based transport (placeholder)

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Stdin, Stdout};
use tokio::sync::mpsc;

use crate::error::McpError;

/// Trait for reading and writing JSON-RPC messages over a transport.
///
/// Implementations handle message framing and I/O for a specific transport
/// mechanism. All methods are async and the trait is object-safe via `async_trait`.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Read the next JSON-RPC message from the transport.
    ///
    /// Returns `Ok(Some(message))` when a message is available,
    /// `Ok(None)` on EOF (the remote side closed the connection),
    /// or `Err` on I/O or protocol errors.
    async fn read_message(&mut self) -> Result<Option<String>, McpError>;

    /// Write a JSON-RPC message to the transport.
    ///
    /// The implementation is responsible for framing (e.g., appending a newline)
    /// and flushing the underlying writer.
    async fn write_message(&mut self, message: &str) -> Result<(), McpError>;

    /// Gracefully close the transport.
    ///
    /// Flushes any buffered output and releases resources.
    async fn close(&mut self) -> Result<(), McpError>;
}

// ---------------------------------------------------------------------------
// StdioTransport
// ---------------------------------------------------------------------------

/// Transport that reads/writes newline-delimited JSON over stdin/stdout.
///
/// Each JSON-RPC message occupies exactly one line (NDJSON framing).
/// This is the standard transport used when the MCP server is launched as a
/// child process by a host application such as Claude Desktop.
pub struct StdioTransport {
    reader: BufReader<Stdin>,
    writer: Stdout,
}

impl StdioTransport {
    /// Create a new `StdioTransport` using the process stdin and stdout.
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Transport for StdioTransport {
    async fn read_message(&mut self) -> Result<Option<String>, McpError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            // EOF — the remote side closed its end.
            return Ok(None);
        }
        // Strip the trailing newline (and possible \r\n on Windows).
        let trimmed = line.trim_end().to_string();
        Ok(Some(trimmed))
    }

    async fn write_message(&mut self, message: &str) -> Result<(), McpError> {
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), McpError> {
        self.writer.flush().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ProcessTransport
// ---------------------------------------------------------------------------

/// Transport that communicates with a child process via stdin/stdout.
///
/// Used to connect to external MCP servers spawned as subprocesses (e.g.,
/// `npx chrome-devtools-mcp@latest`). Messages are framed as NDJSON, the
/// same as `StdioTransport` but operating on child process pipes.
pub struct ProcessTransport {
    child_stdin: tokio::process::ChildStdin,
    reader: BufReader<tokio::process::ChildStdout>,
}

impl std::fmt::Debug for ProcessTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessTransport").finish()
    }
}

impl ProcessTransport {
    /// Spawn a child process and create a transport.
    ///
    /// Returns the transport and the child process handle (which the caller
    /// should keep alive or use to gracefully terminate the server).
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
    ) -> Result<(Self, tokio::process::Child), McpError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| McpError::TransportError {
            message: format!("Failed to spawn {}: {}", command, e),
        })?;

        let stdin = child.stdin.take().ok_or_else(|| McpError::TransportError {
            message: "Failed to capture child stdin".into(),
        })?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::TransportError {
                message: "Failed to capture child stdout".into(),
            })?;

        Ok((
            Self {
                child_stdin: stdin,
                reader: BufReader::new(stdout),
            },
            child,
        ))
    }
}

#[async_trait]
impl Transport for ProcessTransport {
    async fn read_message(&mut self) -> Result<Option<String>, McpError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end().to_string();
        Ok(Some(trimmed))
    }

    async fn write_message(&mut self, message: &str) -> Result<(), McpError> {
        use tokio::io::AsyncWriteExt;
        self.child_stdin.write_all(message.as_bytes()).await?;
        self.child_stdin.write_all(b"\n").await?;
        self.child_stdin.flush().await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), McpError> {
        use tokio::io::AsyncWriteExt;
        self.child_stdin.flush().await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ChannelTransport
// ---------------------------------------------------------------------------

/// In-process transport backed by tokio mpsc channels.
///
/// This is primarily useful for integration tests where you want to exercise
/// the full message-handling pipeline without spawning a real subprocess or
/// touching actual stdio file descriptors.
pub struct ChannelTransport {
    receiver: mpsc::Receiver<String>,
    sender: mpsc::Sender<String>,
}

impl ChannelTransport {
    /// Create a new `ChannelTransport` from the given channel halves.
    ///
    /// * `receiver` — incoming messages (read side)
    /// * `sender`   — outgoing messages (write side)
    pub fn new(receiver: mpsc::Receiver<String>, sender: mpsc::Sender<String>) -> Self {
        Self { receiver, sender }
    }

    /// Create a linked pair of `ChannelTransport`s for testing.
    ///
    /// Messages written by one side can be read by the other, and vice versa.
    /// `buffer` controls the capacity of each underlying mpsc channel.
    pub fn pair(buffer: usize) -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(buffer);
        let (tx_b, rx_b) = mpsc::channel(buffer);
        (
            ChannelTransport::new(rx_a, tx_b),
            ChannelTransport::new(rx_b, tx_a),
        )
    }
}

#[async_trait]
impl Transport for ChannelTransport {
    async fn read_message(&mut self) -> Result<Option<String>, McpError> {
        match self.receiver.recv().await {
            Some(msg) => Ok(Some(msg)),
            None => Ok(None), // All senders dropped — EOF.
        }
    }

    async fn write_message(&mut self, message: &str) -> Result<(), McpError> {
        self.sender
            .send(message.to_string())
            .await
            .map_err(|e| McpError::TransportError {
                message: format!("channel send failed: {e}"),
            })?;
        Ok(())
    }

    async fn close(&mut self) -> Result<(), McpError> {
        // Closing the receiver signals that we will not read any more messages.
        self.receiver.close();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HttpTransport (placeholder)
// ---------------------------------------------------------------------------

/// HTTP-based transport for the MCP server.
///
/// This will eventually support Server-Sent Events (SSE) for server-to-client
/// streaming and regular HTTP POST for client-to-server requests.
///
/// **Not yet implemented** — all methods currently panic with `todo!`.
pub struct HttpTransport {
    /// The port the HTTP server will listen on.
    pub port: u16,
}

impl HttpTransport {
    /// Create a new `HttpTransport` that will bind to the given `port`.
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn read_message(&mut self) -> Result<Option<String>, McpError> {
        Err(McpError::TransportError {
            message: "HTTP transport is not yet implemented".into(),
        })
    }

    async fn write_message(&mut self, _message: &str) -> Result<(), McpError> {
        Err(McpError::TransportError {
            message: "HTTP transport is not yet implemented".into(),
        })
    }

    async fn close(&mut self) -> Result<(), McpError> {
        Err(McpError::TransportError {
            message: "HTTP transport is not yet implemented".into(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_transport_roundtrip() {
        let (mut client, mut server) = ChannelTransport::pair(16);

        // Client sends a message, server reads it.
        client
            .write_message(r#"{"jsonrpc":"2.0","method":"initialize","id":1}"#)
            .await
            .unwrap();

        let received = server.read_message().await.unwrap();
        assert_eq!(
            received,
            Some(r#"{"jsonrpc":"2.0","method":"initialize","id":1}"#.to_string())
        );

        // Server responds, client reads the response.
        server
            .write_message(r#"{"jsonrpc":"2.0","result":{},"id":1}"#)
            .await
            .unwrap();

        let response = client.read_message().await.unwrap();
        assert_eq!(
            response,
            Some(r#"{"jsonrpc":"2.0","result":{},"id":1}"#.to_string())
        );
    }

    #[tokio::test]
    async fn test_channel_transport_eof() {
        let (tx, rx) = mpsc::channel::<String>(16);
        let (dummy_tx, _dummy_rx) = mpsc::channel::<String>(16);
        let mut transport = ChannelTransport::new(rx, dummy_tx);

        // Drop the sender so the receiver observes EOF.
        drop(tx);

        let result = transport.read_message().await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_channel_transport_multiple_messages() {
        let (mut client, mut server) = ChannelTransport::pair(16);

        let messages = vec![
            r#"{"jsonrpc":"2.0","method":"tools/list","id":1}"#,
            r#"{"jsonrpc":"2.0","method":"tools/call","id":2}"#,
            r#"{"jsonrpc":"2.0","method":"resources/list","id":3}"#,
        ];

        // Send all messages from the client side.
        for msg in &messages {
            client.write_message(msg).await.unwrap();
        }

        // Read them back on the server side in order.
        for expected in &messages {
            let received = server.read_message().await.unwrap();
            assert_eq!(received, Some(expected.to_string()));
        }
    }

    #[tokio::test]
    async fn test_channel_transport_write() {
        let (tx, mut rx) = mpsc::channel::<String>(16);
        let (_dummy_tx, dummy_rx) = mpsc::channel::<String>(16);
        let mut transport = ChannelTransport::new(dummy_rx, tx);

        let msg = r#"{"jsonrpc":"2.0","result":{"tools":[]},"id":1}"#;
        transport.write_message(msg).await.unwrap();

        // Verify the raw receiver got the message.
        let received = rx.recv().await.unwrap();
        assert_eq!(received, msg);
    }

    #[tokio::test]
    async fn test_http_transport_returns_error_not_panic() {
        let mut transport = HttpTransport::new(8080);

        // All methods should return errors, NOT panic with todo!()
        let read_result = transport.read_message().await;
        assert!(read_result.is_err());
        assert!(
            read_result
                .unwrap_err()
                .to_string()
                .contains("not yet implemented")
        );

        let write_result = transport.write_message("test").await;
        assert!(write_result.is_err());

        let close_result = transport.close().await;
        assert!(close_result.is_err());
    }

    #[tokio::test]
    async fn test_process_transport_spawn_failure() {
        let result = ProcessTransport::spawn(
            "nonexistent_binary_that_does_not_exist",
            &[],
            &std::collections::HashMap::new(),
        )
        .await;
        match result {
            Err(e) => assert!(e.to_string().contains("Failed to spawn")),
            Ok(_) => panic!("Expected spawn to fail"),
        }
    }

    #[tokio::test]
    async fn test_process_transport_echo_roundtrip() {
        // Use `cat` as a simple echo server (it copies stdin to stdout)
        let result = ProcessTransport::spawn("cat", &[], &std::collections::HashMap::new()).await;

        if let Ok((mut transport, _child)) = result {
            transport
                .write_message(r#"{"jsonrpc":"2.0","method":"test","id":1}"#)
                .await
                .unwrap();

            let received = transport.read_message().await.unwrap();
            assert_eq!(
                received,
                Some(r#"{"jsonrpc":"2.0","method":"test","id":1}"#.to_string())
            );
        }
        // If cat is not available, skip gracefully
    }
}
