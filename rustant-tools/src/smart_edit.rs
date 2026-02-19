//! Semantic code edit tool with fuzzy location matching and diff preview.
//!
//! Accepts natural language descriptions of edit locations (e.g., "the function
//! that handles authentication") and edit operations, then applies precise edits
//! with unified diff output and auto-checkpoint support.

use crate::checkpoint::CheckpointManager;
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{Artifact, RiskLevel, ToolOutput};
use similar::TextDiff;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use tracing::debug;

/// Smart editing tool that accepts fuzzy location descriptions and generates
/// precise edits with diff preview and optional auto-checkpoint.
pub struct SmartEditTool {
    workspace: PathBuf,
    checkpoint_mgr: Mutex<CheckpointManager>,
}

impl SmartEditTool {
    pub fn new(workspace: PathBuf) -> Self {
        let checkpoint_mgr = CheckpointManager::new(workspace.clone());
        Self {
            workspace,
            checkpoint_mgr: Mutex::new(checkpoint_mgr),
        }
    }
}

/// Supported edit operation types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditType {
    /// Replace matched text with new text.
    Replace,
    /// Insert new text after the matched location.
    InsertAfter,
    /// Insert new text before the matched location.
    InsertBefore,
    /// Delete the matched text.
    Delete,
}

impl EditType {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "replace" => Some(Self::Replace),
            "insert_after" | "insert-after" => Some(Self::InsertAfter),
            "insert_before" | "insert-before" => Some(Self::InsertBefore),
            "delete" | "remove" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// A located match within a file.
#[derive(Debug)]
#[allow(dead_code)]
struct LocationMatch {
    /// Start byte offset in file.
    start: usize,
    /// End byte offset in file.
    end: usize,
    /// The matched text.
    matched_text: String,
    /// Line number (1-based) where match starts.
    line_number: usize,
    /// Context lines around the match for preview.
    context_preview: String,
}

/// Find a location in file content using a search pattern.
/// Supports exact text, line-number patterns ("line 42"), and fuzzy substring matching.
fn find_location(content: &str, pattern: &str) -> Result<LocationMatch, String> {
    // Strategy 1: Try exact match first
    if let Some(start) = content.find(pattern) {
        let end = start + pattern.len();
        let line_number = content[..start].matches('\n').count() + 1;
        let preview = extract_context(content, start, end, 2);
        return Ok(LocationMatch {
            start,
            end,
            matched_text: pattern.to_string(),
            line_number,
            context_preview: preview,
        });
    }

    // Strategy 2: Line number pattern (e.g., "line 42", "lines 10-20")
    if let Some(m) = parse_line_pattern(pattern) {
        return find_by_line_range(content, m.0, m.1);
    }

    // Strategy 3: Function/method name pattern (e.g., "fn handle_request", "def process")
    if let Some(m) = find_by_function_pattern(content, pattern) {
        return Ok(m);
    }

    // Strategy 4: Fuzzy line-by-line matching using similarity scoring
    if let Some(m) = find_by_fuzzy_match(content, pattern) {
        return Ok(m);
    }

    Err(format!(
        "Could not locate '{}' in the file. Try using exact text, a line number (e.g., 'line 42'), or a function name.",
        truncate(pattern, 80)
    ))
}

/// Parse "line N" or "lines N-M" patterns.
fn parse_line_pattern(pattern: &str) -> Option<(usize, usize)> {
    let lower = pattern.trim().to_lowercase();

    // "line 42"
    if let Some(rest) = lower.strip_prefix("line ")
        && let Ok(n) = rest.trim().parse::<usize>()
    {
        return Some((n, n));
    }

    // "lines 10-20"
    if let Some(rest) = lower.strip_prefix("lines ") {
        let parts: Vec<&str> = rest.split('-').collect();
        if parts.len() == 2
            && let (Ok(a), Ok(b)) = (
                parts[0].trim().parse::<usize>(),
                parts[1].trim().parse::<usize>(),
            )
        {
            return Some((a, b));
        }
    }

    None
}

/// Find a location by line range.
fn find_by_line_range(
    content: &str,
    start_line: usize,
    end_line: usize,
) -> Result<LocationMatch, String> {
    let lines: Vec<&str> = content.lines().collect();
    let total = lines.len();

    if start_line == 0 || start_line > total {
        return Err(format!(
            "Line {start_line} is out of range (file has {total} lines)"
        ));
    }

    let end_line = end_line.min(total);
    let start_idx = start_line - 1;
    let end_idx = end_line;

    // Calculate byte offsets
    let mut byte_offset = 0;
    let mut start_byte = 0;
    let mut end_byte = content.len();

    for (i, line) in content.lines().enumerate() {
        if i == start_idx {
            start_byte = byte_offset;
        }
        byte_offset += line.len() + 1; // +1 for newline
        if i + 1 == end_idx {
            end_byte = byte_offset.min(content.len());
        }
    }

    let matched = &content[start_byte..end_byte];
    let preview = extract_context(content, start_byte, end_byte, 1);

    Ok(LocationMatch {
        start: start_byte,
        end: end_byte,
        matched_text: matched.to_string(),
        line_number: start_line,
        context_preview: preview,
    })
}

/// Find a function or block by pattern matching common language constructs.
fn find_by_function_pattern(content: &str, pattern: &str) -> Option<LocationMatch> {
    let pattern_lower = pattern.to_lowercase();

    // Common function signature prefixes
    let fn_prefixes = [
        "fn ",
        "def ",
        "func ",
        "function ",
        "pub fn ",
        "async fn ",
        "pub async fn ",
        "impl ",
        "class ",
        "struct ",
        "enum ",
    ];

    // Check if pattern looks like a function reference
    let is_fn_pattern = fn_prefixes.iter().any(|p| pattern_lower.starts_with(p))
        || pattern_lower.starts_with("the ")
        || pattern_lower.contains(" function")
        || pattern_lower.contains(" method");

    if !is_fn_pattern {
        return None;
    }

    // Extract function name from pattern
    let name = extract_identifier_from_pattern(&pattern_lower);
    if name.is_empty() {
        return None;
    }

    // Search for function definition containing this name
    for (i, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        let has_fn_keyword = fn_prefixes.iter().any(|p| line_lower.contains(p));

        if has_fn_keyword && line_lower.contains(&name) {
            let byte_start: usize = content.lines().take(i).map(|l| l.len() + 1).sum();

            // Find the end of the function block (matching braces or indentation)
            let block_end = find_block_end(content, byte_start);

            let matched = &content[byte_start..block_end];
            let preview = extract_context(content, byte_start, block_end, 0);

            return Some(LocationMatch {
                start: byte_start,
                end: block_end,
                matched_text: matched.to_string(),
                line_number: i + 1,
                context_preview: preview,
            });
        }
    }

    None
}

/// Extract a likely identifier name from a natural language pattern.
fn extract_identifier_from_pattern(pattern: &str) -> String {
    // Common language keywords to skip
    const KEYWORDS: &[&str] = &[
        "fn",
        "def",
        "func",
        "function",
        "pub",
        "async",
        "impl",
        "class",
        "struct",
        "enum",
        "let",
        "const",
        "var",
        "type",
        "trait",
        "interface",
        "the",
        "a",
        "an",
        "in",
        "of",
        "for",
        "with",
        "from",
        "to",
    ];

    // Remove common wrappers
    let cleaned = pattern
        .replace("the ", "")
        .replace(" function", "")
        .replace(" method", "")
        .replace(" that ", " ")
        .replace("called ", "");

    // Try to find a snake_case or camelCase identifier (skip keywords)
    for word in cleaned.split_whitespace() {
        let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        if w.len() >= 2
            && !KEYWORDS.contains(&w)
            && (w.contains('_')
                || w.chars().any(|c| c.is_uppercase())
                || w.chars().all(|c| c.is_alphanumeric() || c == '_'))
        {
            return w.to_string();
        }
    }

    // Fall back to last significant word (that isn't a keyword)
    cleaned
        .split_whitespace()
        .rev()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
        .find(|w| w.len() >= 2 && !KEYWORDS.contains(w))
        .unwrap_or("")
        .to_string()
}

/// Find the end of a code block starting at the given byte offset.
/// Uses brace matching for C-like languages, or indentation for Python-like.
fn find_block_end(content: &str, start: usize) -> usize {
    let rest = &content[start..];
    let lines: Vec<&str> = rest.lines().collect();

    if lines.is_empty() {
        return content.len();
    }

    // Check if the block uses braces
    let first_line = lines[0];
    let has_opening_brace = first_line.contains('{');

    if has_opening_brace {
        // Brace matching
        let mut depth = 0;
        let mut byte_pos = start;
        for ch in content[start..].chars() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return byte_pos + ch.len_utf8();
                    }
                }
                _ => {}
            }
            byte_pos += ch.len_utf8();
        }
        content.len()
    } else {
        // Indentation-based: find where indentation returns to same level
        let base_indent = first_line.len() - first_line.trim_start().len();
        let mut end = start + first_line.len() + 1;

        for line in lines.iter().skip(1) {
            if line.trim().is_empty() {
                end += line.len() + 1;
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if indent <= base_indent {
                break;
            }
            end += line.len() + 1;
        }

        end.min(content.len())
    }
}

