//! YAML DSL parser and validator for workflow definitions.

use crate::error::WorkflowError;
use crate::workflow::templates::extract_references;
use crate::workflow::types::WorkflowDefinition;
use std::collections::HashSet;

/// Parse a workflow definition from a YAML string.
pub fn parse_workflow(yaml: &str) -> Result<WorkflowDefinition, WorkflowError> {
    serde_yaml::from_str::<WorkflowDefinition>(yaml).map_err(|e| WorkflowError::ParseError {
        message: e.to_string(),
    })
}

/// Validate a parsed workflow definition for structural correctness.
///
/// Checks:
/// - At least one step exists
/// - No duplicate step IDs
/// - All template step references point to earlier steps
pub fn validate_workflow(workflow: &WorkflowDefinition) -> Result<(), WorkflowError> {
    // Must have at least one step
    if workflow.steps.is_empty() {
        return Err(WorkflowError::ValidationFailed {
            message: "Workflow must have at least one step".to_string(),
        });
    }

    // Check for duplicate step IDs
    let mut seen_ids = HashSet::new();
    for step in &workflow.steps {
        if !seen_ids.insert(&step.id) {
            return Err(WorkflowError::ValidationFailed {
                message: format!("Duplicate step ID: '{}'", step.id),
            });
        }
    }

    // Validate that template references point to known steps
    let step_ids: HashSet<&str> = workflow.steps.iter().map(|s| s.id.as_str()).collect();
    let input_names: HashSet<&str> = workflow.inputs.iter().map(|i| i.name.as_str()).collect();

    for (idx, step) in workflow.steps.iter().enumerate() {
        let earlier_steps: HashSet<&str> =
            workflow.steps[..idx].iter().map(|s| s.id.as_str()).collect();

        // Check param templates
        for value in step.params.values() {
            if let Some(s) = value.as_str() {
                check_template_refs(s, &earlier_steps, &input_names, &step.id)?;
            }
        }

        // Check condition template
        if let Some(cond) = &step.condition {
            check_template_refs(cond, &earlier_steps, &input_names, &step.id)?;
        }

        // Check gate preview/message templates
        if let Some(msg) = &step.gate_message {
            check_template_refs(msg, &earlier_steps, &input_names, &step.id)?;
        }
        if let Some(preview) = &step.gate_preview {
            check_template_refs(preview, &earlier_steps, &input_names, &step.id)?;
        }
    }

    // Validate output templates
    for output in &workflow.outputs {
        let all_steps: HashSet<&str> = step_ids.iter().copied().collect();
        check_template_refs(&output.value, &all_steps, &input_names, "output")?;
    }

    Ok(())
}

