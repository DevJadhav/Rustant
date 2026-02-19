//! Duplicate Detect — Find code duplication using AST fingerprinting.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::duplication::{DuplicationConfig, DuplicationDetector};

/// Find code duplication using AST fingerprinting. Detects exact clones,
/// near-miss clones (renamed variables), and structural clones (same AST shape).
pub struct DuplicateDetectTool;

/// Collect source files from a path, returning (content, path) pairs.
fn collect_source_files(base: &Path, max_files: usize) -> Vec<(String, std::path::PathBuf)> {
    let mut files = Vec::new();

    if base.is_file() {
        if let Ok(content) = std::fs::read_to_string(base)
            && Language::from_path(base) != Language::Unknown
        {
            files.push((content, base.to_path_buf()));
        }
        return files;
    }

    for entry in WalkDir::new(base)
        .max_depth(10)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if files.len() >= max_files {
            break;
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let lang = Language::from_path(path);
        if lang == Language::Unknown {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            files.push((content, path.to_path_buf()));
        }
    }

    files
}

#[async_trait]
impl Tool for DuplicateDetectTool {
    fn name(&self) -> &str {
        "duplicate_detect"
    }

    fn description(&self) -> &str {
        "Find code duplication using AST fingerprinting"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to analyze"
                },
                "min_lines": {
                    "type": "integer",
                    "description": "Minimum lines for a duplicate block (default: 6)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let min_lines = args.get("min_lines").and_then(|v| v.as_i64()).unwrap_or(6);

        if min_lines < 2 {
            return Err(ToolError::InvalidArguments {
                name: "duplicate_detect".into(),
                reason: "min_lines must be at least 2".into(),
            });
        }
        let min_lines = min_lines as usize;

        let base = Path::new(path);
        let source_files = collect_source_files(base, 500);
        if source_files.is_empty() {
            return Ok(ToolOutput::text(format!(
                "Duplicate detection for '{path}' (min_lines: {min_lines}): \
                 no source files found to analyze."
            )));
        }

        let config = DuplicationConfig {
            min_lines,
            min_tokens: 50,
            window_size: min_lines,
        };
        let detector = DuplicationDetector::new(config);

        let file_pairs: Vec<(&str, &Path)> = source_files
            .iter()
            .map(|(content, p)| (content.as_str(), p.as_path()))
            .collect();
        let report = detector.analyze_files(&file_pairs);

        let mut output = format!(
            "Duplicate detection for '{path}' (min_lines: {min_lines}):\n\
             Total lines analyzed: {}\n\
             Duplicated lines: {}\n\
             Duplication percentage: {:.1}%\n\
             Duplicate groups found: {}\n",
            report.total_lines,
            report.duplicated_lines,
            report.duplication_percentage,
            report.duplications.len(),
        );

        if !report.duplications.is_empty() {
            output.push_str(&format!(
                "\nDuplicate groups (top {}):\n",
                report.duplications.len().min(30)
            ));
            for (i, dup) in report.duplications.iter().take(30).enumerate() {
                output.push_str(&format!(
                    "\n  Group {} — {} lines, similarity: {:.0}%, {} duplicate(s):\n",
                    i + 1,
                    dup.line_count,
                    dup.similarity * 100.0,
                    dup.duplicates.len(),
                ));
                output.push_str(&format!(
                    "    Original: {}:{}-{}\n",
                    dup.original.file.display(),
                    dup.original.start_line,
                    dup.original.end_line,
                ));
                for d in &dup.duplicates {
                    output.push_str(&format!(
                        "    Duplicate: {}:{}-{}\n",
                        d.file.display(),
                        d.start_line,
                        d.end_line,
                    ));
                }
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
        let tool = DuplicateDetectTool;
        assert_eq!(tool.name(), "duplicate_detect");
    }

    #[test]
    fn test_schema() {
        let tool = DuplicateDetectTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["min_lines"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = DuplicateDetectTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = DuplicateDetectTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("min_lines: 6"));
    }

    #[tokio::test]
    async fn test_execute_custom_min_lines() {
        let tool = DuplicateDetectTool;
        let result = tool
            .execute(serde_json::json!({"path": "src/", "min_lines": 10}))
            .await
            .unwrap();
        assert!(result.content.contains("src/"));
        assert!(result.content.contains("min_lines: 10"));
    }

    #[tokio::test]
    async fn test_execute_invalid_min_lines() {
        let tool = DuplicateDetectTool;
        let result = tool.execute(serde_json::json!({"min_lines": 1})).await;
        assert!(result.is_err());
        if let Err(ToolError::InvalidArguments { name, reason }) = result {
            assert_eq!(name, "duplicate_detect");
            assert!(reason.contains("at least 2"));
        }
    }
}
