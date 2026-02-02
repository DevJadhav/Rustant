//! Workflow type definitions for the Rustant workflow engine.
//!
//! Defines the core data structures: workflow definitions, steps, gates,
//! state tracking, and related enums.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use uuid::Uuid;

/// A complete workflow definition parsed from YAML DSL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub inputs: Vec<WorkflowInput>,
    pub steps: Vec<WorkflowStep>,
    #[serde(default)]
    pub outputs: Vec<WorkflowOutput>,
}

fn default_version() -> String {
    "1.0".to_string()
}

/// A typed input parameter for a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowInput {
    pub name: String,
    #[serde(rename = "type", default = "default_input_type")]
    pub input_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
}

fn default_input_type() -> String {
    "string".to_string()
}

/// A single step in a workflow execution plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub on_error: Option<ErrorAction>,
    #[serde(default)]
    pub gate: Option<GateConfig>,
    #[serde(default)]
    pub gate_message: Option<String>,
    #[serde(default)]
    pub gate_preview: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// An output declaration for a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowOutput {
    pub name: String,
    pub value: String,
}

/// Type of approval gate on a workflow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateType {
    ApprovalRequired,
    ApprovalOptional,
    ReviewOnly,
    Conditional,
}

impl fmt::Display for GateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateType::ApprovalRequired => write!(f, "approval_required"),
            GateType::ApprovalOptional => write!(f, "approval_optional"),
            GateType::ReviewOnly => write!(f, "review_only"),
            GateType::Conditional => write!(f, "conditional"),
        }
    }
}

/// Configuration for an approval gate on a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    #[serde(rename = "type")]
    pub gate_type: GateType,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub preview: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub default_action: Option<String>,
    #[serde(default)]
    pub required_approvers: Option<usize>,
}

/// Error handling strategy for a failed step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum ErrorAction {
    Fail,
    Skip,
    Retry { max_retries: usize },
}

/// Status of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    Pending,
    Running,
    WaitingApproval,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl fmt::Display for WorkflowStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WorkflowStatus::Pending => write!(f, "pending"),
            WorkflowStatus::Running => write!(f, "running"),
            WorkflowStatus::WaitingApproval => write!(f, "waiting_approval"),
            WorkflowStatus::Paused => write!(f, "paused"),
            WorkflowStatus::Completed => write!(f, "completed"),
            WorkflowStatus::Failed => write!(f, "failed"),
            WorkflowStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

/// Persistent state of a workflow run, supporting pause/resume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    pub run_id: Uuid,
    pub workflow_name: String,
    pub status: WorkflowStatus,
    pub current_step_index: usize,
    pub step_outputs: HashMap<String, serde_json::Value>,
    pub inputs: HashMap<String, serde_json::Value>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub error: Option<String>,
}

impl WorkflowState {
    /// Create a new workflow state for a fresh run.
    pub fn new(workflow_name: String, inputs: HashMap<String, serde_json::Value>) -> Self {
        let now = Utc::now();
        Self {
            run_id: Uuid::new_v4(),
            workflow_name,
            status: WorkflowStatus::Pending,
            current_step_index: 0,
            step_outputs: HashMap::new(),
            inputs,
            started_at: now,
            updated_at: now,
            error: None,
        }
    }
}

