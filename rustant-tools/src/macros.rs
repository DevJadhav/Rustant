//! Tool definition macros — reduce boilerplate when implementing the `Tool` trait.
//!
//! # Usage
//!
//! ## Unit struct (no fields)
//!
//! ```rust,ignore
//! define_tool!(
//!     MyTool,
//!     "my_tool",
//!     "Does something useful.",
//!     ReadOnly,
//!     serde_json::json!({
//!         "type": "object",
//!         "properties": {
//!             "input": { "type": "string", "description": "The input" }
//!         },
//!         "required": ["input"]
//!     }),
//!     |args| {
//!         let input = args["input"]
//!             .as_str()
//!             .ok_or_else(|| ToolError::InvalidArguments {
//!                 name: "my_tool".to_string(),
//!                 reason: "missing 'input'".to_string(),
//!             })?;
//!         Ok(ToolOutput::text(input.to_string()))
//!     }
//! );
//! ```
//!
//! ## Struct with fields
//!
//! ```rust,ignore
//! define_tool!(
//!     MyTool { workspace: std::path::PathBuf },
//!     "my_tool",
//!     "Does something useful with a workspace.",
//!     Write,
//!     serde_json::json!({
//!         "type": "object",
//!         "properties": {
//!             "path": { "type": "string", "description": "File path" }
//!         },
//!         "required": ["path"]
//!     }),
//!     |self_, args| {
//!         let path = args["path"]
//!             .as_str()
//!             .ok_or_else(|| ToolError::InvalidArguments {
//!                 name: "my_tool".to_string(),
//!                 reason: "missing 'path'".to_string(),
//!             })?;
//!         Ok(ToolOutput::text(format!("{}: {}", self_.workspace.display(), path)))
//!     }
//! );
//! ```

