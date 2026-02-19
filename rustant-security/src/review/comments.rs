//! AI-powered review comments — code review comment model and generation.
//!
//! Provides the review comment data model, categories, severity scoring,
//! and review engine skeleton for generating code review feedback.

use crate::finding::FindingSeverity;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Category of a review comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentCategory {
    /// Potential bug or logic error.
    Bug,
    /// Security vulnerability or concern.
    Security,
    /// Performance issue or optimization opportunity.
    Performance,
    /// Code style or convention violation.
    Style,
    /// Maintainability concern (readability, complexity).
    Maintainability,
    /// Missing or incorrect documentation.
    Documentation,
    /// Testing concern (missing tests, edge cases).
    Testing,
}

impl std::fmt::Display for CommentCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommentCategory::Bug => write!(f, "bug"),
            CommentCategory::Security => write!(f, "security"),
            CommentCategory::Performance => write!(f, "performance"),
            CommentCategory::Style => write!(f, "style"),
            CommentCategory::Maintainability => write!(f, "maintainability"),
            CommentCategory::Documentation => write!(f, "documentation"),
            CommentCategory::Testing => write!(f, "testing"),
        }
    }
}

/// A single review comment on a code change.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewComment {
    /// Unique identifier.
    pub id: Uuid,
    /// File path the comment applies to.
    pub file_path: String,
    /// Line number (1-indexed) the comment targets.
    pub line: usize,
    /// Optional end line for multi-line comments.
    pub end_line: Option<usize>,
    /// Category of the comment.
    pub category: CommentCategory,
    /// Severity (1-5 scale, 5 = most severe).
    pub severity: u8,
    /// The comment body (markdown).
    pub body: String,
    /// Optional code suggestion (replacement text).
    pub suggestion: Option<String>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Step-by-step reasoning for why this was flagged.
    pub reasoning: Vec<String>,
}

impl ReviewComment {
    /// Map severity to FindingSeverity.
    pub fn finding_severity(&self) -> FindingSeverity {
        match self.severity {
            5 => FindingSeverity::Critical,
            4 => FindingSeverity::High,
            3 => FindingSeverity::Medium,
            2 => FindingSeverity::Low,
            _ => FindingSeverity::Info,
        }
    }

    /// Whether this comment has a code suggestion.
    pub fn has_suggestion(&self) -> bool {
        self.suggestion.is_some()
    }
}

/// Result of a code review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewResult {
    /// All review comments generated.
    pub comments: Vec<ReviewComment>,
    /// Summary of the review.
    pub summary: String,
    /// Overall quality assessment.
    pub quality_summary: Option<String>,
    /// Number of critical/high severity comments.
    pub critical_count: usize,
    /// Whether the review recommends blocking the change.
    pub should_block: bool,
}

impl ReviewResult {
    /// Create a new review result from comments.
    pub fn from_comments(comments: Vec<ReviewComment>, summary: String) -> Self {
        let critical_count = comments.iter().filter(|c| c.severity >= 4).count();
        let should_block = comments.iter().any(|c| c.severity >= 5);

        Self {
            comments,
            summary,
            quality_summary: None,
            critical_count,
            should_block,
        }
    }

    /// Get comments for a specific file.
    pub fn comments_for_file(&self, path: &str) -> Vec<&ReviewComment> {
        self.comments
            .iter()
            .filter(|c| c.file_path == path)
            .collect()
    }

    /// Get comments by category.
    pub fn comments_by_category(&self, category: CommentCategory) -> Vec<&ReviewComment> {
        self.comments
            .iter()
            .filter(|c| c.category == category)
            .collect()
    }

    /// Get comments sorted by severity (highest first).
    pub fn comments_by_severity(&self) -> Vec<&ReviewComment> {
        let mut sorted: Vec<&ReviewComment> = self.comments.iter().collect();
        sorted.sort_by(|a, b| b.severity.cmp(&a.severity));
        sorted
    }
}

/// Builder for constructing review comments.
pub struct ReviewCommentBuilder {
    file_path: String,
    line: usize,
    end_line: Option<usize>,
    category: CommentCategory,
    severity: u8,
    body: String,
    suggestion: Option<String>,
    confidence: f32,
    reasoning: Vec<String>,
}

