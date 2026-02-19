//! Code quality scoring — static metrics with A-F grade system.
//!
//! Provides cyclomatic complexity analysis, documentation coverage,
//! and composite quality scoring without requiring LLM calls.

use crate::ast::{self, Language};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Quality grade (A through F).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Grade {
    A,
    B,
    C,
    D,
    F,
}

impl Grade {
    /// Numeric value (4.0 for A down to 0.0 for F).
    pub fn score(&self) -> f64 {
        match self {
            Grade::A => 4.0,
            Grade::B => 3.0,
            Grade::C => 2.0,
            Grade::D => 1.0,
            Grade::F => 0.0,
        }
    }

    /// Grade from a numeric score (0.0 to 4.0).
    pub fn from_score(score: f64) -> Self {
        if score >= 3.5 {
            Grade::A
        } else if score >= 2.5 {
            Grade::B
        } else if score >= 1.5 {
            Grade::C
        } else if score >= 0.5 {
            Grade::D
        } else {
            Grade::F
        }
    }
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Grade::A => write!(f, "A"),
            Grade::B => write!(f, "B"),
            Grade::C => write!(f, "C"),
            Grade::D => write!(f, "D"),
            Grade::F => write!(f, "F"),
        }
    }
}

/// Complexity metrics for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionComplexity {
    /// Function name.
    pub name: String,
    /// Cyclomatic complexity.
    pub complexity: u32,
    /// Number of lines.
    pub lines: usize,
    /// Start line in file.
    pub start_line: usize,
    /// Grade for this function.
    pub grade: Grade,
}

/// Quality metrics for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileQuality {
    /// File path.
    pub path: String,
    /// Language detected.
    pub language: String,
    /// Per-function complexity.
    pub functions: Vec<FunctionComplexity>,
    /// Average cyclomatic complexity.
    pub avg_complexity: f64,
    /// Maximum cyclomatic complexity.
    pub max_complexity: u32,
    /// Total lines of code (excluding blanks and comments).
    pub loc: usize,
    /// Total lines (including blanks and comments).
    pub total_lines: usize,
    /// Documentation coverage (0.0 to 1.0).
    pub doc_coverage: f64,
    /// Complexity grade.
    pub complexity_grade: Grade,
    /// Documentation grade.
    pub doc_grade: Grade,
    /// Overall grade.
    pub overall_grade: Grade,
}

/// Quality metrics for an entire project or set of files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    /// Per-file quality metrics.
    pub files: Vec<FileQuality>,
    /// Average complexity across all files.
    pub avg_complexity: f64,
    /// Average documentation coverage.
    pub avg_doc_coverage: f64,
    /// Overall complexity grade.
    pub complexity_grade: Grade,
    /// Overall documentation grade.
    pub doc_grade: Grade,
    /// Overall quality grade.
    pub overall_grade: Grade,
    /// Total lines of code.
    pub total_loc: usize,
    /// Total functions analyzed.
    pub total_functions: usize,
    /// Functions exceeding complexity threshold.
    pub high_complexity_functions: Vec<FunctionComplexity>,
}

/// Configuration for quality scoring thresholds.
#[derive(Debug, Clone)]
pub struct QualityConfig {
    /// Complexity thresholds for grades.
    pub complexity_thresholds: ComplexityThresholds,
    /// Doc coverage thresholds for grades.
    pub doc_thresholds: DocThresholds,
    /// Complexity above which a function is flagged.
    pub high_complexity_threshold: u32,
}

/// Complexity grade thresholds.
#[derive(Debug, Clone)]
pub struct ComplexityThresholds {
    pub a_max: u32, // A: <=5
    pub b_max: u32, // B: <=10
    pub c_max: u32, // C: <=15
    pub d_max: u32, // D: <=25
}

/// Documentation coverage grade thresholds.
#[derive(Debug, Clone)]
pub struct DocThresholds {
    pub a_min: f64, // A: >90%
    pub b_min: f64, // B: >75%
    pub c_min: f64, // C: >50%
    pub d_min: f64, // D: >25%
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            complexity_thresholds: ComplexityThresholds {
                a_max: 5,
                b_max: 10,
                c_max: 15,
                d_max: 25,
            },
            doc_thresholds: DocThresholds {
                a_min: 0.90,
                b_min: 0.75,
                c_min: 0.50,
                d_min: 0.25,
            },
            high_complexity_threshold: 15,
        }
    }
}

