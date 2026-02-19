//! Quality Score — Calculate code quality metrics with A-F grades.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::quality::QualityScorer;

/// Calculate code quality metrics and assign A-F letter grades. Analyzes
/// complexity, duplication, test coverage, documentation, and maintainability.
pub struct QualityScoreTool;

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
impl Tool for QualityScoreTool {
    fn name(&self) -> &str {
        "quality_score"
    }

    fn description(&self) -> &str {
        "Calculate code quality metrics with A-F grades"
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
                "Quality score for '{path}': no source files found to analyze."
            )));
        }

        let scorer = QualityScorer::new();
        let file_pairs: Vec<(&str, &Path)> = source_files
            .iter()
            .map(|(content, p)| (content.as_str(), p.as_path()))
            .collect();
        let report = scorer.analyze_files(&file_pairs);

        let mut output = format!(
            "Quality score for '{path}':\n\
             Overall grade: {}\n\
             Complexity grade: {} (avg: {:.1})\n\
             Documentation grade: {} (coverage: {:.0}%)\n\
             Total files: {}\n\
             Total functions: {}\n\
             Total LOC: {}\n",
            report.overall_grade,
            report.complexity_grade,
            report.avg_complexity,
            report.doc_grade,
            report.avg_doc_coverage * 100.0,
            report.files.len(),
            report.total_functions,
            report.total_loc,
        );

        // High complexity hotspots
        if !report.high_complexity_functions.is_empty() {
            output.push_str(&format!(
                "\nHigh complexity functions ({}):\n",
                report.high_complexity_functions.len()
            ));
            for func in report.high_complexity_functions.iter().take(20) {
                output.push_str(&format!(
                    "  - {} (complexity: {}, grade: {}, line: {})\n",
                    func.name, func.complexity, func.grade, func.start_line,
                ));
            }
        }

        // Per-file breakdown (top files by worst grade)
        let mut worst_files: Vec<_> = report.files.iter().collect();
        worst_files.sort_by(|a, b| b.overall_grade.cmp(&a.overall_grade));
        if worst_files.len() > 10 {
            output.push_str(&format!("\nWorst 10 files (of {}):\n", worst_files.len()));
            for fq in worst_files.iter().rev().take(10) {
                output.push_str(&format!(
                    "  {} — grade: {}, complexity: {:.1}, docs: {:.0}%, loc: {}\n",
                    fq.path,
                    fq.overall_grade,
                    fq.avg_complexity,
                    fq.doc_coverage * 100.0,
                    fq.loc,
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
        let tool = QualityScoreTool;
        assert_eq!(tool.name(), "quality_score");
    }

    #[test]
    fn test_schema() {
        let tool = QualityScoreTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = QualityScoreTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_default_path() {
        let tool = QualityScoreTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        // Should produce real output or "no source files found"
        assert!(
            result.content.contains("Quality score for '.'")
                || result.content.contains("quality_score")
        );
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = QualityScoreTool;
        let result = tool
            .execute(serde_json::json!({"path": "src/"}))
            .await
            .unwrap();
        assert!(result.content.contains("src/"));
    }
}
