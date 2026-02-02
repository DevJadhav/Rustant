//! # Rustant MCP
//!
//! Model Context Protocol (MCP) server implementation for Rustant.
//!
//! The MCP server exposes Rustant's tools and workspace resources via JSON-RPC 2.0
//! over stdio or HTTP, allowing external clients (like Claude Desktop) to use Rustant
//! as a tool provider.
//!
//! ## Architecture
//!
//! ```text
//! Client <-> Transport (stdio/channel) <-> McpServer <-> RequestHandler
//!                                                        |-- ToolRegistry
//!                                                        +-- ResourceManager
//! ```

pub mod client;
pub mod discovery;
pub mod error;
pub mod handlers;
pub mod protocol;
pub mod resources;
pub mod transport;

use error::McpError;
use handlers::RequestHandler;
use protocol::{IncomingMessage, JsonRpcResponse, RequestId};
use resources::ResourceManager;
use rustant_tools::registry::ToolRegistry;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use transport::Transport;

/// The MCP server that processes JSON-RPC messages over a transport.
pub struct McpServer {
    handler: RequestHandler,
}

impl McpServer {
    /// Create a new MCP server with the given tool registry and workspace path.
    pub fn new(tool_registry: Arc<ToolRegistry>, workspace: PathBuf) -> Self {
        let resource_manager = ResourceManager::new(workspace);
        let handler = RequestHandler::new(tool_registry, resource_manager);
        Self { handler }
    }

    /// Run the MCP server on the given transport, processing messages until EOF or error.
    pub async fn run<T: Transport>(&mut self, transport: &mut T) -> Result<(), McpError> {
        info!("MCP server starting");

        loop {
            let message = match transport.read_message().await {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    info!("Transport closed (EOF), shutting down MCP server");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "Transport read error");
                    break;
                }
            };

            if message.trim().is_empty() {
                continue;
            }

            debug!(message = %message, "Received MCP message");

