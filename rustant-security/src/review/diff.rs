//! Diff-aware code analysis — structural diff classification using git2 and AST.
//!
//! Extends existing `git2` usage from `rustant-tools/src/checkpoint.rs` and
//! `similar` from `rustant-tools/src/smart_edit.rs`.

use crate::ast::Language;
use crate::error::ReviewError;
use std::path::{Path, PathBuf};

/// Classification of a code change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    /// New function, modified signature, class/module change.
    Structural,
    /// Logic change, control flow modification.
    Behavioral,
    /// Formatting, comments, whitespace.
    Cosmetic,
    /// Settings, dependencies, build config.
    Configuration,
    /// New file added.
    Added,
    /// File deleted entirely.
    Deleted,
    /// File renamed (possibly with modifications).
    Renamed,
}

impl std::fmt::Display for ChangeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeKind::Structural => write!(f, "structural"),
            ChangeKind::Behavioral => write!(f, "behavioral"),
            ChangeKind::Cosmetic => write!(f, "cosmetic"),
            ChangeKind::Configuration => write!(f, "configuration"),
            ChangeKind::Added => write!(f, "added"),
            ChangeKind::Deleted => write!(f, "deleted"),
            ChangeKind::Renamed => write!(f, "renamed"),
        }
    }
}

/// A single hunk of changes within a file.
#[derive(Debug, Clone)]
pub struct DiffHunk {
    /// Starting line in old file (1-indexed).
    pub old_start: usize,
    /// Number of lines in old file.
    pub old_lines: usize,
    /// Starting line in new file (1-indexed).
    pub new_start: usize,
    /// Number of lines in new file.
    pub new_lines: usize,
    /// The hunk header text (e.g. function name).
    pub header: String,
    /// Added lines (without '+' prefix).
    pub additions: Vec<String>,
    /// Removed lines (without '-' prefix).
    pub deletions: Vec<String>,
    /// Context lines around the change.
    pub context: Vec<String>,
}

/// A changed file in a diff.
#[derive(Debug, Clone)]
pub struct FileChange {
    /// Path of the file (new path for renames).
    pub path: PathBuf,
    /// Old path (for renames only).
    pub old_path: Option<PathBuf>,
    /// Detected language.
    pub language: Language,
    /// Classification of the change.
    pub change_kind: ChangeKind,
    /// Individual hunks of changes.
    pub hunks: Vec<DiffHunk>,
    /// Lines added.
    pub additions: usize,
    /// Lines removed.
    pub deletions: usize,
    /// Old file content (if available).
    pub old_content: Option<String>,
    /// New file content (if available).
    pub new_content: Option<String>,
}

impl FileChange {
    /// Total lines changed (additions + deletions).
    pub fn total_changes(&self) -> usize {
        self.additions + self.deletions
    }

    /// Whether this is a pure addition (new file).
    pub fn is_new_file(&self) -> bool {
        self.change_kind == ChangeKind::Added
    }

    /// Whether this is a pure deletion.
    pub fn is_deleted(&self) -> bool {
        self.change_kind == ChangeKind::Deleted
    }
}

/// Result of analyzing a diff.
#[derive(Debug, Clone)]
pub struct DiffAnalysis {
    /// All file changes.
    pub files: Vec<FileChange>,
    /// Total lines added across all files.
    pub total_additions: usize,
    /// Total lines deleted across all files.
    pub total_deletions: usize,
    /// Number of files changed.
    pub files_changed: usize,
    /// Summary of change kinds.
    pub kind_summary: Vec<(ChangeKind, usize)>,
}

impl DiffAnalysis {
    /// Get files of a specific change kind.
    pub fn files_of_kind(&self, kind: ChangeKind) -> Vec<&FileChange> {
        self.files
            .iter()
            .filter(|f| f.change_kind == kind)
            .collect()
    }

    /// Get files for a specific language.
    pub fn files_for_language(&self, language: Language) -> Vec<&FileChange> {
        self.files
            .iter()
            .filter(|f| f.language == language)
            .collect()
    }