/// Quality scorer for code analysis.
pub struct QualityScorer {
    config: QualityConfig,
}

impl QualityScorer {
    /// Create a new quality scorer with default thresholds.
    pub fn new() -> Self {
        Self {
            config: QualityConfig::default(),
        }
    }

    /// Create a new quality scorer with custom configuration.
    pub fn with_config(config: QualityConfig) -> Self {
        Self { config }
    }

    /// Analyze a single file.
    pub fn analyze_file(&self, source: &str, path: &Path) -> FileQuality {
        let language = Language::from_path(path);
        let total_lines = source.lines().count();
        let loc = count_loc(source, language);

        // Extract functions and compute complexity
        let symbols = ast::extract_symbols(source, language, path).unwrap_or_default();
        let functions: Vec<FunctionComplexity> = symbols
            .iter()
            .filter(|s| matches!(s.kind, ast::SymbolKind::Function | ast::SymbolKind::Method))
            .map(|s| {
                let func_body = extract_lines(source, s.start_line, s.end_line);
                let complexity = ast::cyclomatic_complexity(&func_body, language);
                let grade = self.complexity_grade(complexity);
                FunctionComplexity {
                    name: s.name.clone(),
                    complexity,
                    lines: s.end_line.saturating_sub(s.start_line) + 1,
                    start_line: s.start_line,
                    grade,
                }
            })
            .collect();

        let avg_complexity = if functions.is_empty() {
            1.0
        } else {
            functions.iter().map(|f| f.complexity as f64).sum::<f64>() / functions.len() as f64
        };
        let max_complexity = functions.iter().map(|f| f.complexity).max().unwrap_or(1);
        let doc_coverage = compute_doc_coverage(source, language);

        let complexity_grade = self.complexity_grade(avg_complexity as u32);
        let doc_grade = self.doc_grade(doc_coverage);
        let overall_grade = self.composite_grade(complexity_grade, doc_grade);

        FileQuality {
            path: path.display().to_string(),
            language: language.as_str().to_string(),
            functions,
            avg_complexity,
            max_complexity,
            loc,
            total_lines,
            doc_coverage,
            complexity_grade,
            doc_grade,
            overall_grade,
        }
    }

    /// Analyze multiple files and produce a report.
    pub fn analyze_files(&self, files: &[(&str, &Path)]) -> QualityReport {
        let file_results: Vec<FileQuality> = files
            .iter()
            .map(|(source, path)| self.analyze_file(source, path))
            .collect();

        let total_functions: usize = file_results.iter().map(|f| f.functions.len()).sum();
        let total_loc: usize = file_results.iter().map(|f| f.loc).sum();

        let avg_complexity = if file_results.is_empty() {
            1.0
        } else {
            file_results.iter().map(|f| f.avg_complexity).sum::<f64>() / file_results.len() as f64
        };

        let avg_doc_coverage = if file_results.is_empty() {
            0.0
        } else {
            file_results.iter().map(|f| f.doc_coverage).sum::<f64>() / file_results.len() as f64
        };

        let complexity_grade = self.complexity_grade(avg_complexity as u32);
        let doc_grade = self.doc_grade(avg_doc_coverage);
        let overall_grade = self.composite_grade(complexity_grade, doc_grade);

        let high_complexity_functions: Vec<FunctionComplexity> = file_results
            .iter()
            .flat_map(|f| f.functions.iter())
            .filter(|f| f.complexity > self.config.high_complexity_threshold)
            .cloned()
            .collect();

        QualityReport {
            files: file_results,
            avg_complexity,
            avg_doc_coverage,
            complexity_grade,
            doc_grade,
            overall_grade,
            total_loc,
            total_functions,
            high_complexity_functions,
        }
    }

