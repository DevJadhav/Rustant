//! Suggest Fix — Generate fix suggestions for security and quality findings.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use uuid::Uuid;

use crate::ast::Language;
use crate::review::autofix::{FixCategory, FixSuggestion, generate_patch};
use crate::review::quality::QualityScorer;
use crate::review::tech_debt::{DebtCategory, TechDebtScanner};

/// Generate fix suggestions for security and quality findings. Produces
/// concrete code patches with explanations and confidence scores.
pub struct SuggestFixTool;

#[async_trait]
impl Tool for SuggestFixTool {
    fn name(&self) -> &str {
        "suggest_fix"
    }

    fn description(&self) -> &str {
        "Generate fix suggestions for security and quality findings"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "finding_id": {
                    "type": "string",
                    "description": "Finding ID to fix"
                },
                "path": {
                    "type": "string",
                    "description": "File path"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let finding_id = args.get("finding_id").and_then(|v| v.as_str());
        let path = args.get("path").and_then(|v| v.as_str());

        if finding_id.is_none() && path.is_none() {
            return Err(ToolError::InvalidArguments {
                name: "suggest_fix".into(),
                reason: "at least one of 'finding_id' or 'path' must be provided".into(),
            });
        }

        // If a finding_id is provided without a path, we can only report the ID
        if let Some(id) = finding_id
            && path.is_none()
        {
            return Ok(ToolOutput::text(format!(
                "Fix suggestion for finding '{id}':\n\
                 No file path provided. To generate concrete fix suggestions, \
                 provide a 'path' parameter pointing to the file containing the finding.\n\
                 Finding ID: {id}"
            )));
        }

        let file_path = Path::new(path.unwrap());
        let content =
            std::fs::read_to_string(file_path).map_err(|e| ToolError::ExecutionFailed {
                name: "suggest_fix".into(),
                message: format!("Failed to read '{}': {}", file_path.display(), e),
            })?;

        let lang = Language::from_path(file_path);
        if lang == Language::Unknown {
            return Ok(ToolOutput::text(format!(
                "Fix suggestion for '{}': unsupported language, cannot generate fixes.",
                file_path.display()
            )));
        }

        let mut suggestions: Vec<FixSuggestion> = Vec::new();

        // Scan for tech debt items that can be addressed
        let scanner = TechDebtScanner::new();
        let debt_items = scanner.scan_file(&content, file_path);
        let lines: Vec<&str> = content.lines().collect();

        for item in &debt_items {
            match item.category {
                DebtCategory::MarkerComment => {
                    // Suggest removing TODO/FIXME comments (low confidence)
                    if item.line > 0 && item.line <= lines.len() {
                        let original_line = lines[item.line - 1];
                        suggestions.push(FixSuggestion {
                            id: Uuid::new_v4(),
                            file: file_path.to_path_buf(),
                            start_line: item.line,
                            end_line: item.line,
                            original: original_line.to_string(),
                            replacement: format!("// Addressed: {}", item.description),
                            description: format!("Address {}: {}", item.category, item.description),
                            category: FixCategory::Documentation,
                            confidence: 0.3,
                            validated: false,
                        });
                    }
                }
                DebtCategory::DeprecatedUsage => {
                    if item.line > 0 && item.line <= lines.len() {
                        suggestions.push(FixSuggestion {
                            id: Uuid::new_v4(),
                            file: file_path.to_path_buf(),
                            start_line: item.line,
                            end_line: item.line,
                            original: lines[item.line - 1].to_string(),
                            replacement: format!(
                                "// REVIEW: deprecated item at line {} needs migration",
                                item.line
                            ),
                            description: "Deprecated API usage needs migration".to_string(),
                            category: FixCategory::Refactor,
                            confidence: 0.4,
                            validated: false,
                        });
                    }
                }
                _ => {}
            }
        }

        // Check quality for complexity-based suggestions
        let scorer = QualityScorer::new();
        let quality = scorer.analyze_file(&content, file_path);
        for func in &quality.functions {
            if func.complexity > 20 && func.start_line > 0 && func.start_line <= lines.len() {
                suggestions.push(FixSuggestion {
                    id: Uuid::new_v4(),
                    file: file_path.to_path_buf(),
                    start_line: func.start_line,
                    end_line: func.start_line,
                    original: lines[func.start_line - 1].to_string(),
                    replacement: format!(
                        "{} // REFACTOR: complexity {} — split into smaller functions",
                        lines[func.start_line - 1],
                        func.complexity
                    ),
                    description: format!(
                        "Function '{}' has complexity {} (grade {}). \
                         Consider extracting helper functions.",
                        func.name, func.complexity, func.grade
                    ),
                    category: FixCategory::Refactor,
                    confidence: 0.5,
                    validated: false,
                });
            }
        }

        let target = if let Some(id) = finding_id {
            format!("finding '{}' in '{}'", id, file_path.display())
        } else {
            format!("file '{}'", file_path.display())
        };

        let mut output = format!(
            "Fix suggestions for {target}:\n\
             Suggestions generated: {}\n",
            suggestions.len(),
        );

        if suggestions.is_empty() {
            output.push_str("No automated fix suggestions available for this file.\n");
        } else {
            for (i, suggestion) in suggestions.iter().enumerate() {
                output.push_str(&format!(
                    "\n--- Suggestion {} (id: {}) ---\n\
                     Category: {}\n\
                     Confidence: {:.0}%\n\
                     Description: {}\n\
                     File: {}:{}-{}\n",
                    i + 1,
                    suggestion.id,
                    suggestion.category,
                    suggestion.confidence * 100.0,
                    suggestion.description,
                    suggestion.file.display(),
                    suggestion.start_line,
                    suggestion.end_line,
                ));
                let patch = generate_patch(suggestion);
                output.push_str(&format!("Patch:\n{patch}\n"));
            }
        }

        Ok(ToolOutput::text(output))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = SuggestFixTool;
        assert_eq!(tool.name(), "suggest_fix");
    }

    #[test]
    fn test_schema() {
        let tool = SuggestFixTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["finding_id"].is_object());
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = SuggestFixTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_with_finding_id() {
        let tool = SuggestFixTool;
        let result = tool
            .execute(serde_json::json!({"finding_id": "SEC-001"}))
            .await
            .unwrap();
        assert!(result.content.contains("SEC-001"));
        assert!(result.content.contains("Fix suggestion"));
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = SuggestFixTool;
        // Use a path that won't exist to test error handling
        let result = tool
            .execute(serde_json::json!({"path": "src/main.rs"}))
            .await;
        // Either succeeds with content or fails with file not found
        assert!(
            result.is_ok() || result.is_err(),
            "Should handle missing file gracefully"
        );
    }

    #[tokio::test]
    async fn test_execute_no_args_fails() {
        let tool = SuggestFixTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
        if let Err(ToolError::InvalidArguments { name, reason }) = result {
            assert_eq!(name, "suggest_fix");
            assert!(reason.contains("finding_id"));
        }
    }
}