    /// Whether the diff contains any structural changes.
    pub fn has_structural_changes(&self) -> bool {
        self.files
            .iter()
            .any(|f| f.change_kind == ChangeKind::Structural)
    }
}

/// Analyzes git diffs with structural classification.
pub struct DiffAnalyzer;

impl DiffAnalyzer {
    /// Create a new diff analyzer.
    pub fn new() -> Self {
        Self
    }

    /// Analyze a git diff between two refs (e.g., commits, branches).
    pub fn analyze_git_diff(
        &self,
        repo_path: &Path,
        base: &str,
        head: &str,
    ) -> Result<DiffAnalysis, ReviewError> {
        let repo = git2::Repository::open(repo_path)
            .map_err(|e| ReviewError::Git(format!("Failed to open repo: {e}")))?;

        let base_obj = repo
            .revparse_single(base)
            .map_err(|e| ReviewError::Git(format!("Failed to parse ref '{base}': {e}")))?;
        let head_obj = repo
            .revparse_single(head)
            .map_err(|e| ReviewError::Git(format!("Failed to parse ref '{head}': {e}")))?;

        let base_tree = base_obj
            .peel_to_tree()
            .map_err(|e| ReviewError::Git(format!("Failed to get base tree: {e}")))?;
        let head_tree = head_obj
            .peel_to_tree()
            .map_err(|e| ReviewError::Git(format!("Failed to get head tree: {e}")))?;

        let diff = repo
            .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
            .map_err(|e| ReviewError::DiffAnalysis(format!("Failed to create diff: {e}")))?;

        self.analyze_diff(&repo, &diff)
    }

    /// Analyze the working directory diff (unstaged changes).
    pub fn analyze_working_diff(&self, repo_path: &Path) -> Result<DiffAnalysis, ReviewError> {
        let repo = git2::Repository::open(repo_path)
            .map_err(|e| ReviewError::Git(format!("Failed to open repo: {e}")))?;

        let head = repo
            .head()
            .map_err(|e| ReviewError::Git(format!("Failed to get HEAD: {e}")))?;
        let head_tree = head
            .peel_to_tree()
            .map_err(|e| ReviewError::Git(format!("Failed to get HEAD tree: {e}")))?;

        let diff = repo
            .diff_tree_to_workdir(Some(&head_tree), None)
            .map_err(|e| {
                ReviewError::DiffAnalysis(format!("Failed to create workdir diff: {e}"))
            })?;

        self.analyze_diff(&repo, &diff)
    }

    /// Analyze a text diff (non-git, for standalone use).
    pub fn analyze_text_diff(
        &self,
        old_content: &str,
        new_content: &str,
        file_path: &Path,
    ) -> DiffAnalysis {
        let diff = similar::TextDiff::from_lines(old_content, new_content);
        let language = Language::from_path(file_path);

        let mut additions = 0;
        let mut deletions = 0;
        let mut hunks = Vec::new();

        for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
            let mut hunk_additions = Vec::new();
            let mut hunk_deletions = Vec::new();
            let mut hunk_context = Vec::new();
            let mut old_start = usize::MAX;
            let mut new_start = usize::MAX;
            let mut old_count = 0usize;
            let mut new_count = 0usize;

            for change in hunk.iter_changes() {
                match change.tag() {
                    similar::ChangeTag::Insert => {
                        if let Some(new_idx) = change.new_index() {
                            if new_start == usize::MAX {
                                new_start = new_idx + 1;
                            }
                            new_count += 1;
                        }
                        additions += 1;
                        hunk_additions.push(change.value().to_string());
                    }
                    similar::ChangeTag::Delete => {
                        if let Some(old_idx) = change.old_index() {
                            if old_start == usize::MAX {
                                old_start = old_idx + 1;
                            }
                            old_count += 1;
                        }
                        deletions += 1;
                        hunk_deletions.push(change.value().to_string());
                    }
                    similar::ChangeTag::Equal => {
                        if let Some(old_idx) = change.old_index() {
                            if old_start == usize::MAX {
                                old_start = old_idx + 1;
                            }
                            old_count += 1;
                        }
                        if let Some(new_idx) = change.new_index() {
                            if new_start == usize::MAX {
                                new_start = new_idx + 1;
                            }
                            new_count += 1;
                        }
                        hunk_context.push(change.value().to_string());
                    }
                }
            }

            hunks.push(DiffHunk {
                old_start: if old_start == usize::MAX {
                    1
                } else {
                    old_start
                },
                old_lines: old_count,
                new_start: if new_start == usize::MAX {
                    1
                } else {
                    new_start
                },
                new_lines: new_count,
                header: String::new(),
                additions: hunk_additions,
                deletions: hunk_deletions,
                context: hunk_context,
            });
        }

