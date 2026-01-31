//! JSON-RPC 2.0 and MCP protocol types.
//!
//! This module defines the wire-format types used for communication between
//! MCP clients and the Rustant MCP server. All types follow the JSON-RPC 2.0
//! specification and the Model Context Protocol (MCP) schema.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::error::McpError;

/// The MCP protocol version supported by this implementation.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// JSON-RPC 2.0 core types
// ---------------------------------------------------------------------------

/// A JSON-RPC 2.0 request identifier.
///
/// Per the spec the `id` field can be a number (integer), a string, or JSON
/// null. Custom [`Serialize`] / [`Deserialize`] implementations ensure that
/// each variant is transmitted as the bare JSON value (no wrapping object or
/// tag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestId {
    /// Numeric id (transmitted as a JSON integer).
    Number(i64),
    /// String id.
    String(String),
    /// Null id — allowed by JSON-RPC 2.0 but discouraged.
    Null,
}

impl Serialize for RequestId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            RequestId::Number(n) => serializer.serialize_i64(*n),
            RequestId::String(s) => serializer.serialize_str(s),
            RequestId::Null => serializer.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match value {
            Value::Number(n) => {
                let i = n.as_i64().ok_or_else(|| {
                    serde::de::Error::custom("request id number must be an integer")
                })?;
                Ok(RequestId::Number(i))
            }
            Value::String(s) => Ok(RequestId::String(s)),
            Value::Null => Ok(RequestId::Null),
            _ => Err(serde::de::Error::custom(
                "request id must be a number, string, or null",
            )),
        }
    }
}

/// A JSON-RPC 2.0 request object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Request identifier.
    pub id: RequestId,
    /// Method name.
    pub method: String,
    /// Optional parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 error object included in error responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Numeric error code.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
    /// Optional additional data about the error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A JSON-RPC 2.0 response object.
///
/// Exactly one of `result` or `error` should be present.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// The id from the corresponding request.
    pub id: RequestId,
    /// The result on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// The error on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Create a successful response.
    pub fn success(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: RequestId, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }

    /// Create an error response from an [`McpError`].
    pub fn from_mcp_error(id: RequestId, err: McpError) -> Self {
        Self::error(
            id,
            JsonRpcError {
                code: err.error_code(),
                message: err.to_string(),
                data: None,
            },
        )
    }
}

/// A JSON-RPC 2.0 notification (a request without an `id`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Method name.
    pub method: String,
    /// Optional parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// An incoming JSON-RPC message that could be either a request or a notification.
///
/// This helper type is used during deserialization when the server does not yet
/// know whether the message carries an `id` (request) or not (notification).
/// The `params` field defaults to [`Value::Null`] when absent.
#[derive(Debug, Clone, Deserialize)]
pub struct IncomingMessage {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Present for requests, absent for notifications.
    #[serde(default)]
    pub id: Option<RequestId>,
    /// Method name.
    pub method: String,
    /// Parameters — defaults to `Value::Null` when missing from the JSON.
    #[serde(default)]
    pub params: Value,
}

impl IncomingMessage {
    /// Returns `true` if this message is a notification (no `id`).
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }
}

// ---------------------------------------------------------------------------
// MCP initialization types
// ---------------------------------------------------------------------------

/// Parameters sent by the client in an `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    /// The MCP protocol version the client supports.
    pub protocol_version: String,
    /// Client capabilities.
    pub capabilities: ClientCapabilities,
    /// Information about the client.
    pub client_info: ClientInfo,
}

/// Information about the connecting client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name.
    pub name: String,
    /// Client version (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Capabilities advertised by the client.
///
/// Currently empty; reserved for future extensions.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {}

/// Result returned by the server for an `initialize` request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    /// The MCP protocol version the server selected.
    pub protocol_version: String,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Information about the server.
    pub server_info: ServerInfo,
}

/// Information about the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name.
    pub name: String,
    /// Server version.
    pub version: String,
}

/// Capabilities advertised by the server.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Tool-related capabilities, if the server exposes tools.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
    /// Resource-related capabilities, if the server exposes resources.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
}

/// Capability descriptor for the tools subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    /// Whether the server may send `notifications/tools/listChanged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

/// Capability descriptor for the resources subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    /// Whether the server supports resource subscriptions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribe: Option<bool>,
    /// Whether the server may send `notifications/resources/listChanged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_changed: Option<bool>,
}

// ---------------------------------------------------------------------------
// MCP tool types
// ---------------------------------------------------------------------------