/// Fuzzy match by finding the line with highest similarity to the pattern.
/// Uses both bigram similarity and word containment for better matching.
fn find_by_fuzzy_match(content: &str, pattern: &str) -> Option<LocationMatch> {
    let pattern_lower = pattern.to_lowercase();
    let pattern_words: Vec<&str> = pattern_lower.split_whitespace().collect();
    let mut best_score = 0.0_f64;
    let mut best_line_idx = None;

    for (i, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        let line_trimmed = line_lower.trim();
        if line_trimmed.is_empty() {
            continue;
        }

        // Combined score: bigram similarity + word containment bonus
        let bigram_score = similarity_score(line_trimmed, &pattern_lower);
        let word_score = pattern_words
            .iter()
            .filter(|w| line_trimmed.contains(**w))
            .count() as f64
            / pattern_words.len().max(1) as f64;

        let score = (bigram_score + word_score) / 2.0;
        if score > best_score && score > 0.25 {
            best_score = score;
            best_line_idx = Some(i);
        }
    }

    let line_idx = best_line_idx?;
    let byte_start: usize = content.lines().take(line_idx).map(|l| l.len() + 1).sum();
    let line_text = content.lines().nth(line_idx)?;
    let byte_end = byte_start + line_text.len();
    let preview = extract_context(content, byte_start, byte_end, 2);

    Some(LocationMatch {
        start: byte_start,
        end: byte_end,
        matched_text: line_text.to_string(),
        line_number: line_idx + 1,
        context_preview: preview,
    })
}

