//! File organizer tool — organize, deduplicate, and clean up files.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use walkdir::WalkDir;

use crate::registry::Tool;

pub struct FileOrganizerTool {
    workspace: PathBuf,
}

impl FileOrganizerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn hash_file(path: &std::path::Path) -> Option<String> {
        let data = std::fs::read(path).ok()?;
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Some(format!("{:x}", hasher.finalize()))
    }
}

#[async_trait]
impl Tool for FileOrganizerTool {
    fn name(&self) -> &str {
        "file_organizer"
    }
    fn description(&self) -> &str {
        "Organize, deduplicate, and clean up files. Actions: organize, dedup, cleanup, preview."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["organize", "dedup", "cleanup", "preview"],
                    "description": "Action to perform"
                },
                "path": { "type": "string", "description": "Target directory path" },
                "pattern": { "type": "string", "description": "File glob pattern for cleanup (e.g., '*.tmp')" },
                "dry_run": { "type": "boolean", "description": "Preview changes without applying (default: true)", "default": true }
            },
            "required": ["action"]
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
        let target = args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| self.workspace.join(p))
            .unwrap_or_else(|| self.workspace.clone());
        let dry_run = args
            .get("dry_run")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        // Validate target is within workspace
        let canonical = target.canonicalize().unwrap_or_else(|_| target.clone());
        if !canonical.starts_with(&self.workspace) {
            return Ok(ToolOutput::text("Error: Path must be within workspace."));
        }

        match action {
            "organize" => {
                // Group files by extension
                let mut by_ext: HashMap<String, Vec<String>> = HashMap::new();
                for entry in WalkDir::new(&target)
                    .max_depth(1)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        let ext = entry
                            .path()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("no_extension")
                            .to_lowercase();
                        by_ext
                            .entry(ext)
                            .or_default()
                            .push(entry.file_name().to_string_lossy().to_string());
                    }
                }
                let mut output = String::from("File organization preview:\n");
                for (ext, files) in &by_ext {
                    output.push_str(&format!(
                        "  .{} ({} files): {}\n",
                        ext,
                        files.len(),
                        files.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                    ));
                }
                if dry_run {
                    output.push_str("\n(Dry run — no changes made. Set dry_run=false to apply.)");
                } else {
                    // Create subdirectories by extension and move files
                    let mut moved = 0;
                    for (ext, files) in &by_ext {
                        let ext_dir = target.join(ext);
                        std::fs::create_dir_all(&ext_dir).ok();
                        for file in files {
                            let src = target.join(file);
                            let dst = ext_dir.join(file);
                            if src != dst && std::fs::rename(&src, &dst).is_ok() {
                                moved += 1;
                            }
                        }
                    }
                    output.push_str(&format!(
                        "\nMoved {} files into extension-based folders.",
                        moved
                    ));
                }
                Ok(ToolOutput::text(output))
            }
            "dedup" => {
                let mut hashes: HashMap<String, Vec<PathBuf>> = HashMap::new();
                let mut file_count = 0;
                for entry in WalkDir::new(&target)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        file_count += 1;
                        if let Some(hash) = Self::hash_file(entry.path()) {
                            hashes
                                .entry(hash)
                                .or_default()
                                .push(entry.path().to_path_buf());
                        }
                    }
                }
                let dups: Vec<_> = hashes.values().filter(|v| v.len() > 1).collect();
                if dups.is_empty() {
                    return Ok(ToolOutput::text(format!(
                        "No duplicates found among {} files.",
                        file_count
                    )));
                }
                let mut output = format!(
                    "Found {} duplicate groups among {} files:\n",
                    dups.len(),
                    file_count
                );
                for (i, group) in dups.iter().enumerate().take(20) {
                    output.push_str(&format!("  Group {}:\n", i + 1));
                    for path in *group {
                        let rel = path.strip_prefix(&self.workspace).unwrap_or(path);
                        output.push_str(&format!("    {}\n", rel.display()));
                    }
                }
                if dry_run {
                    output.push_str("\n(Dry run — no files deleted.)");
                }
                Ok(ToolOutput::text(output))
            }
            "cleanup" => {
                let pattern = args
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("*.tmp");
                let glob = globset::GlobBuilder::new(pattern)
                    .build()
                    .map(|g| g.compile_matcher())
                    .ok();
                let mut matches = Vec::new();
                for entry in WalkDir::new(&target)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        let name = entry.file_name().to_string_lossy();
                        if let Some(ref glob) = glob {
                            if glob.is_match(name.as_ref()) {
                                matches.push(entry.path().to_path_buf());
                            }
                        }
                    }
                }
                if matches.is_empty() {
                    return Ok(ToolOutput::text(format!(
                        "No files matching '{}'.",
                        pattern
                    )));
                }
                let mut output = format!("Found {} files matching '{}':\n", matches.len(), pattern);
                for path in &matches {
                    let rel = path.strip_prefix(&self.workspace).unwrap_or(path);
                    output.push_str(&format!("  {}\n", rel.display()));
                }
                if dry_run {
                    output.push_str("\n(Dry run — no files deleted.)");
                } else {
                    let mut deleted = 0;
                    for path in &matches {
                        if std::fs::remove_file(path).is_ok() {
                            deleted += 1;
                        }
                    }
                    output.push_str(&format!("\nDeleted {} files.", deleted));
                }
                Ok(ToolOutput::text(output))
            }
            "preview" => {
                let mut total_files = 0;
                let mut total_size: u64 = 0;
                let mut by_ext: HashMap<String, (usize, u64)> = HashMap::new();
                for entry in WalkDir::new(&target)
                    .max_depth(3)
                    .into_iter()
                    .filter_map(|e| e.ok())
                {
                    if entry.file_type().is_file() {
                        total_files += 1;
                        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                        total_size += size;
                        let ext = entry
                            .path()
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("none")
                            .to_lowercase();
                        let entry = by_ext.entry(ext).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += size;
                    }
                }
                let mut output = format!(
                    "Directory preview: {} files, {:.1} MB\n",
                    total_files,
                    total_size as f64 / 1_048_576.0
                );
                let mut sorted: Vec<_> = by_ext.iter().collect();
                sorted.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
                for (ext, (count, size)) in sorted.iter().take(15) {
                    output.push_str(&format!(
                        "  .{:<10} {:>5} files  {:>8.1} KB\n",
                        ext,
                        count,
                        *size as f64 / 1024.0
                    ));
                }
                Ok(ToolOutput::text(output))
            }
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: {}. Use: organize, dedup, cleanup, preview",
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
    async fn test_file_organizer_preview() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        std::fs::write(workspace.join("test.txt"), "hello").unwrap();
        std::fs::write(workspace.join("data.csv"), "a,b").unwrap();

        let tool = FileOrganizerTool::new(workspace);
        let result = tool.execute(json!({"action": "preview"})).await.unwrap();
        assert!(result.content.contains("files"));
    }

    #[tokio::test]
    async fn test_file_organizer_dedup() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        std::fs::write(workspace.join("a.txt"), "same content").unwrap();
        std::fs::write(workspace.join("b.txt"), "same content").unwrap();
        std::fs::write(workspace.join("c.txt"), "different").unwrap();

        let tool = FileOrganizerTool::new(workspace);
        let result = tool
            .execute(json!({"action": "dedup", "dry_run": true}))
            .await
            .unwrap();
        assert!(result.content.contains("duplicate"));
    }

    #[tokio::test]
    async fn test_file_organizer_no_dupes() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        std::fs::write(workspace.join("a.txt"), "unique a").unwrap();
        std::fs::write(workspace.join("b.txt"), "unique b").unwrap();

        let tool = FileOrganizerTool::new(workspace);
        let result = tool.execute(json!({"action": "dedup"})).await.unwrap();
        assert!(result.content.contains("No duplicates"));
    }

    #[tokio::test]
    async fn test_file_organizer_schema() {
        let dir = TempDir::new().unwrap();
        let tool = FileOrganizerTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "file_organizer");
    }
}
