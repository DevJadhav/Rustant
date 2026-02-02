//! MCP Client — connects to external MCP servers.
//!
//! The `McpClient` is the inverse of `McpServer`: it connects TO an external MCP server
//! (via stdio transport), performs the initialization handshake, discovers available tools,
//! and can execute tool calls.

use crate::error::McpError;
use crate::protocol::{JsonRpcResponse, McpTool, RequestId, MCP_PROTOCOL_VERSION};
use crate::transport::Transport;
use serde_json::json;
use std::sync::atomic::{AtomicI64, Ordering};
use tracing::{debug, info, warn};

/// MCP client that connects to an external MCP server.
pub struct McpClient {
    initialized: bool,
    server_info: Option<ServerInfo>,
    available_tools: Vec<McpTool>,
    next_id: AtomicI64,
}

/// Information about the connected MCP server.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
}

impl McpClient {
    /// Create a new MCP client.
    pub fn new() -> Self {
        Self {
            initialized: false,
            server_info: None,
            available_tools: Vec::new(),
            next_id: AtomicI64::new(1),
        }
    }

    /// Get the next request ID.
    fn next_id(&self) -> RequestId {
        RequestId::Number(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    /// Whether the client has completed initialization.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get info about the connected server.
    pub fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }

    /// Get the list of available tools from the server.
    pub fn available_tools(&self) -> &[McpTool] {
        &self.available_tools
    }

    /// Perform the initialization handshake with the server.
    pub async fn initialize<T: Transport>(
        &mut self,
        transport: &mut T,
    ) -> Result<ServerInfo, McpError> {
        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "rustant",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        });

        transport
            .write_message(&serde_json::to_string(&request).unwrap())
            .await?;

        let response = self.read_response(transport).await?;
        let result = response.result.ok_or_else(|| McpError::InternalError {
            message: "Initialize response has no result".into(),
        })?;

        let server_info = ServerInfo {
            name: result["serverInfo"]["name"]
                .as_str()
                .unwrap_or("unknown")
                .into(),
            version: result["serverInfo"]["version"]
                .as_str()
                .unwrap_or("0.0.0")
                .into(),
            protocol_version: result["protocolVersion"]
                .as_str()
                .unwrap_or(MCP_PROTOCOL_VERSION)
                .into(),
        };

        info!(
            server = %server_info.name,
            version = %server_info.version,
            "MCP client initialized"
        );

        // Send initialized notification
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        transport
            .write_message(&serde_json::to_string(&notification).unwrap())
            .await?;

        self.server_info = Some(server_info.clone());
        self.initialized = true;

        Ok(server_info)
    }

    /// Discover tools from the connected server.
    pub async fn discover_tools<T: Transport>(
        &mut self,
        transport: &mut T,
    ) -> Result<Vec<McpTool>, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/list",
            "params": {}
        });

        transport
            .write_message(&serde_json::to_string(&request).unwrap())
            .await?;

        let response = self.read_response(transport).await?;
        let result = response.result.ok_or_else(|| McpError::InternalError {
            message: "tools/list response has no result".into(),
        })?;

        let tools_value = result["tools"]
            .as_array()
            .ok_or_else(|| McpError::InternalError {
                message: "tools/list result has no tools array".into(),
            })?;

        let tools: Vec<McpTool> = tools_value
            .iter()
            .filter_map(|v| serde_json::from_value(v.clone()).ok())
            .collect();

        debug!(count = tools.len(), "Discovered tools from MCP server");
        self.available_tools = tools.clone();
        Ok(tools)
    }

    /// Call a tool on the connected server.
    pub async fn call_tool<T: Transport>(
        &self,
        transport: &mut T,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        let id = self.next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments
            }
        });

        transport
            .write_message(&serde_json::to_string(&request).unwrap())
            .await?;

        let response = self.read_response(transport).await?;

        if let Some(error) = response.error {
            return Err(McpError::ToolError {
                message: format!(
                    "Tool '{}' failed: {} (code {})",
                    tool_name, error.message, error.code
                ),
            });
        }

        response.result.ok_or_else(|| McpError::InternalError {
            message: format!("Tool '{}' returned no result", tool_name),
        })
    }

    /// Read and parse a JSON-RPC response from the transport.
    async fn read_response<T: Transport>(
        &self,
        transport: &mut T,
    ) -> Result<JsonRpcResponse, McpError> {
        let raw = transport
            .read_message()
            .await?
            .ok_or_else(|| McpError::TransportError {
                message: "Transport closed while waiting for response".into(),
            })?;

        debug!(raw = %raw, "Received MCP response");

        let response: JsonRpcResponse =
            serde_json::from_str(&raw).map_err(|e| McpError::ParseError {
                message: format!("Invalid JSON-RPC response: {}", e),
            })?;

        if let Some(ref error) = response.error {
            warn!(
                code = error.code,
                message = %error.message,
                "MCP server returned error"
            );
        }

        Ok(response)
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ChannelTransport;
    use crate::McpServer;
    use rustant_tools::registry::ToolRegistry;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn setup_server() -> (McpServer, TempDir) {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, dir.path().to_path_buf());
        let server = McpServer::new(Arc::new(registry), dir.path().to_path_buf());
        (server, dir)
    }

    #[tokio::test]
    async fn test_client_initialize() {
        let (mut server, _dir) = setup_server();
        let (mut client_transport, mut server_transport) = ChannelTransport::pair(32);

        let server_handle = tokio::spawn(async move { server.run(&mut server_transport).await });

        let mut client = McpClient::new();
        assert!(!client.is_initialized());

        let info = client.initialize(&mut client_transport).await.unwrap();
        assert_eq!(info.name, "rustant");
        assert!(client.is_initialized());

        drop(client_transport);
        let _ = server_handle.await;
    }

    #[tokio::test]
    async fn test_client_discover_tools() {
        let (mut server, _dir) = setup_server();
        let (mut client_transport, mut server_transport) = ChannelTransport::pair(32);

        let server_handle = tokio::spawn(async move { server.run(&mut server_transport).await });

        let mut client = McpClient::new();
        client.initialize(&mut client_transport).await.unwrap();

        let tools = client.discover_tools(&mut client_transport).await.unwrap();
        assert_eq!(tools.len(), 12); // 12 builtin tools
        assert!(client.available_tools().len() == 12);

        drop(client_transport);
        let _ = server_handle.await;
    }

    #[tokio::test]
    async fn test_client_call_tool() {
        let (mut server, _dir) = setup_server();
        let (mut client_transport, mut server_transport) = ChannelTransport::pair(32);

        let server_handle = tokio::spawn(async move { server.run(&mut server_transport).await });

        let mut client = McpClient::new();
        client.initialize(&mut client_transport).await.unwrap();

        let result = client
            .call_tool(
                &mut client_transport,
                "echo",
                json!({"text": "hello from client"}),
            )
            .await
            .unwrap();

        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello from client"));

        drop(client_transport);
        let _ = server_handle.await;
    }

    #[tokio::test]
    async fn test_client_call_tool_not_initialized() {
        let client = McpClient::new();
        let (_a, mut b) = ChannelTransport::pair(1);

        let result = client
            .call_tool(&mut b, "echo", json!({"text": "test"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_discover_not_initialized() {
        let mut client = McpClient::new();
        let (_a, mut b) = ChannelTransport::pair(1);

        let result = client.discover_tools(&mut b).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_server_info_serialization() {
        let info = ServerInfo {
            name: "test".into(),
            version: "1.0.0".into(),
            protocol_version: MCP_PROTOCOL_VERSION.into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let restored: ServerInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.name, "test");
    }
}