/// Simple similarity score between two strings (Jaccard on character bigrams).
fn similarity_score(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let bigrams_a: std::collections::HashSet<(char, char)> =
        a.chars().zip(a.chars().skip(1)).collect();
    let bigrams_b: std::collections::HashSet<(char, char)> =
        b.chars().zip(b.chars().skip(1)).collect();

    if bigrams_a.is_empty() || bigrams_b.is_empty() {
        return 0.0;
    }

    let intersection = bigrams_a.intersection(&bigrams_b).count() as f64;
    let union = bigrams_a.union(&bigrams_b).count() as f64;

    intersection / union
}

/// Extract context lines around a byte range.
fn extract_context(content: &str, start: usize, end: usize, context_lines: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start_line = content[..start].lines().count().saturating_sub(1);
    let end_line = content[..end].lines().count();

    let from = start_line.saturating_sub(context_lines);
    let to = (end_line + context_lines).min(lines.len());

    lines[from..to]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:4} | {}", from + i + 1, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate a unified diff between old and new content.
fn generate_diff(path: &str, old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    output.push_str(&format!("--- a/{path}\n"));
    output.push_str(&format!("+++ b/{path}\n"));

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        output.push_str(&format!("{hunk}"));
    }

    output
}

/// Apply an edit operation to content.
fn apply_edit(
    content: &str,
    location: &LocationMatch,
    edit_type: EditType,
    new_text: &str,
) -> String {
    match edit_type {
        EditType::Replace => {
            let mut result = String::with_capacity(content.len());
            result.push_str(&content[..location.start]);
            result.push_str(new_text);
            result.push_str(&content[location.end..]);
            result
        }
        EditType::InsertAfter => {
            let mut result = String::with_capacity(content.len() + new_text.len());
            result.push_str(&content[..location.end]);
            if !new_text.starts_with('\n') && !content[..location.end].ends_with('\n') {
                result.push('\n');
            }
            result.push_str(new_text);
            result.push_str(&content[location.end..]);
            result
        }
        EditType::InsertBefore => {
            let mut result = String::with_capacity(content.len() + new_text.len());
            result.push_str(&content[..location.start]);
            result.push_str(new_text);
            if !new_text.ends_with('\n') && !content[location.start..].starts_with('\n') {
                result.push('\n');
            }
            result.push_str(&content[location.start..]);
            result
        }
        EditType::Delete => {
            let mut result = String::with_capacity(content.len());
            result.push_str(&content[..location.start]);
            result.push_str(&content[location.end..]);
            result
        }
    }
}

/// Truncate a string for display.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

