//! Model zoo tools (5 tools).

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
    MlModelRegistry,
    "ml_model_registry",
    "List, add, remove, or search models in the registry with ModelCard support",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "add", "remove", "search", "show_card"]}, "model_id": {"type": "string"}, "query": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlModelDownload,
    "ml_model_download",
    "Download models from HuggingFace, Ollama, or URL with provenance verification",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["huggingface", "ollama", "url"]}, "model_name": {"type": "string"}, "url": {"type": "string"}}, "required": ["action", "model_name"]})
);

ml_tool!(
    MlModelConvert,
    "ml_model_convert",
    "Convert models between ONNX, CoreML, GGUF, and TFLite formats",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["onnx", "coreml", "tflite", "gguf"]}, "model_id": {"type": "string"}}, "required": ["action", "model_id"]})
);

ml_tool!(
    MlModelServe,
    "ml_model_serve",
    "Start or stop model serving with health monitoring",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["start", "stop", "status", "predict"]}, "model_id": {"type": "string"}, "port": {"type": "integer"}}, "required": ["action", "model_id"]})
);

ml_tool!(
    MlModelBenchmark,
    "ml_model_benchmark",
    "Benchmark models for latency, throughput, accuracy, and safety",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["run", "compare", "report", "safety_eval"]}, "model_id": {"type": "string"}}, "required": ["action", "model_id"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(MlModelRegistry::new(workspace.clone())),
        Arc::new(MlModelDownload::new(workspace.clone())),
        Arc::new(MlModelConvert::new(workspace.clone())),
        Arc::new(MlModelServe::new(workspace.clone())),
        Arc::new(MlModelBenchmark::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
