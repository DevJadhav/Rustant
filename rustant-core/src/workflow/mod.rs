//! Workflow Engine for Rustant.
//!
//! Provides a declarative YAML DSL for defining multi-step workflows with
//! typed parameters, approval gates, conditional execution, and error handling.

pub mod builtins;
pub mod executor;
pub mod parser;
pub mod templates;
pub mod types;

pub use builtins::{all_builtins, get_builtin, list_builtin_names};
pub use executor::{ApprovalHandler, AutoApproveHandler, AutoDenyHandler, ToolExecutor, WorkflowExecutor};
pub use parser::{parse_workflow, validate_workflow};
pub use types::{
    ApprovalDecision, ErrorAction, GateConfig, GateType, WorkflowDefinition, WorkflowInput,
    WorkflowOutput, WorkflowState, WorkflowStatus, WorkflowStep,
};
