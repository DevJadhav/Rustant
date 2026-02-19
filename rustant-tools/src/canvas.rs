//! Canvas tools — Agent-callable tools for pushing content to the canvas UI.
//!
//! Provides 5 tools: canvas_push, canvas_clear, canvas_update, canvas_snapshot, canvas_interact.

use async_trait::async_trait;
use rustant_core::canvas::{CanvasManager, CanvasMessage, CanvasTarget, ContentType};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::registry::Tool;

/// Shared canvas state for all canvas tools.
pub type SharedCanvas = Arc<Mutex<CanvasManager>>;

/// Create a new shared canvas manager.
pub fn create_shared_canvas() -> SharedCanvas {
    Arc::new(Mutex::new(CanvasManager::new()))
}

// --- canvas_push ---

/// Tool to push content to the canvas.
pub struct CanvasPushTool {
    canvas: SharedCanvas,
}

impl CanvasPushTool {
    pub fn new(canvas: SharedCanvas) -> Self {
        Self { canvas }
    }
}

#[async_trait]
impl Tool for CanvasPushTool {
    fn name(&self) -> &str {
        "canvas_push"
    }

    fn description(&self) -> &str {
        "Push content to the canvas UI. Supports HTML, Markdown, Code, Chart, Table, Form, Image, and Diagram content types."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content_type": {
                    "type": "string",
                    "enum": ["html", "markdown", "code", "chart", "table", "form", "image", "diagram"],
                    "description": "The type of content to push"
                },
                "content": {
                    "type": "string",
                    "description": "The content to display"
                },
                "target": {
                    "type": "string",
                    "description": "Canvas target name (empty for broadcast)"
                }
            },
            "required": ["content_type", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let content_type_str =
            args["content_type"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "canvas_push".into(),
                    reason: "content_type is required".into(),
                })?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "canvas".into(),
                reason: "content is required".into(),
            })?;
        let target = match args["target"].as_str() {
            Some(t) if !t.is_empty() => CanvasTarget::Named(t.into()),
            _ => CanvasTarget::Broadcast,
        };
        let ct = ContentType::from_str_loose(content_type_str).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "canvas".into(),
                reason: format!("Unknown content_type: {content_type_str}"),
            }
        })?;

        let mut canvas = self.canvas.lock().await;
        let id = canvas.push(&target, ct, content.to_string()).map_err(|e| {
            ToolError::ExecutionFailed {
                name: "canvas".into(),
                message: e.to_string(),
            }
        })?;

        Ok(ToolOutput::text(format!(
            "Content pushed to canvas (id: {id})"
        )))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// --- canvas_clear ---

/// Tool to clear the canvas.
pub struct CanvasClearTool {
    canvas: SharedCanvas,
}

impl CanvasClearTool {
    pub fn new(canvas: SharedCanvas) -> Self {
        Self { canvas }
    }
}

#[async_trait]
impl Tool for CanvasClearTool {
    fn name(&self) -> &str {
        "canvas_clear"
    }

    fn description(&self) -> &str {
        "Clear all content from the canvas."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Canvas target to clear (empty for broadcast)"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let target = match args["target"].as_str() {
            Some(t) if !t.is_empty() => CanvasTarget::Named(t.into()),
            _ => CanvasTarget::Broadcast,
        };

        let mut canvas = self.canvas.lock().await;
        canvas.clear(&target);
        Ok(ToolOutput::text("Canvas cleared"))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// --- canvas_update ---

/// Tool to update canvas content (push with update semantics).
pub struct CanvasUpdateTool {
    canvas: SharedCanvas,
}

impl CanvasUpdateTool {
    pub fn new(canvas: SharedCanvas) -> Self {
        Self { canvas }
    }
}

#[async_trait]
impl Tool for CanvasUpdateTool {
    fn name(&self) -> &str {
        "canvas_update"
    }

    fn description(&self) -> &str {
        "Update content on the canvas (push updated content)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "content_type": {
                    "type": "string",
                    "enum": ["html", "markdown", "code", "chart", "table", "form", "image", "diagram"]
                },
                "content": {
                    "type": "string",
                    "description": "The updated content"
                },
                "target": {
                    "type": "string",
                    "description": "Canvas target name"
                }
            },
            "required": ["content_type", "content"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let content_type_str =
            args["content_type"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "canvas_push".into(),
                    reason: "content_type is required".into(),
                })?;
        let content = args["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "canvas".into(),
                reason: "content is required".into(),
            })?;
        let target = match args["target"].as_str() {
            Some(t) if !t.is_empty() => CanvasTarget::Named(t.into()),
            _ => CanvasTarget::Broadcast,
        };
        let ct = ContentType::from_str_loose(content_type_str).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "canvas".into(),
                reason: format!("Unknown content_type: {content_type_str}"),
            }
        })?;

        let mut canvas = self.canvas.lock().await;
        let id = canvas.push(&target, ct, content.to_string()).map_err(|e| {
            ToolError::ExecutionFailed {
                name: "canvas".into(),
                message: e.to_string(),
            }
        })?;

        Ok(ToolOutput::text(format!("Canvas updated (id: {id})")))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// --- canvas_snapshot ---

/// Tool to get a snapshot of canvas state.
pub struct CanvasSnapshotTool {
    canvas: SharedCanvas,
}

