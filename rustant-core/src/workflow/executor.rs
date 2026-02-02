//! Workflow executor — runs workflow steps sequentially, gating on approvals,
//! handling errors, and persisting state for pause/resume.

use crate::error::WorkflowError;
use crate::workflow::templates::{evaluate_condition, render_value, TemplateContext};
use crate::workflow::types::{
    ApprovalDecision, ErrorAction, GateType, WorkflowDefinition, WorkflowState, WorkflowStatus,
};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Trait for executing a tool by name with JSON arguments.
/// This abstracts over the real ToolRegistry for testability.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute_tool(
        &self,
        tool_name: &str,
        args: Value,
    ) -> Result<Value, String>;
}

/// Trait for requesting approval from the user for gated steps.
#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn request_approval(
        &self,
        workflow: &str,
        step_id: &str,
        message: &str,
        preview: Option<&str>,
    ) -> ApprovalDecision;
}

/// A simple auto-approve handler for testing.
pub struct AutoApproveHandler;

#[async_trait]
impl ApprovalHandler for AutoApproveHandler {
    async fn request_approval(
        &self,
        _workflow: &str,
        _step_id: &str,
        _message: &str,
        _preview: Option<&str>,
    ) -> ApprovalDecision {
        ApprovalDecision::Approved
    }
}

/// A handler that always denies for testing.
pub struct AutoDenyHandler;

#[async_trait]
impl ApprovalHandler for AutoDenyHandler {
    async fn request_approval(
        &self,
        _workflow: &str,
        _step_id: &str,
        _message: &str,
        _preview: Option<&str>,
    ) -> ApprovalDecision {
        ApprovalDecision::Denied
    }
}

/// The workflow executor manages workflow runs and their lifecycle.
pub struct WorkflowExecutor {
    tool_executor: Arc<dyn ToolExecutor>,
    approval_handler: Arc<dyn ApprovalHandler>,
    runs: Arc<Mutex<HashMap<Uuid, WorkflowState>>>,
    state_path: Option<PathBuf>,
}

impl WorkflowExecutor {
    pub fn new(
        tool_executor: Arc<dyn ToolExecutor>,
        approval_handler: Arc<dyn ApprovalHandler>,
        state_path: Option<PathBuf>,
    ) -> Self {
        Self {
            tool_executor,
            approval_handler,
            runs: Arc::new(Mutex::new(HashMap::new())),
            state_path,
        }
    }

    /// Start a new workflow run.
    pub async fn start(
        &self,
        workflow: &WorkflowDefinition,
        inputs: HashMap<String, Value>,
    ) -> Result<WorkflowState, WorkflowError> {
        let mut state = WorkflowState::new(workflow.name.clone(), inputs);
        state.status = WorkflowStatus::Running;
        state.updated_at = chrono::Utc::now();

        // Store the run
        {
            let mut runs = self.runs.lock().await;
            runs.insert(state.run_id, state.clone());
        }

        // Execute steps
        let final_state = self
            .execute_steps(workflow, state)
            .await?;

        // Update stored state
        {
            let mut runs = self.runs.lock().await;
            runs.insert(final_state.run_id, final_state.clone());
        }

        // Persist state if configured
        if let Some(ref path) = self.state_path {
            self.persist_state(&final_state, path).await?;
        }

        Ok(final_state)
    }

