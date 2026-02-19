//! Code Review — Generate static code review comments from analysis engines.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::comments::{CommentCategory, ReviewCommentBuilder, ReviewResult};
use crate::review::quality::QualityScorer;
use crate::review::tech_debt::TechDebtScanner;

/// Generate code review comments based on static analysis engines.
/// Reviews code for complexity, style, documentation, and technical debt.
pub struct CodeReviewTool;

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
impl Tool for CodeReviewTool {
    fn name(&self) -> &str {
        "review_code"
    }

    fn description(&self) -> &str {
        "Generate static analysis code review comments"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File or directory to review"
                },
                "diff": {
                    "type": "string",
                    "description": "Diff spec (default: HEAD~1)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let diff = args
            .get("diff")
            .and_then(|v| v.as_str())
            .unwrap_or("HEAD~1");
        let base = Path::new(path);

        let source_files = collect_source_files(base, 200);
        if source_files.is_empty() {
            return Ok(ToolOutput::text(format!(
                "Code review for '{path}' (diff: {diff}): no source files found to review."
            )));
        }

        let scorer = QualityScorer::new();
        let debt_scanner = TechDebtScanner::new();
        let mut comments = Vec::new();

        for (content, file_path) in &source_files {
            let file_str = file_path.display().to_string();

            // Quality analysis -> review comments for complexity
            let fq = scorer.analyze_file(content, file_path);
            for func in &fq.functions {
                if func.complexity > 15 {
                    let comment = ReviewCommentBuilder::new(
                        &file_str,
                        func.start_line,
                        CommentCategory::Maintainability,
                    )
                    .severity(if func.complexity > 25 { 4 } else { 3 })
                    .body(&format!(
                        "Function '{}' has high cyclomatic complexity ({}, grade: {}). \
                         Consider splitting into smaller functions.",
                        func.name, func.complexity, func.grade
                    ))
                    .confidence(0.9)
                    .reason("Cyclomatic complexity exceeds threshold of 15")
                    .build();
                    comments.push(comment);
                }
            }

            // Documentation gaps
            if fq.doc_coverage < 0.5 && !fq.functions.is_empty() {
                let comment = ReviewCommentBuilder::new(
                    &file_str,
                    1,
                    CommentCategory::Documentation,
                )
                .severity(2)
                .body(&format!(
                    "Low documentation coverage ({:.0}%). Consider documenting public functions.",
                    fq.doc_coverage * 100.0
                ))
                .confidence(0.8)
                .reason("Documentation coverage below 50%")
                .build();
                comments.push(comment);
            }

            // Tech debt markers -> review comments
            let debt_items = debt_scanner.scan_file(content, file_path);
            for item in &debt_items {
                let (category, severity) = match item.category {
                    crate::review::tech_debt::DebtCategory::MarkerComment => {
                        (CommentCategory::Maintainability, 2)
                    }
                    crate::review::tech_debt::DebtCategory::LongFunction => {
                        (CommentCategory::Maintainability, 3)
                    }
                    crate::review::tech_debt::DebtCategory::LargeFile => {
                        (CommentCategory::Maintainability, 2)
                    }
                    crate::review::tech_debt::DebtCategory::DeprecatedUsage => {
                        (CommentCategory::Bug, 3)
                    }
                    _ => (CommentCategory::Style, 2),
                };

                let comment = ReviewCommentBuilder::new(&file_str, item.line, category)
                    .severity(severity)
                    .body(&item.description)
                    .confidence(0.7)
                    .reason("Detected by tech debt scanner")
                    .build();
                comments.push(comment);
            }
        }

        let review = ReviewResult::from_comments(comments, format!("Static review of {path}"));

        let mut output = format!(
            "Code review for '{path}' (diff: {diff}):\n\
             Total comments: {}\n\
             Critical/high severity: {}\n\
             Should block: {}\n",
            review.comments.len(),
            review.critical_count,
            review.should_block,
        );

        if !review.comments.is_empty() {
            // Sort by severity descending
            let sorted = review.comments_by_severity();
            output.push_str("\nReview comments:\n");
            for comment in sorted.iter().take(50) {
                output.push_str(&format!(
                    "\n  [{}] {} (severity: {}, confidence: {:.0}%)\n",
                    comment.category,
                    comment.file_path,
                    comment.severity,
                    comment.confidence * 100.0,
                ));
                output.push_str(&format!("  Line {}: {}\n", comment.line, comment.body));
                if let Some(ref suggestion) = comment.suggestion {
                    output.push_str(&format!("  Suggestion: {suggestion}\n"));
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
        let tool = CodeReviewTool;
        assert_eq!(tool.name(), "review_code");
    }

    #[test]
    fn test_schema() {
        let tool = CodeReviewTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["diff"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = CodeReviewTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = CodeReviewTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("HEAD~1"));
        assert!(result.content.contains("Code review"));
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = CodeReviewTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "src/lib.rs",
                "diff": "main..feature"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("src/lib.rs"));
        assert!(result.content.contains("main..feature"));
    }
}