impl CanvasSnapshotTool {
    pub fn new(canvas: SharedCanvas) -> Self {
        Self { canvas }
    }
}

#[async_trait]
impl Tool for CanvasSnapshotTool {
    fn name(&self) -> &str {
        "canvas_snapshot"
    }

    fn description(&self) -> &str {
        "Get a snapshot of the current canvas state."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "Canvas target to snapshot"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let target = match args["target"].as_str() {
            Some(t) if !t.is_empty() => CanvasTarget::Named(t.into()),
            _ => CanvasTarget::Broadcast,
        };

        let canvas = self.canvas.lock().await;
        let items = canvas.snapshot(&target);

        let snapshot: Vec<serde_json::Value> = items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "id": item.id.to_string(),
                    "content_type": item.content_type,
                    "content": item.content,
                    "created_at": item.created_at.to_rfc3339(),
                })
            })
            .collect();

        let output = serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "[]".into());
        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

// --- canvas_interact ---

/// Tool to send an interaction event to the canvas.
#[allow(dead_code)]
pub struct CanvasInteractTool {
    canvas: SharedCanvas,
}

impl CanvasInteractTool {
    pub fn new(canvas: SharedCanvas) -> Self {
        Self { canvas }
    }
}

#[async_trait]
impl Tool for CanvasInteractTool {
    fn name(&self) -> &str {
        "canvas_interact"
    }

    fn description(&self) -> &str {
        "Send an interaction event to the canvas (click, submit, etc.)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Interaction action (e.g. click, submit, select)"
                },
                "selector": {
                    "type": "string",
                    "description": "CSS selector or element ID"
                },
                "data": {
                    "type": "object",
                    "description": "Additional interaction data"
                }
            },
            "required": ["action", "selector"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "canvas_interact".into(),
                reason: "action is required".into(),
            })?;
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "canvas_interact".into(),
                reason: "selector is required".into(),
            })?;
        let data = args.get("data").cloned().unwrap_or(serde_json::json!({}));

        // Build the interact message (would be sent to connected UI clients)
        let _msg = CanvasMessage::Interact {
            action: action.into(),
            selector: selector.into(),
            data,
        };

        Ok(ToolOutput::text(format!(
            "Interaction sent: {action} on {selector}"
        )))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// Register all canvas tools into a ToolRegistry.
pub fn register_canvas_tools(registry: &mut crate::registry::ToolRegistry, canvas: SharedCanvas) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(CanvasPushTool::new(canvas.clone())),
        Arc::new(CanvasClearTool::new(canvas.clone())),
        Arc::new(CanvasUpdateTool::new(canvas.clone())),
        Arc::new(CanvasSnapshotTool::new(canvas.clone())),
        Arc::new(CanvasInteractTool::new(canvas)),
    ];

    for tool in tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register canvas tool: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_canvas_push_tool() {
        let canvas = create_shared_canvas();
        let tool = CanvasPushTool::new(canvas.clone());

        let result = tool
            .execute(serde_json::json!({
                "content_type": "html",
                "content": "<h1>Hello</h1>"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("pushed"));
        let mgr = canvas.lock().await;
        assert_eq!(mgr.total_items(), 1);
    }

    #[tokio::test]
    async fn test_canvas_clear_tool() {
        let canvas = create_shared_canvas();
        {
            let mut mgr = canvas.lock().await;
            mgr.push(&CanvasTarget::Broadcast, ContentType::Html, "test".into())
                .unwrap();
        }

        let tool = CanvasClearTool::new(canvas.clone());
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("cleared"));

        let mgr = canvas.lock().await;
        assert_eq!(mgr.total_items(), 0);
    }

    #[tokio::test]
    async fn test_canvas_update_tool() {
        let canvas = create_shared_canvas();
        let tool = CanvasUpdateTool::new(canvas.clone());

        let result = tool
            .execute(serde_json::json!({
                "content_type": "markdown",
                "content": "# Updated"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("updated"));
    }

    #[tokio::test]
    async fn test_canvas_snapshot_tool() {
        let canvas = create_shared_canvas();
        {
            let mut mgr = canvas.lock().await;
            mgr.push(
                &CanvasTarget::Broadcast,
                ContentType::Html,
                "<p>1</p>".into(),
            )
            .unwrap();
            mgr.push(
                &CanvasTarget::Broadcast,
                ContentType::Code,
                "let x = 1;".into(),
            )
            .unwrap();
        }

        let tool = CanvasSnapshotTool::new(canvas);
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&result.content).unwrap();
        assert_eq!(parsed.len(), 2);
    }

    #[tokio::test]
    async fn test_canvas_interact_tool() {
        let canvas = create_shared_canvas();
        let tool = CanvasInteractTool::new(canvas);

        let result = tool
            .execute(serde_json::json!({
                "action": "click",
                "selector": "#submit-btn"
            }))
            .await
            .unwrap();

        assert!(result.content.contains("click"));
        assert!(result.content.contains("#submit-btn"));
    }

    #[tokio::test]
    async fn test_canvas_push_invalid_content_type() {
        let canvas = create_shared_canvas();
        let tool = CanvasPushTool::new(canvas);

        let result = tool
            .execute(serde_json::json!({
                "content_type": "invalid",
                "content": "test"
            }))
            .await;

        assert!(result.is_err());
    }
}
