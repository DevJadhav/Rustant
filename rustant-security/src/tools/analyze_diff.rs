//! Analyze Diff — Parse and classify code changes in a diff.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::review::diff::DiffAnalyzer;

/// Parse and classify code changes in a diff, identifying additions,
/// deletions, modifications, and their categories (logic, style, deps, tests, etc.).
pub struct AnalyzeDiffTool;

#[async_trait]
impl Tool for AnalyzeDiffTool {
    fn name(&self) -> &str {
        "analyze_diff"
    }

    fn description(&self) -> &str {
        "Parse and classify code changes in a diff"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File or directory path"
                },
                "base": {
                    "type": "string",
                    "description": "Base ref (default: HEAD~1)"
                },
                "head": {
                    "type": "string",
                    "description": "Head ref (default: HEAD)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let base = args
            .get("base")
            .and_then(|v| v.as_str())
            .unwrap_or("HEAD~1");
        let head = args.get("head").and_then(|v| v.as_str()).unwrap_or("HEAD");

        let repo_path = Path::new(path);
        let analyzer = DiffAnalyzer::new();

        // Try git diff analysis first
        let analysis = match analyzer.analyze_git_diff(repo_path, base, head) {
            Ok(result) => result,
            Err(_) => {
                // If the path is inside a git repo, try from the current dir
                match analyzer.analyze_git_diff(Path::new("."), base, head) {
                    Ok(result) => result,
                    Err(e) => {
                        return Ok(ToolOutput::text(format!(
                            "Diff analysis for '{path}' ({base}..{head}):\n\
                             Could not analyze git diff: {e}\n\
                             Ensure the path is inside a git repository and refs are valid."
                        )));
                    }
                }
            }
        };

        let mut output = format!(
            "Diff analysis for '{path}' ({base}..{head}):\n\
             Files changed: {}\n\
             Total additions: +{}\n\
             Total deletions: -{}\n",
            analysis.files_changed, analysis.total_additions, analysis.total_deletions,
        );

        // Change kind summary
        if !analysis.kind_summary.is_empty() {
            output.push_str("\nChange categories:\n");
            for (kind, count) in &analysis.kind_summary {
                output.push_str(&format!("  {kind}: {count} file(s)\n"));
            }
        }

        // Per-file details
        if !analysis.files.is_empty() {
            output.push_str("\nChanged files:\n");
            for file in &analysis.files {
                output.push_str(&format!(
                    "  {} — {} (+{}, -{}, {} hunk(s), lang: {})\n",
                    file.path.display(),
                    file.change_kind,
                    file.additions,
                    file.deletions,
                    file.hunks.len(),
                    file.language,
                ));
            }
        }

        if analysis.has_structural_changes() {
            output.push_str(
                "\nNote: structural changes detected (function/type definitions modified).\n",
            );
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
        let tool = AnalyzeDiffTool;
        assert_eq!(tool.name(), "analyze_diff");
    }

    #[test]
    fn test_schema() {
        let tool = AnalyzeDiffTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["base"].is_object());
        assert!(schema["properties"]["head"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = AnalyzeDiffTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = AnalyzeDiffTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("HEAD~1"));
        assert!(result.content.contains("HEAD"));
        assert!(result.content.contains("Diff analysis"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = AnalyzeDiffTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "src/main.rs",
                "base": "v1.0",
                "head": "v2.0"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("src/main.rs"));
        assert!(result.content.contains("v1.0"));
        assert!(result.content.contains("v2.0"));
    }
}
