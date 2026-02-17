//! PDF generation tool — create PDF documents from text/markdown content.

use async_trait::async_trait;
use genpdf::elements::{Break, Paragraph};
use genpdf::{Document, SimplePageDecorator};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

pub struct PdfGenerateTool {
    workspace: PathBuf,
}

impl PdfGenerateTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for PdfGenerateTool {
    fn name(&self) -> &str {
        "pdf_generate"
    }
    fn description(&self) -> &str {
        "Generate PDF documents from text content. Provide title, content lines, and output path."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["generate"],
                    "description": "Action to perform"
                },
                "title": { "type": "string", "description": "Document title" },
                "content": { "type": "string", "description": "Document content (plain text, paragraphs separated by blank lines)" },
                "output": { "type": "string", "description": "Output file path (e.g., 'report.pdf')" }
            },
            "required": ["action", "output"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("generate");
        if action != "generate" {
            return Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: generate",
                action
            )));
        }

        let title = args
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Document");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let output_str = args
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("output.pdf");
        let output_path = self.workspace.join(output_str);

        // Use the built-in font — try several system locations
        let font_family =
            genpdf::fonts::from_files("", "LiberationSans", None).unwrap_or_else(|_| {
                genpdf::fonts::from_files(
                    "/usr/share/fonts/truetype/liberation",
                    "LiberationSans",
                    None,
                )
                .unwrap_or_else(|_| {
                    genpdf::fonts::from_files("/System/Library/Fonts", "Helvetica", None)
                        .unwrap_or_else(|_| {
                            genpdf::fonts::from_files("/Library/Fonts", "Arial", None)
                                .expect("No suitable font found on this system")
                        })
                })
            });

        let mut doc = Document::new(font_family);
        doc.set_title(title);

        let mut decorator = SimplePageDecorator::new();
        decorator.set_margins(30);
        doc.set_page_decorator(decorator);

        // Add title as a styled paragraph
        let title_style = genpdf::style::Style::new().bold().with_font_size(18);
        doc.push(Paragraph::new(genpdf::style::StyledString::new(
            title.to_string(),
            title_style,
        )));
        doc.push(Break::new(1));

        // Add content paragraphs
        for paragraph in content.split("\n\n") {
            let trimmed = paragraph.trim();
            if !trimmed.is_empty() {
                doc.push(Paragraph::new(trimmed));
                doc.push(Break::new(0.5));
            }
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        doc.render_to_file(&output_path)
            .map_err(|e| ToolError::ExecutionFailed {
                name: "pdf_generate".into(),
                message: format!("Failed to render PDF: {}", e),
            })?;

        let size = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        Ok(ToolOutput::text(format!(
            "Generated PDF: {} ({} bytes)",
            output_str, size
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pdf_generate_schema() {
        let dir = tempfile::TempDir::new().unwrap();
        let tool = PdfGenerateTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "pdf_generate");
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
    }

    // Note: PDF generation test requires fonts available on the system.
    // The actual render test is platform-dependent.
}
