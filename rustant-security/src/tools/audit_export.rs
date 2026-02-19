//! Audit Export — Export audit trail for compliance evidence.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::finding::Finding;
use crate::report::sarif;

/// Export audit trail records for compliance evidence collection.
/// Supports filtering by date range and exporting in JSON, CSV, or SARIF
/// format for integration with GRC platforms.
pub struct AuditExportTool;

/// Load findings from the security state directory (if any).
fn load_persisted_findings(workspace: &Path) -> Vec<Finding> {
    let state_dir = workspace.join(".rustant").join("security");
    let findings_file = state_dir.join("findings.json");

    if let Ok(content) = std::fs::read_to_string(&findings_file)
        && let Ok(findings) = serde_json::from_str::<Vec<Finding>>(&content)
    {
        return findings;
    }

    // Return empty if no persisted findings
    Vec::new()
}

/// Filter findings by date range (ISO 8601 strings).
fn filter_by_date_range(
    findings: Vec<Finding>,
    start: Option<&str>,
    end: Option<&str>,
) -> Vec<Finding> {
    use chrono::Utc;

    let start_dt = start.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });
    let end_dt = end.and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });

    findings
        .into_iter()
        .filter(|f| {
            if let Some(ref start) = start_dt
                && f.created_at < *start
            {
                return false;
            }
            if let Some(ref end) = end_dt
                && f.created_at > *end
            {
                return false;
            }
            true
        })
        .collect()
}

/// Format findings as CSV.
fn findings_to_csv(findings: &[Finding]) -> String {
    let mut csv = String::from("id,title,severity,category,scanner,status,created_at,location\n");
    for f in findings {
        let location = f
            .location
            .as_ref()
            .map(|l| format!("{}:{}", l.file.display(), l.start_line))
            .unwrap_or_else(|| "-".to_string());
        csv.push_str(&format!(
            "{},{},{},{:?},{},{:?},{},{}\n",
            f.id,
            escape_csv(&f.title),
            f.severity,
            f.category,
            f.provenance.scanner,
            f.status,
            f.created_at.to_rfc3339(),
            location,
        ));
    }
    csv
}

/// Simple CSV field escaping.
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[async_trait]
impl Tool for AuditExportTool {
    fn name(&self) -> &str {
        "audit_export"
    }

    fn description(&self) -> &str {
        "Export audit trail for compliance evidence"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "start": {
                    "type": "string",
                    "description": "Start date (ISO 8601)"
                },
                "end": {
                    "type": "string",
                    "description": "End date (ISO 8601)"
                },
                "format": {
                    "type": "string",
                    "description": "Export format (json/csv/sarif)"
                },
                "path": {
                    "type": "string",
                    "description": "Workspace path to load findings from"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = args.get("start").and_then(|v| v.as_str());
        let end = args.get("end").and_then(|v| v.as_str());
        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("json");
        let workspace_path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let start_display = start.unwrap_or("(all time)");
        let end_display = end.unwrap_or("(now)");

        // Load persisted findings
        let workspace = Path::new(workspace_path);
        let all_findings = load_persisted_findings(workspace);

        // Apply date filter
        let findings = filter_by_date_range(all_findings, start, end);

        let mut header =
            format!("Audit export (range: {start_display} to {end_display}, format: {format}):\n");
        header.push_str(&format!("Records: {}\n\n", findings.len()));

        if findings.is_empty() {
            header.push_str(
                "No audit records found for the specified range.\n\
                 Findings are persisted to .rustant/security/findings.json during security scans.",
            );
            return Ok(ToolOutput::text(header));
        }

        let body = match format {
            "csv" => findings_to_csv(&findings),
            "sarif" => {
                let sarif_log = sarif::findings_to_sarif(
                    &findings,
                    "rustant-security",
                    env!("CARGO_PKG_VERSION"),
                );
                sarif::to_json(&sarif_log).map_err(|e| ToolError::ExecutionFailed {
                    name: "audit_export".to_string(),
                    message: format!("SARIF serialization failed: {e}"),
                })?
            }
            _ => {
                // Default: JSON
                serde_json::to_string_pretty(&findings).map_err(|e| ToolError::ExecutionFailed {
                    name: "audit_export".to_string(),
                    message: format!("JSON serialization failed: {e}"),
                })?
            }
        };

        Ok(ToolOutput::text(format!("{header}{body}")))
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{FindingCategory, FindingProvenance, FindingSeverity};

    #[test]
    fn test_tool_name() {
        let tool = AuditExportTool;
        assert_eq!(tool.name(), "audit_export");
    }

    #[test]
    fn test_schema() {
        let tool = AuditExportTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["start"].is_object());
        assert!(schema["properties"]["end"].is_object());
        assert!(schema["properties"]["format"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = AuditExportTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = AuditExportTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(result.content.contains("(all time)"));
        assert!(result.content.contains("json"));
        assert!(result.content.contains("Audit export"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = AuditExportTool;
        let result = tool
            .execute(serde_json::json!({
                "start": "2026-01-01T00:00:00Z",
                "end": "2026-02-01T00:00:00Z",
                "format": "csv"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("2026-01-01"));
        assert!(result.content.contains("2026-02-01"));
        assert!(result.content.contains("csv"));
    }

    #[tokio::test]
    async fn test_execute_sarif_format() {
        let tool = AuditExportTool;
        let result = tool
            .execute(serde_json::json!({
                "format": "sarif"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("sarif"));
        assert!(result.content.contains("Audit export"));
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(escape_csv("simple"), "simple");
        assert_eq!(escape_csv("has,comma"), "\"has,comma\"");
        assert_eq!(escape_csv("has\"quote"), "\"has\"\"quote\"");
    }

    #[test]
    fn test_filter_by_date_range_no_filter() {
        let findings = vec![Finding::new(
            "test",
            "desc",
            FindingSeverity::Low,
            FindingCategory::Security,
            FindingProvenance::new("test", 0.9),
        )];
        let filtered = filter_by_date_range(findings, None, None);
        assert_eq!(filtered.len(), 1);
    }
}
