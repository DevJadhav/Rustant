//! Auto-fix suggestion engine — generates and applies code fixes.
//!
//! Safety-gated writes with git checkpointing. Supports batch fixes
//! across multiple files with dry-run validation.

use crate::error::ReviewError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// A suggested code fix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixSuggestion {
    /// Unique identifier.
    pub id: Uuid,
    /// File to modify.
    pub file: PathBuf,
    /// Start line of the code to replace (1-indexed).
    pub start_line: usize,
    /// End line of the code to replace (1-indexed).
    pub end_line: usize,
    /// Original code that would be replaced.
    pub original: String,
    /// Replacement code.
    pub replacement: String,
    /// Description of the fix.
    pub description: String,
    /// Category of the fix.
    pub category: FixCategory,
    /// Confidence that this fix is correct (0.0-1.0).
    pub confidence: f32,
    /// Whether this fix has been validated (linting, type checking).
    pub validated: bool,
}

/// Category of a code fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixCategory {
    /// Security vulnerability fix.
    Security,
    /// Bug fix.
    BugFix,
    /// Performance improvement.
    Performance,
    /// Code style/formatting.
    Style,
    /// Complexity reduction / refactor.
    Refactor,
    /// Documentation improvement.
    Documentation,
}

impl std::fmt::Display for FixCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FixCategory::Security => write!(f, "security"),
            FixCategory::BugFix => write!(f, "bug fix"),
            FixCategory::Performance => write!(f, "performance"),
            FixCategory::Style => write!(f, "style"),
            FixCategory::Refactor => write!(f, "refactor"),
            FixCategory::Documentation => write!(f, "documentation"),
        }
    }
}

/// Result of applying fixes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixResult {
    /// Fixes successfully applied.
    pub applied: Vec<Uuid>,
    /// Fixes that failed to apply.
    pub failed: Vec<(Uuid, String)>,
    /// Fixes skipped (low confidence or user rejection).
    pub skipped: Vec<Uuid>,
    /// Git checkpoint commit hash (for rollback).
    pub checkpoint: Option<String>,
}

/// A batch of fixes to apply atomically.
#[derive(Debug, Clone)]
pub struct FixBatch {
    /// All suggestions in this batch.
    pub suggestions: Vec<FixSuggestion>,
    /// Minimum confidence threshold for auto-application.
    pub min_confidence: f32,
    /// Whether to create a git checkpoint before applying.
    pub create_checkpoint: bool,
    /// Whether to run validation after applying.
    pub run_validation: bool,
}

impl FixBatch {
    /// Create a new batch from suggestions.
    pub fn new(suggestions: Vec<FixSuggestion>) -> Self {
        Self {
            suggestions,
            min_confidence: 0.85,
            create_checkpoint: true,
            run_validation: true,
        }
    }

