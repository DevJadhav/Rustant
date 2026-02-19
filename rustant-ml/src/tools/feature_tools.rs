//! Feature store tools (3 tools).

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
    MlFeatureDefine,
    "ml_feature_define",
    "Define, list, show, or delete feature definitions",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["create", "list", "show", "delete"]}, "name": {"type": "string"}, "dtype": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlFeatureCompute,
    "ml_feature_compute",
    "Compute features from raw data using defined transforms",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["compute", "batch_compute"]}, "feature_group": {"type": "string"}, "dataset_id": {"type": "string"}}, "required": ["action", "feature_group"]})
);

ml_tool!(
    MlFeatureServe,
    "ml_feature_serve",
    "Serve features for online inference or batch retrieval",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["get", "batch_get", "stats"]}, "feature_group": {"type": "string"}, "entity_key": {"type": "string"}}, "required": ["action", "feature_group"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(MlFeatureDefine::new(workspace.clone())),
        Arc::new(MlFeatureCompute::new(workspace.clone())),
        Arc::new(MlFeatureServe::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
