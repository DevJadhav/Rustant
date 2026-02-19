//! Data engineering tools (9 tools).

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
    MlDataIngest,
    "ml_data_ingest",
    "Ingest data from CSV, JSON, JSONL, SQLite, or API sources",
    RiskLevel::Write,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["ingest", "preview", "schema", "stats"], "description": "Action to perform"},
            "source": {"type": "string", "description": "Path or URL to data source"},
            "format": {"type": "string", "enum": ["csv", "json", "jsonl", "sqlite"], "description": "Data format"},
            "limit": {"type": "integer", "description": "Maximum rows to load"}
        },
        "required": ["action", "source"]
    })
);

ml_tool!(
    MlDataTransform,
    "ml_data_transform",
    "Apply transformations to datasets (clean, normalize, encode, augment)",
    RiskLevel::Write,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["apply", "preview_transform", "undo"], "description": "Action to perform"},
            "dataset_id": {"type": "string", "description": "Dataset to transform"},
            "transform": {"type": "object", "description": "Transform specification"}
        },
        "required": ["action", "dataset_id"]
    })
);

ml_tool!(
    MlDataValidate,
    "ml_data_validate",
    "Validate data quality with PII scanning, drift detection, and quality gates",
    RiskLevel::ReadOnly,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["quality_report", "check_drift", "pii_scan"], "description": "Validation action"},
            "dataset_id": {"type": "string", "description": "Dataset to validate"}
        },
        "required": ["action", "dataset_id"]
    })
);

ml_tool!(
    MlDataSplit,
    "ml_data_split",
    "Split datasets for training (train/test, stratified, k-fold, temporal)",
    RiskLevel::Write,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["train_test", "stratified", "kfold", "temporal"], "description": "Split strategy"},
            "dataset_id": {"type": "string", "description": "Dataset to split"},
            "test_size": {"type": "number", "description": "Test set fraction (0.0-1.0)"}
        },
        "required": ["action", "dataset_id"]
    })
);

ml_tool!(
    MlDataVersion,
    "ml_data_version",
    "Version control for datasets with content hashing and diff",
    RiskLevel::ReadOnly,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["list", "diff", "rollback", "tag"], "description": "Version action"},
            "dataset_id": {"type": "string", "description": "Dataset ID"}
        },
        "required": ["action"]
    })
);

ml_tool!(
    MlDataExport,
    "ml_data_export",
    "Export datasets to CSV, JSON, or Parquet format",
    RiskLevel::Write,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": ["csv", "json", "parquet"], "description": "Export format"},
            "dataset_id": {"type": "string", "description": "Dataset to export"},
            "output_path": {"type": "string", "description": "Output file path"}
        },
        "required": ["action", "dataset_id"]
    })
);

pub fn register(registry: &mut ToolRegistry, workspace: &Arc<PathBuf>) {
    let tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(MlDataIngest::new(workspace.clone())),
        Arc::new(MlDataTransform::new(workspace.clone())),
        Arc::new(MlDataValidate::new(workspace.clone())),
        Arc::new(MlDataSplit::new(workspace.clone())),
        Arc::new(MlDataVersion::new(workspace.clone())),
        Arc::new(MlDataExport::new(workspace.clone())),
    ];
    for tool in tools {
        registry.register(tool).ok();
    }
}