impl ReviewCommentBuilder {
    /// Start building a review comment.
    pub fn new(file_path: &str, line: usize, category: CommentCategory) -> Self {
        Self {
            file_path: file_path.to_string(),
            line,
            end_line: None,
            category,
            severity: 1,
            body: String::new(),
            suggestion: None,
            confidence: 0.5,
            reasoning: Vec::new(),
        }
    }

    /// Set the end line for multi-line comments.
    pub fn end_line(mut self, end_line: usize) -> Self {
        self.end_line = Some(end_line);
        self
    }

    /// Set the severity (1-5).
    pub fn severity(mut self, severity: u8) -> Self {
        self.severity = severity.clamp(1, 5);
        self
    }

    /// Set the comment body.
    pub fn body(mut self, body: &str) -> Self {
        self.body = body.to_string();
        self
    }

    /// Set a code suggestion.
    pub fn suggestion(mut self, suggestion: &str) -> Self {
        self.suggestion = Some(suggestion.to_string());
        self
    }

    /// Set confidence (0.0 to 1.0).
    pub fn confidence(mut self, confidence: f32) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    /// Add a reasoning step.
    pub fn reason(mut self, step: &str) -> Self {
        self.reasoning.push(step.to_string());
        self
    }

    /// Build the review comment.
    pub fn build(self) -> ReviewComment {
        ReviewComment {
            id: Uuid::new_v4(),
            file_path: self.file_path,
            line: self.line,
            end_line: self.end_line,
            category: self.category,
            severity: self.severity,
            body: self.body,
            suggestion: self.suggestion,
            confidence: self.confidence,
            reasoning: self.reasoning,
        }
    }
}

use crate::ast::{self, Language, SymbolKind};
use regex::Regex;
use std::path::Path;

/// Thresholds for review engine checks.
const COMPLEXITY_THRESHOLD: u32 = 15;
const LONG_FUNCTION_LINES: usize = 50;
const MAX_NESTING_DEPTH: usize = 4;

/// Engine that generates review comments from static analysis of source code.
///
/// Uses AST-based symbol extraction and cyclomatic complexity together with
/// lightweight pattern-based checks (unwrap, magic numbers, deep nesting,
/// TODO/FIXME markers) to produce actionable review feedback.
pub struct ReviewEngine {
    /// Cyclomatic complexity threshold above which a function is flagged.
    pub complexity_threshold: u32,
    /// Maximum function length (in lines) before flagging.
    pub long_function_lines: usize,
    /// Maximum nesting depth before flagging.
    pub max_nesting_depth: usize,
}

impl Default for ReviewEngine {
    fn default() -> Self {
        Self {
            complexity_threshold: COMPLEXITY_THRESHOLD,
            long_function_lines: LONG_FUNCTION_LINES,
            max_nesting_depth: MAX_NESTING_DEPTH,
        }
    }
}

impl ReviewEngine {
    /// Create a new review engine with default thresholds.
    pub fn new() -> Self {
        Self::default()
    }

