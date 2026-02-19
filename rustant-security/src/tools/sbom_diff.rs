//! SBOM Diff — Compare two SBOM versions to show changes.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_tools::registry::Tool;

use crate::compliance::sbom::{self, Sbom};

/// Compare two SBOM versions to identify added, removed, and updated
/// dependencies. Highlights license changes, version bumps, and new
/// transitive dependencies introduced between releases.
pub struct SbomDiffTool;

#[async_trait]
impl Tool for SbomDiffTool {
    fn name(&self) -> &str {
        "sbom_diff"
    }

    fn description(&self) -> &str {
        "Compare two SBOM versions to show changes"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "old": {
                    "type": "string",
                    "description": "Path to old SBOM"
                },
                "new": {
                    "type": "string",
                    "description": "Path to new SBOM"
                }
            },
            "required": ["old", "new"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let old_path = args.get("old").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "sbom_diff".to_string(),
                reason: "'old' parameter is required".to_string(),
            }
        })?;
        let new_path = args.get("new").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "sbom_diff".to_string(),
                reason: "'new' parameter is required".to_string(),
            }
        })?;

        // Read and parse the old SBOM
        let old_content =
            std::fs::read_to_string(old_path).map_err(|e| ToolError::ExecutionFailed {
                name: "sbom_diff".to_string(),
                message: format!("Failed to read old SBOM '{old_path}': {e}"),
            })?;
        let old_sbom: Sbom =
            serde_json::from_str(&old_content).map_err(|e| ToolError::ExecutionFailed {
                name: "sbom_diff".to_string(),
                message: format!("Failed to parse old SBOM '{old_path}': {e}"),
            })?;

        // Read and parse the new SBOM
        let new_content =
            std::fs::read_to_string(new_path).map_err(|e| ToolError::ExecutionFailed {
                name: "sbom_diff".to_string(),
                message: format!("Failed to read new SBOM '{new_path}': {e}"),
            })?;
        let new_sbom: Sbom =
            serde_json::from_str(&new_content).map_err(|e| ToolError::ExecutionFailed {
                name: "sbom_diff".to_string(),
                message: format!("Failed to parse new SBOM '{new_path}': {e}"),
            })?;

        // Compute diff
        let diff = sbom::diff_sboms(&old_sbom, &new_sbom);

        // Format output
        let mut output = format!("SBOM diff between '{old_path}' and '{new_path}':\n\n");

        output.push_str(&format!(
            "Summary: {} total changes ({} added, {} removed, {} version changes)\n\n",
            diff.summary.total_changes,
            diff.summary.added_count,
            diff.summary.removed_count,
            diff.summary.changed_count,
        ));

        // Added components
        if !diff.added.is_empty() {
            output.push_str(&format!("Added ({}):\n", diff.added.len()));
            for comp in &diff.added {
                output.push_str(&format!(
                    "  + {} v{} [{}] ({})\n",
                    comp.name,
                    comp.version,
                    comp.purl_type,
                    comp.license.as_deref().unwrap_or("unknown license"),
                ));
            }
            output.push('\n');
        }

        // Removed components
        if !diff.removed.is_empty() {
            output.push_str(&format!("Removed ({}):\n", diff.removed.len()));
            for comp in &diff.removed {
                output.push_str(&format!(
                    "  - {} v{} [{}]\n",
                    comp.name, comp.version, comp.purl_type,
                ));
            }
            output.push('\n');
        }

        // Version changes
        if !diff.changed.is_empty() {
            output.push_str(&format!("Version changes ({}):\n", diff.changed.len()));
            for change in &diff.changed {
                output.push_str(&format!(
                    "  ~ {} {} -> {} [{}]\n",
                    change.name, change.old_version, change.new_version, change.ecosystem,
                ));
            }
            output.push('\n');
        }

        if diff.summary.total_changes == 0 {
            output.push_str("No changes detected between SBOMs.\n");
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
        let tool = SbomDiffTool;
        assert_eq!(tool.name(), "sbom_diff");
    }

    #[test]
    fn test_schema() {
        let tool = SbomDiffTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["old"].is_object());
        assert!(schema["properties"]["new"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&serde_json::json!("old")));
        assert!(required.contains(&serde_json::json!("new")));
    }

    #[test]
    fn test_risk_level() {
        let tool = SbomDiffTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[tokio::test]
    async fn test_execute_with_args() {
        let tool = SbomDiffTool;
        // These files won't exist, so we expect an execution error
        let result = tool
            .execute(serde_json::json!({
                "old": "sbom-v1.json",
                "new": "sbom-v2.json"
            }))
            .await;
        // Should fail because files don't exist
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = format!("{err}");
        assert!(err_str.contains("sbom-v1.json") || err_str.contains("Failed to read"));
    }

    #[tokio::test]
    async fn test_execute_missing_required() {
        let tool = SbomDiffTool;
        let result = tool.execute(serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_with_temp_files() {
        use crate::compliance::sbom::{SbomFormat, SbomGenerator};
        use crate::dep_graph::DepNode;

        let generator = SbomGenerator::new("rustant-security", "1.0.0");

        let old_deps = vec![
            DepNode {
                name: "serde".into(),
                version: "1.0.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("MIT".into()),
                source: None,
            },
            DepNode {
                name: "old-dep".into(),
                version: "0.1.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("MIT".into()),
                source: None,
            },
        ];

        let new_deps = vec![
            DepNode {
                name: "serde".into(),
                version: "1.1.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("MIT".into()),
                source: None,
            },
            DepNode {
                name: "new-dep".into(),
                version: "0.2.0".into(),
                ecosystem: "cargo".into(),
                is_direct: true,
                is_dev: false,
                license: Some("Apache-2.0".into()),
                source: None,
            },
        ];

        let old_sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.0.0", &old_deps);
        let new_sbom = generator.generate(SbomFormat::CycloneDx, "test", "1.1.0", &new_deps);

        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old.json");
        let new_path = dir.path().join("new.json");

        std::fs::write(&old_path, serde_json::to_string(&old_sbom).unwrap()).unwrap();
        std::fs::write(&new_path, serde_json::to_string(&new_sbom).unwrap()).unwrap();

        let tool = SbomDiffTool;
        let result = tool
            .execute(serde_json::json!({
                "old": old_path.to_str().unwrap(),
                "new": new_path.to_str().unwrap()
            }))
            .await
            .unwrap();

        assert!(result.content.contains("SBOM diff"));
        assert!(result.content.contains("added"));
        assert!(result.content.contains("removed"));
    }
}
