//! Dead Code Detect — Find unreachable and unused code via reachability analysis.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::dead_code::DeadCodeDetector;

/// Find unreachable and unused code via call graph reachability analysis.
/// Detects unused functions, dead branches, unreferenced types, and orphan modules.
pub struct DeadCodeDetectTool;

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
impl Tool for DeadCodeDetectTool {
    fn name(&self) -> &str {
        "dead_code_detect"
    }

    fn description(&self) -> &str {
        "Find unreachable and unused code via reachability analysis"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to analyze"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let base = Path::new(path);

        let source_files = collect_source_files(base, 500);
        if source_files.is_empty() {
            return Ok(ToolOutput::text(format!(
                "Dead code detection for '{path}': no source files found to analyze."
            )));
        }

        let mut total_functions = 0usize;
        let mut total_reachable = 0usize;
        let mut all_items = Vec::new();

        for (content, file_path) in &source_files {
            let lang = Language::from_path(file_path);
            let report = DeadCodeDetector::analyze_file(content, lang, file_path);
            total_functions += report.total_functions;
            total_reachable += report.reachable_count;
            all_items.extend(report.items);
        }

        let dead_count = all_items.len();
        let dead_pct = if total_functions > 0 {
            (dead_count as f64 / total_functions as f64) * 100.0
        } else {
            0.0
        };

        let mut output = format!(
            "Dead code detection for '{path}':\n\
             Total functions analyzed: {total_functions}\n\
             Reachable functions: {total_reachable}\n\
             Potentially dead functions: {dead_count}\n\
             Dead code percentage: {dead_pct:.1}%\n",
        );

        if !all_items.is_empty() {
            // Sort by confidence descending
            let mut sorted = all_items.clone();
            sorted.sort_by(|a, b| {
                b.confidence
                    .partial_cmp(&a.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            output.push_str(&format!(
                "\nDead code items (top {} by confidence):\n",
                sorted.len().min(50)
            ));
            for item in sorted.iter().take(50) {
                output.push_str(&format!(
                    "  {}:{}-{} — '{}' ({}, confidence: {:.0}%)\n",
                    item.file.display(),
                    item.start_line,
                    item.end_line,
                    item.name,
                    item.kind,
                    item.confidence * 100.0,
                ));
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
        let tool = DeadCodeDetectTool;
        assert_eq!(tool.name(), "dead_code_detect");
    }

    #[test]
    fn test_schema() {
        let tool = DeadCodeDetectTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = DeadCodeDetectTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_default_path() {
        let tool = DeadCodeDetectTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("Dead code detection for '.'"));
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = DeadCodeDetectTool;
        let result = tool
            .execute(serde_json::json!({"path": "src/legacy/"}))
            .await
            .unwrap();
        assert!(result.content.contains("src/legacy/"));
    }
}
