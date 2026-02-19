# Adding Tools

This guide walks through creating a new tool for Rustant.

## Step 1: Implement the Tool Trait

Create a new file in `rustant-tools/src/`:

```rust
use crate::registry::{Tool, ToolOutput, ToolError, RiskLevel};
use async_trait::async_trait;
use serde_json::Value;
use std::time::Duration;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str {
        "my_tool"
    }

    fn description(&self) -> &str {
        "Description of what this tool does"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "create"],
                    "description": "Action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Name parameter"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"].as_str().unwrap_or("list");

        match action {
            "list" => Ok(ToolOutput::text("Items listed")),
            "create" => {
                let name = args["name"].as_str().unwrap_or("unnamed");
                Ok(ToolOutput::text(format!("Created: {}", name)))
            }
            _ => Err(ToolError::InvalidArguments {
                name: self.name().to_string(),
                reason: format!("Unknown action: {}", action),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly  // or Write, Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }
}
```

## Step 2: Register the Tool

In `rustant-tools/src/lib.rs`, add to `register_builtin_tools()`:

```rust
registry.register(Box::new(my_tool::MyTool));
```

## Step 3: Update Tool Count Assertions

Update the count in these test files:
- `rustant-tools/src/lib.rs`
- `rustant-mcp/src/lib.rs`
- `rustant-mcp/src/handlers.rs`
- `rustant-mcp/src/client.rs`

## Step 4: Add ActionDetails (if needed)

For tools with write/execute risk, add handling in `rustant-core/src/agent.rs`:

```rust
// In parse_action_details()
"my_tool" => {
    // Parse tool-specific action details
    ActionDetails::Custom { ... }
}
```

## Step 5: Add Display (optional)

In `rustant-cli/src/repl.rs`, add a display case in `extract_tool_detail()` for enriched tool execution display.

## Key Patterns

- **Unit structs**: Most tools are `pub struct MyTool;` (no fields)
- **Workspace tools**: Fullstack tools have `workspace: PathBuf` field
- **`ToolOutput::text()`**: This is a constructor, not a getter. Access content via `.content`
- **macOS only**: Use `#[cfg(target_os = "macos")]` for platform-specific tools
- **AppleScript**: Always use `sanitize_applescript_string()` for injection prevention

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;  // Must import inside test module

    #[tokio::test]
    async fn test_my_tool_list() {
        let tool = MyTool;
        let args = serde_json::json!({"action": "list"});
        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("Items listed"));
    }
}
```

For file-based tools using `TempDir`, always use `dir.path().canonicalize().unwrap()` as the workspace path (macOS symlink issue).
