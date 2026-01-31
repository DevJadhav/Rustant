//! MCP-specific error types.

/// Errors that can occur during MCP server operation.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("JSON-RPC parse error: {message}")]
    ParseError { message: String },

    #[error("Invalid request: {message}")]
    InvalidRequest { message: String },

    #[error("Method not found: {method}")]
    MethodNotFound { method: String },

    #[error("Invalid parameters: {message}")]
    InvalidParams { message: String },

    #[error("Internal error: {message}")]
    InternalError { message: String },

    #[error("Tool execution failed: {message}")]
    ToolError { message: String },

    #[error("Resource not found: {uri}")]
    ResourceNotFound { uri: String },

    #[error("Transport error: {message}")]
    TransportError { message: String },

    #[error("Server not initialized")]
    NotInitialized,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl McpError {
    /// Convert to a JSON-RPC error code.
    pub fn error_code(&self) -> i64 {
        match self {
            McpError::ParseError { .. } => -32700,
            McpError::InvalidRequest { .. } => -32600,
            McpError::MethodNotFound { .. } => -32601,
            McpError::InvalidParams { .. } => -32602,
            McpError::InternalError { .. } => -32603,
            McpError::ToolError { .. } => -32000,
            McpError::ResourceNotFound { .. } => -32001,
            McpError::TransportError { .. } => -32002,
            McpError::NotInitialized => -32003,
            McpError::Io(_) => -32603,
            McpError::Json(_) => -32700,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(
            McpError::ParseError {
                message: "bad json".into()
            }
            .error_code(),
            -32700
        );
        assert_eq!(
            McpError::InvalidRequest {
                message: "missing id".into()
            }
            .error_code(),
            -32600
        );
        assert_eq!(
            McpError::MethodNotFound {
                method: "unknown".into()
            }
            .error_code(),
            -32601
        );
        assert_eq!(
            McpError::InvalidParams {
                message: "bad params".into()
            }
            .error_code(),
            -32602
        );
        assert_eq!(
            McpError::InternalError {
                message: "crash".into()
            }
            .error_code(),
            -32603
        );
        assert_eq!(
            McpError::ToolError {
                message: "fail".into()
            }
            .error_code(),
            -32000
        );
        assert_eq!(
            McpError::ResourceNotFound {
                uri: "file://x".into()
            }
            .error_code(),
            -32001
        );
        assert_eq!(McpError::NotInitialized.error_code(), -32003);
    }

    #[test]
    fn test_error_display() {
        let err = McpError::MethodNotFound {
            method: "tools/execute".into(),
        };
        assert_eq!(err.to_string(), "Method not found: tools/execute");
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe broken");
        let mcp_err: McpError = io_err.into();
        assert!(matches!(mcp_err, McpError::Io(_)));
        assert_eq!(mcp_err.error_code(), -32603);
    }

    #[test]
    fn test_json_error_conversion() {
        let json_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let mcp_err: McpError = json_err.into();
        assert!(matches!(mcp_err, McpError::Json(_)));
        assert_eq!(mcp_err.error_code(), -32700);
    }
}
