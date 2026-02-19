//! SBOM Generate — Generate Software Bill of Materials (SBOM).

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;
use std::path::Path;

use crate::compliance::sbom::{SbomFormat, SbomGenerator};
use crate::dep_graph::DependencyGraph;

/// Generate a Software Bill of Materials (SBOM) for a project in CycloneDX,
/// SPDX, or CSV format. Enumerates all direct and transitive dependencies
/// with version, license, and provenance information.
pub struct SbomGenerateTool;

#[async_trait]
impl Tool for SbomGenerateTool {
    fn name(&self) -> &str {
        "sbom_generate"
    }

    fn description(&self) -> &str {
        "Generate Software Bill of Materials (SBOM)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to project"
                },
                "format": {
                    "type": "string",
                    "description": "Output format (cyclonedx/spdx/csv)"
                }
            },
            "required": []
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let format_str = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("cyclonedx");

        let sbom_format = match format_str.to_lowercase().as_str() {
            "cyclonedx" | "cdx" => SbomFormat::CycloneDx,
            "spdx" => SbomFormat::Spdx,
            "csv" => SbomFormat::Csv,
            other => {
                return Err(ToolError::InvalidArguments {
                    name: "sbom_generate".to_string(),
                    reason: format!("Unsupported format '{other}'. Use cyclonedx, spdx, or csv."),
                });
            }
        };

        let workspace = Path::new(path);

        // Build dependency graph from lockfiles
        let dep_graph = match DependencyGraph::build(workspace) {
            Ok(g) => g,
            Err(e) => {
                return Ok(ToolOutput::text(format!(
                    "SBOM generation for '{path}' (format: {sbom_format}):\n\
                     Error building dependency graph: {e}\n\
                     No lockfiles found or failed to parse."
                )));
            }
        };

        let all_deps: Vec<_> = dep_graph.all_packages().into_iter().cloned().collect();

        if all_deps.is_empty() {
            return Ok(ToolOutput::text(format!(
                "SBOM generation for '{path}' (format: {sbom_format}):\n\
                 No dependencies found in project lockfiles."
            )));
        }

        // Detect project name from path
        let subject_name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());

        let generator = SbomGenerator::new("rustant-security", env!("CARGO_PKG_VERSION"));
        let sbom = generator.generate(sbom_format, &subject_name, "0.0.0", &all_deps);

        // Format output based on requested format
        let output = match sbom_format {
            SbomFormat::CycloneDx => match generator.to_cyclonedx_json(&sbom) {
                Ok(json) => format!(
                    "SBOM generation for '{}' (format: {}):\n\n\
                         Components: {} ({} direct, {} transitive, {} dev)\n\n\
                         {}",
                    path,
                    sbom_format,
                    sbom.summary.total_components,
                    sbom.summary.direct_dependencies,
                    sbom.summary.transitive_dependencies,
                    sbom.summary.dev_dependencies,
                    json
                ),
                Err(e) => format!("SBOM generation failed during serialization: {e}"),
            },
            SbomFormat::Csv => {
                let csv = generator.to_csv(&sbom);
                format!(
                    "SBOM generation for '{}' (format: {}):\n\n\
                     Components: {} ({} direct, {} transitive, {} dev)\n\n\
                     {}",
                    path,
                    sbom_format,
                    sbom.summary.total_components,
                    sbom.summary.direct_dependencies,
                    sbom.summary.transitive_dependencies,
                    sbom.summary.dev_dependencies,
                    csv
                )
            }
            SbomFormat::Spdx => {
                // SPDX uses same JSON structure with different metadata
                match serde_json::to_string_pretty(&sbom) {
                    Ok(json) => format!(
                        "SBOM generation for '{}' (format: {}):\n\n\
                         Components: {} ({} direct, {} transitive, {} dev)\n\n\
                         {}",
                        path,
                        sbom_format,
                        sbom.summary.total_components,
                        sbom.summary.direct_dependencies,
                        sbom.summary.transitive_dependencies,
                        sbom.summary.dev_dependencies,
                        json
                    ),
                    Err(e) => format!("SBOM generation failed during serialization: {e}"),
                }
            }
        };

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
        let tool = SbomGenerateTool;
        assert_eq!(tool.name(), "sbom_generate");
    }

    #[test]
    fn test_schema() {
        let tool = SbomGenerateTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["format"].is_object());
    }

    #[test]
    fn test_risk_level() {
        let tool = SbomGenerateTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_defaults() {
        let tool = SbomGenerateTool;
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert!(
            result.content.contains("cyclonedx")
                || result.content.contains("CycloneDX")
                || result.content.contains("No lockfiles")
                || result.content.contains("No dependencies")
        );
        assert!(result.content.contains("SBOM generation"));
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = SbomGenerateTool;
        let result = tool
            .execute(serde_json::json!({
                "path": "/my/project",
                "format": "spdx"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("/my/project"));
        assert!(result.content.contains("spdx") || result.content.contains("SPDX"));
    }
}