/// Approval decision from a user for a gated step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalDecision {
    Approved,
    Denied,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_status_display() {
        assert_eq!(WorkflowStatus::Pending.to_string(), "pending");
        assert_eq!(WorkflowStatus::Running.to_string(), "running");
        assert_eq!(
            WorkflowStatus::WaitingApproval.to_string(),
            "waiting_approval"
        );
        assert_eq!(WorkflowStatus::Paused.to_string(), "paused");
        assert_eq!(WorkflowStatus::Completed.to_string(), "completed");
        assert_eq!(WorkflowStatus::Failed.to_string(), "failed");
        assert_eq!(WorkflowStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_workflow_status_serde_roundtrip() {
        let statuses = vec![
            WorkflowStatus::Pending,
            WorkflowStatus::Running,
            WorkflowStatus::WaitingApproval,
            WorkflowStatus::Paused,
            WorkflowStatus::Completed,
            WorkflowStatus::Failed,
            WorkflowStatus::Cancelled,
        ];
        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let deserialized: WorkflowStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, deserialized);
        }
    }

    #[test]
    fn test_workflow_state_new() {
        let inputs: HashMap<String, serde_json::Value> = HashMap::new();
        let state = WorkflowState::new("test_workflow".to_string(), inputs.clone());

        assert_eq!(state.workflow_name, "test_workflow");
        assert_eq!(state.status, WorkflowStatus::Pending);
        assert_eq!(state.current_step_index, 0);
        assert!(state.step_outputs.is_empty());
        assert!(state.error.is_none());
    }

    #[test]
    fn test_workflow_definition_serde_roundtrip() {
        let def = WorkflowDefinition {
            name: "test".to_string(),
            description: "A test workflow".to_string(),
            version: "1.0".to_string(),
            author: Some("rustant".to_string()),
            inputs: vec![WorkflowInput {
                name: "path".to_string(),
                input_type: "string".to_string(),
                description: "File path".to_string(),
                optional: false,
                default: None,
            }],
            steps: vec![WorkflowStep {
                id: "step1".to_string(),
                tool: "file_read".to_string(),
                params: {
                    let mut m = HashMap::new();
                    m.insert(
                        "path".to_string(),
                        serde_json::Value::String("test.txt".to_string()),
                    );
                    m
                },
                output: Some("content".to_string()),
                condition: None,
                on_error: None,
                gate: None,
                gate_message: None,
                gate_preview: None,
                timeout_secs: None,
            }],
            outputs: vec![WorkflowOutput {
                name: "result".to_string(),
                value: "{{ steps.step1.output }}".to_string(),
            }],
        };

        let json = serde_json::to_string(&def).unwrap();
        let deserialized: WorkflowDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(def.name, deserialized.name);
        assert_eq!(def.steps.len(), deserialized.steps.len());
        assert_eq!(def.inputs.len(), deserialized.inputs.len());
        assert_eq!(def.outputs.len(), deserialized.outputs.len());
    }

    #[test]
    fn test_gate_type_variants() {
        let types = vec![
            (GateType::ApprovalRequired, "\"approval_required\""),
            (GateType::ApprovalOptional, "\"approval_optional\""),
            (GateType::ReviewOnly, "\"review_only\""),
            (GateType::Conditional, "\"conditional\""),
        ];
        for (gate, expected_json) in types {
            let json = serde_json::to_string(&gate).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: GateType = serde_json::from_str(&json).unwrap();
            assert_eq!(gate, deserialized);
        }
    }

    #[test]
    fn test_error_action_variants() {
        let fail = ErrorAction::Fail;
        let json = serde_json::to_string(&fail).unwrap();
        assert!(json.contains("fail"));

        let skip = ErrorAction::Skip;
        let json = serde_json::to_string(&skip).unwrap();
        assert!(json.contains("skip"));

        let retry = ErrorAction::Retry { max_retries: 3 };
        let json = serde_json::to_string(&retry).unwrap();
        assert!(json.contains("retry"));
        assert!(json.contains("3"));
        let deserialized: ErrorAction = serde_json::from_str(&json).unwrap();
        match deserialized {
            ErrorAction::Retry { max_retries } => assert_eq!(max_retries, 3),
            _ => panic!("Expected Retry variant"),
        }
    }

    #[test]
    fn test_workflow_input_with_default() {
        let input = WorkflowInput {
            name: "focus_areas".to_string(),
            input_type: "string[]".to_string(),
            description: "Areas to focus on".to_string(),
            optional: true,
            default: Some(serde_json::json!(["security", "performance"])),
        };

        let json = serde_json::to_string(&input).unwrap();
        let deserialized: WorkflowInput = serde_json::from_str(&json).unwrap();
        assert!(deserialized.optional);
        assert!(deserialized.default.is_some());
        let defaults = deserialized.default.unwrap();
        assert!(defaults.is_array());
        assert_eq!(defaults.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_workflow_step_with_condition() {
        let step = WorkflowStep {
            id: "conditional_step".to_string(),
            tool: "echo".to_string(),
            params: HashMap::new(),
            output: None,
            condition: Some("{{ steps.check.output }} == 'pass'".to_string()),
            on_error: None,
            gate: None,
            gate_message: None,
            gate_preview: None,
            timeout_secs: Some(60),
        };

        let json = serde_json::to_string(&step).unwrap();
        let deserialized: WorkflowStep = serde_json::from_str(&json).unwrap();
        assert_eq!(
            deserialized.condition.unwrap(),
            "{{ steps.check.output }} == 'pass'"
        );
        assert_eq!(deserialized.timeout_secs, Some(60));
    }
}
