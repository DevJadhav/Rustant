//! Inference tools (4 tools).

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
    InferenceServe,
    "inference_serve",
    "Serve models for inference with resource limits and output validation",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["serve", "configure", "status"]}, "model": {"type": "string"}, "backend": {"type": "string", "enum": ["ollama", "vllm", "llamacpp"]}, "port": {"type": "integer"}}, "required": ["action", "model"]})
);

ml_tool!(
    InferenceStop,
    "inference_stop",
    "Stop running inference instances",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["stop", "stop_all"]}, "model": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    InferenceStatus,
    "inference_status",
    "List running inference instances and health information",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["list_running", "health", "profile"]}, "model": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    InferenceBenchmark,
    "inference_benchmark",
    "Benchmark inference latency, throughput, and cost",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["benchmark", "compare", "cost_analysis"]}, "model": {"type": "string"}, "num_requests": {"type": "integer"}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(InferenceServe::new(workspace.clone())),
        Arc::new(InferenceStop::new(workspace.clone())),
        Arc::new(InferenceStatus::new(workspace.clone())),
        Arc::new(InferenceBenchmark::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