    /// Resume a paused workflow run with an approval decision.
    pub async fn resume(
        &self,
        run_id: Uuid,
        workflow: &WorkflowDefinition,
        decision: ApprovalDecision,
    ) -> Result<WorkflowState, WorkflowError> {
        let state = {
            let runs = self.runs.lock().await;
            runs.get(&run_id)
                .cloned()
                .ok_or(WorkflowError::RunNotFound { run_id })?
        };

        if state.status != WorkflowStatus::WaitingApproval {
            return Err(WorkflowError::StepFailed {
                step: format!("step_{}", state.current_step_index),
                message: format!("Cannot resume workflow in status: {}", state.status),
            });
        }

        let mut state = state;

        match decision {
            ApprovalDecision::Approved => {
                // Execute the current gated step
                let step = &workflow.steps[state.current_step_index];
                let ctx = TemplateContext::new(
                    state.inputs.clone(),
                    state.step_outputs.clone(),
                );

                let rendered_params = render_value(&serde_json::to_value(&step.params).unwrap(), &ctx)
                    .map_err(|e| WorkflowError::StepFailed {
                        step: step.id.clone(),
                        message: e.to_string(),
                    })?;

                let output = self
                    .tool_executor
                    .execute_tool(&step.tool, rendered_params)
                    .await
                    .map_err(|e| WorkflowError::StepFailed {
                        step: step.id.clone(),
                        message: e,
                    })?;

                state.step_outputs.insert(step.id.clone(), output);
                state.current_step_index += 1;
                state.status = WorkflowStatus::Running;
                state.updated_at = chrono::Utc::now();

                // Continue executing remaining steps
                let final_state = self.execute_steps(workflow, state).await?;
                let mut runs = self.runs.lock().await;
                runs.insert(final_state.run_id, final_state.clone());
                Ok(final_state)
            }
            ApprovalDecision::Denied => {
                state.status = WorkflowStatus::Failed;
                state.error = Some("Approval denied by user".to_string());
                state.updated_at = chrono::Utc::now();
                let mut runs = self.runs.lock().await;
                runs.insert(state.run_id, state.clone());
                Ok(state)
            }
        }
    }

    /// Cancel a running workflow.
    pub async fn cancel(&self, run_id: Uuid) -> Result<WorkflowState, WorkflowError> {
        let mut runs = self.runs.lock().await;
        let state = runs
            .get_mut(&run_id)
            .ok_or(WorkflowError::RunNotFound { run_id })?;
        state.status = WorkflowStatus::Cancelled;
        state.updated_at = chrono::Utc::now();
        Ok(state.clone())
    }

    /// Get the current status of a workflow run.
    pub async fn get_status(&self, run_id: Uuid) -> Result<WorkflowState, WorkflowError> {
        let runs = self.runs.lock().await;
        runs.get(&run_id)
            .cloned()
            .ok_or(WorkflowError::RunNotFound { run_id })
    }

    /// List all workflow runs.
    pub async fn list_runs(&self) -> Vec<WorkflowState> {
        let runs = self.runs.lock().await;
        runs.values().cloned().collect()
    }