    /// Generate review comments for a single source file.
    ///
    /// Combines AST-based analysis (complexity, function length) with
    /// pattern-based checks (unwrap, magic numbers, nesting, TODO/FIXME).
    pub fn review_file(&self, path: &Path, source: &str, language: &str) -> Vec<ReviewComment> {
        let mut comments = Vec::new();
        let file_path = path.display().to_string();

        let lang = Language::from_extension(language);

        // --- AST-based checks: complexity and function length ---
        let symbols = ast::extract_symbols(source, lang, path).unwrap_or_default();
        let lines: Vec<&str> = source.lines().collect();

        for sym in &symbols {
            if !matches!(sym.kind, SymbolKind::Function | SymbolKind::Method) {
                continue;
            }

            // When start_line == end_line (regex fallback), find the actual
            // function end by scanning for matching braces.
            let (func_start, func_end) = if sym.start_line == sym.end_line {
                let end = self.find_function_end(&lines, sym.start_line);
                (sym.start_line, end)
            } else {
                (sym.start_line, sym.end_line)
            };

            let func_lines = self.extract_function_lines(source, func_start, func_end);
            let complexity = ast::cyclomatic_complexity(&func_lines, lang);

            // High cyclomatic complexity
            if complexity > self.complexity_threshold {
                comments.push(
                    ReviewCommentBuilder::new(
                        &file_path,
                        func_start,
                        CommentCategory::Maintainability,
                    )
                    .severity(4)
                    .body(&format!(
                        "Function `{}` has cyclomatic complexity of {} (threshold: {}). \
                             Consider breaking it into smaller functions.",
                        sym.name, complexity, self.complexity_threshold
                    ))
                    .end_line(func_end)
                    .confidence(0.9)
                    .reason(&format!(
                        "Cyclomatic complexity {} exceeds threshold {}",
                        complexity, self.complexity_threshold
                    ))
                    .build(),
                );
            }

            // Long functions
            let func_line_count = func_end.saturating_sub(func_start) + 1;
            if func_line_count > self.long_function_lines {
                comments.push(
                    ReviewCommentBuilder::new(
                        &file_path,
                        func_start,
                        CommentCategory::Maintainability,
                    )
                    .severity(3)
                    .body(&format!(
                        "Function `{}` is {} lines long (threshold: {}). \
                             Long functions are harder to understand and test.",
                        sym.name, func_line_count, self.long_function_lines
                    ))
                    .end_line(func_end)
                    .confidence(0.85)
                    .reason(&format!(
                        "Function length {} exceeds threshold {}",
                        func_line_count, self.long_function_lines
                    ))
                    .build(),
                );
            }
        }

        // --- Pattern-based checks on individual lines ---
        self.check_unwrap_calls(&lines, &file_path, &mut comments);
        self.check_todo_fixme(&lines, &file_path, &mut comments);
        self.check_magic_numbers(&lines, &file_path, lang, &mut comments);
        self.check_deep_nesting(&lines, &file_path, lang, &mut comments);

        comments
    }

    /// Generate review comments for diff hunks.
    ///
    /// Each hunk is a `(start_line, content)` pair where `content` is the
    /// unified-diff text of the hunk. Only *added* lines (those starting
    /// with `+` but not `+++`) are inspected.
    pub fn review_diff(&self, diff_hunks: &[(usize, String)]) -> Vec<ReviewComment> {
        let mut comments = Vec::new();
        let file_path = "diff".to_string();

        for (hunk_start, hunk_content) in diff_hunks {
            let mut line_offset = 0usize;

            for line in hunk_content.lines() {
                // Only inspect added lines (skip diff headers and context)
                if line.starts_with('+') && !line.starts_with("+++") {
                    let content = &line[1..]; // strip leading '+'
                    let current_line = hunk_start + line_offset;

                    // New unwrap() calls
                    if content.contains(".unwrap()") {
                        comments.push(
                            ReviewCommentBuilder::new(
                                &file_path,
                                current_line,
                                CommentCategory::Bug,
                            )
                            .severity(3)
                            .body(
                                "New `.unwrap()` call added. Consider using `?`, \
                                     `.expect(\"reason\")`, or proper error handling instead.",
                            )
                            .confidence(0.8)
                            .reason("unwrap() can panic at runtime on None/Err values")
                            .build(),
                        );
                    }

                    // New TODO/FIXME markers
                    let upper = content.to_uppercase();
                    if upper.contains("TODO") || upper.contains("FIXME") {
                        comments.push(
                            ReviewCommentBuilder::new(
                                &file_path,
                                current_line,
                                CommentCategory::Maintainability,
                            )
                            .severity(2)
                            .body(
                                "New TODO/FIXME comment added. Consider creating \
                                     a tracking issue instead of leaving inline markers.",
                            )
                            .confidence(0.75)
                            .reason("TODO/FIXME markers indicate deferred work")
                            .build(),
                        );
                    }
                }

                // Track line offsets for added and context lines
                // (removed lines don't correspond to new file lines)
                if !line.starts_with('-') || line.starts_with("---") {
                    line_offset += 1;
                }
            }

            // Check for large function additions in the hunk
            let added_lines: Vec<&str> = hunk_content
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .collect();

            if added_lines.len() > self.long_function_lines {
                // Check if this looks like a new function
                let has_function_def = added_lines.iter().any(|l| {
                    let trimmed = l.trim_start_matches('+').trim();
                    trimmed.contains("fn ")
                        || trimmed.contains("def ")
                        || trimmed.contains("function ")
                        || trimmed.contains("func ")
                });

                if has_function_def {
                    comments.push(
                        ReviewCommentBuilder::new(
                            &file_path,
                            *hunk_start,
                            CommentCategory::Maintainability,
                        )
                        .severity(3)
                        .body(&format!(
                            "Large function addition ({} lines). Consider breaking \
                                 it into smaller, more focused functions.",
                            added_lines.len()
                        ))
                        .confidence(0.7)
                        .reason(&format!(
                            "Hunk adds {} lines containing a function definition",
                            added_lines.len()
                        ))
                        .build(),
                    );
                }
            }
        }

        comments
    }