    /// Set minimum confidence threshold.
    pub fn with_min_confidence(mut self, confidence: f32) -> Self {
        self.min_confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Get suggestions that pass the confidence threshold.
    pub fn auto_applicable(&self) -> Vec<&FixSuggestion> {
        self.suggestions
            .iter()
            .filter(|s| s.confidence >= self.min_confidence)
            .collect()
    }

    /// Get suggestions grouped by file.
    pub fn by_file(&self) -> std::collections::HashMap<&Path, Vec<&FixSuggestion>> {
        let mut map = std::collections::HashMap::new();
        for suggestion in &self.suggestions {
            map.entry(suggestion.file.as_path())
                .or_insert_with(Vec::new)
                .push(suggestion);
        }
        // Sort each file's suggestions by start_line (descending) for safe application
        for fixes in map.values_mut() {
            fixes.sort_by(|a, b| b.start_line.cmp(&a.start_line));
        }
        map
    }
}

/// Generate a unified diff patch for a fix suggestion.
pub fn generate_patch(suggestion: &FixSuggestion) -> String {
    let mut patch = String::new();
    patch.push_str(&format!("--- a/{}\n", suggestion.file.display()));
    patch.push_str(&format!("+++ b/{}\n", suggestion.file.display()));
    patch.push_str(&format!(
        "@@ -{},{} +{},{} @@\n",
        suggestion.start_line,
        suggestion.original.lines().count(),
        suggestion.start_line,
        suggestion.replacement.lines().count(),
    ));

    for line in suggestion.original.lines() {
        patch.push_str(&format!("-{line}\n"));
    }
    for line in suggestion.replacement.lines() {
        patch.push_str(&format!("+{line}\n"));
    }

    patch
}

/// Apply a fix suggestion to source code.
///
/// Returns the modified source code or an error.
pub fn apply_fix(source: &str, suggestion: &FixSuggestion) -> Result<String, ReviewError> {
    let lines: Vec<&str> = source.lines().collect();

    if suggestion.start_line == 0 || suggestion.start_line > lines.len() {
        return Err(ReviewError::FixApplication(format!(
            "start_line {} out of range (file has {} lines)",
            suggestion.start_line,
            lines.len()
        )));
    }
    if suggestion.end_line > lines.len() {
        return Err(ReviewError::FixApplication(format!(
            "end_line {} out of range (file has {} lines)",
            suggestion.end_line,
            lines.len()
        )));
    }
    if suggestion.start_line > suggestion.end_line {
        return Err(ReviewError::FixApplication(
            "start_line > end_line".to_string(),
        ));
    }

    let start_idx = suggestion.start_line - 1;
    let end_idx = suggestion.end_line;

    let mut result = String::new();

    // Lines before the fix
    for line in &lines[..start_idx] {
        result.push_str(line);
        result.push('\n');
    }

    // The replacement
    result.push_str(&suggestion.replacement);
    if !suggestion.replacement.ends_with('\n') {
        result.push('\n');
    }

    // Lines after the fix
    for line in &lines[end_idx..] {
        result.push_str(line);
        result.push('\n');
    }

    // Remove trailing newline if original didn't have one
    if !source.ends_with('\n') && result.ends_with('\n') {
        result.pop();
    }

    Ok(result)
}

/// Apply multiple non-overlapping fixes to source code.
///
/// Fixes must be sorted by start_line descending to avoid offset issues.
pub fn apply_fixes(source: &str, suggestions: &[&FixSuggestion]) -> Result<String, ReviewError> {
    // Verify no overlaps
    let mut sorted: Vec<&&FixSuggestion> = suggestions.iter().collect();
    sorted.sort_by(|a, b| a.start_line.cmp(&b.start_line));

    for window in sorted.windows(2) {
        if window[0].end_line >= window[1].start_line && window[0].file == window[1].file {
            return Err(ReviewError::FixApplication(format!(
                "Overlapping fixes at lines {}-{} and {}-{}",
                window[0].start_line, window[0].end_line, window[1].start_line, window[1].end_line,
            )));
        }
    }

    // Apply from bottom to top to preserve line numbers
    let mut result = source.to_string();
    for suggestion in suggestions.iter().rev() {
        result = apply_fix(&result, suggestion)?;
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_fix(start: usize, end: usize, original: &str, replacement: &str) -> FixSuggestion {
        FixSuggestion {
            id: Uuid::new_v4(),
            file: PathBuf::from("test.rs"),
            start_line: start,
            end_line: end,
            original: original.to_string(),
            replacement: replacement.to_string(),
            description: "Test fix".to_string(),
            category: FixCategory::BugFix,
            confidence: 0.9,
            validated: false,
        }
    }

    #[test]
    fn test_apply_single_fix() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let fix = make_fix(2, 3, "line2\nline3", "replaced2\nreplaced3");
        let result = apply_fix(source, &fix).unwrap();
        assert!(result.contains("replaced2"));
        assert!(result.contains("replaced3"));
        assert!(result.contains("line1"));
        assert!(result.contains("line4"));
    }

    #[test]
    fn test_apply_fix_out_of_range() {
        let source = "line1\nline2";
        let fix = make_fix(5, 6, "x", "y");
        let result = apply_fix(source, &fix);
        assert!(result.is_err());
    }

    #[test]
    fn test_apply_fix_start_gt_end() {
        let source = "line1\nline2\nline3";
        let fix = make_fix(3, 1, "x", "y");
        let result = apply_fix(source, &fix);
        assert!(result.is_err());
    }

    #[test]
    fn test_generate_patch() {
        let fix = make_fix(10, 12, "old code\nmore old", "new code\nmore new\nextra");
        let patch = generate_patch(&fix);
        assert!(patch.contains("--- a/test.rs"));
        assert!(patch.contains("+++ b/test.rs"));
        assert!(patch.contains("-old code"));
        assert!(patch.contains("+new code"));
    }

    #[test]
    fn test_fix_batch_auto_applicable() {
        let fixes = vec![make_fix(1, 1, "a", "b"), {
            let mut f = make_fix(3, 3, "c", "d");
            f.confidence = 0.5;
            f
        }];
        let batch = FixBatch::new(fixes).with_min_confidence(0.85);
        assert_eq!(batch.auto_applicable().len(), 1);
    }

    #[test]
    fn test_fix_batch_by_file() {
        let fixes = vec![make_fix(1, 1, "a", "b"), make_fix(5, 5, "c", "d")];
        let batch = FixBatch::new(fixes);
        let by_file = batch.by_file();
        assert_eq!(by_file.len(), 1); // All in test.rs
        assert_eq!(by_file[Path::new("test.rs")].len(), 2);
    }

    #[test]
    fn test_apply_multiple_non_overlapping() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let fix1 = make_fix(1, 1, "line1", "new1");
        let fix2 = make_fix(4, 4, "line4", "new4");
        let result = apply_fixes(source, &[&fix1, &fix2]).unwrap();
        assert!(result.contains("new1"));
        assert!(result.contains("line2"));
        assert!(result.contains("new4"));
    }

    #[test]
    fn test_apply_overlapping_fails() {
        let source = "line1\nline2\nline3\nline4\nline5";
        let fix1 = make_fix(1, 3, "x", "y");
        let fix2 = make_fix(2, 4, "x", "y");
        let result = apply_fixes(source, &[&fix1, &fix2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_fix_category_display() {
        assert_eq!(FixCategory::Security.to_string(), "security");
        assert_eq!(FixCategory::BugFix.to_string(), "bug fix");
        assert_eq!(FixCategory::Refactor.to_string(), "refactor");
    }
}