    /// Execute steps starting from `state.current_step_index`.
    async fn execute_steps(
        &self,
        workflow: &WorkflowDefinition,
        mut state: WorkflowState,
    ) -> Result<WorkflowState, WorkflowError> {
        while state.current_step_index < workflow.steps.len() {
            let step = &workflow.steps[state.current_step_index];
            let ctx = TemplateContext::new(
                state.inputs.clone(),
                state.step_outputs.clone(),
            );

            // Check condition
            if let Some(ref condition) = step.condition {
                let should_run = evaluate_condition(condition, &ctx).unwrap_or(false);
                if !should_run {
                    state.current_step_index += 1;
                    state.updated_at = chrono::Utc::now();
                    continue;
                }
            }

            // Check gate
            if let Some(ref gate) = step.gate {
                if gate.gate_type == GateType::ApprovalRequired {
                    let message = step
                        .gate_message
                        .as_deref()
                        .unwrap_or(&gate.message);
                    let preview = step.gate_preview.as_deref().or(gate.preview.as_deref());

                    let decision = self
                        .approval_handler
                        .request_approval(
                            &state.workflow_name,
                            &step.id,
                            message,
                            preview,
                        )
                        .await;

                    match decision {
                        ApprovalDecision::Denied => {
                            state.status = WorkflowStatus::WaitingApproval;
                            state.updated_at = chrono::Utc::now();
                            return Ok(state);
                        }
                        ApprovalDecision::Approved => {
                            // Continue to execute the step
                        }
                    }
                }
            }

            // Render params
            let params_value = serde_json::to_value(&step.params).unwrap_or(Value::Object(Default::default()));
            let rendered_params = render_value(&params_value, &ctx).map_err(|e| {
                WorkflowError::StepFailed {
                    step: step.id.clone(),
                    message: e.to_string(),
                }
            })?;

            // Execute tool
            let result = self
                .tool_executor
                .execute_tool(&step.tool, rendered_params)
                .await;

            match result {
                Ok(output) => {
                    state.step_outputs.insert(step.id.clone(), output);
                    state.current_step_index += 1;
                    state.updated_at = chrono::Utc::now();
                }
                Err(err) => {
                    match &step.on_error {
                        Some(ErrorAction::Skip) => {
                            state.step_outputs.insert(
                                step.id.clone(),
                                Value::String(format!("skipped: {}", err)),
                            );
                            state.current_step_index += 1;
                            state.updated_at = chrono::Utc::now();
                        }
                        Some(ErrorAction::Retry { max_retries }) => {
                            let mut retries = 0;
                            let mut last_err = err;
                            while retries < *max_retries {
                                retries += 1;
                                let ctx2 = TemplateContext::new(
                                    state.inputs.clone(),
                                    state.step_outputs.clone(),
                                );
                                let params_value2 = serde_json::to_value(&step.params)
                                    .unwrap_or(Value::Object(Default::default()));
                                let rendered2 = render_value(&params_value2, &ctx2)
                                    .map_err(|e| WorkflowError::StepFailed {
                                        step: step.id.clone(),
                                        message: e.to_string(),
                                    })?;
                                match self.tool_executor.execute_tool(&step.tool, rendered2).await {
                                    Ok(output) => {
                                        state.step_outputs.insert(step.id.clone(), output);
                                        state.current_step_index += 1;
                                        state.updated_at = chrono::Utc::now();
                                        last_err = String::new();
                                        break;
                                    }
                                    Err(e) => {
                                        last_err = e;
                                    }
                                }
                            }
                            if !last_err.is_empty() {
                                state.status = WorkflowStatus::Failed;
                                state.error = Some(format!(
                                    "Step '{}' failed after {} retries: {}",
                                    step.id, max_retries, last_err
                                ));
                                state.updated_at = chrono::Utc::now();
                                return Ok(state);
                            }
                        }
                        Some(ErrorAction::Fail) | None => {
                            state.status = WorkflowStatus::Failed;
                            state.error =
                                Some(format!("Step '{}' failed: {}", step.id, err));
                            state.updated_at = chrono::Utc::now();
                            return Ok(state);
                        }
                    }
                }
            }
        }

        state.status = WorkflowStatus::Completed;
        state.updated_at = chrono::Utc::now();
        Ok(state)
    }

    /// Persist workflow state to disk.
    async fn persist_state(
        &self,
        state: &WorkflowState,
        base_path: &PathBuf,
    ) -> Result<(), WorkflowError> {
        let file_path = base_path.join(format!("{}.json", state.run_id));
        let json = serde_json::to_string_pretty(state).map_err(|e| {
            WorkflowError::StepFailed {
                step: "persistence".to_string(),
                message: e.to_string(),
            }
        })?;
        tokio::fs::create_dir_all(base_path)
            .await
            .map_err(|e| WorkflowError::StepFailed {
                step: "persistence".to_string(),
                message: e.to_string(),
            })?;
        tokio::fs::write(file_path, json)
            .await
            .map_err(|e| WorkflowError::StepFailed {
                step: "persistence".to_string(),
                message: e.to_string(),
            })?;
        Ok(())
    }

