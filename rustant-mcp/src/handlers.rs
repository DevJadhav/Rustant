//! MCP request handlers — routes JSON-RPC requests to the appropriate handler.

use crate::error::McpError;
use crate::protocol::{
    CallToolParams, CallToolResult, InitializeParams, InitializeResult, ListResourcesResult,
    ListToolsResult, McpTool, ReadResourceParams, ReadResourceResult, ResourcesCapability,
    ServerCapabilities, ServerInfo, ToolContent, ToolsCapability, MCP_PROTOCOL_VERSION,
};
use crate::resources::ResourceManager;
use rustant_tools::registry::ToolRegistry;
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Handles MCP protocol requests by delegating to the tool registry and resource manager.
pub struct RequestHandler {
    tool_registry: Arc<ToolRegistry>,
    resource_manager: ResourceManager,
    initialized: bool,
    server_info: ServerInfo,
}

impl RequestHandler {
    /// Create a new request handler.
    pub fn new(tool_registry: Arc<ToolRegistry>, resource_manager: ResourceManager) -> Self {
        Self {
            tool_registry,
            resource_manager,
            initialized: false,
            server_info: ServerInfo {
                name: "rustant".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// Check if the server has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Handle the `initialize` request.
    pub fn handle_initialize(&mut self, params: InitializeParams) -> Result<Value, McpError> {
        info!(
            client = %params.client_info.name,
            client_version = ?params.client_info.version,
            protocol_version = %params.protocol_version,
            "MCP client connecting"
        );

        self.initialized = true;

        let result = InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(false),
                }),
            },
            server_info: self.server_info.clone(),
        };