/// Define a tool with minimal boilerplate.
///
/// Generates:
/// - The struct definition (unit or with named fields)
/// - `#[async_trait]` impl of `crate::registry::Tool`
/// - All required methods: `name()`, `description()`, `parameters_schema()`,
///   `execute()`, `risk_level()`
///
/// # Variants
///
/// **Unit struct** (no fields):
/// ```text
/// define_tool!(Name, "name", "description", RiskLevel, schema_expr, |args| { body })
/// ```
///
/// **Struct with fields**:
/// ```text
/// define_tool!(Name { field: Type, ... }, "name", "description", RiskLevel, schema_expr, |self_, args| { body })
/// ```
///
/// The closure body must return `Result<ToolOutput, ToolError>`. Inside the body,
/// `ToolOutput`, `ToolError`, and `serde_json::Value` are all in scope from the
/// surrounding module's imports.
#[macro_export]
macro_rules! define_tool {
    // ── Case 1: Unit struct (no fields) ──────────────────────────────────
    (
        $name:ident,
        $tool_name:expr,
        $desc:expr,
        $risk:ident,
        $schema:expr,
        |$args:ident| $body:expr
    ) => {
        pub struct $name;

        #[async_trait::async_trait]
        impl $crate::registry::Tool for $name {
            fn name(&self) -> &str {
                $tool_name
            }

            fn description(&self) -> &str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                $schema
            }

            fn risk_level(&self) -> rustant_core::types::RiskLevel {
                rustant_core::types::RiskLevel::$risk
            }

            async fn execute(
                &self,
                $args: serde_json::Value,
            ) -> Result<rustant_core::types::ToolOutput, rustant_core::error::ToolError> {
                $body
            }
        }
    };

    // ── Case 2: Struct with fields ───────────────────────────────────────
    (
        $name:ident { $($field:ident : $ftype:ty),* $(,)? },
        $tool_name:expr,
        $desc:expr,
        $risk:ident,
        $schema:expr,
        |$self_:ident, $args:ident| $body:expr
    ) => {
        pub struct $name {
            $(pub $field: $ftype),*
        }

        #[async_trait::async_trait]
        impl $crate::registry::Tool for $name {
            fn name(&self) -> &str {
                $tool_name
            }

            fn description(&self) -> &str {
                $desc
            }

            fn parameters_schema(&self) -> serde_json::Value {
                $schema
            }

            fn risk_level(&self) -> rustant_core::types::RiskLevel {
                rustant_core::types::RiskLevel::$risk
            }

            async fn execute(
                &self,
                $args: serde_json::Value,
            ) -> Result<rustant_core::types::ToolOutput, rustant_core::error::ToolError> {
                let $self_ = self;
                $body
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use rustant_core::error::ToolError;
    use rustant_core::types::{RiskLevel, ToolOutput};

    // Test Case 1: Unit struct tool
    define_tool!(
        TestEchoTool,
        "test_echo",
        "A test echo tool.",
        ReadOnly,
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to echo"
                }
            },
            "required": ["text"]
        }),
        |args| {
            let text = args["text"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "test_echo".to_string(),
                    reason: "missing 'text'".to_string(),
                })?;
            Ok(ToolOutput::text(text.to_string()))
        }
    );

    // Test Case 2: Struct with fields
    define_tool!(
        TestWorkspaceTool {
            workspace: std::path::PathBuf
        },
        "test_workspace",
        "A test tool with workspace field.",
        Write,
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Action to perform"
                }
            },
            "required": ["action"]
        }),
        |self_, args| {
            let action = args["action"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "test_workspace".to_string(),
                    reason: "missing 'action'".to_string(),
                })?;
            Ok(ToolOutput::text(format!(
                "{}: {}",
                self_.workspace.display(),
                action
            )))
        }
    );

    // Test Case 3: Struct with multiple fields
    define_tool!(
        TestMultiFieldTool {
            workspace: std::path::PathBuf,
            label: String
        },
        "test_multi",
        "A test tool with multiple fields.",
        Execute,
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            }
        }),
        |self_, args| {
            let input = args
                .get("input")
                .and_then(|v| v.as_str())
                .unwrap_or("default");
            Ok(ToolOutput::text(format!(
                "[{}] {}: {}",
                self_.label,
                self_.workspace.display(),
                input
            )))
        }
    );

    use crate::registry::Tool;

    #[test]
    fn test_unit_struct_properties() {
        let tool = TestEchoTool;
        assert_eq!(tool.name(), "test_echo");
        assert_eq!(tool.description(), "A test echo tool.");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.parameters_schema().is_object());
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["text"].is_object());
    }

    #[tokio::test]
    async fn test_unit_struct_execute() {
        let tool = TestEchoTool;
        let result = tool
            .execute(serde_json::json!({"text": "hello"}))
            .await
            .unwrap();
        assert_eq!(result.content, "hello");
    }

    #[tokio::test]
    async fn test_unit_struct_missing_param() {
        let tool = TestEchoTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_struct_with_fields_properties() {
        let tool = TestWorkspaceTool {
            workspace: std::path::PathBuf::from("/tmp/test"),
        };
        assert_eq!(tool.name(), "test_workspace");
        assert_eq!(tool.description(), "A test tool with workspace field.");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert!(tool.parameters_schema().is_object());
    }

    #[tokio::test]
    async fn test_struct_with_fields_execute() {
        let tool = TestWorkspaceTool {
            workspace: std::path::PathBuf::from("/tmp/test"),
        };
        let result = tool
            .execute(serde_json::json!({"action": "build"}))
            .await
            .unwrap();
        assert_eq!(result.content, "/tmp/test: build");
    }

    #[tokio::test]
    async fn test_struct_with_fields_missing_param() {
        let tool = TestWorkspaceTool {
            workspace: std::path::PathBuf::from("/tmp/test"),
        };
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_multi_field_struct_properties() {
        let tool = TestMultiFieldTool {
            workspace: std::path::PathBuf::from("/projects"),
            label: "dev".to_string(),
        };
        assert_eq!(tool.name(), "test_multi");
        assert_eq!(tool.description(), "A test tool with multiple fields.");
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
    }

    #[tokio::test]
    async fn test_multi_field_struct_execute() {
        let tool = TestMultiFieldTool {
            workspace: std::path::PathBuf::from("/projects"),
            label: "dev".to_string(),
        };
        let result = tool
            .execute(serde_json::json!({"input": "test"}))
            .await
            .unwrap();
        assert_eq!(result.content, "[dev] /projects: test");
    }

    #[tokio::test]
    async fn test_multi_field_struct_default_input() {
        let tool = TestMultiFieldTool {
            workspace: std::path::PathBuf::from("/projects"),
            label: "prod".to_string(),
        };
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(result.content, "[prod] /projects: default");
    }

    #[test]
    fn test_all_risk_levels() {
        // Verify the macro works with each RiskLevel variant
        define_tool!(
            RlReadOnly,
            "rl_ro",
            "r",
            ReadOnly,
            serde_json::json!({"type": "object"}),
            |_args| Ok(ToolOutput::text("ok"))
        );
        define_tool!(
            RlWrite,
            "rl_w",
            "w",
            Write,
            serde_json::json!({"type": "object"}),
            |_args| Ok(ToolOutput::text("ok"))
        );
        define_tool!(
            RlExecute,
            "rl_x",
            "x",
            Execute,
            serde_json::json!({"type": "object"}),
            |_args| Ok(ToolOutput::text("ok"))
        );
        define_tool!(
            RlNetwork,
            "rl_n",
            "n",
            Network,
            serde_json::json!({"type": "object"}),
            |_args| Ok(ToolOutput::text("ok"))
        );
        define_tool!(
            RlDestructive,
            "rl_d",
            "d",
            Destructive,
            serde_json::json!({"type": "object"}),
            |_args| Ok(ToolOutput::text("ok"))
        );

        assert_eq!(RlReadOnly.risk_level(), RiskLevel::ReadOnly);
        assert_eq!(RlWrite.risk_level(), RiskLevel::Write);
        assert_eq!(RlExecute.risk_level(), RiskLevel::Execute);
        assert_eq!(RlNetwork.risk_level(), RiskLevel::Network);
        assert_eq!(RlDestructive.risk_level(), RiskLevel::Destructive);
    }
}