    /// Load a workflow state from disk.
    pub async fn load_state(
        base_path: &std::path::Path,
        run_id: Uuid,
    ) -> Result<WorkflowState, WorkflowError> {
        let file_path = base_path.join(format!("{}.json", run_id));
        let json = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|_| WorkflowError::RunNotFound { run_id })?;
        serde_json::from_str(&json).map_err(|e| WorkflowError::ParseError {
            message: e.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::parser::parse_workflow;

    /// A mock tool executor that returns configurable responses.
    struct MockToolExecutor {
        responses: Mutex<Vec<Result<Value, String>>>,
    }

    impl MockToolExecutor {
        fn new(responses: Vec<Result<Value, String>>) -> Self {
            Self {
                responses: Mutex::new(responses),
            }
        }

        fn succeeding(count: usize) -> Self {
            let responses: Vec<_> = (0..count)
                .map(|i| Ok(Value::String(format!("output_{}", i))))
                .collect();
            Self::new(responses)
        }
    }

    #[async_trait]
    impl ToolExecutor for MockToolExecutor {
        async fn execute_tool(
            &self,
            _tool_name: &str,
            _args: Value,
        ) -> Result<Value, String> {
            let mut responses = self.responses.lock().await;
            if responses.is_empty() {
                Ok(Value::String("default_output".to_string()))
            } else {
                responses.remove(0)
            }
        }
    }

    fn simple_workflow_yaml() -> &'static str {
        r#"
name: test_workflow
description: A test workflow
steps:
  - id: step1
    tool: echo
    params:
      text: "hello"
"#
    }

    fn multi_step_yaml() -> &'static str {
        r#"
name: multi_step
description: Multi-step workflow
inputs:
  - name: greeting
    type: string
steps:
  - id: step1
    tool: echo
    params:
      text: "{{ inputs.greeting }}"
  - id: step2
    tool: echo
    params:
      text: "{{ steps.step1.output }}"
  - id: step3
    tool: echo
    params:
      text: "final"
"#
    }