    /// Grade complexity value.
    fn complexity_grade(&self, complexity: u32) -> Grade {
        let t = &self.config.complexity_thresholds;
        if complexity <= t.a_max {
            Grade::A
        } else if complexity <= t.b_max {
            Grade::B
        } else if complexity <= t.c_max {
            Grade::C
        } else if complexity <= t.d_max {
            Grade::D
        } else {
            Grade::F
        }
    }

    /// Grade documentation coverage.
    fn doc_grade(&self, coverage: f64) -> Grade {
        let t = &self.config.doc_thresholds;
        if coverage >= t.a_min {
            Grade::A
        } else if coverage >= t.b_min {
            Grade::B
        } else if coverage >= t.c_min {
            Grade::C
        } else if coverage >= t.d_min {
            Grade::D
        } else {
            Grade::F
        }
    }

    /// Composite grade from individual metrics.
    fn composite_grade(&self, complexity: Grade, docs: Grade) -> Grade {
        // Weight: 60% complexity, 40% documentation
        let score = complexity.score() * 0.6 + docs.score() * 0.4;
        Grade::from_score(score)
    }
}

impl Default for QualityScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Count lines of code (excluding blanks and comments).
fn count_loc(source: &str, language: Language) -> usize {
    let mut count = 0;
    let mut in_block_comment = false;

    for line in source.lines() {
        let trimmed = line.trim();

        // Handle block comments
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.is_empty() {
            continue;
        }

        // Check for block comment start
        if trimmed.starts_with("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            continue;
        }

        // Check for line comments
        let is_comment = match language {
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
            | Language::Scala => trimmed.starts_with("//"),
            Language::Python | Language::Ruby | Language::Bash | Language::Elixir => {
                trimmed.starts_with('#')
            }
            Language::Sql => trimmed.starts_with("--"),
            _ => false,
        };

        if !is_comment {
            count += 1;
        }
    }

    count
}

/// Compute documentation coverage (fraction of functions with doc comments).
fn compute_doc_coverage(source: &str, language: Language) -> f64 {
    let lines: Vec<&str> = source.lines().collect();
    if lines.is_empty() {
        return 0.0;
    }

    let func_pattern = match language {
        Language::Rust => Some(regex::Regex::new(r"(?m)^\s*(?:pub\s+)?(?:async\s+)?fn\s+\w+").ok()),
        Language::Python => Some(regex::Regex::new(r"(?m)^\s*(?:async\s+)?def\s+\w+").ok()),
        Language::JavaScript | Language::TypeScript => {
            Some(regex::Regex::new(r"(?m)^\s*(?:export\s+)?(?:async\s+)?function\s+\w+").ok())
        }
        Language::Go => Some(regex::Regex::new(r"(?m)^func\s+").ok()),
        Language::Java | Language::Kotlin | Language::CSharp => {
            Some(regex::Regex::new(r"(?m)(?:public|private|protected)\s+.*\w+\s*\(").ok())
        }
        _ => None,
    };

    let func_re = match func_pattern {
        Some(Some(re)) => re,
        _ => return 0.0,
    };

    let mut total_functions = 0;
    let mut documented_functions = 0;

    for (i, line) in lines.iter().enumerate() {
        if func_re.is_match(line) {
            total_functions += 1;
            // Check if preceding lines have doc comments
            if has_doc_comment(&lines, i, language) {
                documented_functions += 1;
            }
        }
    }

    if total_functions == 0 {
        return 1.0; // No functions = 100% coverage
    }

    documented_functions as f64 / total_functions as f64
}