    // --- Private helpers ---

    /// Find the end line of a function starting at `start_line` (1-indexed)
    /// by scanning for matching braces. Returns the line number of the
    /// closing brace, or `start_line` if no opening brace is found.
    fn find_function_end(&self, lines: &[&str], start_line: usize) -> usize {
        let mut depth: i32 = 0;
        let mut found_open = false;

        for (i, line) in lines.iter().enumerate().skip(start_line.saturating_sub(1)) {
            for ch in line.chars() {
                if ch == '{' {
                    depth += 1;
                    found_open = true;
                } else if ch == '}' {
                    depth -= 1;
                    if found_open && depth == 0 {
                        return i + 1; // 1-indexed
                    }
                }
            }
        }

        // Fallback: couldn't find matching brace
        start_line
    }

    /// Extract function body text from source lines (1-indexed).
    fn extract_function_lines(&self, source: &str, start: usize, end: usize) -> String {
        source
            .lines()
            .skip(start.saturating_sub(1))
            .take(end.saturating_sub(start.saturating_sub(1)))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check for `.unwrap()` calls that may panic at runtime.
    fn check_unwrap_calls(
        &self,
        lines: &[&str],
        file_path: &str,
        comments: &mut Vec<ReviewComment>,
    ) {
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Skip lines that are comments
            if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
                continue;
            }
            // Skip test code
            if trimmed.contains("#[test]") || trimmed.contains("#[cfg(test)]") {
                continue;
            }

            if trimmed.contains(".unwrap()") {
                comments.push(
                    ReviewCommentBuilder::new(file_path, i + 1, CommentCategory::Bug)
                        .severity(3)
                        .body(
                            "`.unwrap()` can panic at runtime. Consider using `?`, \
                             `.expect(\"descriptive message\")`, or pattern matching.",
                        )
                        .suggestion(&trimmed.replace(".unwrap()", ".expect(\"TODO: add context\")"))
                        .confidence(0.8)
                        .reason("unwrap() on None/Err causes a panic with no context")
                        .build(),
                );
            }
        }
    }

    /// Check for TODO and FIXME comments.
    fn check_todo_fixme(&self, lines: &[&str], file_path: &str, comments: &mut Vec<ReviewComment>) {
        let re = Regex::new(r"(?i)\b(TODO|FIXME)\b").expect("valid regex");

        for (i, line) in lines.iter().enumerate() {
            if re.is_match(line) {
                let is_fixme = line.to_uppercase().contains("FIXME");
                let severity = if is_fixme { 3 } else { 2 };
                let label = if is_fixme { "FIXME" } else { "TODO" };

                comments.push(
                    ReviewCommentBuilder::new(file_path, i + 1, CommentCategory::Maintainability)
                        .severity(severity)
                        .body(&format!(
                            "{label} comment found. Track this as an issue to ensure it gets resolved."
                        ))
                        .confidence(0.9)
                        .reason(&format!("{label} marker indicates incomplete or deferred work"))
                        .build(),
                );
            }
        }
    }

    /// Check for hardcoded magic numbers in non-trivial positions.
    ///
    /// Flags numeric literals that are not 0, 1, 2, or common small values
    /// when they appear outside of constant/static declarations.
    fn check_magic_numbers(
        &self,
        lines: &[&str],
        file_path: &str,
        language: Language,
        comments: &mut Vec<ReviewComment>,
    ) {
        // Match standalone integer literals >= 3 that aren't in const/static/enum lines
        let num_re = Regex::new(r"\b(\d+)\b").expect("valid regex");

        let const_keywords: &[&str] = match language {
            Language::Rust => &["const ", "static ", "enum ", "#[", "assert"],
            Language::Python => &["=", "range(", "# "],
            Language::Go => &["const ", "var ", "case "],
            _ => &["const ", "static ", "final ", "enum "],
        };

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Skip comments, blank lines, and constant/config declarations
            if trimmed.is_empty()
                || trimmed.starts_with("//")
                || trimmed.starts_with('#')
                || trimmed.starts_with('*')
                || trimmed.starts_with("use ")
                || trimmed.starts_with("import ")
            {
                continue;
            }

            // Skip lines that are defining constants
            if const_keywords.iter().any(|kw| trimmed.contains(kw)) {
                continue;
            }

            for cap in num_re.captures_iter(trimmed) {
                if let Some(m) = cap.get(1)
                    && let Ok(n) = m.as_str().parse::<i64>()
                {
                    // Flag numbers that are "magic" (not 0, 1, or 2)
                    // and not part of common patterns like array indexing
                    if !(-1..=100).contains(&n) {
                        comments.push(
                            ReviewCommentBuilder::new(
                                file_path,
                                i + 1,
                                CommentCategory::Maintainability,
                            )
                            .severity(2)
                            .body(&format!(
                                "Magic number `{n}` found. Consider extracting it \
                                 into a named constant for clarity."
                            ))
                            .confidence(0.6)
                            .reason(
                                "Hardcoded numeric literals reduce readability and maintainability",
                            )
                            .build(),
                        );
                        // Only flag the first magic number per line
                        break;
                    }
                }
            }
        }
    }

    /// Check for deeply nested code blocks.
    ///
    /// Counts brace-based nesting depth for C-like languages and
    /// indentation-based depth for Python.
    fn check_deep_nesting(
        &self,
        lines: &[&str],
        file_path: &str,
        language: Language,
        comments: &mut Vec<ReviewComment>,
    ) {
        let nesting_comments = match language {
            Language::Python => self.check_deep_nesting_indent(lines, file_path),
            _ => self.check_deep_nesting_braces(lines, file_path),
        };
        comments.extend(nesting_comments);
    }

    /// Brace-based nesting depth check for C-like languages.
    fn check_deep_nesting_braces(&self, lines: &[&str], file_path: &str) -> Vec<ReviewComment> {
        let mut comments = Vec::new();
        let mut depth: usize = 0;
        let mut flagged_depths: std::collections::HashSet<usize> = std::collections::HashSet::new();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Skip pure comment lines
            if trimmed.starts_with("//") || trimmed.starts_with('#') || trimmed.starts_with('*') {
                continue;
            }

            let opens = trimmed.matches('{').count();
            let closes = trimmed.matches('}').count();

            depth = depth.saturating_add(opens);

            if depth > self.max_nesting_depth && !flagged_depths.contains(&depth) {
                flagged_depths.insert(depth);
                comments.push(
                    ReviewCommentBuilder::new(file_path, i + 1, CommentCategory::Maintainability)
                        .severity(3)
                        .body(&format!(
                            "Code is nested {} levels deep (threshold: {}). \
                             Consider extracting inner logic into separate functions \
                             or using early returns.",
                            depth, self.max_nesting_depth
                        ))
                        .confidence(0.75)
                        .reason(&format!(
                            "Nesting depth {} exceeds maximum {}",
                            depth, self.max_nesting_depth
                        ))
                        .build(),
                );
            }

            depth = depth.saturating_sub(closes);
        }

        comments
    }

    /// Indentation-based nesting depth check for Python.
    fn check_deep_nesting_indent(&self, lines: &[&str], file_path: &str) -> Vec<ReviewComment> {
        let mut comments = Vec::new();
        let mut flagged = false;

        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() || line.trim().starts_with('#') {
                continue;
            }

            let indent = line.len() - line.trim_start().len();
            // Assume 4-space indent = 1 nesting level
            let depth = indent / 4;

            if depth > self.max_nesting_depth && !flagged {
                flagged = true;
                comments.push(
                    ReviewCommentBuilder::new(file_path, i + 1, CommentCategory::Maintainability)
                        .severity(3)
                        .body(&format!(
                            "Code is nested {} levels deep (threshold: {}). \
                             Consider refactoring to reduce nesting.",
                            depth, self.max_nesting_depth
                        ))
                        .confidence(0.7)
                        .reason(&format!(
                            "Indentation depth {} exceeds maximum {}",
                            depth, self.max_nesting_depth
                        ))
                        .build(),
                );
            }
        }

        comments
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_comment_builder() {
        let comment = ReviewCommentBuilder::new("src/main.rs", 42, CommentCategory::Bug)
            .severity(4)
            .body("Potential null pointer dereference")
            .confidence(0.85)
            .reason("Variable `x` may be None at this point")
            .reason("No null check before dereference")
            .build();

        assert_eq!(comment.file_path, "src/main.rs");
        assert_eq!(comment.line, 42);
        assert_eq!(comment.severity, 4);
        assert_eq!(comment.category, CommentCategory::Bug);
        assert_eq!(comment.confidence, 0.85);
        assert_eq!(comment.reasoning.len(), 2);
        assert_eq!(comment.finding_severity(), FindingSeverity::High);
    }

    #[test]
    fn test_review_result() {
        let comments = vec![
            ReviewCommentBuilder::new("a.rs", 1, CommentCategory::Bug)
                .severity(5)
                .body("Critical bug")
                .build(),
            ReviewCommentBuilder::new("b.rs", 10, CommentCategory::Style)
                .severity(2)
                .body("Style issue")
                .build(),
        ];

        let result = ReviewResult::from_comments(comments, "Review summary".into());

        assert_eq!(result.critical_count, 1);
        assert!(result.should_block);
        assert_eq!(result.comments_for_file("a.rs").len(), 1);
        assert_eq!(result.comments_by_category(CommentCategory::Style).len(), 1);
    }

    #[test]
    fn test_severity_mapping() {
        let comment = ReviewCommentBuilder::new("test.rs", 1, CommentCategory::Security)
            .severity(5)
            .build();
        assert_eq!(comment.finding_severity(), FindingSeverity::Critical);

        let comment = ReviewCommentBuilder::new("test.rs", 1, CommentCategory::Style)
            .severity(1)
            .build();
        assert_eq!(comment.finding_severity(), FindingSeverity::Info);
    }

    #[test]
    fn test_comment_category_display() {
        assert_eq!(CommentCategory::Bug.to_string(), "bug");
        assert_eq!(CommentCategory::Security.to_string(), "security");
        assert_eq!(CommentCategory::Performance.to_string(), "performance");
    }

    // --- ReviewEngine tests ---

    #[test]
    fn test_review_file_detects_high_complexity() {
        // This function body has complexity > 15:
        // base(1) + if(5) + &&(3) + ||(3) + while(1) + for(1) + match(1) + loop(1) = 16+
        let source = r#"
fn complex_function(x: i32) -> i32 {
    if x > 0 && x < 1000 {
        if x > 10 && x < 100 {
            if x > 50 || x < 25 {
                while x > 0 {
                    for i in 0..x {
                        if i > 5 && i < 10 {
                            match x {
                                1 => 1,
                                2 => 2,
                                _ => {
                                    if x > 99 || x < 3 {
                                        loop {
                                            if x > 0 || x < 200 {
                                                break;
                                            }
                                        }
                                        0
                                    } else {
                                        1
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    0
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("complex.rs"), source, "rs");

        // Should flag the function for high cyclomatic complexity
        let complexity_comments: Vec<_> = comments
            .iter()
            .filter(|c| {
                c.category == CommentCategory::Maintainability
                    && c.body.contains("cyclomatic complexity")
            })
            .collect();
        assert!(
            !complexity_comments.is_empty(),
            "Expected a complexity comment for the highly complex function"
        );
        // Severity should be 4 (high)
        assert_eq!(complexity_comments[0].severity, 4);
    }

    #[test]
    fn test_review_file_detects_unwrap_calls() {
        let source = r#"
fn risky() {
    let val = some_option.unwrap();
    let other = another.unwrap();
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("risky.rs"), source, "rs");

        let unwrap_comments: Vec<_> = comments
            .iter()
            .filter(|c| c.category == CommentCategory::Bug && c.body.contains("unwrap"))
            .collect();
        assert_eq!(
            unwrap_comments.len(),
            2,
            "Expected two unwrap comments, got {}",
            unwrap_comments.len()
        );
        // Each should have a suggestion
        assert!(unwrap_comments[0].has_suggestion());
    }

    #[test]
    fn test_review_file_detects_todo_fixme() {
        let source = r#"
fn work_in_progress() {
    // TODO: implement proper validation
    let x = 1;
    // FIXME: this will break under high load
    process(x);
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("wip.rs"), source, "rs");

        let todo_comments: Vec<_> = comments
            .iter()
            .filter(|c| {
                c.category == CommentCategory::Maintainability
                    && (c.body.contains("TODO") || c.body.contains("FIXME"))
            })
            .collect();
        assert!(
            todo_comments.len() >= 2,
            "Expected at least 2 TODO/FIXME comments, got {}",
            todo_comments.len()
        );

        // FIXME should have higher severity than TODO
        let fixme = todo_comments
            .iter()
            .find(|c| c.body.contains("FIXME"))
            .unwrap();
        let todo = todo_comments
            .iter()
            .find(|c| c.body.contains("TODO"))
            .unwrap();
        assert!(fixme.severity > todo.severity);
    }

    #[test]
    fn test_review_file_detects_deep_nesting() {
        let source = r#"
fn deeply_nested() {
    if true {
        if true {
            if true {
                if true {
                    if true {
                        println!("too deep");
                    }
                }
            }
        }
    }
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("nested.rs"), source, "rs");

        let nesting_comments: Vec<_> = comments
            .iter()
            .filter(|c| c.category == CommentCategory::Maintainability && c.body.contains("nested"))
            .collect();
        assert!(
            !nesting_comments.is_empty(),
            "Expected at least one deep nesting comment"
        );
    }

    #[test]
    fn test_review_file_detects_magic_numbers() {
        let source = r#"
fn calculate() {
    let timeout = sleep(3600);
    let buffer_size = vec![0u8; 8192];
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("magic.rs"), source, "rs");

        let magic_comments: Vec<_> = comments
            .iter()
            .filter(|c| {
                c.category == CommentCategory::Maintainability && c.body.contains("Magic number")
            })
            .collect();
        assert!(
            !magic_comments.is_empty(),
            "Expected at least one magic number comment, found {:?}",
            comments.iter().map(|c| &c.body).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_review_diff_detects_new_unwrap() {
        let hunks = vec![(
            10,
            "+    let value = opt.unwrap();\n+    let other = res.unwrap();\n context line\n"
                .to_string(),
        )];

        let engine = ReviewEngine::new();
        let comments = engine.review_diff(&hunks);

        let unwrap_comments: Vec<_> = comments
            .iter()
            .filter(|c| c.category == CommentCategory::Bug && c.body.contains("unwrap"))
            .collect();
        assert_eq!(
            unwrap_comments.len(),
            2,
            "Expected 2 unwrap comments in diff, got {}",
            unwrap_comments.len()
        );
    }

    #[test]
    fn test_review_diff_detects_new_todo() {
        let hunks = vec![(
            5,
            "+    // TODO: fix this later\n+    do_stuff();\n".to_string(),
        )];

        let engine = ReviewEngine::new();
        let comments = engine.review_diff(&hunks);

        let todo_comments: Vec<_> = comments
            .iter()
            .filter(|c| c.body.contains("TODO"))
            .collect();
        assert!(
            !todo_comments.is_empty(),
            "Expected a TODO comment in the diff review"
        );
    }

    #[test]
    fn test_review_file_clean_code_no_issues() {
        let source = r#"
/// Adds two numbers together.
fn add(a: i32, b: i32) -> i32 {
    a + b
}

/// Subtracts b from a.
fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#;
        let engine = ReviewEngine::new();
        let comments = engine.review_file(Path::new("clean.rs"), source, "rs");

        // Clean code should produce no bug or high-severity comments
        let serious_comments: Vec<_> = comments.iter().filter(|c| c.severity >= 3).collect();
        assert!(
            serious_comments.is_empty(),
            "Expected no serious comments for clean code, got: {:?}",
            serious_comments.iter().map(|c| &c.body).collect::<Vec<_>>()
        );
    }
}