    fn gated_workflow_yaml() -> &'static str {
        r#"
name: gated
description: Workflow with gate
steps:
  - id: step1
    tool: echo
    params:
      text: "before gate"
  - id: gated_step
    tool: echo
    params:
      text: "after gate"
    gate:
      type: approval_required
      message: "Approve this?"
"#
    }

    #[tokio::test]
    async fn test_executor_start_creates_run() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(1)),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.workflow_name, "test_workflow");
        assert_eq!(state.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_executor_step_executes_tool() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![Ok(Value::String(
                "tool_output".to_string(),
            ))])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
        assert!(state.step_outputs.contains_key("step1"));
        assert_eq!(
            state.step_outputs["step1"],
            Value::String("tool_output".to_string())
        );
    }

    #[tokio::test]
    async fn test_executor_multi_step_sequential() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(3)),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(multi_step_yaml()).unwrap();
        let mut inputs = HashMap::new();
        inputs.insert("greeting".to_string(), Value::String("hi".to_string()));
        let state = executor.start(&wf, inputs).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
        assert_eq!(state.step_outputs.len(), 3);
        assert!(state.step_outputs.contains_key("step1"));
        assert!(state.step_outputs.contains_key("step2"));
        assert!(state.step_outputs.contains_key("step3"));
    }

    #[tokio::test]
    async fn test_executor_step_output_forwarded() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![
                Ok(Value::String("from_step1".to_string())),
                Ok(Value::String("from_step2".to_string())),
                Ok(Value::String("from_step3".to_string())),
            ])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(multi_step_yaml()).unwrap();
        let mut inputs = HashMap::new();
        inputs.insert("greeting".to_string(), Value::String("hi".to_string()));
        let state = executor.start(&wf, inputs).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
        assert_eq!(
            state.step_outputs["step1"],
            Value::String("from_step1".to_string())
        );
    }

    #[tokio::test]
    async fn test_executor_gate_pauses_workflow() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(2)),
            Arc::new(AutoDenyHandler),
            None,
        );
        let wf = parse_workflow(gated_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::WaitingApproval);
        assert_eq!(state.current_step_index, 1); // paused at the gated step
    }

    #[tokio::test]
    async fn test_executor_resume_after_approval() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(3)),
            Arc::new(AutoDenyHandler),
            None,
        );
        let wf = parse_workflow(gated_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::WaitingApproval);

        let resumed = executor
            .resume(state.run_id, &wf, ApprovalDecision::Approved)
            .await
            .unwrap();
        assert_eq!(resumed.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_executor_cancel_sets_cancelled() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(2)),
            Arc::new(AutoDenyHandler),
            None,
        );
        let wf = parse_workflow(gated_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::WaitingApproval);

        let cancelled = executor.cancel(state.run_id).await.unwrap();
        assert_eq!(cancelled.status, WorkflowStatus::Cancelled);
    }

    #[tokio::test]
    async fn test_executor_step_failure_with_fail_action() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![Err(
                "tool crashed".to_string(),
            )])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Failed);
        assert!(state.error.unwrap().contains("tool crashed"));
    }

    #[tokio::test]
    async fn test_executor_step_failure_with_skip_action() {
        let yaml = r#"
name: skip_test
description: Test skip on error
steps:
  - id: failing
    tool: bad_tool
    params: {}
    on_error:
      action: skip
  - id: after
    tool: echo
    params:
      text: "continued"
"#;
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![
                Err("fail".to_string()),
                Ok(Value::String("ok".to_string())),
            ])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(yaml).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
        assert!(state.step_outputs.contains_key("failing"));
        assert!(state.step_outputs["failing"]
            .as_str()
            .unwrap()
            .contains("skipped"));
    }

    #[tokio::test]
    async fn test_executor_step_failure_with_retry() {
        let yaml = r#"
name: retry_test
description: Test retry on error
steps:
  - id: flaky
    tool: flaky_tool
    params: {}
    on_error:
      action: retry
      max_retries: 3
"#;
        // Fail twice, succeed on third try
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![
                Err("fail1".to_string()),
                Err("fail2".to_string()),
                Ok(Value::String("success".to_string())),
            ])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(yaml).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
    }

    #[tokio::test]
    async fn test_executor_condition_skip_step() {
        let yaml = r#"
name: conditional
description: Conditional step test
steps:
  - id: check
    tool: echo
    params:
      text: "fail"
  - id: skipped
    tool: echo
    params:
      text: "should not run"
    condition: "{{ steps.check.output }} == 'pass'"
  - id: final_step
    tool: echo
    params:
      text: "done"
"#;
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::new(vec![
                Ok(Value::String("fail".to_string())),
                // step2 is skipped, so only step1 and step3 execute
                Ok(Value::String("done".to_string())),
            ])),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(yaml).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        assert_eq!(state.status, WorkflowStatus::Completed);
        assert!(state.step_outputs.contains_key("check"));
        assert!(!state.step_outputs.contains_key("skipped"));
        assert!(state.step_outputs.contains_key("final_step"));
    }

    #[tokio::test]
    async fn test_executor_get_status_returns_current() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(1)),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();
        let status = executor.get_status(state.run_id).await.unwrap();
        assert_eq!(status.status, WorkflowStatus::Completed);
        assert_eq!(status.run_id, state.run_id);
    }

    #[tokio::test]
    async fn test_executor_list_runs() {
        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(5)),
            Arc::new(AutoApproveHandler),
            None,
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        executor.start(&wf, HashMap::new()).await.unwrap();
        executor.start(&wf, HashMap::new()).await.unwrap();
        let runs = executor.list_runs().await;
        assert_eq!(runs.len(), 2);
    }

    #[tokio::test]
    async fn test_executor_state_persistence() {
        let temp_dir = tempfile::tempdir().unwrap();
        let state_path = temp_dir.path().to_path_buf();

        let executor = WorkflowExecutor::new(
            Arc::new(MockToolExecutor::succeeding(1)),
            Arc::new(AutoApproveHandler),
            Some(state_path.clone()),
        );
        let wf = parse_workflow(simple_workflow_yaml()).unwrap();
        let state = executor.start(&wf, HashMap::new()).await.unwrap();

        // Load from disk
        let loaded = WorkflowExecutor::load_state(&state_path, state.run_id)
            .await
            .unwrap();
        assert_eq!(loaded.run_id, state.run_id);
        assert_eq!(loaded.status, WorkflowStatus::Completed);
        assert_eq!(loaded.workflow_name, "test_workflow");
    }
}