/// Check if lines preceding a function definition contain doc comments.
fn has_doc_comment(lines: &[&str], func_line: usize, language: Language) -> bool {
    if func_line == 0 {
        return false;
    }

    let doc_prefixes: &[&str] = match language {
        Language::Rust => &["///", "//!"],
        Language::Python => &["\"\"\"", "'''"],
        Language::JavaScript | Language::TypeScript => &["/**", "* "],
        Language::Go => &["//"],
        Language::Java | Language::Kotlin | Language::CSharp => &["/**", "* ", "///"],
        _ => return false,
    };

    // Look at the line immediately before the function
    let prev_line = lines[func_line - 1].trim();
    for prefix in doc_prefixes {
        if prev_line.starts_with(prefix) {
            return true;
        }
    }

    // For block doc comments, check a few lines back
    if matches!(
        language,
        Language::JavaScript
            | Language::TypeScript
            | Language::Java
            | Language::Kotlin
            | Language::CSharp
    ) {
        for i in (0..func_line).rev().take(10) {
            let line = lines[i].trim();
            if line.ends_with("*/") || line.starts_with("/**") {
                return true;
            }
            if line.is_empty() || (!line.starts_with('*') && !line.starts_with("/**")) {
                break;
            }
        }
    }

    false
}

/// Extract specific lines from source (1-indexed).
fn extract_lines(source: &str, start: usize, end: usize) -> String {
    source
        .lines()
        .skip(start.saturating_sub(1))
        .take(end.saturating_sub(start.saturating_sub(1)))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_scoring() {
        assert_eq!(Grade::A.score(), 4.0);
        assert_eq!(Grade::F.score(), 0.0);
        assert_eq!(Grade::from_score(3.5), Grade::A);
        assert_eq!(Grade::from_score(2.5), Grade::B);
        assert_eq!(Grade::from_score(1.5), Grade::C);
        assert_eq!(Grade::from_score(0.5), Grade::D);
        assert_eq!(Grade::from_score(0.0), Grade::F);
    }

    #[test]
    fn test_grade_display() {
        assert_eq!(Grade::A.to_string(), "A");
        assert_eq!(Grade::F.to_string(), "F");
    }

    #[test]
    fn test_count_loc() {
        let source = r#"
// This is a comment
fn main() {
    // another comment
    let x = 1;

    println!("{}", x);
}
"#;
        let loc = count_loc(source, Language::Rust);
        assert_eq!(loc, 4); // fn, let, println, }
    }

    #[test]
    fn test_analyze_simple_file() {
        let source = r#"
/// Adds two numbers.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn subtract(a: i32, b: i32) -> i32 {
    a - b
}
"#;

        let scorer = QualityScorer::new();
        let result = scorer.analyze_file(source, Path::new("math.rs"));

        assert_eq!(result.language, "rust");
        assert!(result.avg_complexity <= 5.0);
        assert_eq!(result.complexity_grade, Grade::A);
    }

    #[test]
    fn test_high_complexity_detection() {
        let source = r#"
fn complex(x: i32) -> i32 {
    if x > 0 {
        if x > 10 && x < 100 {
            if x > 50 || x < 25 {
                while x > 0 {
                    for i in 0..x {
                        if i > 5 && i < 10 {
                            match x {
                                1 => 1,
                                2 => 2,
                                _ => 3,
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

        let scorer = QualityScorer::new();
        let result = scorer.analyze_file(source, Path::new("complex.rs"));

        // Should have high complexity
        assert!(result.max_complexity > 5);
    }

    #[test]
    fn test_doc_coverage() {
        let source = r#"
/// Documented function.
fn documented() {}

fn undocumented() {}
"#;

        let coverage = compute_doc_coverage(source, Language::Rust);
        assert!((coverage - 0.5).abs() < 0.01); // 1 of 2 documented
    }

    #[test]
    fn test_multi_file_report() {
        let file1 = ("fn simple() { let x = 1; }\n", Path::new("simple.rs"));
        let file2 = ("fn another() { if true { } }\n", Path::new("another.rs"));

        let scorer = QualityScorer::new();
        let report = scorer.analyze_files(&[file1, file2]);

        assert_eq!(report.files.len(), 2);
        assert!(report.total_functions >= 2);
        assert!(report.total_loc > 0);
    }

    #[test]
    fn test_composite_grade() {
        let scorer = QualityScorer::new();
        // A complexity + A docs = A
        let grade = scorer.composite_grade(Grade::A, Grade::A);
        assert_eq!(grade, Grade::A);

        // F complexity + F docs = F
        let grade = scorer.composite_grade(Grade::F, Grade::F);
        assert_eq!(grade, Grade::F);
    }
}
