//! Compression tool — create and extract zip archives.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

pub struct CompressTool {
    workspace: PathBuf,
}

impl CompressTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl Tool for CompressTool {
    fn name(&self) -> &str {
        "compress"
    }
    fn description(&self) -> &str {
        "Create and extract zip archives. Actions: create_zip, extract_zip, list_zip."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create_zip", "extract_zip", "list_zip"],
                    "description": "Action to perform"
                },
                "archive": { "type": "string", "description": "Path to the zip archive" },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Files to add to archive (for create_zip)"
                },
                "output_dir": { "type": "string", "description": "Output directory (for extract_zip)" }
            },
            "required": ["action", "archive"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let archive_str = args.get("archive").and_then(|v| v.as_str()).unwrap_or("");
        let archive_path = self.workspace.join(archive_str);

        match action {
            "create_zip" => {
                let files: Vec<String> = args
                    .get("files")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                if files.is_empty() {
                    return Ok(ToolOutput::text(
                        "Please provide files to add to the archive.",
                    ));
                }

                let file = std::fs::File::create(&archive_path).map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "compress".into(),
                        message: format!("Failed to create archive: {}", e),
                    }
                })?;
                let mut zip = zip::ZipWriter::new(file);
                let options = zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Deflated);

                let mut added = 0;
                for file_str in &files {
                    let file_path = self.workspace.join(file_str);
                    if !file_path.exists() {
                        continue;
                    }
                    let name = file_path
                        .strip_prefix(&self.workspace)
                        .unwrap_or(&file_path)
                        .to_string_lossy()
                        .to_string();
                    if let Ok(mut f) = std::fs::File::open(&file_path) {
                        let mut buf = Vec::new();
                        if f.read_to_end(&mut buf).is_ok()
                            && zip.start_file(&name, options).is_ok()
                            && zip.write_all(&buf).is_ok()
                        {
                            added += 1;
                        }
                    }
                }
                zip.finish().map_err(|e| ToolError::ExecutionFailed {
                    name: "compress".into(),
                    message: format!("Failed to finalize archive: {}", e),
                })?;

                Ok(ToolOutput::text(format!(
                    "Created {} with {} files.",
                    archive_str, added
                )))
            }
            "extract_zip" => {
                if !archive_path.exists() {
                    return Ok(ToolOutput::text(format!(
                        "Archive not found: {}",
                        archive_str
                    )));
                }
                let output_dir = args
                    .get("output_dir")
                    .and_then(|v| v.as_str())
                    .map(|p| self.workspace.join(p))
                    .unwrap_or_else(|| self.workspace.clone());

                let file =
                    std::fs::File::open(&archive_path).map_err(|e| ToolError::ExecutionFailed {
                        name: "compress".into(),
                        message: format!("Failed to open archive: {}", e),
                    })?;
                let mut archive =
                    zip::ZipArchive::new(file).map_err(|e| ToolError::ExecutionFailed {
                        name: "compress".into(),
                        message: format!("Invalid zip archive: {}", e),
                    })?;

                let mut extracted = 0;
                for i in 0..archive.len() {
                    if let Ok(mut entry) = archive.by_index(i) {
                        let name = entry.name().to_string();
                        // Security: prevent path traversal
                        if name.contains("..") {
                            continue;
                        }
                        let out_path = output_dir.join(&name);
                        if entry.is_dir() {
                            std::fs::create_dir_all(&out_path).ok();
                        } else {
                            if let Some(parent) = out_path.parent() {
                                std::fs::create_dir_all(parent).ok();
                            }
                            let mut buf = Vec::new();
                            if entry.read_to_end(&mut buf).is_ok()
                                && std::fs::write(&out_path, &buf).is_ok()
                            {
                                extracted += 1;
                            }
                        }
                    }
                }
                Ok(ToolOutput::text(format!(
                    "Extracted {} files from {}.",
                    extracted, archive_str
                )))
            }
            "list_zip" => {
                if !archive_path.exists() {
                    return Ok(ToolOutput::text(format!(
                        "Archive not found: {}",
                        archive_str
                    )));
                }
                let file =
                    std::fs::File::open(&archive_path).map_err(|e| ToolError::ExecutionFailed {
                        name: "compress".into(),
                        message: format!("Failed to open archive: {}", e),
                    })?;
                let mut archive =
                    zip::ZipArchive::new(file).map_err(|e| ToolError::ExecutionFailed {
                        name: "compress".into(),
                        message: format!("Invalid zip archive: {}", e),
                    })?;

                let mut output = format!("Archive: {} ({} entries)\n", archive_str, archive.len());
                for i in 0..archive.len() {
                    if let Ok(entry) = archive.by_index_raw(i) {
                        output.push_str(&format!("  {} ({} bytes)\n", entry.name(), entry.size()));
                    }
                }
                Ok(ToolOutput::text(output))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: create_zip, extract_zip, list_zip",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_compress_create_extract_roundtrip() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();

        // Create test files
        std::fs::write(workspace.join("a.txt"), "Hello A").unwrap();
        std::fs::write(workspace.join("b.txt"), "Hello B").unwrap();

        let tool = CompressTool::new(workspace.clone());

        // Create archive
        let result = tool
            .execute(json!({
                "action": "create_zip",
                "archive": "test.zip",
                "files": ["a.txt", "b.txt"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("2 files"));

        // List archive
        let result = tool
            .execute(json!({"action": "list_zip", "archive": "test.zip"}))
            .await
            .unwrap();
        assert!(result.content.contains("a.txt"));
        assert!(result.content.contains("b.txt"));

        // Extract to subdir
        std::fs::create_dir_all(workspace.join("output")).unwrap();
        let result = tool
            .execute(json!({
                "action": "extract_zip",
                "archive": "test.zip",
                "output_dir": "output"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Extracted 2"));

        // Verify extracted files
        assert_eq!(
            std::fs::read_to_string(workspace.join("output/a.txt")).unwrap(),
            "Hello A"
        );
    }

    #[tokio::test]
    async fn test_compress_nonexistent_archive() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = CompressTool::new(workspace);

        let result = tool
            .execute(json!({"action": "list_zip", "archive": "nope.zip"}))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_compress_schema() {
        let dir = TempDir::new().unwrap();
        let tool = CompressTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "compress");
    }
}