        serde_json::to_value(result).map_err(|e| McpError::InternalError {
            message: format!("Failed to serialize initialize result: {}", e),
        })
    }

    /// Handle the `notifications/initialized` notification.
    pub fn handle_initialized(&self) {
        info!("MCP client initialized successfully");
    }

    /// Handle the `tools/list` request.
    pub fn handle_tools_list(&self) -> Result<Value, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        let definitions = self.tool_registry.list_definitions();
        let tools: Vec<McpTool> = definitions
            .into_iter()
            .map(|def| McpTool {
                name: def.name,
                description: Some(def.description),
                input_schema: def.parameters,
            })
            .collect();

        debug!(count = tools.len(), "Listing tools");

        let result = ListToolsResult { tools };
        serde_json::to_value(result).map_err(|e| McpError::InternalError {
            message: format!("Failed to serialize tools list: {}", e),
        })
    }

    /// Handle the `tools/call` request.
    pub async fn handle_tools_call(&self, params: CallToolParams) -> Result<Value, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        let tool_name = &params.name;
        let arguments = params
            .arguments
            .unwrap_or(Value::Object(Default::default()));

        info!(tool = %tool_name, "Calling tool via MCP");
        debug!(tool = %tool_name, args = %arguments, "Tool call arguments");

        // Verify the tool exists before executing
        if self.tool_registry.get(tool_name).is_none() {
            return Err(McpError::ToolError {
                message: format!("Tool not found: {}", tool_name),
            });
        }

        match self.tool_registry.execute(tool_name, arguments).await {
            Ok(output) => {
                let is_error = output
                    .metadata
                    .get("is_error")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let result = CallToolResult {
                    content: vec![ToolContent::Text {
                        text: output.content,
                    }],
                    is_error: if is_error { Some(true) } else { None },
                };

                serde_json::to_value(result).map_err(|e| McpError::InternalError {
                    message: format!("Failed to serialize tool result: {}", e),
                })
            }
            Err(e) => {
                warn!(tool = %tool_name, error = %e, "Tool execution failed");
                let result = CallToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Error: {}", e),
                    }],
                    is_error: Some(true),
                };

                serde_json::to_value(result).map_err(|e| McpError::InternalError {
                    message: format!("Failed to serialize error result: {}", e),
                })
            }
        }
    }

    /// Handle the `resources/list` request.
    pub fn handle_resources_list(&self) -> Result<Value, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        let resources = self.resource_manager.list_resources()?;
        debug!(count = resources.len(), "Listing resources");

        let result = ListResourcesResult { resources };
        serde_json::to_value(result).map_err(|e| McpError::InternalError {
            message: format!("Failed to serialize resources list: {}", e),
        })
    }

    /// Handle the `resources/read` request.
    pub fn handle_resources_read(&self, params: ReadResourceParams) -> Result<Value, McpError> {
        if !self.initialized {
            return Err(McpError::NotInitialized);
        }

        info!(uri = %params.uri, "Reading resource via MCP");

        let contents = self.resource_manager.read_resource(&params.uri)?;
        let result = ReadResourceResult { contents };

        serde_json::to_value(result).map_err(|e| McpError::InternalError {
            message: format!("Failed to serialize resource contents: {}", e),
        })
    }

    /// Route a JSON-RPC method to the appropriate handler.
    /// Returns the result value or an error.
    pub async fn route(&mut self, method: &str, params: Value) -> Result<Value, McpError> {
        match method {
            "initialize" => {
                let init_params: InitializeParams =
                    serde_json::from_value(params).map_err(|e| McpError::InvalidParams {
                        message: format!("Invalid initialize params: {}", e),
                    })?;
                self.handle_initialize(init_params)
            }
            "notifications/initialized" => {
                self.handle_initialized();
                Ok(Value::Null)
            }
            "tools/list" => self.handle_tools_list(),
            "tools/call" => {
                let call_params: CallToolParams =
                    serde_json::from_value(params).map_err(|e| McpError::InvalidParams {
                        message: format!("Invalid tools/call params: {}", e),
                    })?;
                self.handle_tools_call(call_params).await
            }
            "resources/list" => self.handle_resources_list(),
            "resources/read" => {
                let read_params: ReadResourceParams =
                    serde_json::from_value(params).map_err(|e| McpError::InvalidParams {
                        message: format!("Invalid resources/read params: {}", e),
                    })?;
                self.handle_resources_read(read_params)
            }
            _ => Err(McpError::MethodNotFound {
                method: method.to_string(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientCapabilities, ClientInfo};
    use rustant_tools::registry::ToolRegistry;
    use tempfile::TempDir;

    fn create_test_handler() -> (RequestHandler, TempDir) {
        let dir = TempDir::new().unwrap();
        let registry = ToolRegistry::new();
        let resource_manager = ResourceManager::new(dir.path().to_path_buf());
        let handler = RequestHandler::new(Arc::new(registry), resource_manager);
        (handler, dir)
    }

    fn create_handler_with_tools() -> (RequestHandler, TempDir) {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, dir.path().to_path_buf());
        let resource_manager = ResourceManager::new(dir.path().to_path_buf());
        let handler = RequestHandler::new(Arc::new(registry), resource_manager);
        (handler, dir)
    }

    fn init_params() -> InitializeParams {
        InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.to_string(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "test-client".to_string(),
                version: Some("1.0".to_string()),
            },
        }
    }

    #[test]
    fn test_handler_creation() {
        let (handler, _dir) = create_test_handler();
        assert!(!handler.is_initialized());
    }

    #[test]
    fn test_initialize() {
        let (mut handler, _dir) = create_test_handler();
        assert!(!handler.is_initialized());

        let result = handler.handle_initialize(init_params()).unwrap();
        assert!(handler.is_initialized());

        let result_obj = result.as_object().unwrap();
        assert_eq!(
            result_obj["protocolVersion"].as_str().unwrap(),
            MCP_PROTOCOL_VERSION
        );
        assert!(result_obj["capabilities"]["tools"].is_object());
        assert!(result_obj["capabilities"]["resources"].is_object());
        assert_eq!(
            result_obj["serverInfo"]["name"].as_str().unwrap(),
            "rustant"
        );
    }

    #[test]
    fn test_tools_list_not_initialized() {
        let (handler, _dir) = create_test_handler();
        let result = handler.handle_tools_list();
        assert!(matches!(result.unwrap_err(), McpError::NotInitialized));
    }

    #[test]
    fn test_tools_list_empty() {
        let (mut handler, _dir) = create_test_handler();
        handler.handle_initialize(init_params()).unwrap();

        let result = handler.handle_tools_list().unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(tools.is_empty());
    }

    #[test]
    fn test_tools_list_with_builtin_tools() {
        let (mut handler, _dir) = create_handler_with_tools();
        handler.handle_initialize(init_params()).unwrap();

        let result = handler.handle_tools_list().unwrap();
        let tools = result["tools"].as_array().unwrap();
        // 12 base tools + 3 iMessage tools on macOS
        #[cfg(target_os = "macos")]
        assert_eq!(tools.len(), 15);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(tools.len(), 12);

        // Check that each tool has required fields
        for tool in tools {
            assert!(tool["name"].is_string());
            assert!(tool["inputSchema"].is_object());
        }
    }

    #[tokio::test]
    async fn test_tools_call_not_initialized() {
        let (handler, _dir) = create_test_handler();
        let params = CallToolParams {
            name: "echo".to_string(),
            arguments: Some(serde_json::json!({"text": "hello"})),
        };
        let result = handler.handle_tools_call(params).await;
        assert!(matches!(result.unwrap_err(), McpError::NotInitialized));
    }

    #[tokio::test]
    async fn test_tools_call_echo() {
        let (mut handler, _dir) = create_handler_with_tools();
        handler.handle_initialize(init_params()).unwrap();

        let params = CallToolParams {
            name: "echo".to_string(),
            arguments: Some(serde_json::json!({"text": "hello world"})),
        };
        let result = handler.handle_tools_call(params).await.unwrap();

        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"].as_str().unwrap(), "text");
        assert!(content[0]["text"].as_str().unwrap().contains("hello world"));
        assert!(result.get("isError").is_none() || result["isError"].is_null());
    }

    #[tokio::test]
    async fn test_tools_call_not_found() {
        let (mut handler, _dir) = create_handler_with_tools();
        handler.handle_initialize(init_params()).unwrap();

        let params = CallToolParams {
            name: "nonexistent_tool".to_string(),
            arguments: None,
        };
        let result = handler.handle_tools_call(params).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_tools_call_with_error() {
        let (mut handler, _dir) = create_handler_with_tools();
        handler.handle_initialize(init_params()).unwrap();

        // Call echo without required text parameter
        let params = CallToolParams {
            name: "echo".to_string(),
            arguments: Some(serde_json::json!({})),
        };
        let result = handler.handle_tools_call(params).await.unwrap();
        assert!(result["isError"].as_bool().unwrap());
    }

    #[test]
    fn test_resources_list_not_initialized() {
        let (handler, _dir) = create_test_handler();
        let result = handler.handle_resources_list();
        assert!(matches!(result.unwrap_err(), McpError::NotInitialized));
    }

    #[test]
    fn test_resources_list_empty() {
        let (mut handler, _dir) = create_test_handler();
        handler.handle_initialize(init_params()).unwrap();

        let result = handler.handle_resources_list().unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert!(resources.is_empty());
    }

    #[test]
    fn test_resources_list_with_files() {
        let (mut handler, dir) = create_test_handler();
        handler.handle_initialize(init_params()).unwrap();

        // Create a test file
        std::fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

        let result = handler.handle_resources_list().unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 1);
        assert!(resources[0]["uri"].as_str().unwrap().contains("test.rs"));
    }

    #[test]
    fn test_resources_read_not_initialized() {
        let (handler, _dir) = create_test_handler();
        let params = ReadResourceParams {
            uri: "file:///test.rs".to_string(),
        };
        let result = handler.handle_resources_read(params);
        assert!(matches!(result.unwrap_err(), McpError::NotInitialized));
    }

    #[test]
    fn test_resources_read_file() {
        let (mut handler, dir) = create_test_handler();
        handler.handle_initialize(init_params()).unwrap();

        let file_path = dir.path().join("hello.txt");
        std::fs::write(&file_path, "Hello, MCP!").unwrap();

        let uri = format!("file://{}", file_path.display());
        let params = ReadResourceParams { uri };
        let result = handler.handle_resources_read(params).unwrap();

        let contents = result["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["text"].as_str().unwrap(), "Hello, MCP!");
    }

    #[tokio::test]
    async fn test_route_initialize() {
        let (mut handler, _dir) = create_test_handler();
        let params = serde_json::to_value(init_params()).unwrap();

        let result = handler.route("initialize", params).await.unwrap();
        assert!(handler.is_initialized());
        assert!(result.is_object());
    }

    #[tokio::test]
    async fn test_route_tools_list() {
        let (mut handler, _dir) = create_handler_with_tools();
        let params = serde_json::to_value(init_params()).unwrap();
        handler.route("initialize", params).await.unwrap();

        let result = handler.route("tools/list", Value::Null).await.unwrap();
        assert!(result["tools"].is_array());
    }

    #[tokio::test]
    async fn test_route_unknown_method() {
        let (mut handler, _dir) = create_test_handler();
        let params = serde_json::to_value(init_params()).unwrap();
        handler.route("initialize", params).await.unwrap();

        let result = handler.route("unknown/method", Value::Null).await;
        assert!(matches!(
            result.unwrap_err(),
            McpError::MethodNotFound { .. }
        ));
    }

    #[tokio::test]
    async fn test_route_notifications_initialized() {
        let (mut handler, _dir) = create_test_handler();
        let params = serde_json::to_value(init_params()).unwrap();
        handler.route("initialize", params).await.unwrap();

        let result = handler
            .route("notifications/initialized", Value::Null)
            .await
            .unwrap();
        assert!(result.is_null());
    }

    #[tokio::test]
    async fn test_full_lifecycle() {
        let (mut handler, dir) = create_handler_with_tools();

        // Create test file
        std::fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();

        // 1. Initialize
        let init = serde_json::to_value(init_params()).unwrap();
        let result = handler.route("initialize", init).await.unwrap();
        assert_eq!(
            result["protocolVersion"].as_str().unwrap(),
            MCP_PROTOCOL_VERSION
        );

        // 2. Initialized notification
        handler
            .route("notifications/initialized", Value::Null)
            .await
            .unwrap();

        // 3. List tools
        let tools_result = handler.route("tools/list", Value::Null).await.unwrap();
        let tools = tools_result["tools"].as_array().unwrap();
        assert!(!tools.is_empty());

        // 4. Call a tool
        let call_params = serde_json::json!({"name": "echo", "arguments": {"text": "test"}});
        let call_result = handler.route("tools/call", call_params).await.unwrap();
        assert!(call_result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("test"));

        // 5. List resources
        let resources_result = handler.route("resources/list", Value::Null).await.unwrap();
        let resources = resources_result["resources"].as_array().unwrap();
        assert!(!resources.is_empty());

        // 6. Read a resource
        let uri = resources[0]["uri"].as_str().unwrap();
        let read_params = serde_json::json!({"uri": uri});
        let read_result = handler.route("resources/read", read_params).await.unwrap();
        assert!(read_result["contents"][0]["text"]
            .as_str()
            .unwrap()
            .contains("fn main()"));
    }
}