        let change_kind = classify_change(old_content, new_content, language);

        let file_change = FileChange {
            path: file_path.to_path_buf(),
            old_path: None,
            language,
            change_kind,
            hunks,
            additions,
            deletions,
            old_content: Some(old_content.to_string()),
            new_content: Some(new_content.to_string()),
        };

        let kind_summary = vec![(change_kind, 1)];

        DiffAnalysis {
            total_additions: additions,
            total_deletions: deletions,
            files_changed: 1,
            kind_summary,
            files: vec![file_change],
        }
    }

    /// Internal: analyze a git2::Diff into DiffAnalysis.
    fn analyze_diff(
        &self,
        repo: &git2::Repository,
        diff: &git2::Diff,
    ) -> Result<DiffAnalysis, ReviewError> {
        let mut files = Vec::new();
        let mut total_additions = 0;
        let mut total_deletions = 0;
        let mut kind_counts: std::collections::HashMap<ChangeKind, usize> =
            std::collections::HashMap::new();

        for (delta_idx, delta) in diff.deltas().enumerate() {
            let new_path = delta
                .new_file()
                .path()
                .unwrap_or(Path::new("unknown"))
                .to_path_buf();
            let old_path = delta.old_file().path().map(|p| p.to_path_buf());

            let language = Language::from_path(&new_path);

            let status = delta.status();
            let base_kind = match status {
                git2::Delta::Added => ChangeKind::Added,
                git2::Delta::Deleted => ChangeKind::Deleted,
                git2::Delta::Renamed => ChangeKind::Renamed,
                _ => ChangeKind::Behavioral, // default, will be refined
            };

            // Extract hunks for this delta
            let mut hunks = Vec::new();
            let mut file_additions = 0;
            let mut file_deletions = 0;

            if let Ok(Some(patch)) = git2::Patch::from_diff(diff, delta_idx) {
                let num_hunks = patch.num_hunks();
                for hunk_idx in 0..num_hunks {
                    if let Ok((hunk, _)) = patch.hunk(hunk_idx) {
                        let mut hunk_add = Vec::new();
                        let mut hunk_del = Vec::new();
                        let mut hunk_ctx = Vec::new();

                        let num_lines = patch.num_lines_in_hunk(hunk_idx).unwrap_or(0);
                        for line_idx in 0..num_lines {
                            if let Ok(line) = patch.line_in_hunk(hunk_idx, line_idx) {
                                let content = String::from_utf8_lossy(line.content()).to_string();
                                match line.origin() {
                                    '+' => {
                                        file_additions += 1;
                                        hunk_add.push(content);
                                    }
                                    '-' => {
                                        file_deletions += 1;
                                        hunk_del.push(content);
                                    }
                                    ' ' => {
                                        hunk_ctx.push(content);
                                    }
                                    _ => {}
                                }
                            }
                        }

                        let header = String::from_utf8_lossy(hunk.header()).to_string();
                        hunks.push(DiffHunk {
                            old_start: hunk.old_start() as usize,
                            old_lines: hunk.old_lines() as usize,
                            new_start: hunk.new_start() as usize,
                            new_lines: hunk.new_lines() as usize,
                            header,
                            additions: hunk_add,
                            deletions: hunk_del,
                            context: hunk_ctx,
                        });
                    }
                }
            }

            // Try to refine the change kind if it's not already classified
            let change_kind = if matches!(
                base_kind,
                ChangeKind::Added | ChangeKind::Deleted | ChangeKind::Renamed
            ) {
                base_kind
            } else {
                // Try to read file contents for classification
                let old_content = self.read_blob(repo, &delta.old_file());
                let new_content = self.read_blob(repo, &delta.new_file());
                match (old_content, new_content) {
                    (Some(old), Some(new)) => classify_change(&old, &new, language),
                    _ => base_kind,
                }
            };

            total_additions += file_additions;
            total_deletions += file_deletions;
            *kind_counts.entry(change_kind).or_insert(0) += 1;

            files.push(FileChange {
                path: new_path,
                old_path,
                language,
                change_kind,
                hunks,
                additions: file_additions,
                deletions: file_deletions,
                old_content: None, // expensive to store for all files
                new_content: None,
            });
        }

        let kind_summary: Vec<(ChangeKind, usize)> = kind_counts.into_iter().collect();

        Ok(DiffAnalysis {
            files_changed: files.len(),
            total_additions,
            total_deletions,
            kind_summary,
            files,
        })
    }

    /// Read a blob from a git diff file.
    fn read_blob(&self, repo: &git2::Repository, file: &git2::DiffFile) -> Option<String> {
        let oid = file.id();
        if oid.is_zero() {
            return None;
        }
        repo.find_blob(oid).ok().and_then(|blob| {
            std::str::from_utf8(blob.content())
                .ok()
                .map(|s| s.to_string())
        })
    }
}

