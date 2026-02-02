//! Template expression engine for workflow parameter substitution.
//!
//! Supports `{{ inputs.name }}` and `{{ steps.step_id.output }}` style
//! expressions, and simple condition evaluation for conditional steps.

use crate::error::WorkflowError;
use serde_json::Value;
use std::collections::HashMap;

/// Context for template rendering, containing available variable values.
pub struct TemplateContext {
    pub inputs: HashMap<String, Value>,
    pub step_outputs: HashMap<String, Value>,
}

impl TemplateContext {
    pub fn new(inputs: HashMap<String, Value>, step_outputs: HashMap<String, Value>) -> Self {
        Self {
            inputs,
            step_outputs,
        }
    }
}

/// Render template expressions in a string value, replacing `{{ ... }}` patterns.
pub fn render_string(template: &str, ctx: &TemplateContext) -> Result<String, WorkflowError> {
    let mut result = String::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let end = after_open
            .find("}}")
            .ok_or_else(|| WorkflowError::TemplateError {
                message: format!("Unclosed template expression in: {}", template),
            })?;
        let expr = after_open[..end].trim();
        let value = resolve_expression(expr, ctx)?;
        result.push_str(&value_to_string(&value));
        rest = &after_open[end + 2..];
    }
    result.push_str(rest);

    Ok(result)
}

/// Render template expressions within a JSON Value, recursively processing
/// strings, objects, and arrays.
pub fn render_value(value: &Value, ctx: &TemplateContext) -> Result<Value, WorkflowError> {
    match value {
        Value::String(s) => {
            if s.contains("{{") {
                let rendered = render_string(s, ctx)?;
                Ok(Value::String(rendered))
            } else {
                Ok(value.clone())
            }
        }
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), render_value(v, ctx)?);
            }
            Ok(Value::Object(new_map))
        }
        Value::Array(arr) => {
            let new_arr: Result<Vec<Value>, WorkflowError> =
                arr.iter().map(|v| render_value(v, ctx)).collect();
            Ok(Value::Array(new_arr?))
        }
        _ => Ok(value.clone()),
    }
}

/// Evaluate a condition expression, returning true/false.
///
/// Supports simple comparisons: `==`, `!=`
pub fn evaluate_condition(condition: &str, ctx: &TemplateContext) -> Result<bool, WorkflowError> {
    let rendered = render_string(condition, ctx)?;

    if let Some((left, right)) = rendered.split_once("!=") {
        let left = left.trim().trim_matches('\'').trim_matches('"');
        let right = right.trim().trim_matches('\'').trim_matches('"');
        return Ok(left != right);
    }

    if let Some((left, right)) = rendered.split_once("==") {
        let left = left.trim().trim_matches('\'').trim_matches('"');
        let right = right.trim().trim_matches('\'').trim_matches('"');
        return Ok(left == right);
    }

    // Truthy check: non-empty, non-"false", non-"0" strings are true
    let trimmed = rendered.trim();
    Ok(!trimmed.is_empty() && trimmed != "false" && trimmed != "0")
}

/// Resolve a dotted expression like `inputs.path` or `steps.fetch_pr.output`.
fn resolve_expression(expr: &str, ctx: &TemplateContext) -> Result<Value, WorkflowError> {
    let parts: Vec<&str> = expr.splitn(3, '.').collect();

    match parts.first() {
        Some(&"inputs") => {
            let key = parts.get(1).ok_or_else(|| WorkflowError::TemplateError {
                message: format!("Invalid input reference: {}", expr),
            })?;
            ctx.inputs
                .get(*key)
                .cloned()
                .ok_or_else(|| WorkflowError::TemplateError {
                    message: format!("Input '{}' not found", key),
                })
        }
        Some(&"steps") => {
            let step_id = parts.get(1).ok_or_else(|| WorkflowError::TemplateError {
                message: format!("Invalid step reference: {}", expr),
            })?;
            // Accept both `steps.id.output` and just `steps.id`
            ctx.step_outputs
                .get(*step_id)
                .cloned()
                .ok_or_else(|| WorkflowError::TemplateError {
                    message: format!("Step output '{}' not found", step_id),
                })
        }
        _ => Err(WorkflowError::TemplateError {
            message: format!("Unknown template variable: {}", expr),
        }),
    }
}