/// Check that all template references in a string point to known steps/inputs.
fn check_template_refs(
    template: &str,
    known_steps: &HashSet<&str>,
    known_inputs: &HashSet<&str>,
    context_step: &str,
) -> Result<(), WorkflowError> {
    let refs = extract_references(template);
    for (namespace, name) in refs {
        match namespace.as_str() {
            "steps" => {
                if !known_steps.contains(name.as_str()) {
                    return Err(WorkflowError::ValidationFailed {
                        message: format!(
                            "Step '{}' references unknown step '{}' (steps must reference earlier steps)",
                            context_step, name
                        ),
                    });
                }
            }
            "inputs" => {
                if !known_inputs.contains(name.as_str()) {
                    return Err(WorkflowError::ValidationFailed {
                        message: format!(
                            "Step '{}' references unknown input '{}'",
                            context_step, name
                        ),
                    });
                }
            }
            _ => {
                return Err(WorkflowError::ValidationFailed {
                    message: format!(
                        "Step '{}' has unknown template namespace '{}'",
                        context_step, namespace
                    ),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_workflow() {
        let yaml = r#"
name: minimal
description: A minimal workflow
steps:
  - id: step1
    tool: echo
    params:
      text: "hello"
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.name, "minimal");
        assert_eq!(wf.steps.len(), 1);
        assert_eq!(wf.steps[0].id, "step1");
        assert_eq!(wf.steps[0].tool, "echo");
    }

    #[test]
    fn test_parse_workflow_with_inputs() {
        let yaml = r#"
name: with_inputs
description: Workflow with typed inputs
inputs:
  - name: path
    type: string
    description: File path to process
  - name: focus_areas
    type: "string[]"
    optional: true
    default: ["security", "performance"]
steps:
  - id: read
    tool: file_read
    params:
      path: "{{ inputs.path }}"
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.inputs.len(), 2);
        assert_eq!(wf.inputs[0].name, "path");
        assert_eq!(wf.inputs[0].input_type, "string");
        assert!(wf.inputs[1].optional);
        assert!(wf.inputs[1].default.is_some());
    }

    #[test]
    fn test_parse_workflow_with_gates() {
        let yaml = r#"
name: gated
description: Workflow with approval gates
steps:
  - id: review
    tool: echo
    params:
      text: "Review this"
    gate:
      type: approval_required
      message: "Approve this action?"
      timeout_secs: 300
"#;
        let wf = parse_workflow(yaml).unwrap();
        let gate = wf.steps[0].gate.as_ref().unwrap();
        assert_eq!(gate.gate_type, super::super::types::GateType::ApprovalRequired);
        assert_eq!(gate.message, "Approve this action?");
        assert_eq!(gate.timeout_secs, Some(300));
    }

    #[test]
    fn test_parse_workflow_with_conditions() {
        let yaml = r#"
name: conditional
description: Workflow with conditional steps
inputs:
  - name: mode
    type: string
steps:
  - id: check
    tool: echo
    params:
      text: "checking"
  - id: optional_step
    tool: echo
    params:
      text: "conditional"
    condition: "{{ steps.check.output }} == 'pass'"
"#;
        let wf = parse_workflow(yaml).unwrap();
        assert_eq!(wf.steps.len(), 2);
        assert!(wf.steps[1].condition.is_some());
    }

    #[test]
    fn test_parse_workflow_with_error_handling() {
        let yaml = r#"
name: error_handling
description: Workflow with error handling
steps:
  - id: risky
    tool: shell_exec
    params:
      command: "ls"
    on_error:
      action: retry
      max_retries: 3
"#;
        let wf = parse_workflow(yaml).unwrap();
        let on_error = wf.steps[0].on_error.as_ref().unwrap();
        match on_error {
            super::super::types::ErrorAction::Retry { max_retries } => {
                assert_eq!(*max_retries, 3)
            }
            _ => panic!("Expected Retry error action"),
        }
    }

    #[test]
    fn test_parse_invalid_yaml_returns_error() {
        let yaml = "this is not: valid: yaml: {{{}}}";
        let result = parse_workflow(yaml);
        assert!(result.is_err());
        match result.unwrap_err() {
            WorkflowError::ParseError { .. } => {}
            other => panic!("Expected ParseError, got: {:?}", other),
        }
    }

    #[test]
    fn test_validate_workflow_missing_steps() {
        let yaml = r#"
name: empty
description: No steps
steps: []
"#;
        let wf = parse_workflow(yaml).unwrap();
        let result = validate_workflow(&wf);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("at least one step"));
    }

    #[test]
    fn test_validate_workflow_duplicate_step_ids() {
        let yaml = r#"
name: dupes
description: Duplicate step IDs
steps:
  - id: step1
    tool: echo
    params:
      text: "first"
  - id: step1
    tool: echo
    params:
      text: "second"
"#;
        let wf = parse_workflow(yaml).unwrap();
        let result = validate_workflow(&wf);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate step ID"));
    }

    #[test]
    fn test_validate_workflow_unknown_step_reference() {
        let yaml = r#"
name: bad_ref
description: References unknown step
steps:
  - id: step1
    tool: echo
    params:
      text: "{{ steps.nonexistent.output }}"
"#;
        let wf = parse_workflow(yaml).unwrap();
        let result = validate_workflow(&wf);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unknown step"));
    }

    #[test]
    fn test_validate_workflow_valid_passes() {
        let yaml = r#"
name: valid
description: A valid workflow
inputs:
  - name: path
    type: string
steps:
  - id: read
    tool: file_read
    params:
      path: "{{ inputs.path }}"
  - id: process
    tool: echo
    params:
      text: "{{ steps.read.output }}"
outputs:
  - name: result
    value: "{{ steps.process.output }}"
"#;
        let wf = parse_workflow(yaml).unwrap();
        let result = validate_workflow(&wf);
        assert!(result.is_ok());
    }
}