/// Validate that a path stays inside the workspace.
fn validate_workspace_path(workspace: &Path, path_str: &str) -> Result<PathBuf, ToolError> {
    let workspace_canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    let resolved = if Path::new(path_str).is_absolute() {
        PathBuf::from(path_str)
    } else {
        workspace_canonical.join(path_str)
    };

    if resolved.exists() {
        let canonical = resolved
            .canonicalize()
            .map_err(|e| ToolError::ExecutionFailed {
                name: "smart_edit".into(),
                message: format!("Path resolution failed: {e}"),
            })?;

        if !canonical.starts_with(&workspace_canonical) {
            return Err(ToolError::PermissionDenied {
                name: "smart_edit".into(),
                reason: format!("Path '{path_str}' is outside the workspace"),
            });
        }
        return Ok(canonical);
    }

    // Non-existent path: normalize components
    let mut normalized = Vec::new();
    for component in resolved.components() {
        match component {
            std::path::Component::ParentDir => {
                if normalized.pop().is_none() {
                    return Err(ToolError::PermissionDenied {
                        name: "smart_edit".into(),
                        reason: format!("Path '{path_str}' escapes the workspace"),
                    });
                }
            }
            std::path::Component::CurDir => {}
            other => normalized.push(other),
        }
    }
    let normalized_path: PathBuf = normalized.iter().collect();

    if !normalized_path.starts_with(&workspace_canonical) {
        return Err(ToolError::PermissionDenied {
            name: "smart_edit".into(),
            reason: format!("Path '{path_str}' is outside the workspace"),
        });
    }

    Ok(resolved)
}

#[async_trait]
impl Tool for SmartEditTool {
    fn name(&self) -> &str {
        "smart_edit"
    }