            match self.process_message(&message).await {
                Ok(Some(response)) => {
                    let response_json =
                        serde_json::to_string(&response).map_err(|e| McpError::InternalError {
                            message: format!("Failed to serialize response: {}", e),
                        })?;
                    debug!(response = %response_json, "Sending MCP response");
                    transport.write_message(&response_json).await?;
                }
                Ok(None) => {
                    // Notification — no response needed
                }
                Err(e) => {
                    error!(error = %e, "Error processing MCP message");
                    let error_response = JsonRpcResponse::from_mcp_error(RequestId::Null, e);
                    let error_json =
                        serde_json::to_string(&error_response).unwrap_or_else(|_| {
                            r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32603,"message":"Internal error"}}"#
                                .to_string()
                        });
                    transport.write_message(&error_json).await?;
                }
            }
        }

        transport.close().await?;
        info!("MCP server stopped");
        Ok(())
    }

    /// Process a single incoming JSON-RPC message.
    /// Returns `Some(response)` for requests, `None` for notifications.
    async fn process_message(&mut self, raw: &str) -> Result<Option<JsonRpcResponse>, McpError> {
        let incoming: IncomingMessage =
            serde_json::from_str(raw).map_err(|e| McpError::ParseError {
                message: format!("Invalid JSON-RPC message: {}", e),
            })?;

        if incoming.jsonrpc != "2.0" {
            return Err(McpError::InvalidRequest {
                message: format!("Expected jsonrpc version 2.0, got: {}", incoming.jsonrpc),
            });
        }

        if incoming.is_notification() {
            debug!(method = %incoming.method, "Processing notification");
            match self.handler.route(&incoming.method, incoming.params).await {
                Ok(_) => Ok(None),
                Err(e) => {
                    warn!(method = %incoming.method, error = %e, "Notification handler error");
                    Ok(None) // Still no response for notifications
                }
            }
        } else {
            let id = incoming.id.unwrap_or(RequestId::Null);
            debug!(method = %incoming.method, "Processing request");

            match self.handler.route(&incoming.method, incoming.params).await {
                Ok(result) => Ok(Some(JsonRpcResponse::success(id, result))),
                Err(e) => Ok(Some(JsonRpcResponse::from_mcp_error(id, e))),
            }
        }
    }

    /// Check if the server has been initialized by a client.
    pub fn is_initialized(&self) -> bool {
        self.handler.is_initialized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MCP_PROTOCOL_VERSION;
    use crate::transport::ChannelTransport;
    use serde_json::json;
    use tempfile::TempDir;

    fn setup_server() -> (McpServer, TempDir) {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, dir.path().to_path_buf());
        let server = McpServer::new(Arc::new(registry), dir.path().to_path_buf());
        (server, dir)
    }

    fn init_request(id: i64) -> String {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "test-client", "version": "1.0"}
            }
        })
        .to_string()
    }

    fn initialized_notification() -> String {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })
        .to_string()
    }

    #[test]
    fn test_mcp_server_creation() {
        let (server, _dir) = setup_server();
        assert!(!server.is_initialized());
    }

    #[tokio::test]
    async fn test_process_initialize() {
        let (mut server, _dir) = setup_server();
        let resp = server.process_message(&init_request(1)).await.unwrap();
        assert!(resp.is_some());

        let resp = resp.unwrap();
        let result = resp.result.unwrap();
        assert_eq!(
            result["protocolVersion"].as_str().unwrap(),
            MCP_PROTOCOL_VERSION
        );
        assert_eq!(result["serverInfo"]["name"].as_str().unwrap(), "rustant");
        assert!(server.is_initialized());
    }

    #[tokio::test]
    async fn test_process_notification() {
        let (mut server, _dir) = setup_server();
        server.process_message(&init_request(1)).await.unwrap();

        let resp = server
            .process_message(&initialized_notification())
            .await
            .unwrap();
        assert!(resp.is_none());
    }

    #[tokio::test]
    async fn test_process_tools_list() {
        let (mut server, _dir) = setup_server();
        server.process_message(&init_request(1)).await.unwrap();

        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })
        .to_string();
        let resp = server.process_message(&list_req).await.unwrap().unwrap();
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        // 12 base tools + 3 iMessage tools on macOS
        #[cfg(target_os = "macos")]
        assert_eq!(tools.len(), 15);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(tools.len(), 12);
    }

    #[tokio::test]
    async fn test_process_tools_call() {
        let (mut server, _dir) = setup_server();
        server.process_message(&init_request(1)).await.unwrap();

        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "echo", "arguments": {"text": "hello mcp"}}
        })
        .to_string();
        let resp = server.process_message(&call_req).await.unwrap().unwrap();
        let result = resp.result.unwrap();
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello mcp"));
    }

    #[tokio::test]
    async fn test_process_invalid_json() {
        let (mut server, _dir) = setup_server();
        let result = server.process_message("not json").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_process_unknown_method() {
        let (mut server, _dir) = setup_server();
        server.process_message(&init_request(1)).await.unwrap();

        let req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "unknown/method",
            "params": {}
        })
        .to_string();
        let resp = server.process_message(&req).await.unwrap().unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn test_run_with_channel_transport() {
        let (mut server, dir) = setup_server();
        std::fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

        let (mut client, mut server_transport) = ChannelTransport::pair(32);

        let server_handle = tokio::spawn(async move { server.run(&mut server_transport).await });

        // 1. Initialize
        client.write_message(&init_request(1)).await.unwrap();
        let resp_str = client.read_message().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
        assert!(resp.result.is_some());
        assert_eq!(
            resp.result.as_ref().unwrap()["protocolVersion"]
                .as_str()
                .unwrap(),
            MCP_PROTOCOL_VERSION
        );

        // 2. Initialized notification (no response expected)
        client
            .write_message(&initialized_notification())
            .await
            .unwrap();

        // 3. List tools
        let list_req = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })
        .to_string();
        client.write_message(&list_req).await.unwrap();
        let resp_str = client.read_message().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
        let tools = resp.result.unwrap()["tools"].as_array().unwrap().clone();
        // 12 base tools + 3 iMessage tools on macOS
        #[cfg(target_os = "macos")]
        assert_eq!(tools.len(), 15);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(tools.len(), 12);

        // 4. Call echo tool
        let call_req = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "echo", "arguments": {"text": "channel test"}}
        })
        .to_string();
        client.write_message(&call_req).await.unwrap();
        let resp_str = client.read_message().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
        assert!(resp.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("channel test"));

        // 5. List resources
        let res_req = json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "resources/list",
            "params": {}
        })
        .to_string();
        client.write_message(&res_req).await.unwrap();
        let resp_str = client.read_message().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
        let resources = resp.result.unwrap()["resources"]
            .as_array()
            .unwrap()
            .clone();
        assert!(!resources.is_empty());

        // 6. Read resource
        let uri = resources[0]["uri"].as_str().unwrap();
        let read_req = json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "resources/read",
            "params": {"uri": uri}
        })
        .to_string();
        client.write_message(&read_req).await.unwrap();
        let resp_str = client.read_message().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_str).unwrap();
        assert!(resp.result.unwrap()["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("fn main()"));

        // Close client to signal EOF
        drop(client);
        let result = server_handle.await.unwrap();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_empty_transport() {
        let (mut server, _dir) = setup_server();
        let (client, mut server_transport) = ChannelTransport::pair(1);
        drop(client);
        let result = server.run(&mut server_transport).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_process_wrong_jsonrpc_version() {
        let (mut server, _dir) = setup_server();
        let req = json!({
            "jsonrpc": "1.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        })
        .to_string();
        let result = server.process_message(&req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tools_list_not_initialized() {
        let (mut server, _dir) = setup_server();
        let req = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list",
            "params": {}
        })
        .to_string();
        let resp = server.process_message(&req).await.unwrap().unwrap();
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32003);
    }
}