/// Convert a JSON Value to its string representation for template insertion.
fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Extract all template variable references from a string.
/// Returns pairs like `("inputs", "path")` or `("steps", "fetch_pr")`.
pub fn extract_references(template: &str) -> Vec<(String, String)> {
    let mut refs = Vec::new();
    let mut rest = template;

    while let Some(start) = rest.find("{{") {
        let after_open = &rest[start + 2..];
        if let Some(end) = after_open.find("}}") {
            let expr = after_open[..end].trim();
            let parts: Vec<&str> = expr.splitn(3, '.').collect();
            if parts.len() >= 2 {
                refs.push((parts[0].to_string(), parts[1].to_string()));
            }
            rest = &after_open[end + 2..];
        } else {
            break;
        }
    }

    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ctx(inputs: Vec<(&str, &str)>, step_outputs: Vec<(&str, &str)>) -> TemplateContext {
        let inputs_map: HashMap<String, Value> = inputs
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();
        let step_map: HashMap<String, Value> = step_outputs
            .into_iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();
        TemplateContext::new(inputs_map, step_map)
    }

    #[test]
    fn test_render_simple_substitution() {
        let ctx = make_ctx(vec![("path", "/home/user/file.rs")], vec![]);
        let result = render_string("{{ inputs.path }}", &ctx).unwrap();
        assert_eq!(result, "/home/user/file.rs");
    }

    #[test]
    fn test_render_step_output_reference() {
        let ctx = make_ctx(vec![], vec![("read_file", "file contents here")]);
        let result = render_string("Content: {{ steps.read_file.output }}", &ctx).unwrap();
        assert_eq!(result, "Content: file contents here");
    }

    #[test]
    fn test_render_no_substitution_needed() {
        let ctx = make_ctx(vec![], vec![]);
        let result = render_string("plain text with no templates", &ctx).unwrap();
        assert_eq!(result, "plain text with no templates");
    }

    #[test]
    fn test_render_missing_variable_returns_error() {
        let ctx = make_ctx(vec![], vec![]);
        let result = render_string("{{ inputs.missing }}", &ctx);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_render_nested_json_value() {
        let ctx = make_ctx(vec![("url", "https://example.com")], vec![]);
        let value = serde_json::json!({
            "url": "{{ inputs.url }}",
            "headers": {
                "host": "{{ inputs.url }}"
            }
        });
        let rendered = render_value(&value, &ctx).unwrap();
        assert_eq!(rendered["url"].as_str().unwrap(), "https://example.com");
        assert_eq!(
            rendered["headers"]["host"].as_str().unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn test_evaluate_condition_true() {
        let ctx = make_ctx(vec![], vec![("check", "pass")]);
        let result = evaluate_condition("{{ steps.check.output }} == 'pass'", &ctx).unwrap();
        assert!(result);
    }

    #[test]
    fn test_evaluate_condition_false() {
        let ctx = make_ctx(vec![], vec![("check", "fail")]);
        let result = evaluate_condition("{{ steps.check.output }} == 'pass'", &ctx).unwrap();
        assert!(!result);
    }

    #[test]
    fn test_evaluate_condition_not_equals() {
        let ctx = make_ctx(vec![], vec![("check", "fail")]);
        let result = evaluate_condition("{{ steps.check.output }} != 'pass'", &ctx).unwrap();
        assert!(result);

        let ctx2 = make_ctx(vec![], vec![("check", "pass")]);
        let result2 = evaluate_condition("{{ steps.check.output }} != 'pass'", &ctx2).unwrap();
        assert!(!result2);
    }
}
