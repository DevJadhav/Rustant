//! Complexity Check — Analyze cyclomatic complexity of code.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::quality::QualityScorer;

/// Analyze cyclomatic complexity of code at the function, file, and module
/// level. Flags functions exceeding the configured threshold.
pub struct ComplexityCheckTool;

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
impl Tool for ComplexityCheckTool {
    fn name(&self) -> &str {
        "complexity_check"
    }

    fn description(&self) -> &str {
        "Analyze cyclomatic complexity of code"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "threshold": {
                    "type": "integer",
                    "description": "Complexity threshold (default: 10)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let threshold = args.get("threshold").and_then(|v| v.as_i64()).unwrap_or(10);

        if threshold < 1 {
            return Err(ToolError::InvalidArguments {
                name: "complexity_check".into(),
                reason: "threshold must be a positive integer".into(),
            });
        }
        let threshold = threshold as u32;

        let base = Path::new(path);
        let source_files = collect_source_files(base, 500);
        if source_files.is_empty() {
            return Ok(ToolOutput::text(format!(
                "Complexity check for '{path}' (threshold: {threshold}): \
                 no source files found to analyze."
            )));
        }

        let scorer = QualityScorer::new();

        // Collect all functions with their file context
        let mut all_functions: Vec<(String, String, u32, usize, String)> = Vec::new(); // (file, name, complexity, line, grade)
        let mut total_functions = 0usize;
        let mut above_threshold = 0usize;
        let mut total_complexity = 0u64;

        for (content, file_path) in &source_files {
            let fq = scorer.analyze_file(content, file_path);
            for func in &fq.functions {
                total_functions += 1;
                total_complexity += func.complexity as u64;
                if func.complexity > threshold {
                    above_threshold += 1;
                }
                all_functions.push((
                    file_path.display().to_string(),
                    func.name.clone(),
                    func.complexity,
                    func.start_line,
                    func.grade.to_string(),
                ));
            }
        }

        // Sort by complexity descending
        all_functions.sort_by(|a, b| b.2.cmp(&a.2));

        let avg_complexity = if total_functions > 0 {
            total_complexity as f64 / total_functions as f64
        } else {
            0.0
        };

        let mut output = format!(
            "Complexity check for '{path}' (threshold: {threshold}):\n\
             Total functions analyzed: {total_functions}\n\
             Functions above threshold: {above_threshold}\n\
             Average cyclomatic complexity: {avg_complexity:.1}\n",
        );

        // List functions above threshold
        let hotspots: Vec<_> = all_functions
            .iter()
            .filter(|(_, _, c, _, _)| *c > threshold)
            .collect();

        if !hotspots.is_empty() {
            output.push_str(&format!("\nHotspots (complexity > {threshold}):\n"));
            for (file, name, complexity, line, grade) in hotspots.iter().take(50) {
                output.push_str(&format!(
                    "  {file}:{line} — {name} (complexity: {complexity}, grade: {grade})\n",
                ));
            }
        }

        // Top 20 most complex functions regardless of threshold
        output.push_str("\nTop functions by complexity:\n");
        for (file, name, complexity, line, grade) in all_functions.iter().take(20) {
            output.push_str(&format!(
                "  {file}:{line} — {name} (complexity: {complexity}, grade: {grade})\n",
            ));
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
        let tool = ComplexityCheckTool;
        assert_eq!(tool.name(), "complexity_check");
    }

    #[test]
    fn test_schema() {
        let tool = ComplexityCheckTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["threshold"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = ComplexityCheckTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = ComplexityCheckTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("threshold: 10"));
        assert!(result.content.contains("complexity"));
    }

    #[tokio::test]
    async fn test_execute_custom_threshold() {
        let tool = ComplexityCheckTool;
        let result = tool
            .execute(serde_json::json!({"path": "src/agent.rs", "threshold": 15}))
            .await
            .unwrap();
        assert!(result.content.contains("src/agent.rs"));
        assert!(result.content.contains("threshold: 15"));
    }

    #[tokio::test]
    async fn test_execute_invalid_threshold() {
        let tool = ComplexityCheckTool;
        let result = tool.execute(serde_json::json!({"threshold": 0})).await;
        assert!(result.is_err());
        if let Err(ToolError::InvalidArguments { name, reason }) = result {
            assert_eq!(name, "complexity_check");
            assert!(reason.contains("positive"));
        }
    }
}