impl Default for DiffAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify a change based on content comparison.
fn classify_change(old: &str, new: &str, language: Language) -> ChangeKind {
    // Check if it's a config file
    if is_config_language(language) {
        return ChangeKind::Configuration;
    }

    // Use similar to get the actual changes
    let diff = similar::TextDiff::from_lines(old, new);
    let mut has_code_changes = false;
    let mut only_whitespace_or_comments = true;

    for change in diff.iter_all_changes() {
        if change.tag() == similar::ChangeTag::Equal {
            continue;
        }
        let line = change.value().trim();
        if line.is_empty() {
            continue;
        }

        // Check if line is a comment
        if is_comment_line(line, language) {
            continue;
        }

        only_whitespace_or_comments = false;

        // Check for structural changes
        if is_structural_change(line, language) {
            return ChangeKind::Structural;
        }

        has_code_changes = true;
    }

    if !has_code_changes || only_whitespace_or_comments {
        return ChangeKind::Cosmetic;
    }

    ChangeKind::Behavioral
}

/// Check if a language is typically a configuration format.
fn is_config_language(language: Language) -> bool {
    matches!(
        language,
        Language::Yaml | Language::Toml | Language::Json | Language::Hcl | Language::Dockerfile
    )
}

/// Check if a line is a comment in the given language.
fn is_comment_line(line: &str, language: Language) -> bool {
    match language {
        Language::Rust
        | Language::Go
        | Language::Java
        | Language::Kotlin
        | Language::CSharp
        | Language::Cpp
        | Language::C
        | Language::JavaScript
        | Language::TypeScript
        | Language::Swift
        | Language::Dart
        | Language::Scala => {
            line.starts_with("//")
                || line.starts_with("/*")
                || line.starts_with("* ")
                || line.starts_with("*/")
        }
        Language::Python | Language::Ruby | Language::Bash | Language::Elixir => {
            line.starts_with('#')
        }
        Language::Php => {
            line.starts_with("//")
                || line.starts_with('#')
                || line.starts_with("/*")
                || line.starts_with("* ")
        }
        Language::Html | Language::Css => line.starts_with("<!--") || line.starts_with("/*"),
        Language::Sql => line.starts_with("--") || line.starts_with("/*"),
        _ => false,
    }
}

