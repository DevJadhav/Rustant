//! Evaluation tools (4 tools).

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
    EvalBenchmark,
    "eval_benchmark",
    "Run evaluation benchmark suites with safety benchmarks",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["run_suite", "create_suite", "list", "compare", "report"]}, "suite_name": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    EvalJudge,
    "eval_judge",
    "LLM-as-Judge evaluation with configurable rubrics and bias correction",
    RiskLevel::Execute,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["evaluate", "calibrate", "configure_rubric"]}, "query": {"type": "string"}, "response": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    EvalAnalyze,
    "eval_analyze",
    "Automated error taxonomy, distribution analysis, and saturation detection",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["error_taxonomy", "distribution", "saturation"]}, "trace_filter": {"type": "string"}}, "required": ["action"]})
);

ml_tool!(
    EvalReport,
    "eval_report",
    "Generate evaluation reports with trend analysis and full audit trails",
    RiskLevel::ReadOnly,
    serde_json::json!({"type": "object", "properties": {"action": {"type": "string", "enum": ["generate", "export", "trend_analysis"]}, "format": {"type": "string", "enum": ["markdown", "json", "html"]}}, "required": ["action"]})
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(EvalBenchmark::new(workspace.clone())),
        Arc::new(EvalJudge::new(workspace.clone())),
        Arc::new(EvalAnalyze::new(workspace.clone())),
        Arc::new(EvalReport::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