/// Describes a single tool exposed by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    /// Unique tool name.
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the expected input.
    pub input_schema: Value,
}

/// Result for `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    /// The list of available tools.
    pub tools: Vec<McpTool>,
}

/// Parameters for `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    /// The name of the tool to invoke.
    pub name: String,
    /// Optional arguments to pass to the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// Result of a `tools/call` invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    /// One or more content blocks returned by the tool.
    pub content: Vec<ToolContent>,
    /// If `true`, the content represents an error message from the tool.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

/// A single content block inside a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolContent {
    /// Plain text content.
    #[serde(rename = "text")]
    Text {
        /// The text value.
        text: String,
    },
    /// Base64-encoded image content.
    #[serde(rename = "image")]
    Image {
        /// Base64-encoded image data.
        data: String,
        /// MIME type of the image (e.g. `"image/png"`).
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

// ---------------------------------------------------------------------------
// MCP resource types
// ---------------------------------------------------------------------------

/// Describes a single resource exposed by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpResource {
    /// Unique resource URI.
    pub uri: String,
    /// Human-readable name.
    pub name: String,
    /// Optional description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional MIME type of the resource content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

/// Result for `resources/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResourcesResult {
    /// The list of available resources.
    pub resources: Vec<McpResource>,
}

/// Parameters for `resources/read`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceParams {
    /// The URI of the resource to read.
    pub uri: String,
}

/// Result of a `resources/read` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResourceResult {
    /// Content blocks for the resource.
    pub contents: Vec<ResourceContent>,
}

