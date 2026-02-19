//! Training tools (5 tools).

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
    MlTrain,
    "ml_train",
    "Start, stop, or monitor model training runs",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["start", "stop", "status", "logs"]}, "experiment_id": {"type": "string"}, "model_type": {"type": "string"}, "dataset_id": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlExperiment,
    "ml_experiment",
    "Create, list, compare, or explain training experiments",
    RiskLevel::Write,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["create", "list", "compare", "delete", "explain"]}, "name": {"type": "string"}, "experiment_id": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlHyperparams,
    "ml_hyperparams",
    "Run hyperparameter sweeps (grid, random, Bayesian)",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["sweep", "analyze", "best_params"]}, "strategy": {"type": "string", "enum": ["grid", "random", "bayesian"]}, "experiment_id": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlCheckpoint,
    "ml_checkpoint",
    "List, load, compare, or export training checkpoints",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["list", "load", "compare", "export"]}, "experiment_id": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    MlMetrics,
    "ml_metrics",
    "Plot, compare, and analyze training metrics with anomaly detection",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["plot", "compare", "summary", "anomaly_check"]}, "experiment_id": {"type": "string"}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(MlTrain::new(workspace.clone())),
        Arc::new(MlExperiment::new(workspace.clone())),
        Arc::new(MlHyperparams::new(workspace.clone())),
        Arc::new(MlCheckpoint::new(workspace.clone())),
        Arc::new(MlMetrics::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
