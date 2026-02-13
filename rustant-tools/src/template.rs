//! Template engine tool — render Handlebars templates.

use async_trait::async_trait;
use handlebars::Handlebars;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{json, Value};
use std::path::PathBuf;

use crate::registry::Tool;

pub struct TemplateTool {
    workspace: PathBuf,
}

impl TemplateTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for TemplateTool {
    fn name(&self) -> &str {
        "template"
    }
    fn description(&self) -> &str {
        "Render Handlebars templates with variables. Actions: render, list_templates."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["render", "list_templates"],
                    "description": "Action to perform"
                },
                "template": { "type": "string", "description": "Template string or file path" },
                "variables": { "type": "object", "description": "Template variables as key-value pairs" },
                "output_path": { "type": "string", "description": "Optional file path to write output" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "render" => {
                let template_str = args.get("template").and_then(|v| v.as_str()).unwrap_or("");
                if template_str.is_empty() {
                    return Ok(ToolOutput::text(
                        "Please provide a template string or file path.",
                    ));
                }

                // Check if it's a file path
                let template_content =
                    if template_str.ends_with(".hbs") || template_str.ends_with(".handlebars") {
                        let path = self.workspace.join(template_str);
                        std::fs::read_to_string(&path).map_err(|e| ToolError::ExecutionFailed {
                            name: "template".into(),
                            message: format!("Failed to read template file: {}", e),
                        })?
                    } else {
                        template_str.to_string()
                    };

                let variables = args.get("variables").cloned().unwrap_or(json!({}));

                let mut handlebars = Handlebars::new();
                handlebars.set_strict_mode(false);
                let rendered = handlebars
                    .render_template(&template_content, &variables)
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "template".into(),
                        message: format!("Template render error: {}", e),
                    })?;

                // Optionally write to file
                if let Some(output_path) = args.get("output_path").and_then(|v| v.as_str()) {
                    let path = self.workspace.join(output_path);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).ok();
                    }
                    std::fs::write(&path, &rendered).map_err(|e| ToolError::ExecutionFailed {
                        name: "template".into(),
                        message: format!("Failed to write output: {}", e),
                    })?;
                    return Ok(ToolOutput::text(format!(
                        "Rendered template written to {}.",
                        output_path
                    )));
                }

                Ok(ToolOutput::text(rendered))
            }
            "list_templates" => {
                let templates_dir = self.workspace.join(".rustant").join("templates");
                if !templates_dir.exists() {
                    return Ok(ToolOutput::text(
                        "No templates directory found. Create .rustant/templates/ with .hbs files.",
                    ));
                }
                let mut templates = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&templates_dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.ends_with(".hbs") || name.ends_with(".handlebars") {
                            templates.push(name);
                        }
                    }
                }
                if templates.is_empty() {
                    Ok(ToolOutput::text(
                        "No template files found in .rustant/templates/.",
                    ))
                } else {
                    Ok(ToolOutput::text(format!(
                        "Templates ({}):\n{}",
                        templates.len(),
                        templates
                            .iter()
                            .map(|t| format!("  {}", t))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )))
                }
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: render, list_templates",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_template_render() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = TemplateTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "render",
                "template": "Hello, {{name}}! You have {{count}} messages.",
                "variables": {"name": "Alice", "count": 5}
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Hello, Alice!"));
        assert!(result.content.contains("5 messages"));
    }

    #[tokio::test]
    async fn test_template_render_missing_vars() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = TemplateTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "render",
                "template": "Hello, {{name}}!",
                "variables": {}
            }))
            .await
            .unwrap();
        // With strict_mode=false, missing vars render as empty
        assert!(result.content.contains("Hello, !"));
    }

    #[tokio::test]
    async fn test_template_list_empty() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = TemplateTool::new(workspace);

        let result = tool
            .execute(json!({"action": "list_templates"}))
            .await
            .unwrap();
        assert!(result.content.contains("No templates"));
    }

    #[tokio::test]
    async fn test_template_schema() {
        let dir = TempDir::new().unwrap();
        let tool = TemplateTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "template");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }
}