/// A single content block inside a resource read result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceContent {
    /// The resource URI this content belongs to.
    pub uri: String,
    /// MIME type of the content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Textual content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- JSON-RPC 2.0 tests ------------------------------------------------

    #[test]
    fn test_serialize_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: RequestId::Number(1),
            method: "tools/list".into(),
            params: Some(json!({})),
        };
        let serialized = serde_json::to_value(&req).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], 1);
        assert_eq!(serialized["method"], "tools/list");
        assert_eq!(serialized["params"], json!({}));

        // A request without params should omit the field entirely.
        let req_no_params = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: RequestId::Number(2),
            method: "ping".into(),
            params: None,
        };
        let serialized = serde_json::to_value(&req_no_params).unwrap();
        assert!(serialized.get("params").is_none());
    }

    #[test]
    fn test_deserialize_request() {
        let raw = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test-client" }
            }
        });
        let req: JsonRpcRequest = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(req.jsonrpc, "2.0");
        assert_eq!(req.id, RequestId::Number(42));
        assert_eq!(req.method, "initialize");
        assert!(req.params.is_some());

        // Verify the nested params can be parsed as InitializeParams.
        let params: InitializeParams = serde_json::from_value(req.params.unwrap()).unwrap();
        assert_eq!(params.protocol_version, "2024-11-05");
        assert_eq!(params.client_info.name, "test-client");

        // Re-serialize and verify structural equivalence.
        let re_req: JsonRpcRequest = serde_json::from_value(raw.clone()).unwrap();
        let roundtrip = serde_json::to_value(&re_req).unwrap();
        assert_eq!(roundtrip["jsonrpc"], raw["jsonrpc"]);
        assert_eq!(roundtrip["id"], raw["id"]);
        assert_eq!(roundtrip["method"], raw["method"]);
    }

    #[test]
    fn test_serialize_success_response() {
        let resp = JsonRpcResponse::success(RequestId::Number(1), json!({ "tools": [] }));
        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], 1);
        assert_eq!(serialized["result"], json!({ "tools": [] }));
        // `error` must not be present.
        assert!(serialized.get("error").is_none());

        // Round-trip back.
        let deser: JsonRpcResponse = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.id, RequestId::Number(1));
        assert!(deser.result.is_some());
        assert_eq!(deser.result.unwrap(), json!({ "tools": [] }));
        assert!(deser.error.is_none());
    }

    #[test]
    fn test_serialize_error_response() {
        let resp = JsonRpcResponse::error(
            RequestId::String("req-abc".into()),
            JsonRpcError {
                code: -32601,
                message: "Method not found".into(),
                data: Some(json!({ "method": "nonexistent" })),
            },
        );
        let serialized = serde_json::to_value(&resp).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["id"], "req-abc");
        // `result` must not be present.
        assert!(serialized.get("result").is_none());
        let err = &serialized["error"];
        assert_eq!(err["code"], -32601);
        assert_eq!(err["message"], "Method not found");
        assert_eq!(err["data"]["method"], "nonexistent");

        // Round-trip back.
        let deser: JsonRpcResponse = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.id, RequestId::String("req-abc".into()));
        assert!(deser.result.is_none());
        let rpc_err = deser.error.unwrap();
        assert_eq!(rpc_err.code, -32601);
        assert_eq!(rpc_err.message, "Method not found");
        assert_eq!(rpc_err.data.unwrap()["method"], "nonexistent");

        // Also test from_mcp_error helper.
        let mcp_err = McpError::MethodNotFound {
            method: "unknown/method".into(),
        };
        let err_resp = JsonRpcResponse::from_mcp_error(RequestId::Number(5), mcp_err);
        assert_eq!(err_resp.id, RequestId::Number(5));
        let inner = err_resp.error.unwrap();
        assert_eq!(inner.code, -32601);
        assert!(inner.message.contains("unknown/method"));
    }

    #[test]
    fn test_notification_serialization() {
        let note = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "notifications/tools/listChanged".into(),
            params: None,
        };
        let serialized = serde_json::to_value(&note).unwrap();
        assert_eq!(serialized["jsonrpc"], "2.0");
        assert_eq!(serialized["method"], "notifications/tools/listChanged");
        // params should be absent.
        assert!(serialized.get("params").is_none());
        // Must not have `id`.
        assert!(serialized.get("id").is_none());

        // Round-trip.
        let deser: JsonRpcNotification = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.jsonrpc, "2.0");
        assert_eq!(deser.method, "notifications/tools/listChanged");
        assert!(deser.params.is_none());

        // Notification with params.
        let note_with_params = JsonRpcNotification {
            jsonrpc: "2.0".into(),
            method: "notifications/message".into(),
            params: Some(json!({ "level": "info", "data": "hello" })),
        };
        let s = serde_json::to_value(&note_with_params).unwrap();
        assert_eq!(s["params"]["level"], "info");
        let d: JsonRpcNotification = serde_json::from_value(s).unwrap();
        assert_eq!(d.params.unwrap()["data"], "hello");
    }

    #[test]
    fn test_request_id_variants() {
        // --- Number ---
        let id_num = RequestId::Number(7);
        let v = serde_json::to_value(&id_num).unwrap();
        assert_eq!(v, json!(7));
        let back: RequestId = serde_json::from_value(v).unwrap();
        assert_eq!(back, RequestId::Number(7));

        // --- String ---
        let id_str = RequestId::String("abc-123".into());
        let v = serde_json::to_value(&id_str).unwrap();
        assert_eq!(v, json!("abc-123"));
        let back: RequestId = serde_json::from_value(v).unwrap();
        assert_eq!(back, RequestId::String("abc-123".into()));

        // --- Null ---
        let id_null = RequestId::Null;
        let v = serde_json::to_value(&id_null).unwrap();
        assert_eq!(v, json!(null));
        let back: RequestId = serde_json::from_value(v).unwrap();
        assert_eq!(back, RequestId::Null);

        // --- Deserialization from raw JSON ---
        let id: RequestId = serde_json::from_value(json!(99)).unwrap();
        assert_eq!(id, RequestId::Number(99));

        let id: RequestId = serde_json::from_value(json!("req-1")).unwrap();
        assert_eq!(id, RequestId::String("req-1".into()));

        let id: RequestId = serde_json::from_value(json!(null)).unwrap();
        assert_eq!(id, RequestId::Null);
    }

    // -- MCP initialization types ------------------------------------------

    #[test]
    fn test_initialize_params_serde() {
        let params = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.into(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "rustant-test".into(),
                version: Some("0.1.0".into()),
            },
        };
        let serialized = serde_json::to_value(&params).unwrap();
        assert_eq!(serialized["protocolVersion"], "2024-11-05");
        assert_eq!(serialized["capabilities"], json!({}));
        assert_eq!(serialized["clientInfo"]["name"], "rustant-test");
        assert_eq!(serialized["clientInfo"]["version"], "0.1.0");

        // Round-trip.
        let deser: InitializeParams = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(deser.client_info.name, "rustant-test");
        assert_eq!(deser.client_info.version.as_deref(), Some("0.1.0"));

        // Client without version.
        let params_no_ver = InitializeParams {
            protocol_version: MCP_PROTOCOL_VERSION.into(),
            capabilities: ClientCapabilities {},
            client_info: ClientInfo {
                name: "minimal".into(),
                version: None,
            },
        };
        let s = serde_json::to_value(&params_no_ver).unwrap();
        assert!(s["clientInfo"].get("version").is_none());
        let d: InitializeParams = serde_json::from_value(s).unwrap();
        assert!(d.client_info.version.is_none());
    }

    #[test]
    fn test_initialize_result_serde() {
        let result = InitializeResult {
            protocol_version: MCP_PROTOCOL_VERSION.into(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(true),
                }),
                resources: Some(ResourcesCapability {
                    subscribe: Some(false),
                    list_changed: Some(true),
                }),
            },
            server_info: ServerInfo {
                name: "rustant-mcp".into(),
                version: "0.1.0".into(),
            },
        };
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["protocolVersion"], "2024-11-05");
        assert_eq!(serialized["serverInfo"]["name"], "rustant-mcp");
        assert_eq!(serialized["serverInfo"]["version"], "0.1.0");
        assert_eq!(serialized["capabilities"]["tools"]["listChanged"], true);
        assert_eq!(serialized["capabilities"]["resources"]["subscribe"], false);
        assert_eq!(serialized["capabilities"]["resources"]["listChanged"], true);

        // Round-trip.
        let deser: InitializeResult = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.protocol_version, MCP_PROTOCOL_VERSION);
        assert_eq!(deser.server_info.name, "rustant-mcp");
        assert_eq!(deser.server_info.version, "0.1.0");
        let tools_cap = deser.capabilities.tools.unwrap();
        assert_eq!(tools_cap.list_changed, Some(true));
        let res_cap = deser.capabilities.resources.unwrap();
        assert_eq!(res_cap.subscribe, Some(false));
        assert_eq!(res_cap.list_changed, Some(true));
    }

    // -- MCP tool types ----------------------------------------------------

    #[test]
    fn test_mcp_tool_serde() {
        let tool = McpTool {
            name: "read_file".into(),
            description: Some("Read a file from disk".into()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path" }
                },
                "required": ["path"]
            }),
        };
        let serialized = serde_json::to_value(&tool).unwrap();
        assert_eq!(serialized["name"], "read_file");
        assert_eq!(serialized["description"], "Read a file from disk");
        assert_eq!(
            serialized["inputSchema"]["properties"]["path"]["type"],
            "string"
        );
        assert_eq!(serialized["inputSchema"]["required"][0], "path");

        // Round-trip.
        let deser: McpTool = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.name, "read_file");
        assert_eq!(deser.description.as_deref(), Some("Read a file from disk"));
        assert_eq!(deser.input_schema["type"], "object");

        // Tool without description.
        let tool_no_desc = McpTool {
            name: "ping".into(),
            description: None,
            input_schema: json!({"type": "object"}),
        };
        let s = serde_json::to_value(&tool_no_desc).unwrap();
        assert!(s.get("description").is_none());
        let d: McpTool = serde_json::from_value(s).unwrap();
        assert!(d.description.is_none());
    }

    #[test]
    fn test_call_tool_params_serde() {
        let params = CallToolParams {
            name: "bash".into(),
            arguments: Some(json!({ "command": "ls -la" })),
        };
        let serialized = serde_json::to_value(&params).unwrap();
        assert_eq!(serialized["name"], "bash");
        assert_eq!(serialized["arguments"]["command"], "ls -la");

        // Round-trip.
        let deser: CallToolParams = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.name, "bash");
        let args = deser.arguments.unwrap();
        assert_eq!(args["command"], "ls -la");

        // Without arguments.
        let params_no_args = CallToolParams {
            name: "list_files".into(),
            arguments: None,
        };
        let s = serde_json::to_value(&params_no_args).unwrap();
        assert!(s.get("arguments").is_none());
        let d: CallToolParams = serde_json::from_value(s).unwrap();
        assert_eq!(d.name, "list_files");
        assert!(d.arguments.is_none());
    }

    #[test]
    fn test_call_tool_result_with_text() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Hello, world!".into(),
            }],
            is_error: None,
        };
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["content"][0]["type"], "text");
        assert_eq!(serialized["content"][0]["text"], "Hello, world!");
        // isError should be absent.
        assert!(serialized.get("isError").is_none());

        // Round-trip.
        let deser: CallToolResult = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.content.len(), 1);
        match &deser.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "Hello, world!"),
            _ => panic!("expected Text variant"),
        }
        assert!(deser.is_error.is_none());

        // Image content round-trip.
        let img_result = CallToolResult {
            content: vec![ToolContent::Image {
                data: "iVBORw0KGgo=".into(),
                mime_type: "image/png".into(),
            }],
            is_error: None,
        };
        let s = serde_json::to_value(&img_result).unwrap();
        assert_eq!(s["content"][0]["type"], "image");
        assert_eq!(s["content"][0]["data"], "iVBORw0KGgo=");
        assert_eq!(s["content"][0]["mimeType"], "image/png");
        let d: CallToolResult = serde_json::from_value(s).unwrap();
        match &d.content[0] {
            ToolContent::Image { data, mime_type } => {
                assert_eq!(data, "iVBORw0KGgo=");
                assert_eq!(mime_type, "image/png");
            }
            _ => panic!("expected Image variant"),
        }
    }

    #[test]
    fn test_call_tool_result_with_error() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "Something went wrong".into(),
            }],
            is_error: Some(true),
        };
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["isError"], true);
        assert_eq!(serialized["content"][0]["type"], "text");
        assert_eq!(serialized["content"][0]["text"], "Something went wrong");

        // Round-trip.
        let deser: CallToolResult = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.is_error, Some(true));
        assert_eq!(deser.content.len(), 1);
        match &deser.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "Something went wrong"),
            _ => panic!("expected Text variant"),
        }
    }

    // -- MCP resource types ------------------------------------------------

    #[test]
    fn test_resource_serde() {
        let resource = McpResource {
            uri: "file:///home/user/project/src/main.rs".into(),
            name: "main.rs".into(),
            description: Some("Application entry point".into()),
            mime_type: Some("text/x-rust".into()),
        };
        let serialized = serde_json::to_value(&resource).unwrap();
        assert_eq!(serialized["uri"], "file:///home/user/project/src/main.rs");
        assert_eq!(serialized["name"], "main.rs");
        assert_eq!(serialized["description"], "Application entry point");
        assert_eq!(serialized["mimeType"], "text/x-rust");

        // Round-trip.
        let deser: McpResource = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.uri, "file:///home/user/project/src/main.rs");
        assert_eq!(deser.name, "main.rs");
        assert_eq!(
            deser.description.as_deref(),
            Some("Application entry point")
        );
        assert_eq!(deser.mime_type.as_deref(), Some("text/x-rust"));

        // Without optional fields.
        let minimal = McpResource {
            uri: "file:///tmp/data.txt".into(),
            name: "data.txt".into(),
            description: None,
            mime_type: None,
        };
        let s = serde_json::to_value(&minimal).unwrap();
        assert!(s.get("description").is_none());
        assert!(s.get("mimeType").is_none());
        let d: McpResource = serde_json::from_value(s).unwrap();
        assert_eq!(d.name, "data.txt");
        assert!(d.description.is_none());
        assert!(d.mime_type.is_none());
    }

    #[test]
    fn test_read_resource_result_serde() {
        let result = ReadResourceResult {
            contents: vec![ResourceContent {
                uri: "file:///tmp/hello.txt".into(),
                mime_type: Some("text/plain".into()),
                text: Some("Hello from resource!".into()),
            }],
        };
        let serialized = serde_json::to_value(&result).unwrap();
        assert_eq!(serialized["contents"][0]["uri"], "file:///tmp/hello.txt");
        assert_eq!(serialized["contents"][0]["mimeType"], "text/plain");
        assert_eq!(serialized["contents"][0]["text"], "Hello from resource!");

        // Round-trip.
        let deser: ReadResourceResult = serde_json::from_value(serialized).unwrap();
        assert_eq!(deser.contents.len(), 1);
        let c = &deser.contents[0];
        assert_eq!(c.uri, "file:///tmp/hello.txt");
        assert_eq!(c.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(c.text.as_deref(), Some("Hello from resource!"));

        // Resource content without optional fields.
        let result_minimal = ReadResourceResult {
            contents: vec![ResourceContent {
                uri: "file:///tmp/binary.dat".into(),
                mime_type: None,
                text: None,
            }],
        };
        let s = serde_json::to_value(&result_minimal).unwrap();
        assert!(s["contents"][0].get("mimeType").is_none());
        assert!(s["contents"][0].get("text").is_none());
        let d: ReadResourceResult = serde_json::from_value(s).unwrap();
        assert!(d.contents[0].mime_type.is_none());
        assert!(d.contents[0].text.is_none());
    }

    // -- Constant ----------------------------------------------------------

    #[test]
    fn test_mcp_protocol_version_constant() {
        assert_eq!(MCP_PROTOCOL_VERSION, "2024-11-05");
    }
}