    fn description(&self) -> &str {
        "Smart code editor that accepts fuzzy location descriptions (function names, \
         line numbers, search patterns) and edit types (replace, insert_after, \
         insert_before, delete). Creates an auto-checkpoint before writing and \
         returns a unified diff preview."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit (relative to workspace)"
                },
                "location": {
                    "type": "string",
                    "description": "Where to apply the edit. Supports: exact text to match, \
                        'line N' or 'lines N-M', function/method names (e.g. 'fn handle_request'), \
                        or fuzzy descriptions."
                },
                "edit_type": {
                    "type": "string",
                    "enum": ["replace", "insert_after", "insert_before", "delete"],
                    "description": "Type of edit to perform"
                },
                "new_text": {
                    "type": "string",
                    "description": "The new text (required for replace, insert_after, insert_before; \
                        omit for delete)"
                }
            },
            "required": ["path", "location", "edit_type"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let path_str = args["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: "smart_edit".into(),
                reason: "'path' parameter is required".into(),
            })?;

        let location_str =
            args["location"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "smart_edit".into(),
                    reason: "'location' parameter is required".into(),
                })?;

        let edit_type_str =
            args["edit_type"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArguments {
                    name: "smart_edit".into(),
                    reason: "'edit_type' parameter is required".into(),
                })?;

        let edit_type = EditType::from_str(edit_type_str).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "smart_edit".into(),
                reason: format!(
                    "Invalid edit_type '{edit_type_str}'. Must be one of: replace, insert_after, insert_before, delete"
                ),
            }
        })?;

        let new_text = args["new_text"].as_str().unwrap_or("");

        if edit_type != EditType::Delete && new_text.is_empty() {
            return Err(ToolError::InvalidArguments {
                name: "smart_edit".into(),
                reason: "'new_text' is required for replace and insert operations".into(),
            });
        }

        // Validate path
        let _ = validate_workspace_path(&self.workspace, path_str)?;
        let path = self.workspace.join(path_str);

        // Read file
        let content =
            tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "smart_edit".into(),
                    message: format!("Failed to read '{path_str}': {e}"),
                })?;

        // Find the location
        let location =
            find_location(&content, location_str).map_err(|e| ToolError::ExecutionFailed {
                name: "smart_edit".into(),
                message: e,
            })?;

        debug!(
            "smart_edit: matched at line {} ({} bytes)",
            location.line_number,
            location.matched_text.len()
        );

        // Apply the edit
        let new_content = apply_edit(&content, &location, edit_type, new_text);

        // Generate diff
        let diff = generate_diff(path_str, &content, &new_content);

        // Create checkpoint before writing
        let checkpoint_result = {
            let mut mgr = self.checkpoint_mgr.lock().await;
            mgr.create_checkpoint(&format!("before smart_edit on {path_str}"))
        };

        if let Err(e) = &checkpoint_result {
            debug!("Checkpoint creation failed (non-fatal): {}", e);
        }

        // Write the file
        tokio::fs::write(&path, &new_content)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "smart_edit".into(),
                message: format!("Failed to write '{path_str}': {e}"),
            })?;

        // Build output
        let edit_desc = match edit_type {
            EditType::Replace => "replaced",
            EditType::InsertAfter => "inserted after",
            EditType::InsertBefore => "inserted before",
            EditType::Delete => "deleted",
        };

        let checkpoint_note = if checkpoint_result.is_ok() {
            " (checkpoint created, use /undo to revert)"
        } else {
            ""
        };

        let summary = format!(
            "Edited '{}': {} at line {}{}\n\nDiff:\n{}",
            path_str, edit_desc, location.line_number, checkpoint_note, diff
        );

        let mut output = ToolOutput::text(summary);
        output.artifacts.push(Artifact::FileModified {
            path: PathBuf::from(path_str),
            diff,
        });

        Ok(output)
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_find_location_exact() {
        let content = "fn main() {\n    println!(\"hello\");\n}\n";
        let loc = find_location(content, "println!(\"hello\")").unwrap();
        assert_eq!(loc.line_number, 2);
        assert_eq!(loc.matched_text, "println!(\"hello\")");
    }

    #[test]
    fn test_find_location_line_number() {
        let content = "line one\nline two\nline three\n";
        let loc = find_location(content, "line 2").unwrap();
        assert_eq!(loc.line_number, 2);
        assert!(loc.matched_text.contains("line two"));
    }

    #[test]
    fn test_find_location_line_range() {
        let content = "a\nb\nc\nd\ne\n";
        let loc = find_location(content, "lines 2-4").unwrap();
        assert_eq!(loc.line_number, 2);
        assert!(loc.matched_text.contains('b'));
        assert!(loc.matched_text.contains('c'));
        assert!(loc.matched_text.contains('d'));
    }

    #[test]
    fn test_find_location_function_pattern() {
        let content = "use std::io;\n\nfn handle_request(req: Request) {\n    process(req);\n}\n\nfn main() {}\n";
        let loc = find_location(content, "fn handle_request").unwrap();
        assert_eq!(loc.line_number, 3);
        assert!(loc.matched_text.contains("handle_request"));
    }

    #[test]
    fn test_find_location_fuzzy() {
        let content = "struct Config {\n    timeout: u64,\n    retries: usize,\n}\n";
        let loc = find_location(content, "timeout field").unwrap();
        assert!(loc.matched_text.contains("timeout"));
    }

    #[test]
    fn test_find_location_not_found() {
        let content = "hello world\n";
        let result = find_location(content, "nonexistent_xyz_123");
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_edit_replace() {
        let content = "fn old_name() {}\n";
        let loc = find_location(content, "old_name").unwrap();
        let result = apply_edit(content, &loc, EditType::Replace, "new_name");
        assert!(result.contains("new_name"));
        assert!(!result.contains("old_name"));
    }

    #[test]
    fn test_apply_edit_insert_after() {
        let content = "use std::io;\n\nfn main() {}\n";
        let loc = find_location(content, "use std::io;").unwrap();
        let result = apply_edit(content, &loc, EditType::InsertAfter, "use std::fs;");
        assert!(result.contains("use std::io;\nuse std::fs;"));
    }

    #[test]
    fn test_apply_edit_insert_before() {
        let content = "fn main() {}\n";
        let loc = find_location(content, "fn main").unwrap();
        let result = apply_edit(content, &loc, EditType::InsertBefore, "// Entry point\n");
        assert!(result.starts_with("// Entry point\n"));
    }

    #[test]
    fn test_apply_edit_delete() {
        let content = "line1\nline2\nline3\n";
        let loc = find_location(content, "line2").unwrap();
        let result = apply_edit(content, &loc, EditType::Delete, "");
        assert!(!result.contains("line2"));
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
    }

    #[test]
    fn test_generate_diff() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let diff = generate_diff("test.rs", old, new);
        assert!(diff.contains("--- a/test.rs"));
        assert!(diff.contains("+++ b/test.rs"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_similarity_score() {
        let a = "handle_request";
        let b = "handle_request";
        assert!((similarity_score(a, b) - 1.0).abs() < 0.01);

        let c = "handle_response";
        let score = similarity_score(a, c);
        assert!(score > 0.3); // Similar but not identical

        let d = "totally_different_thing";
        let score2 = similarity_score(a, d);
        assert!(score2 < score); // Less similar
    }

    #[test]
    fn test_edit_type_from_str() {
        assert_eq!(EditType::from_str("replace"), Some(EditType::Replace));
        assert_eq!(
            EditType::from_str("insert_after"),
            Some(EditType::InsertAfter)
        );
        assert_eq!(
            EditType::from_str("insert-before"),
            Some(EditType::InsertBefore)
        );
        assert_eq!(EditType::from_str("delete"), Some(EditType::Delete));
        assert_eq!(EditType::from_str("remove"), Some(EditType::Delete));
        assert_eq!(EditType::from_str("unknown"), None);
    }

    #[test]
    fn test_parse_line_pattern() {
        assert_eq!(parse_line_pattern("line 42"), Some((42, 42)));
        assert_eq!(parse_line_pattern("lines 10-20"), Some((10, 20)));
        assert_eq!(parse_line_pattern("not a line pattern"), None);
    }

    #[test]
    fn test_extract_identifier() {
        assert_eq!(
            extract_identifier_from_pattern("fn handle_request"),
            "handle_request"
        );
        assert_eq!(
            extract_identifier_from_pattern("the process_data function"),
            "process_data"
        );
    }

    #[test]
    fn test_find_block_end_braces() {
        let content = "fn foo() {\n    bar();\n    baz();\n}\nfn next() {}";
        let end = find_block_end(content, 0);
        let block = &content[0..end];
        assert!(block.contains("baz();"));
        assert!(block.ends_with('}'));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("a long string here", 10), "a long ...");
    }

    #[tokio::test]
    async fn test_smart_edit_tool_execute_replace() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().to_path_buf();

        // Initialize git repo for checkpoint
        git2::Repository::init(&workspace).unwrap();

        // Create a test file
        fs::write(
            workspace.join("test.rs"),
            "fn old_name() {\n    // body\n}\n",
        )
        .unwrap();

        // Initial commit so checkpoint works
        let repo = git2::Repository::open(&workspace).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let tool = SmartEditTool::new(workspace.clone());

        let args = serde_json::json!({
            "path": "test.rs",
            "location": "old_name",
            "edit_type": "replace",
            "new_text": "new_name"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("Edited"));
        assert!(result.content.contains("replaced"));

        // Verify file was modified
        let content = fs::read_to_string(workspace.join("test.rs")).unwrap();
        assert!(content.contains("new_name"));
        assert!(!content.contains("old_name"));
    }

    #[tokio::test]
    async fn test_smart_edit_tool_execute_delete() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().to_path_buf();

        git2::Repository::init(&workspace).unwrap();
        fs::write(
            workspace.join("test.txt"),
            "keep this\ndelete this line\nkeep this too\n",
        )
        .unwrap();

        let repo = git2::Repository::open(&workspace).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let tool = SmartEditTool::new(workspace.clone());

        let args = serde_json::json!({
            "path": "test.txt",
            "location": "delete this line",
            "edit_type": "delete"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("deleted"));

        let content = fs::read_to_string(workspace.join("test.txt")).unwrap();
        assert!(!content.contains("delete this line"));
        assert!(content.contains("keep this"));
    }

    #[tokio::test]
    async fn test_smart_edit_tool_line_number() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().to_path_buf();

        git2::Repository::init(&workspace).unwrap();
        fs::write(workspace.join("test.txt"), "line 1\nline 2\nline 3\n").unwrap();

        let repo = git2::Repository::open(&workspace).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let tool = SmartEditTool::new(workspace.clone());

        let args = serde_json::json!({
            "path": "test.txt",
            "location": "line 2",
            "edit_type": "replace",
            "new_text": "replaced line\n"
        });

        let result = tool.execute(args).await.unwrap();
        assert!(result.content.contains("replaced"));

        let content = fs::read_to_string(workspace.join("test.txt")).unwrap();
        assert!(content.contains("replaced line"));
        assert!(!content.contains("line 2"));
    }

    #[tokio::test]
    async fn test_smart_edit_tool_missing_new_text() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().to_path_buf();
        let tool = SmartEditTool::new(workspace);

        let args = serde_json::json!({
            "path": "test.txt",
            "location": "something",
            "edit_type": "replace"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_smart_edit_tool_invalid_edit_type() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().to_path_buf();
        let tool = SmartEditTool::new(workspace);

        let args = serde_json::json!({
            "path": "test.txt",
            "location": "something",
            "edit_type": "invalid_op",
            "new_text": "hello"
        });

        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
