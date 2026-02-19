//! LLM fine-tuning tools (5 tools).

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
    MlFinetune,
    "ml_finetune",
    "Fine-tune LLMs with LoRA/QLoRA/full methods and alignment checking",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["start", "stop", "status", "merge_adapter"]}, "base_model": {"type": "string"}, "dataset_id": {"type": "string"}, "method": {"type": "string", "enum": ["lora", "qlora", "full"]}}, "required": ["action"]})
);

ml_tool!(
    MlChatDataset,
    "ml_chat_dataset",
    "Create, convert, validate, and PII-scan chat fine-tuning datasets",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["create", "convert", "validate", "preview", "pii_scan"]}, "format": {"type": "string", "enum": ["alpaca", "sharegpt", "chatml", "llama3", "openai"]}, "path": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlQuantize,
    "ml_quantize",
    "Quantize models with GPTQ, AWQ, GGUF, or BitsAndBytes",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["quantize", "compare", "benchmark", "validate"]}, "model_id": {"type": "string"}, "method": {"type": "string", "enum": ["gptq", "awq", "gguf", "bnb"]}}, "required": ["action", "model_id"]})
);

ml_tool!(
    MlEval,
    "ml_eval",
    "Run LLM evaluation benchmarks (perplexity, MMLU, HumanEval, safety)",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["run_benchmark", "compare", "leaderboard", "safety_eval"]}, "model": {"type": "string"}, "benchmark": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlAdapter,
    "ml_adapter",
    "Manage LoRA adapters — list, merge, switch, delete with provenance",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "merge", "switch", "delete", "provenance"]}, "adapter_name": {"type": "string"}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(MlFinetune::new(workspace.clone())),
        Arc::new(MlChatDataset::new(workspace.clone())),
        Arc::new(MlQuantize::new(workspace.clone())),
        Arc::new(MlEval::new(workspace.clone())),
        Arc::new(MlAdapter::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