/// Check if a line represents a structural change (function/class/type definition).
fn is_structural_change(line: &str, language: Language) -> bool {
    match language {
        Language::Rust => {
            line.contains("fn ")
                || line.contains("struct ")
                || line.contains("enum ")
                || line.contains("trait ")
                || line.contains("impl ")
                || line.contains("mod ")
                || line.contains("type ")
                || line.contains("use ")
        }
        Language::Python => {
            line.starts_with("def ")
                || line.starts_with("class ")
                || line.starts_with("async def ")
                || line.starts_with("import ")
                || line.starts_with("from ")
        }
        Language::JavaScript | Language::TypeScript => {
            line.contains("function ")
                || line.contains("class ")
                || line.contains("interface ")
                || line.contains("type ")
                || line.contains("import ")
                || line.contains("export ")
        }
        Language::Go => {
            line.starts_with("func ")
                || line.starts_with("type ")
                || line.starts_with("import ")
                || line.starts_with("package ")
        }
        Language::Java | Language::Kotlin | Language::CSharp => {
            line.contains("class ")
                || line.contains("interface ")
                || line.contains("import ")
                || line.contains("package ")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_diff_analysis() {
        let old = "fn hello() {\n    println!(\"hello\");\n}\n";
        let new = "fn hello() {\n    println!(\"world\");\n}\n";

        let analyzer = DiffAnalyzer::new();
        let result = analyzer.analyze_text_diff(old, new, Path::new("test.rs"));

        assert_eq!(result.files_changed, 1);
        assert_eq!(result.total_additions, 1);
        assert_eq!(result.total_deletions, 1);
        assert_eq!(result.files[0].change_kind, ChangeKind::Behavioral);
    }

    #[test]
    fn test_cosmetic_change_detection() {
        let old = "fn foo() {\n    // old comment\n    let x = 1;\n}\n";
        let new = "fn foo() {\n    // new comment\n    let x = 1;\n}\n";

        let kind = classify_change(old, new, Language::Rust);
        assert_eq!(kind, ChangeKind::Cosmetic);
    }

    #[test]
    fn test_structural_change_detection() {
        let old = "fn foo() {\n    let x = 1;\n}\n";
        let new = "fn foo() {\n    let x = 1;\n}\n\nfn bar() {\n    let y = 2;\n}\n";

        let kind = classify_change(old, new, Language::Rust);
        assert_eq!(kind, ChangeKind::Structural);
    }

    #[test]
    fn test_config_change_detection() {
        let old = "key: value1\n";
        let new = "key: value2\n";

        let kind = classify_change(old, new, Language::Yaml);
        assert_eq!(kind, ChangeKind::Configuration);
    }

    #[test]
    fn test_change_kind_display() {
        assert_eq!(ChangeKind::Structural.to_string(), "structural");
        assert_eq!(ChangeKind::Behavioral.to_string(), "behavioral");
        assert_eq!(ChangeKind::Cosmetic.to_string(), "cosmetic");
    }

    #[test]
    fn test_file_change_helpers() {
        let change = FileChange {
            path: PathBuf::from("test.rs"),
            old_path: None,
            language: Language::Rust,
            change_kind: ChangeKind::Added,
            hunks: Vec::new(),
            additions: 10,
            deletions: 0,
            old_content: None,
            new_content: None,
        };

        assert!(change.is_new_file());
        assert!(!change.is_deleted());
        assert_eq!(change.total_changes(), 10);
    }

    #[test]
    fn test_diff_analysis_filters() {
        let files = vec![
            FileChange {
                path: PathBuf::from("src/main.rs"),
                old_path: None,
                language: Language::Rust,
                change_kind: ChangeKind::Structural,
                hunks: Vec::new(),
                additions: 5,
                deletions: 2,
                old_content: None,
                new_content: None,
            },
            FileChange {
                path: PathBuf::from("config.yaml"),
                old_path: None,
                language: Language::Yaml,
                change_kind: ChangeKind::Configuration,
                hunks: Vec::new(),
                additions: 1,
                deletions: 1,
                old_content: None,
                new_content: None,
            },
        ];

        let analysis = DiffAnalysis {
            files,
            total_additions: 6,
            total_deletions: 3,
            files_changed: 2,
            kind_summary: vec![(ChangeKind::Structural, 1), (ChangeKind::Configuration, 1)],
        };

        assert!(analysis.has_structural_changes());
        assert_eq!(analysis.files_of_kind(ChangeKind::Structural).len(), 1);
        assert_eq!(analysis.files_for_language(Language::Rust).len(), 1);
    }
}
