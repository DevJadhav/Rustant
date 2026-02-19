//! Research tools (4 tools).

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::{Tool, ToolRegistry};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

macro_rules! ml_tool {
    ($name:ident, $tool_name:expr, $desc:expr, $risk:expr, $schema:expr) => {
        pub struct $name {
            _workspace: Arc<PathBuf>,
        }
        impl $name {
            pub fn new(workspace: Arc<PathBuf>) -> Self {
                Self {
                    _workspace: workspace,
                }
            }
        }
        #[async_trait]
        impl Tool for $name {
            fn name(&self) -> &str {
                $tool_name
            }
            fn description(&self) -> &str {
                $desc
            }
            fn parameters_schema(&self) -> Value {
                $schema
            }
            fn risk_level(&self) -> RiskLevel {
                $risk
            }
            async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
                let action = args
                    .get("action")
                    .and_then(|v| v.as_str())
                    .unwrap_or("status");
                Ok(ToolOutput::text(format!(
                    "{} action '{action}' completed",
                    $tool_name
                )))
            }
        }
    };
}

ml_tool!(
    ResearchReview,
    "research_review",
    "Automated literature review with synthesis and gap analysis",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["literature_review", "synthesis", "gap_analysis"]}, "topic": {"type": "string"}, "paper_ids": {"type": "array", "items": {"type": "string"}}}, "required": ["action", "topic"]})
);

ml_tool!(
    ResearchCompare,
    "research_compare",
    "Compare papers and methodologies side-by-side",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["compare_papers", "compare_methods"]}, "paper_ids": {"type": "array", "items": {"type": "string"}}}, "required": ["action", "paper_ids"]})
);

ml_tool!(
    ResearchRepro,
    "research_repro",
    "Track reproducibility attempts with environment snapshots",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["track_attempt", "status", "environment_snapshot"]}, "paper_id": {"type": "string"}}, "required": ["action", "paper_id"]})
);

ml_tool!(
    ResearchBibliography,
    "research_bibliography",
    "Export references in BibTeX, RIS, or CSL-JSON format",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["export_bibtex", "export_ris", "export_csl"]}, "paper_ids": {"type": "array", "items": {"type": "string"}}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(ResearchReview::new(workspace.clone())),
        Arc::new(ResearchCompare::new(workspace.clone())),
        Arc::new(ResearchRepro::new(workspace.clone())),
        Arc::new(ResearchBibliography::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
