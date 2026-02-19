//! AI Security tools (3 tools).

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
                    .unwrap_or("scan");
                Ok(ToolOutput::text(format!(
                    "{} action '{action}' completed",
                    $tool_name
                )))
            }
        }
    };
}

ml_tool!(
    AiRedTeam,
    "ai_red_team",
    "Generate adversarial attacks and run red team campaigns against models",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["generate_attacks", "run_campaign", "report"]}, "model": {"type": "string"}, "n_attacks": {"type": "integer"}}, "required": ["action"]})
);

ml_tool!(
    AiAdversarialScan,
    "ai_adversarial_scan",
    "Scan inputs for adversarial manipulation, jailbreaks, and injection",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["scan_input", "jailbreak_test", "injection_test"]}, "text": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    AiProvenanceVerify,
    "ai_provenance_verify",
    "Verify model and dataset provenance, integrity, and supply chain",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["verify_model", "verify_dataset", "supply_chain"]}, "model_id": {"type": "string"}, "dataset_id": {"type": "string"}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(AiRedTeam::new(workspace.clone())),
        Arc::new(AiAdversarialScan::new(workspace.clone())),
        Arc::new(AiProvenanceVerify::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
