//! Tool Registry — manages dynamic tool registration, validation, and execution.
//!
//! Tools are registered at startup and can be added/removed at runtime.
//! The registry provides tool definitions for the LLM and executes tool calls
//! with proper validation and timeout handling.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolDefinition, ToolOutput};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Trait that all tools must implement.
#[async_trait]
pub trait Tool: Send + Sync {
    /// The unique name of this tool.
    fn name(&self) -> &str;

    /// Human-readable description of what this tool does.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError>;

    /// The risk level of this tool.
    fn risk_level(&self) -> RiskLevel;

    /// Maximum execution time before timeout.
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

/// The tool registry holds all registered tools and handles execution.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Returns error if a tool with the same name is already registered.
    pub fn register(&mut self, tool: Arc<dyn Tool>) -> Result<(), ToolError> {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            return Err(ToolError::AlreadyRegistered { name });
        }
        debug!(tool = %name, "Registering tool");
        self.tools.insert(name, tool);
        Ok(())
    }

    /// Unregister a tool by name.
    pub fn unregister(&mut self, name: &str) -> Result<(), ToolError> {
        if self.tools.remove(name).is_none() {
            return Err(ToolError::NotFound {
                name: name.to_string(),
            });
        }
        debug!(tool = %name, "Unregistered tool");
        Ok(())
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// List all registered tool definitions (for sending to LLM).
    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools
            .values()
            .map(|tool| ToolDefinition {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            })
            .collect()
    }

    /// List all registered tool names.
    pub fn list_names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Execute a tool by name with the given arguments, applying timeout.
    pub async fn execute(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let tool = self.tools.get(name).ok_or_else(|| ToolError::NotFound {
            name: name.to_string(),
        })?;

        let timeout = tool.timeout();
        info!(tool = %name, timeout_secs = timeout.as_secs(), "Executing tool");

        match tokio::time::timeout(timeout, tool.execute(args)).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::Timeout {
                name: name.to_string(),
                timeout_secs: timeout.as_secs(),
            }),
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple echo tool for testing.
    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echoes the input text back"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text to echo" }
                },
                "required": ["text"]
            })
        }

        async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
            let text = args["text"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "echo".to_string(),
                    reason: "missing 'text' parameter".to_string(),
                })?;
            Ok(ToolOutput::text(format!("Echo: {}", text)))
        }

        fn risk_level(&self) -> RiskLevel {
            RiskLevel::ReadOnly
        }
    }

    /// A slow tool for timeout testing.
    struct SlowTool;

    #[async_trait]
    impl Tool for SlowTool {
        fn name(&self) -> &str {
            "slow"
        }

        fn description(&self) -> &str {
            "A tool that takes forever"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(ToolOutput::text("done"))
        }

        fn risk_level(&self) -> RiskLevel {
            RiskLevel::ReadOnly
        }

        fn timeout(&self) -> Duration {
            Duration::from_millis(100) // Very short timeout for testing
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_tool() {
        let mut registry = ToolRegistry::new();
        let tool: Arc<dyn Tool> = Arc::new(EchoTool);
        registry.register(tool).unwrap();

        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());
        assert!(registry.get("echo").is_some());
    }

    #[test]
    fn test_register_duplicate() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

        let result = registry.register(Arc::new(EchoTool));
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::AlreadyRegistered { name } => assert_eq!(name, "echo"),
            _ => panic!("Expected AlreadyRegistered error"),
        }
    }

    #[test]
    fn test_unregister_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();
        assert_eq!(registry.len(), 1);

        registry.unregister("echo").unwrap();
        assert_eq!(registry.len(), 0);
        assert!(registry.get("echo").is_none());
    }

    #[test]
    fn test_unregister_nonexistent() {
        let mut registry = ToolRegistry::new();
        let result = registry.unregister("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_definitions() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

        let defs = registry.list_definitions();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "echo");
        assert_eq!(defs[0].description, "Echoes the input text back");
    }

    #[test]
    fn test_list_names() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

        let names = registry.list_names();
        assert_eq!(names, vec!["echo"]);
    }

    #[tokio::test]
    async fn test_execute_tool() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

        let result = registry
            .execute("echo", serde_json::json!({"text": "hello"}))
            .await
            .unwrap();
        assert_eq!(result.content, "Echo: hello");
    }

    #[tokio::test]
    async fn test_execute_nonexistent_tool() {
        let registry = ToolRegistry::new();
        let result = registry.execute("missing", serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::NotFound { name } => assert_eq!(name, "missing"),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_execute_invalid_args() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(EchoTool)).unwrap();

        // Missing required 'text' parameter
        let result = registry.execute("echo", serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_timeout() {
        let mut registry = ToolRegistry::new();
        registry.register(Arc::new(SlowTool)).unwrap();

        let result = registry.execute("slow", serde_json::json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::Timeout { name, .. } => assert_eq!(name, "slow"),
            e => panic!("Expected Timeout error, got: {:?}", e),
        }
    }

    #[test]
    fn test_get_nonexistent() {
        let registry = ToolRegistry::new();
        assert!(registry.get("missing").is_none());
    }
}
