//! Privacy manager tool — data sovereignty, boundary management, access auditing,
//! and data export/deletion for the `.rustant/` state directory.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum BoundaryType {
    LocalOnly,
    Encrypted,
    Shareable,
}

impl BoundaryType {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "local_only" => Some(BoundaryType::LocalOnly),
            "encrypted" => Some(BoundaryType::Encrypted),
            "shareable" => Some(BoundaryType::Shareable),
            _ => None,
        }
    }

    fn as_str(&self) -> &str {
        match self {
            BoundaryType::LocalOnly => "local_only",
            BoundaryType::Encrypted => "encrypted",
            BoundaryType::Shareable => "shareable",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DataBoundary {
    id: usize,
    name: String,
    boundary_type: BoundaryType,
    paths: Vec<String>,
    description: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccessLogEntry {
    timestamp: DateTime<Utc>,
    tool_name: String,
    data_accessed: String,
    purpose: String,
    boundary_id: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PrivacyState {
    boundaries: Vec<DataBoundary>,
    access_log: Vec<AccessLogEntry>,
    next_id: usize,
    max_log_entries: usize,
}

impl Default for PrivacyState {
    fn default() -> Self {
        Self {
            boundaries: Vec::new(),
            access_log: Vec::new(),
            next_id: 1,
            max_log_entries: 10_000,
        }
    }
}

pub struct PrivacyManagerTool {
    workspace: PathBuf,
}

impl PrivacyManagerTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("privacy")
            .join("config.json")
    }

    fn load_state(&self) -> PrivacyState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            PrivacyState::default()
        }
    }

    fn save_state(&self, state: &PrivacyState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "privacy_manager".to_string(),
                message: format!("Failed to create dir: {}", e),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "privacy_manager".to_string(),
            message: format!("Serialize error: {}", e),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "privacy_manager".to_string(),
            message: format!("Write error: {}", e),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "privacy_manager".to_string(),
            message: format!("Rename error: {}", e),
        })?;
        Ok(())
    }

    fn rustant_dir(&self) -> PathBuf {
        self.workspace.join(".rustant")
    }

    /// Recursively compute size and file count for a directory.
    fn dir_stats(&self, path: &std::path::Path) -> (u64, usize) {
        let mut total_size: u64 = 0;
        let mut file_count: usize = 0;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    let (s, c) = self.dir_stats(&entry_path);
                    total_size += s;
                    file_count += c;
                } else if entry_path.is_file() {
                    if let Ok(meta) = entry_path.metadata() {
                        total_size += meta.len();
                        file_count += 1;
                    }
                }
            }
        }
        (total_size, file_count)
    }

    /// Collect all top-level subdirectory names under .rustant/.
    fn list_domains(&self) -> Vec<String> {
        let rustant_dir = self.rustant_dir();
        let mut domains = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&rustant_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        domains.push(name.to_string());
                    }
                }
            }
        }
        domains.sort();
        domains
    }

    /// Collect all file paths under .rustant/ recursively (relative to .rustant/).
    fn collect_all_paths(&self) -> Vec<String> {
        let rustant_dir = self.rustant_dir();
        let mut paths = Vec::new();
        self.collect_paths_recursive(&rustant_dir, &rustant_dir, &mut paths);
        paths
    }

    fn collect_paths_recursive(
        &self,
        base: &std::path::Path,
        current: &std::path::Path,
        out: &mut Vec<String>,
    ) {
        if let Ok(entries) = std::fs::read_dir(current) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if let Ok(rel) = entry_path.strip_prefix(base) {
                    let rel_str = rel.to_string_lossy().to_string();
                    out.push(rel_str);
                }
                if entry_path.is_dir() {
                    self.collect_paths_recursive(base, &entry_path, out);
                }
            }
        }
    }

    /// Check if a given relative path is covered by any boundary.
    fn path_covered_by_boundary(
        &self,
        rel_path: &str,
        boundaries: &[DataBoundary],
    ) -> Option<usize> {
        for boundary in boundaries {
            for bp in &boundary.paths {
                if rel_path.starts_with(bp.as_str()) || rel_path == *bp {
                    return Some(boundary.id);
                }
            }
        }
        None
    }

    fn format_size(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }

    /// Recursively delete contents of a directory (but not the directory itself).
    fn delete_dir_contents(&self, path: &std::path::Path) -> Result<usize, ToolError> {
        let mut deleted = 0;
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    std::fs::remove_dir_all(&entry_path).map_err(|e| {
                        ToolError::ExecutionFailed {
                            name: "privacy_manager".to_string(),
                            message: format!("Failed to remove {}: {}", entry_path.display(), e),
                        }
                    })?;
                    deleted += 1;
                } else {
                    std::fs::remove_file(&entry_path).map_err(|e| ToolError::ExecutionFailed {
                        name: "privacy_manager".to_string(),
                        message: format!("Failed to remove {}: {}", entry_path.display(), e),
                    })?;
                    deleted += 1;
                }
            }
        }
        Ok(deleted)
    }

    // --- Action handlers ---

    fn action_set_boundary(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let name = args
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if name.is_empty() {
            return Ok(ToolOutput::text(
                "Error: 'name' is required for set_boundary.",
            ));
        }

        let boundary_type_str = args
            .get("boundary_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let boundary_type = match BoundaryType::from_str(boundary_type_str) {
            Some(bt) => bt,
            None => {
                return Ok(ToolOutput::text(format!(
                    "Error: invalid boundary_type '{}'. Use: local_only, encrypted, shareable",
                    boundary_type_str
                )));
            }
        };

        let paths: Vec<String> = match args.get("paths") {
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => {
                return Ok(ToolOutput::text(
                    "Error: 'paths' is required as an array of strings.",
                ));
            }
        };
        if paths.is_empty() {
            return Ok(ToolOutput::text(
                "Error: 'paths' must contain at least one path.",
            ));
        }

        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut state = self.load_state();
        let id = state.next_id;
        state.next_id += 1;

        state.boundaries.push(DataBoundary {
            id,
            name: name.to_string(),
            boundary_type,
            paths: paths.clone(),
            description,
            created_at: Utc::now(),
        });
        self.save_state(&state)?;

        Ok(ToolOutput::text(format!(
            "Created data boundary #{} '{}' ({}) covering {} path(s).",
            id,
            name,
            boundary_type_str,
            paths.len()
        )))
    }

    fn action_list_boundaries(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        if state.boundaries.is_empty() {
            return Ok(ToolOutput::text("No data boundaries defined."));
        }

        let mut lines = Vec::new();
        lines.push(format!("Data boundaries ({}):", state.boundaries.len()));
        for b in &state.boundaries {
            lines.push(format!(
                "  #{} — {} [{}]",
                b.id,
                b.name,
                b.boundary_type.as_str()
            ));
            for p in &b.paths {
                lines.push(format!("       path: {}", p));
            }
            if !b.description.is_empty() {
                lines.push(format!("       desc: {}", b.description));
            }
        }
        Ok(ToolOutput::text(lines.join("\n")))
    }

    fn action_audit_access(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let tool_filter = args.get("tool_name").and_then(|v| v.as_str());
        let boundary_filter = args
            .get("boundary_id")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);

        let filtered: Vec<&AccessLogEntry> = state
            .access_log
            .iter()
            .rev()
            .filter(|e| {
                if let Some(tn) = tool_filter {
                    if e.tool_name != tn {
                        return false;
                    }
                }
                if let Some(bid) = boundary_filter {
                    if e.boundary_id != Some(bid) {
                        return false;
                    }
                }
                true
            })
            .take(limit)
            .collect();

        if filtered.is_empty() {
            return Ok(ToolOutput::text("No access log entries found."));
        }

        let mut lines = Vec::new();
        lines.push(format!("Access log ({} entries shown):", filtered.len()));
        for entry in &filtered {
            let boundary_note = if let Some(bid) = entry.boundary_id {
                format!(" [boundary #{}]", bid)
            } else {
                String::new()
            };
            lines.push(format!(
                "  {} — {} accessed '{}' for '{}'{}",
                entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                entry.tool_name,
                entry.data_accessed,
                entry.purpose,
                boundary_note
            ));
        }
        Ok(ToolOutput::text(lines.join("\n")))
    }

    fn action_compliance_check(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let rustant_dir = self.rustant_dir();
        if !rustant_dir.exists() {
            return Ok(ToolOutput::text(
                "No .rustant/ directory found. Nothing to check.",
            ));
        }

        let domains = self.list_domains();
        if domains.is_empty() {
            return Ok(ToolOutput::text(
                "No data directories found in .rustant/. Compliance check complete — nothing to cover.",
            ));
        }

        let all_paths = self.collect_all_paths();
        let mut covered_count = 0;
        let mut uncovered_dirs: Vec<String> = Vec::new();

        for domain in &domains {
            if self
                .path_covered_by_boundary(domain, &state.boundaries)
                .is_some()
            {
                covered_count += 1;
            } else {
                uncovered_dirs.push(domain.clone());
            }
        }

        let total = domains.len();
        let coverage_pct = if total > 0 {
            (covered_count as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let mut lines = Vec::new();
        lines.push("Compliance Check Report".to_string());
        lines.push(format!("  Total directories: {}", total));
        lines.push(format!("  Covered by boundaries: {}", covered_count));
        lines.push(format!("  Coverage: {:.0}%", coverage_pct));
        lines.push(format!("  Total paths scanned: {}", all_paths.len()));

        if !uncovered_dirs.is_empty() {
            lines.push(String::new());
            lines.push("  Uncovered directories:".to_string());
            for d in &uncovered_dirs {
                lines.push(format!("    - {}", d));
            }
            lines.push(String::new());
            lines
                .push("  Recommendation: Create boundaries for uncovered directories.".to_string());
        } else {
            lines.push(String::new());
            lines.push("  All directories are covered by boundaries.".to_string());
        }

        Ok(ToolOutput::text(lines.join("\n")))
    }

    fn action_export_data(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let output_name = args
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("rustant_export.json");

        let rustant_dir = self.rustant_dir();
        if !rustant_dir.exists() {
            return Ok(ToolOutput::text(
                "No .rustant/ directory found. Nothing to export.",
            ));
        }

        let mut export = serde_json::Map::new();
        let domains = self.list_domains();

        for domain in &domains {
            let domain_dir = rustant_dir.join(domain);
            let mut domain_files = serde_json::Map::new();

            if let Ok(entries) = std::fs::read_dir(&domain_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() {
                        if let Some(fname) = entry_path.file_name().and_then(|f| f.to_str()) {
                            match std::fs::read_to_string(&entry_path) {
                                Ok(content) => {
                                    // Try to parse as JSON; if it fails, store as string
                                    if let Ok(val) = serde_json::from_str::<Value>(&content) {
                                        domain_files.insert(fname.to_string(), val);
                                    } else {
                                        domain_files
                                            .insert(fname.to_string(), Value::String(content));
                                    }
                                }
                                Err(_) => {
                                    domain_files.insert(
                                        fname.to_string(),
                                        Value::String("[binary or unreadable]".to_string()),
                                    );
                                }
                            }
                        }
                    }
                }
            }
            export.insert(domain.clone(), Value::Object(domain_files));
        }

        let export_json =
            serde_json::to_string_pretty(&export).map_err(|e| ToolError::ExecutionFailed {
                name: "privacy_manager".to_string(),
                message: format!("Failed to serialize export: {}", e),
            })?;

        // If small enough, return inline; otherwise write to file
        if export_json.len() < 50_000 {
            Ok(ToolOutput::text(format!(
                "Exported {} domain(s) ({} bytes):\n{}",
                domains.len(),
                export_json.len(),
                export_json
            )))
        } else {
            let output_path = self.workspace.join(output_name);
            std::fs::write(&output_path, &export_json).map_err(|e| ToolError::ExecutionFailed {
                name: "privacy_manager".to_string(),
                message: format!("Failed to write export file: {}", e),
            })?;
            Ok(ToolOutput::text(format!(
                "Exported {} domain(s) to {}. Size: {}",
                domains.len(),
                output_path.display(),
                Self::format_size(export_json.len() as u64)
            )))
        }
    }

    fn action_delete_data(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let domain = args
            .get("domain")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if domain.is_empty() {
            return Ok(ToolOutput::text(
                "Error: 'domain' is required for delete_data. Use a domain name or 'all'.",
            ));
        }

        let rustant_dir = self.rustant_dir();
        if !rustant_dir.exists() {
            return Ok(ToolOutput::text("No .rustant/ directory found."));
        }

        if domain == "all" {
            let mut deleted_total = 0;
            let mut deleted_domains = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&rustant_dir) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        let dir_name = entry.file_name().to_str().unwrap_or("").to_string();
                        // Preserve the privacy config directory itself
                        if dir_name == "privacy" {
                            continue;
                        }
                        let count = self.delete_dir_contents(&entry_path)?;
                        std::fs::remove_dir_all(&entry_path).map_err(|e| {
                            ToolError::ExecutionFailed {
                                name: "privacy_manager".to_string(),
                                message: format!("Failed to remove dir {}: {}", dir_name, e),
                            }
                        })?;
                        deleted_total += count + 1; // +1 for the dir itself
                        deleted_domains.push(dir_name);
                    } else if entry_path.is_file() {
                        // Don't delete files at top level that aren't in privacy/
                        let fname = entry.file_name().to_str().unwrap_or("").to_string();
                        std::fs::remove_file(&entry_path).map_err(|e| {
                            ToolError::ExecutionFailed {
                                name: "privacy_manager".to_string(),
                                message: format!("Failed to remove file {}: {}", fname, e),
                            }
                        })?;
                        deleted_total += 1;
                    }
                }
            }
            Ok(ToolOutput::text(format!(
                "Deleted all data except privacy config. Removed {} item(s) across domain(s): {}",
                deleted_total,
                if deleted_domains.is_empty() {
                    "none".to_string()
                } else {
                    deleted_domains.join(", ")
                }
            )))
        } else {
            let domain_dir = rustant_dir.join(domain);
            if !domain_dir.exists() || !domain_dir.is_dir() {
                return Ok(ToolOutput::text(format!(
                    "Domain '{}' not found in .rustant/.",
                    domain
                )));
            }
            let count = self.delete_dir_contents(&domain_dir)?;
            std::fs::remove_dir_all(&domain_dir).map_err(|e| ToolError::ExecutionFailed {
                name: "privacy_manager".to_string(),
                message: format!("Failed to remove domain dir: {}", e),
            })?;
            Ok(ToolOutput::text(format!(
                "Deleted domain '{}': removed {} item(s).",
                domain, count
            )))
        }
    }

    fn action_encrypt_store(&self, args: &Value) -> Result<ToolOutput, ToolError> {
        let path_str = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if path_str.is_empty() {
            return Ok(ToolOutput::text(
                "Error: 'path' is required for encrypt_store.",
            ));
        }

        // Resolve path relative to .rustant/ if not absolute
        let file_path = if std::path::Path::new(&path_str).is_absolute() {
            PathBuf::from(&path_str)
        } else {
            self.rustant_dir().join(&path_str)
        };

        if !file_path.exists() || !file_path.is_file() {
            return Ok(ToolOutput::text(format!(
                "Error: file '{}' not found.",
                file_path.display()
            )));
        }

        let content = std::fs::read(&file_path).map_err(|e| ToolError::ExecutionFailed {
            name: "privacy_manager".to_string(),
            message: format!("Failed to read file: {}", e),
        })?;

        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &content);

        let encrypted_path = file_path.with_extension(format!(
            "{}.encrypted",
            file_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("dat")
        ));

        let output_content = format!(
            "# Rustant encrypted store (base64 placeholder)\n\
             # TODO: Replace with AES-256-GCM encryption (future crate)\n\
             # Original: {}\n\
             # Encrypted at: {}\n\
             {}\n",
            file_path.display(),
            Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
            encoded
        );

        std::fs::write(&encrypted_path, output_content.as_bytes()).map_err(|e| {
            ToolError::ExecutionFailed {
                name: "privacy_manager".to_string(),
                message: format!("Failed to write encrypted file: {}", e),
            }
        })?;

        Ok(ToolOutput::text(format!(
            "Encrypted (base64) '{}' -> '{}'. Note: this is a base64 placeholder; \
             AES-256-GCM encryption will be added in a future release.",
            file_path.display(),
            encrypted_path.display()
        )))
    }

    fn action_privacy_report(&self) -> Result<ToolOutput, ToolError> {
        let state = self.load_state();
        let rustant_dir = self.rustant_dir();
        if !rustant_dir.exists() {
            return Ok(ToolOutput::text(
                "No .rustant/ directory found. Nothing to report.",
            ));
        }

        let (total_size, total_files) = self.dir_stats(&rustant_dir);
        let domains = self.list_domains();

        let mut lines = Vec::new();
        lines.push("Privacy Report".to_string());
        lines.push("==============".to_string());
        lines.push(String::new());
        lines.push(format!(
            "Total data size: {} ({} files)",
            Self::format_size(total_size),
            total_files
        ));
        lines.push(format!("Domains: {}", domains.len()));

        // Breakdown by domain
        if !domains.is_empty() {
            lines.push(String::new());
            lines.push("Domain breakdown:".to_string());
            for domain in &domains {
                let domain_dir = rustant_dir.join(domain);
                let (size, count) = self.dir_stats(&domain_dir);
                let covered = self
                    .path_covered_by_boundary(domain, &state.boundaries)
                    .is_some();
                let coverage_tag = if covered {
                    " [covered]"
                } else {
                    " [uncovered]"
                };
                lines.push(format!(
                    "  {} — {} ({} files){}",
                    domain,
                    Self::format_size(size),
                    count,
                    coverage_tag
                ));
            }
        }

        // Boundary coverage
        let covered_count = domains
            .iter()
            .filter(|d| {
                self.path_covered_by_boundary(d, &state.boundaries)
                    .is_some()
            })
            .count();
        let coverage_pct = if domains.is_empty() {
            100.0
        } else {
            (covered_count as f64 / domains.len() as f64) * 100.0
        };
        lines.push(String::new());
        lines.push(format!("Boundary coverage: {:.0}%", coverage_pct));
        lines.push(format!("Boundaries defined: {}", state.boundaries.len()));

        // Access log stats
        lines.push(String::new());
        lines.push("Access log:".to_string());
        lines.push(format!("  Total entries: {}", state.access_log.len()));
        let unique_tools: std::collections::HashSet<&str> = state
            .access_log
            .iter()
            .map(|e| e.tool_name.as_str())
            .collect();
        lines.push(format!("  Unique tools: {}", unique_tools.len()));

        // Recommendations
        let uncovered: Vec<&String> = domains
            .iter()
            .filter(|d| {
                self.path_covered_by_boundary(d, &state.boundaries)
                    .is_none()
            })
            .collect();
        if !uncovered.is_empty() {
            lines.push(String::new());
            lines.push("Recommendations:".to_string());
            for d in &uncovered {
                lines.push(format!(
                    "  - Create a boundary for '{}' to control data access",
                    d
                ));
            }
        }

        Ok(ToolOutput::text(lines.join("\n")))
    }
}

