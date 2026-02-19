//! Tech Debt Report — Generate technical debt analysis report.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;
use walkdir::WalkDir;

use crate::ast::Language;
use crate::review::tech_debt::{TechDebtReport, TechDebtScanner};

/// Generate a comprehensive technical debt analysis report. Aggregates
/// findings from complexity, duplication, dead code, and quality analyses
/// into a prioritized debt inventory with remediation cost estimates.
pub struct TechDebtReportTool;

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
impl Tool for TechDebtReportTool {
    fn name(&self) -> &str {
        "tech_debt_report"
    }

    fn description(&self) -> &str {
        "Generate technical debt analysis report"
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
                "Technical debt report for '{path}': no source files found to analyze."
            )));
        }

        let scanner = TechDebtScanner::new();
        let mut all_indicators = Vec::new();

        for (content, file_path) in &source_files {
            let indicators = scanner.scan_file(content, file_path);
            all_indicators.extend(indicators);
        }

        let report = TechDebtReport::from_indicators(all_indicators);

        let mut output = format!(
            "Technical debt report for '{path}':\n\
             Total debt score: {}\n\
             Total indicators: {}\n\
             Files scanned: {}\n",
            report.total_score,
            report.indicators.len(),
            source_files.len(),
        );

        // Category breakdown
        if !report.category_counts.is_empty() {
            output.push_str("\nDebt by category:\n");
            for (category, count) in &report.category_counts {
                output.push_str(&format!("  {category}: {count}\n"));
            }
        }

        // Hotspots
        if !report.hotspots.is_empty() {
            output.push_str(&format!(
                "\nHotspot files (top {}):\n",
                report.hotspots.len().min(15)
            ));
            for (file, score) in report.hotspots.iter().take(15) {
                output.push_str(&format!("  {} (score: {score})\n", file.display(),));
            }
        }

        // Individual items (top by weight)
        if !report.indicators.is_empty() {
            let mut sorted = report.indicators.clone();
            sorted.sort_by(|a, b| b.weight.cmp(&a.weight));

            output.push_str(&format!("\nTop debt items (of {}):\n", sorted.len()));
            for item in sorted.iter().take(30) {
                output.push_str(&format!(
                    "  {}:{} — {} [{}] (weight: {})",
                    item.file.display(),
                    item.line,
                    item.description,
                    item.category,
                    item.weight,
                ));
                if let Some(ref text) = item.source_text {
                    let preview: String = text.chars().take(80).collect();
                    output.push_str(&format!(" — {preview}"));
                }
                output.push('\n');
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
        let tool = TechDebtReportTool;
        assert_eq!(tool.name(), "tech_debt_report");
    }

    #[test]
    fn test_schema() {
        let tool = TechDebtReportTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = TechDebtReportTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_default_path() {
        let tool = TechDebtReportTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("Technical debt report for '.'"));
    }

    #[tokio::test]
    async fn test_execute_with_path() {
        let tool = TechDebtReportTool;
        let result = tool
            .execute(serde_json::json!({"path": "rustant-core/"}))
            .await
            .unwrap();
        assert!(result.content.contains("rustant-core/"));
    }
}
