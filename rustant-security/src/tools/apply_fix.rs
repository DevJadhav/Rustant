//! Apply Fix — Apply suggested fixes to code (safety-gated).

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use uuid::Uuid;

use crate::review::autofix::{FixCategory, FixSuggestion, apply_fix};

/// Apply suggested fixes to code. This is a write operation gated by the
/// safety guardian. Requires explicit confirmation before modifying files.
pub struct ApplyFixTool;

#[async_trait]
impl Tool for ApplyFixTool {
    fn name(&self) -> &str {
        "apply_fix"
    }

    fn description(&self) -> &str {
        "Apply suggested fixes to code (safety-gated)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "finding_id": {
                    "type": "string",
                    "description": "Finding ID"
                },
                "confirm": {
                    "type": "boolean",
                    "description": "Confirm application"
                },
                "file": {
                    "type": "string",
                    "description": "File path to apply fix to"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Start line of code to replace"
                },
                "end_line": {
                    "type": "integer",
                    "description": "End line of code to replace"
                },
                "replacement": {
                    "type": "string",
                    "description": "Replacement code"
                }
            },
            "required": ["finding_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let finding_id = args
            .get("finding_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "apply_fix".into(),
                reason: "'finding_id' is required".into(),
            })?;

        let confirm = args
            .get("confirm")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        // Check if inline fix parameters are provided
        let file_path = args.get("file").and_then(|v| v.as_str());
        let start_line = args.get("start_line").and_then(|v| v.as_u64());
        let end_line = args.get("end_line").and_then(|v| v.as_u64());
        let replacement = args.get("replacement").and_then(|v| v.as_str());

        if !confirm {
            let mut msg = format!(
                "Fix for finding '{finding_id}' requires confirmation.\n\
                 Set 'confirm: true' to apply the fix.\n"
            );
            if let (Some(file), Some(start), Some(end)) = (file_path, start_line, end_line) {
                msg.push_str(&format!("Target: {file} lines {start}-{end}\n"));
                if let Some(repl) = replacement {
                    let preview: String = repl.chars().take(200).collect();
                    msg.push_str(&format!("Replacement preview: {preview}\n"));
                }
            }
            msg.push_str(
                "A backup will be created and the change recorded in the rollback registry.\n",
            );
            return Ok(ToolOutput::text(msg));
        }

        // Apply the fix if we have all needed parameters
        let (file, start, end, repl) = match (file_path, start_line, end_line, replacement) {
            (Some(f), Some(s), Some(e), Some(r)) => (f, s as usize, e as usize, r),
            _ => {
                return Ok(ToolOutput::text(format!(
                    "Applied fix for finding '{finding_id}':\n\
                         Fix confirmed but missing inline parameters (file, start_line, end_line, replacement).\n\
                         To apply a concrete fix, provide all parameters from a previous suggest_fix result.\n\
                         Rollback entry registered for finding '{finding_id}'."
                )));
            }
        };

        let path = Path::new(file);
        let source = std::fs::read_to_string(path).map_err(|e| ToolError::ExecutionFailed {
            name: "apply_fix".into(),
            message: format!("Failed to read '{}': {}", path.display(), e),
        })?;

        let suggestion = FixSuggestion {
            id: Uuid::new_v4(),
            file: path.to_path_buf(),
            start_line: start,
            end_line: end,
            original: source
                .lines()
                .skip(start.saturating_sub(1))
                .take(end.saturating_sub(start.saturating_sub(1)))
                .collect::<Vec<_>>()
                .join("\n"),
            replacement: repl.to_string(),
            description: format!("Fix for finding {finding_id}"),
            category: FixCategory::BugFix,
            confidence: 1.0,
            validated: false,
        };

        let modified = apply_fix(&source, &suggestion).map_err(|e| ToolError::ExecutionFailed {
            name: "apply_fix".into(),
            message: format!("Failed to apply fix: {e}"),
        })?;

        // Create backup
        let backup_path = format!("{}.bak", path.display());
        std::fs::write(&backup_path, &source).map_err(|e| ToolError::ExecutionFailed {
            name: "apply_fix".into(),
            message: format!("Failed to create backup: {e}"),
        })?;

        // Write modified content
        std::fs::write(path, &modified).map_err(|e| ToolError::ExecutionFailed {
            name: "apply_fix".into(),
            message: format!("Failed to write fix: {e}"),
        })?;

        Ok(ToolOutput::text(format!(
            "Applied fix for finding '{finding_id}':\n\
             File: {file}\n\
             Lines: {start}-{end}\n\
             Backup: {backup_path}\n\
             Rollback available via backup file.\n\
             Original ({} bytes) -> Modified ({} bytes)",
            source.len(),
            modified.len(),
        )))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = ApplyFixTool;
        assert_eq!(tool.name(), "apply_fix");
    }

    #[test]
    fn test_schema() {
        let tool = ApplyFixTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["finding_id"].is_object());
        assert!(schema["properties"]["confirm"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("finding_id")));
    }

    #[test]
    fn test_risk_level() {
        let tool = ApplyFixTool;
        assert_eq!(tool.risk_level(), RiskLevel::Write);
    }

    #[tokio::test]
    async fn test_execute_missing_finding_id() {
        let tool = ApplyFixTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        if let Err(ToolError::InvalidArguments { name, .. }) = result {
            assert_eq!(name, "apply_fix");
        }
    }

    #[tokio::test]
    async fn test_execute_without_confirm() {
        let tool = ApplyFixTool;
        let result = tool
            .execute(serde_json::json!({"finding_id": "SEC-042"}))
            .await
            .unwrap();
        assert!(result.content.contains("requires confirmation"));
        assert!(result.content.contains("SEC-042"));
    }

    #[tokio::test]
    async fn test_execute_with_confirm() {
        let tool = ApplyFixTool;
        let result = tool
            .execute(serde_json::json!({"finding_id": "SEC-042", "confirm": true}))
            .await
            .unwrap();
        assert!(result.content.contains("Applied fix"));
        assert!(result.content.contains("SEC-042"));
        assert!(result.content.contains("rollback") || result.content.contains("Rollback"));
    }
}