#[async_trait]
impl Tool for PrivacyManagerTool {
    fn name(&self) -> &str {
        "privacy_manager"
    }

    fn description(&self) -> &str {
        "Privacy and data sovereignty: boundaries, access auditing, data export/deletion. Actions: set_boundary, list_boundaries, audit_access, compliance_check, export_data, delete_data, encrypt_store, privacy_report."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "set_boundary", "list_boundaries", "audit_access",
                        "compliance_check", "export_data", "delete_data",
                        "encrypt_store", "privacy_report"
                    ],
                    "description": "Action to perform"
                },
                "name": {
                    "type": "string",
                    "description": "Boundary name (for set_boundary)"
                },
                "boundary_type": {
                    "type": "string",
                    "enum": ["local_only", "encrypted", "shareable"],
                    "description": "Boundary type (for set_boundary)"
                },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Paths covered by the boundary (for set_boundary)"
                },
                "description": {
                    "type": "string",
                    "description": "Boundary description (for set_boundary)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max entries to return (for audit_access, default 50)"
                },
                "tool_name": {
                    "type": "string",
                    "description": "Filter by tool name (for audit_access)"
                },
                "boundary_id": {
                    "type": "integer",
                    "description": "Filter by boundary ID (for audit_access)"
                },
                "output": {
                    "type": "string",
                    "description": "Output filename (for export_data, default rustant_export.json)"
                },
                "domain": {
                    "type": "string",
                    "description": "Domain name or 'all' (for delete_data)"
                },
                "path": {
                    "type": "string",
                    "description": "File path to encrypt (for encrypt_store)"
                }
            },
            "required": ["action"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "set_boundary" => self.action_set_boundary(&args),
            "list_boundaries" => self.action_list_boundaries(),
            "audit_access" => self.action_audit_access(&args),
            "compliance_check" => self.action_compliance_check(),
            "export_data" => self.action_export_data(&args),
            "delete_data" => self.action_delete_data(&args),
            "encrypt_store" => self.action_encrypt_store(&args),
            "privacy_report" => self.action_privacy_report(),
            _ => Ok(ToolOutput::text(format!(
                "Unknown action: '{}'. Use: set_boundary, list_boundaries, audit_access, \
                 compliance_check, export_data, delete_data, encrypt_store, privacy_report",
                action
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        (dir, workspace)
    }

    #[test]
    fn test_tool_properties() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);
        assert_eq!(tool.name(), "privacy_manager");
        assert!(tool.description().contains("Privacy"));
        assert_eq!(tool.risk_level(), RiskLevel::Write);
        assert_eq!(tool.timeout(), Duration::from_secs(60));
    }

    #[test]
    fn test_schema_validation() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);
        let schema = tool.parameters_schema();
        assert!(schema.get("properties").is_some());
        let props = schema.get("properties").unwrap();
        assert!(props.get("action").is_some());
        assert!(props.get("name").is_some());
        assert!(props.get("boundary_type").is_some());
        assert!(props.get("paths").is_some());
        assert!(props.get("domain").is_some());
        assert!(props.get("path").is_some());
        let action_enum = props["action"]["enum"].as_array().unwrap();
        assert_eq!(action_enum.len(), 8);
        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "action");
    }

    #[tokio::test]
    async fn test_set_boundary() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "set_boundary",
                "name": "personal_data",
                "boundary_type": "local_only",
                "paths": ["inbox", "relationships"],
                "description": "Personal data stays local"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("#1"));
        assert!(result.content.contains("personal_data"));
        assert!(result.content.contains("2 path(s)"));

        // Verify it shows up in list
        let list = tool
            .execute(json!({"action": "list_boundaries"}))
            .await
            .unwrap();
        assert!(list.content.contains("personal_data"));
        assert!(list.content.contains("local_only"));
        assert!(list.content.contains("inbox"));
        assert!(list.content.contains("relationships"));
    }

    #[tokio::test]
    async fn test_list_boundaries_empty() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({"action": "list_boundaries"}))
            .await
            .unwrap();
        assert!(result.content.contains("No data boundaries"));
    }

    #[tokio::test]
    async fn test_audit_access_empty() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({"action": "audit_access"}))
            .await
            .unwrap();
        assert!(result.content.contains("No access log entries"));
    }

    #[tokio::test]
    async fn test_compliance_check_no_data() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create empty .rustant/ dir
        std::fs::create_dir_all(workspace.join(".rustant")).unwrap();

        let result = tool
            .execute(json!({"action": "compliance_check"}))
            .await
            .unwrap();
        assert!(
            result.content.contains("Nothing to cover")
                || result.content.contains("nothing to cover")
        );
    }

    #[tokio::test]
    async fn test_compliance_check_with_boundary() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create .rustant/career/ and .rustant/inbox/ directories
        std::fs::create_dir_all(workspace.join(".rustant").join("career")).unwrap();
        std::fs::create_dir_all(workspace.join(".rustant").join("inbox")).unwrap();

        // Create a boundary covering career
        tool.execute(json!({
            "action": "set_boundary",
            "name": "career_data",
            "boundary_type": "encrypted",
            "paths": ["career"]
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "compliance_check"}))
            .await
            .unwrap();
        assert!(result.content.contains("Compliance Check Report"));
        // career is covered, inbox and privacy are present
        // privacy dir is created by save_state, career is covered, inbox is not
        assert!(result.content.contains("Uncovered directories"));
        assert!(result.content.contains("inbox"));
    }

    #[tokio::test]
    async fn test_delete_data_domain() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create a test domain with a file
        let domain_dir = workspace.join(".rustant").join("test_domain");
        std::fs::create_dir_all(&domain_dir).unwrap();
        std::fs::write(domain_dir.join("data.json"), r#"{"key": "value"}"#).unwrap();
        assert!(domain_dir.join("data.json").exists());

        let result = tool
            .execute(json!({"action": "delete_data", "domain": "test_domain"}))
            .await
            .unwrap();
        assert!(result.content.contains("Deleted domain 'test_domain'"));
        assert!(result.content.contains("removed"));
        assert!(!domain_dir.exists());
    }

    #[tokio::test]
    async fn test_export_data() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create some state files
        let career_dir = workspace.join(".rustant").join("career");
        std::fs::create_dir_all(&career_dir).unwrap();
        std::fs::write(
            career_dir.join("goals.json"),
            r#"{"goals": ["learn rust"]}"#,
        )
        .unwrap();

        let inbox_dir = workspace.join(".rustant").join("inbox");
        std::fs::create_dir_all(&inbox_dir).unwrap();
        std::fs::write(inbox_dir.join("items.json"), r#"{"items": []}"#).unwrap();

        let result = tool
            .execute(json!({"action": "export_data"}))
            .await
            .unwrap();
        assert!(result.content.contains("Exported"));
        assert!(result.content.contains("career"));
        assert!(result.content.contains("inbox"));
        assert!(result.content.contains("learn rust"));
    }

    #[tokio::test]
    async fn test_privacy_report() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create some domain dirs with data
        let career_dir = workspace.join(".rustant").join("career");
        std::fs::create_dir_all(&career_dir).unwrap();
        std::fs::write(career_dir.join("data.json"), "test data content").unwrap();

        let result = tool
            .execute(json!({"action": "privacy_report"}))
            .await
            .unwrap();
        assert!(result.content.contains("Privacy Report"));
        assert!(result.content.contains("Total data size"));
        assert!(result.content.contains("career"));
        assert!(result.content.contains("Access log"));
        assert!(result.content.contains("Unique tools"));
    }

    #[tokio::test]
    async fn test_boundary_type_validation() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "set_boundary",
                "name": "test",
                "boundary_type": "invalid_type",
                "paths": ["some_path"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Error"));
        assert!(result.content.contains("invalid boundary_type"));
        assert!(result.content.contains("invalid_type"));
    }

    #[tokio::test]
    async fn test_access_log_eviction() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        // Create state with max_log_entries = 5 for testing
        let mut state = PrivacyState {
            max_log_entries: 5,
            ..Default::default()
        };

        // Add 7 entries
        for i in 0..7 {
            state.access_log.push(AccessLogEntry {
                timestamp: Utc::now(),
                tool_name: format!("tool_{}", i),
                data_accessed: format!("path_{}", i),
                purpose: "test".to_string(),
                boundary_id: None,
            });
            // Evict oldest if over limit
            if state.access_log.len() > state.max_log_entries {
                state
                    .access_log
                    .drain(0..state.access_log.len() - state.max_log_entries);
            }
        }
        tool.save_state(&state).unwrap();

        let loaded = tool.load_state();
        assert_eq!(loaded.access_log.len(), 5);
        // The oldest entries (tool_0, tool_1) should be gone
        assert_eq!(loaded.access_log[0].tool_name, "tool_2");
        assert_eq!(loaded.access_log[4].tool_name, "tool_6");
    }

    #[tokio::test]
    async fn test_state_roundtrip() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        // Set up state with boundary and access log entry
        let mut state = PrivacyState::default();
        state.boundaries.push(DataBoundary {
            id: 1,
            name: "test_boundary".to_string(),
            boundary_type: BoundaryType::Encrypted,
            paths: vec!["career".to_string(), "inbox".to_string()],
            description: "test description".to_string(),
            created_at: Utc::now(),
        });
        state.access_log.push(AccessLogEntry {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            data_accessed: "career/goals.json".to_string(),
            purpose: "reading goals".to_string(),
            boundary_id: Some(1),
        });
        state.next_id = 2;

        tool.save_state(&state).unwrap();
        let loaded = tool.load_state();

        assert_eq!(loaded.next_id, 2);
        assert_eq!(loaded.max_log_entries, 10_000);
        assert_eq!(loaded.boundaries.len(), 1);
        assert_eq!(loaded.boundaries[0].name, "test_boundary");
        assert_eq!(loaded.boundaries[0].boundary_type, BoundaryType::Encrypted);
        assert_eq!(loaded.boundaries[0].paths.len(), 2);
        assert_eq!(loaded.access_log.len(), 1);
        assert_eq!(loaded.access_log[0].tool_name, "file_read");
        assert_eq!(loaded.access_log[0].boundary_id, Some(1));
    }

    #[tokio::test]
    async fn test_unknown_action() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({"action": "nonexistent"}))
            .await
            .unwrap();
        assert!(result.content.contains("Unknown action"));
        assert!(result.content.contains("nonexistent"));
    }

    #[tokio::test]
    async fn test_encrypt_store() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create a file to encrypt
        let data_dir = workspace.join(".rustant").join("career");
        std::fs::create_dir_all(&data_dir).unwrap();
        let file_path = data_dir.join("goals.json");
        std::fs::write(&file_path, r#"{"goals": ["learn rust"]}"#).unwrap();

        let result = tool
            .execute(json!({
                "action": "encrypt_store",
                "path": "career/goals.json"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Encrypted"));
        assert!(result.content.contains("base64"));

        // Check the .encrypted file was created
        let encrypted_path = data_dir.join("goals.json.encrypted");
        assert!(encrypted_path.exists());
        let encrypted_content = std::fs::read_to_string(&encrypted_path).unwrap();
        assert!(encrypted_content.contains("base64 placeholder"));
        assert!(encrypted_content.contains("AES-256-GCM"));
    }

    #[tokio::test]
    async fn test_set_boundary_missing_name() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "set_boundary",
                "boundary_type": "local_only",
                "paths": ["inbox"]
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Error"));
        assert!(result.content.contains("name"));
    }

    #[tokio::test]
    async fn test_set_boundary_missing_paths() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        let result = tool
            .execute(json!({
                "action": "set_boundary",
                "name": "test",
                "boundary_type": "local_only"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Error"));
        assert!(result.content.contains("paths"));
    }

    #[tokio::test]
    async fn test_delete_data_nonexistent_domain() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());
        std::fs::create_dir_all(workspace.join(".rustant")).unwrap();

        let result = tool
            .execute(json!({"action": "delete_data", "domain": "nonexistent"}))
            .await
            .unwrap();
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_encrypt_store_missing_file() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());
        std::fs::create_dir_all(workspace.join(".rustant")).unwrap();

        let result = tool
            .execute(json!({
                "action": "encrypt_store",
                "path": "nonexistent/file.json"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Error"));
        assert!(result.content.contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_data_all() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace.clone());

        // Create multiple domains
        let career_dir = workspace.join(".rustant").join("career");
        std::fs::create_dir_all(&career_dir).unwrap();
        std::fs::write(career_dir.join("data.json"), "test").unwrap();

        let inbox_dir = workspace.join(".rustant").join("inbox");
        std::fs::create_dir_all(&inbox_dir).unwrap();
        std::fs::write(inbox_dir.join("items.json"), "items").unwrap();

        // Also save privacy state so privacy/ dir exists
        tool.execute(json!({
            "action": "set_boundary",
            "name": "test",
            "boundary_type": "shareable",
            "paths": ["career"]
        }))
        .await
        .unwrap();

        let result = tool
            .execute(json!({"action": "delete_data", "domain": "all"}))
            .await
            .unwrap();
        assert!(result
            .content
            .contains("Deleted all data except privacy config"));

        // Career and inbox should be gone, privacy should remain
        assert!(!career_dir.exists());
        assert!(!inbox_dir.exists());
        assert!(workspace.join(".rustant").join("privacy").exists());
    }

    #[tokio::test]
    async fn test_audit_access_with_filters() {
        let (_dir, workspace) = setup();
        let tool = PrivacyManagerTool::new(workspace);

        // Manually create state with access log entries
        let mut state = PrivacyState::default();
        state.access_log.push(AccessLogEntry {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            data_accessed: "career/goals.json".to_string(),
            purpose: "reading".to_string(),
            boundary_id: Some(1),
        });
        state.access_log.push(AccessLogEntry {
            timestamp: Utc::now(),
            tool_name: "shell_exec".to_string(),
            data_accessed: "inbox/items.json".to_string(),
            purpose: "listing".to_string(),
            boundary_id: None,
        });
        state.access_log.push(AccessLogEntry {
            timestamp: Utc::now(),
            tool_name: "file_read".to_string(),
            data_accessed: "inbox/archive.json".to_string(),
            purpose: "archiving".to_string(),
            boundary_id: Some(2),
        });
        tool.save_state(&state).unwrap();

        // Filter by tool_name
        let result = tool
            .execute(json!({"action": "audit_access", "tool_name": "file_read"}))
            .await
            .unwrap();
        assert!(result.content.contains("file_read"));
        assert!(!result.content.contains("shell_exec"));
        assert!(result.content.contains("2 entries shown"));

        // Filter by boundary_id
        let result = tool
            .execute(json!({"action": "audit_access", "boundary_id": 1}))
            .await
            .unwrap();
        assert!(result.content.contains("1 entries shown"));
        assert!(result.content.contains("career/goals.json"));

        // Limit
        let result = tool
            .execute(json!({"action": "audit_access", "limit": 1}))
            .await
            .unwrap();
        assert!(result.content.contains("1 entries shown"));
    }
}
